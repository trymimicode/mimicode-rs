use std::io::{self, BufRead, Write};

use anyhow::Result;
use serde_json::Value;

use crate::providers::call_claude;
use crate::tools::edit::{edit, EditOp};
use crate::tools::{ToolResult, bash::bash, read::read, write::write};
use crate::types::{ContentBlock, Message, MessageContent};

const MODEL: &str = "claude-haiku-4-5-20251001";
const MAX_STEPS: usize = 25;

fn tool_definitions() -> Vec<serde_json::Value> {
    serde_json::from_str(r#"[
      {
        "name": "bash",
        "description": "Run a shell command. stdout and stderr are merged. Output capped at 100KB (tail kept).",
        "input_schema": {
          "type": "object",
          "properties": {
            "cmd":     { "type": "string", "description": "The shell command to run." },
            "timeout": { "type": "number", "description": "Optional timeout in seconds." }
          },
          "required": ["cmd"]
        }
      },
      {
        "name": "read",
        "description": "Read a text file with 1-indexed line numbers. Returns up to 2000 lines by default.",
        "input_schema": {
          "type": "object",
          "properties": {
            "path":   { "type": "string",  "description": "Path to the file." },
            "offset": { "type": "integer", "description": "1-indexed line to start from (default 1)." },
            "limit":  { "type": "integer", "description": "Max lines to return (default 2000)." }
          },
          "required": ["path"]
        }
      },
      {
        "name": "write",
        "description": "Write (or overwrite) a file with the given content. Creates parent directories.",
        "input_schema": {
          "type": "object",
          "properties": {
            "path":    { "type": "string", "description": "Destination path." },
            "content": { "type": "string", "description": "Full file content to write." }
          },
          "required": ["path", "content"]
        }
      },
      {
        "name": "edit",
        "description": "Replace exact text in a file. Use old_text+new_text for a single edit, or edits[] for a batch (atomic). Each old_text must match exactly once.",
        "input_schema": {
          "type": "object",
          "properties": {
            "path":     { "type": "string", "description": "File to edit." },
            "old_text": { "type": "string", "description": "Exact text to replace (single edit)." },
            "new_text": { "type": "string", "description": "Replacement text (single edit)." },
            "edits": {
              "type": "array",
              "description": "Batch of edits applied atomically.",
              "items": {
                "type": "object",
                "properties": {
                  "old_text": { "type": "string" },
                  "new_text": { "type": "string" }
                },
                "required": ["old_text", "new_text"]
              }
            }
          },
          "required": ["path"]
        }
      }
    ]"#).unwrap()
}

pub async fn agent_turn(
    user_msg: &str,
    history: &mut Vec<Message>,
    system: &str,
    cwd: &str,
) -> Result<String> {
    history.push(Message { role: "user".into(), content: MessageContent::Text(user_msg.into()) });

    for _ in 0..MAX_STEPS {
        let reply = call_claude(history, system, MODEL, tool_definitions()).await?;

        let tool_uses: Vec<(String, String, Value)> = match &reply.content {
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolUse { id, name, input } = b {
                        Some((id.clone(), name.clone(), input.clone()))
                    } else {
                        None
                    }
                })
                .collect(),
            _ => vec![],
        };

        history.push(reply);

        if tool_uses.is_empty() {
            return Ok(extract_text(history.last().unwrap()));
        }

        let mut result_blocks = Vec::new();
        for (id, name, input) in tool_uses {
            let result = dispatch(&name, &input, cwd).await;
            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content: result.output,
                is_error: result.is_error,
            });
        }

        history.push(Message {
            role: "user".into(),
            content: MessageContent::Blocks(result_blocks),
        });
    }

    anyhow::bail!("reached {MAX_STEPS}-step limit without a final response")
}

async fn dispatch(name: &str, input: &Value, cwd: &str) -> ToolResult {
    match name {
        "bash" => {
            let Some(cmd) = input.get("cmd").and_then(|v| v.as_str()) else {
                return ToolResult::err("[error] missing required argument: cmd".to_string());
            };
            let timeout = input.get("timeout").and_then(|v| v.as_f64());
            bash(cmd, cwd, timeout).await
        }

        "read" => {
            let Some(path) = input.get("path").and_then(|v| v.as_str()) else {
                return ToolResult::err("[error] missing required argument: path".to_string());
            };
            let offset = input.get("offset").and_then(|v| v.as_u64()).map(|n| n as usize).unwrap_or(1);
            let limit = input.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
            read(path, cwd, offset, limit).await
        }

        "write" => {
            let Some(path) = input.get("path").and_then(|v| v.as_str()) else {
                return ToolResult::err("[error] missing required argument: path".to_string());
            };
            let Some(content) = input.get("content").and_then(|v| v.as_str()) else {
                return ToolResult::err("[error] missing required argument: content".to_string());
            };
            write(path, content, cwd).await
        }

        "edit" => {
            let Some(path) = input.get("path").and_then(|v| v.as_str()) else {
                return ToolResult::err("[error] missing required argument: path".to_string());
            };
            let old_text = input.get("old_text").and_then(|v| v.as_str());
            let new_text = input.get("new_text").and_then(|v| v.as_str());
            let edits = input.get("edits").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|e| serde_json::from_value::<EditOp>(e.clone()).ok())
                    .collect::<Vec<_>>()
            });
            edit(path, cwd, old_text, new_text, edits).await
        }

        other => ToolResult::err(format!("[error] unknown tool: {other}")),
    }
}

pub async fn run(system: &str, prompt: Option<String>) -> Result<()> {
    let mut history: Vec<Message> = Vec::new();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());

    if let Some(text) = prompt {
        let reply = agent_turn(&text, &mut history, system, &cwd).await?;
        println!("{}", reply);
        return Ok(());
    }

    loop {
        let input = read_line()?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "/exit" || trimmed == "/quit" {
            break;
        }

        let reply = agent_turn(trimmed, &mut history, system, &cwd).await?;
        println!("\n{}\n", reply);
    }

    Ok(())
}

fn extract_text(msg: &Message) -> String {
    match &msg.content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn read_line() -> Result<String> {
    print!("> ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end_matches('\n').trim_end_matches('\r').to_string())
}
