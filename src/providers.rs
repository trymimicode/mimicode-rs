use std::sync::{Arc, LazyLock, Mutex};

pub const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
pub const DEFAULT_MAX_TOKENS: u32 = 8192;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

fn cache_control() -> serde_json::Value {
    serde_json::json!({"type": "ephemeral"})
}

// ── SSE parser ───────────────────────────────────────────────────────────────

struct SseEvent {
    event_type: Option<String>,
    data: Option<String>,
}

fn parse_sse_line(line: &str, current: &mut SseEvent) -> Option<serde_json::Value> {
    if let Some(rest) = line.strip_prefix("event: ") {
        current.event_type = Some(rest.trim().to_string());
        return None;
    }
    if let Some(rest) = line.strip_prefix("data: ") {
        let data_str = rest.trim();
        if data_str == "[DONE]" {
            return None;
        }
        current.data = Some(data_str.to_string());
        return None;
    }
    if line.trim().is_empty() {
        if let Some(data_str) = current.data.take() {
            let result = serde_json::from_str(&data_str).ok();
            *current = SseEvent { event_type: None, data: None };
            return result;
        }
    }
    None
}

// ── Structs / enums ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub model: String,
}

pub enum StreamEvent {
    TextStart { index: usize },
    TextDelta { index: usize, text: String },
    ToolStart { index: usize, id: String, name: String },
    ToolComplete { index: usize, id: String, name: String, input: serde_json::Value },
    Done,
}

// ── Global usage state ───────────────────────────────────────────────────────

static LAST_USAGE: LazyLock<Mutex<UsageStats>> =
    LazyLock::new(|| Mutex::new(UsageStats::default()));

pub fn get_last_usage() -> UsageStats {
    LAST_USAGE.lock().unwrap().clone()
}

fn set_last_usage(stats: UsageStats) {
    *LAST_USAGE.lock().unwrap() = stats;
}

// ── Cache helpers ────────────────────────────────────────────────────────────

fn wrap_system(system: &str) -> serde_json::Value {
    serde_json::json!([{
        "type": "text",
        "text": system,
        "cache_control": {"type": "ephemeral"}
    }])
}

fn cache_last_tool(tools: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut tools = tools.to_vec();
    if let Some(last) = tools.last_mut() {
        if let Some(obj) = last.as_object_mut() {
            obj.insert("cache_control".to_string(), cache_control());
        }
    }
    tools
}

fn cache_last_message(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    if messages.is_empty() {
        return vec![];
    }
    let mut messages = messages.to_vec();
    let last = messages.last_mut().unwrap();
    match last.get("content").cloned() {
        Some(serde_json::Value::String(s)) => {
            last["content"] = serde_json::json!([{
                "type": "text",
                "text": s,
                "cache_control": {"type": "ephemeral"}
            }]);
        }
        Some(serde_json::Value::Array(mut blocks)) => {
            if let Some(last_block) = blocks.last_mut() {
                if let Some(obj) = last_block.as_object_mut() {
                    obj.insert("cache_control".to_string(), cache_control());
                }
            }
            last["content"] = serde_json::Value::Array(blocks);
        }
        _ => {}
    }
    messages
}

// ── API call ─────────────────────────────────────────────────────────────────

pub async fn call_claude(
    messages: &[serde_json::Value],
    system: &str,
    tools: &[serde_json::Value],
    model: &str,
    cache: bool,
) -> anyhow::Result<serde_json::Value> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "messages": if cache { cache_last_message(messages) } else { messages.to_vec() }
    });

    if !system.is_empty() {
        body["system"] = if cache {
            wrap_system(system)
        } else {
            serde_json::json!(system)
        };
    }

    if !tools.is_empty() {
        body["tools"] = if cache {
            serde_json::json!(cache_last_tool(tools))
        } else {
            serde_json::json!(tools)
        };
    }

    crate::logger::log("model_request", serde_json::json!({
        "model": model,
        "n_messages": messages.len(),
        "n_tools": tools.len(),
        "cache": cache
    }));

    let client = reqwest::Client::new();
    let resp = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .header("anthropic-beta", "prompt-caching-2024-07-31")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("API error {status}: {text}");
    }

    let data: serde_json::Value = resp.json().await?;

    let usage = &data["usage"];
    let stats = UsageStats {
        tokens_in:   usage["input_tokens"].as_u64().unwrap_or(0),
        tokens_out:  usage["output_tokens"].as_u64().unwrap_or(0),
        cache_read:  usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_write: usage["cache_creation_input_tokens"].as_u64().unwrap_or(0),
        model: model.to_string(),
    };
    set_last_usage(stats.clone());

    crate::logger::log("model_response", serde_json::json!({
        "model": model,
        "stop_reason": data["stop_reason"],
        "tokens_in": stats.tokens_in,
        "tokens_out": stats.tokens_out,
        "cache_read": stats.cache_read,
        "cache_write": stats.cache_write,
    }));

    let content = data["content"].clone();
    Ok(serde_json::json!({
        "role": "assistant",
        "content": content
    }))
}

// ── Streaming API call ────────────────────────────────────────────────────────

pub async fn call_claude_streaming(
    messages: &[serde_json::Value],
    system: &str,
    tools: &[serde_json::Value],
    model: &str,
    cache: bool,
    on_event: Option<tokio::sync::mpsc::Sender<StreamEvent>>,
    cancel: Option<Arc<tokio::sync::Notify>>,
) -> anyhow::Result<serde_json::Value> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": DEFAULT_MAX_TOKENS,
        "messages": if cache { cache_last_message(messages) } else { messages.to_vec() }
    });

    if !system.is_empty() {
        body["system"] = if cache { wrap_system(system) } else { serde_json::json!(system) };
    }

    if !tools.is_empty() {
        body["tools"] = if cache {
            serde_json::json!(cache_last_tool(tools))
        } else {
            serde_json::json!(tools)
        };
    }

    body["stream"] = serde_json::json!(true);

    crate::logger::log("model_request_streaming", serde_json::json!({
        "model": model,
        "n_messages": messages.len(),
        "n_tools": tools.len(),
        "cache": cache
    }));

    let client = reqwest::Client::new();
    let resp = client
        .post(ANTHROPIC_API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .header("anthropic-beta", "prompt-caching-2024-07-31")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("API error {status}: {text}");
    }

    use futures_util::StreamExt;

    let mut stream = resp.bytes_stream();
    let mut line_buf = String::new();
    let mut sse_event = SseEvent { event_type: None, data: None };
    let mut content_blocks: Vec<serde_json::Value> = Vec::new();
    let mut usage_stats = UsageStats { model: model.to_string(), ..Default::default() };

    'stream: while let Some(chunk) = stream.next().await {
        if let Some(ref notify) = cancel {
            tokio::select! {
                biased;
                _ = notify.notified() => break 'stream,
                _ = std::future::ready(()) => {}
            }
        }

        let bytes = chunk?;
        let text = String::from_utf8_lossy(&bytes);
        line_buf.push_str(&text);

        while let Some(newline_pos) = line_buf.find('\n') {
            let line = line_buf[..newline_pos].trim_end_matches('\r').to_string();
            line_buf.drain(..=newline_pos);

            if let Some(event_data) = parse_sse_line(&line, &mut sse_event) {
                handle_sse_event(event_data, &mut content_blocks, &mut usage_stats, &on_event).await?;
            }
        }
    }

    set_last_usage(usage_stats.clone());

    crate::logger::log("model_response_streaming", serde_json::json!({
        "model": model,
        "tokens_in": usage_stats.tokens_in,
        "tokens_out": usage_stats.tokens_out,
        "cache_read": usage_stats.cache_read,
        "cache_write": usage_stats.cache_write,
    }));

    for block in &mut content_blocks {
        if let Some(obj) = block.as_object_mut() {
            obj.remove("_input_json");
        }
    }

    Ok(serde_json::json!({
        "role": "assistant",
        "content": content_blocks
    }))
}

async fn handle_sse_event(
    data: serde_json::Value,
    blocks: &mut Vec<serde_json::Value>,
    usage: &mut UsageStats,
    on_event: &Option<tokio::sync::mpsc::Sender<StreamEvent>>,
) -> anyhow::Result<()> {
    match data["type"].as_str() {
        Some("message_start") => {
            let u = &data["message"]["usage"];
            usage.tokens_in   = u["input_tokens"].as_u64().unwrap_or(0);
            usage.cache_read  = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
            usage.cache_write = u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
        }
        Some("content_block_start") => {
            let idx = data["index"].as_u64().unwrap_or(0) as usize;
            match data["content_block"]["type"].as_str().unwrap_or("") {
                "text" => {
                    blocks.push(serde_json::json!({"type": "text", "text": ""}));
                    if let Some(tx) = on_event {
                        tx.send(StreamEvent::TextStart { index: idx }).await.ok();
                    }
                }
                "tool_use" => {
                    let id   = data["content_block"]["id"].as_str().unwrap_or("").to_string();
                    let name = data["content_block"]["name"].as_str().unwrap_or("").to_string();
                    blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": {},
                        "_input_json": ""
                    }));
                    if let Some(tx) = on_event {
                        tx.send(StreamEvent::ToolStart { index: idx, id, name }).await.ok();
                    }
                }
                _ => {}
            }
        }
        Some("content_block_delta") => {
            let idx = data["index"].as_u64().unwrap_or(0) as usize;
            match data["delta"]["type"].as_str().unwrap_or("") {
                "text_delta" => {
                    let text = data["delta"]["text"].as_str().unwrap_or("").to_string();
                    if let Some(block) = blocks.get_mut(idx) {
                        if let Some(existing) = block["text"].as_str() {
                            let new_text = format!("{existing}{text}");
                            block["text"] = serde_json::json!(new_text);
                        }
                    }
                    if let Some(tx) = on_event {
                        tx.send(StreamEvent::TextDelta { index: idx, text }).await.ok();
                    }
                }
                "input_json_delta" => {
                    let partial = data["delta"]["partial_json"].as_str().unwrap_or("").to_string();
                    if let Some(block) = blocks.get_mut(idx) {
                        let current = block["_input_json"].as_str().unwrap_or("").to_string();
                        block["_input_json"] = serde_json::json!(format!("{current}{partial}"));
                    }
                }
                _ => {}
            }
        }
        Some("content_block_stop") => {
            let idx = data["index"].as_u64().unwrap_or(0) as usize;
            if let Some(block) = blocks.get_mut(idx) {
                if block["type"] == "tool_use" {
                    let input_json = block["_input_json"].as_str().unwrap_or("{}").to_string();
                    let parsed: serde_json::Value =
                        serde_json::from_str(&input_json).unwrap_or(serde_json::json!({}));
                    block["input"] = parsed.clone();
                    if let Some(obj) = block.as_object_mut() {
                        obj.remove("_input_json");
                    }
                    let id   = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    if let Some(tx) = on_event {
                        tx.send(StreamEvent::ToolComplete { index: idx, id, name, input: parsed }).await.ok();
                    }
                }
            }
        }
        Some("message_delta") => {
            usage.tokens_out = data["usage"]["output_tokens"].as_u64().unwrap_or(0);
        }
        Some("message_stop") | None => {}
        _ => {}
    }
    Ok(())
}
