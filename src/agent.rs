use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{Mutex, mpsc};

use crate::mimi_memory::{load_memory, load_rules};
use crate::providers::{call_claude, call_claude_streaming};
use crate::router::{augment_system_prompt, route_turn};
use crate::tools::edit::{edit, EditOp};
use crate::tools::{ToolResult, bash::bash, read::read, write::write};
use crate::types::{ContentBlock, Message, MessageContent};

pub const SYSTEM_PROMPT: &str = r#"You are a coding agent in a minimal harness called mimicode.

You have four tools: read, bash, edit, write. Use them deliberately.

SEARCH RULES (non-negotiable):
- Use `rg` (ripgrep) for every search. rg respects .gitignore by default.
- List files: rg --files (not `find .` or `ls -R`)
- List by extension: rg --files -t py (not `find . -name '*.py'`)
- Search content: rg 'pattern' (not `grep -r`)
- Scope to a dir: rg 'pattern' path/
- Case-insensitive: rg -i 'pattern'
- With line numbers: rg -n 'pattern' (on by default for content search)
- List matching files: rg -l 'pattern'
Never run `find`, `grep -r`, `ls -R`, or `cat <codefile>`. Use the `read` tool for code files.
ALWAYS EXCLUDE from exploration: .venv/ .git/ node_modules/ sessions/ __pycache__/ dist/ build/ .pytest_cache/

EDITING RULES:
- `read` before `edit`. Always.
- `edit` requires old_text to match exactly once. Include 2-3 lines of surrounding context so the match is unique.
- For multiple changes to the SAME file in one logical operation, prefer ONE `edit` call with
  `edits=[{old_text, new_text}, ...]` over multiple sequential `edit` calls. Batched edits are
  atomic: all succeed or none apply.
- `write` only for new files or full rewrites. Never for partial changes.

MEMORY RULES:
- After a turn that modified files OR made a meaningful decision, call `memory_write` with a one-sentence
  summary, the touched component name, and a `change_entry` describing what/why.
- For purely read-only / exploratory turns that produced no carry-forward insight, skip memory_write.
- Do not write speculative or vague summaries.
- When the user asks about something that may have been worked on before ("how did we previously...",
  "have we built...", "where did we decide..."), call `memory_search` before reading source files.

DEBUGGING RULES:
- Before editing any file in response to an error, determine whether the error is in the code or
  in how it was invoked.
- `command not found: <file>.py` means the shell can't execute the file as a program — the script's
  code is almost certainly fine. ALWAYS explain `python <file>.py` as the fix. Do NOT edit the file.
- Non-zero exit codes from test runners (pytest, etc.) are expected when tests fail — read the output.

STYLE:
- Prefer one targeted tool call over a broad one. Scope searches.
- Tool output is capped at 100KB. If you hit that, your scope was too wide.
- Be concise. Cite file:line where relevant.
- Do NOT create markdown (.md) files to summarize what is happening. Respond directly.
- Add Diffs for different files with which files has been changed and which line has been added.
- Remove redundant word usage like 'Now I will', 'Perfect! Now', etc."#;

pub fn build_system(cwd: &str, repo_map: &str) -> String {
    let mut system = SYSTEM_PROMPT.to_string();

    let today = chrono::Local::now().date_naive().to_string();
    system.push_str(&format!("\n\nCurrent date: {today}"));
    system.push_str(&format!("\n\nCurrent working directory: {cwd}"));

    if !repo_map.is_empty() {
        system.push_str(
            "\n\n## Repository map (Python symbols by file; not source of truth — read the file before editing)\n",
        );
        system.push_str(repo_map);
    }

    let rules = load_rules(cwd);
    if !rules.is_empty() {
        system.push_str("\n\n## Behavioral rules\n");
        system.push_str(&rules);
    }

    let memory = load_memory(cwd);
    if !memory.is_empty() {
        system.push_str("\n\n## Memory\n");
        system.push_str(&memory);
    }

    system
}

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
      },
      {
        "name": "memory_write",
        "description": "Persist a structured note about what changed this turn so future sessions can pick up where this one left off. Call this when you completed a task, modified files, made an architectural decision, or surfaced an unresolved issue. Skip only for purely read-only turns with no carry-forward insight.",
        "input_schema": {
          "type": "object",
          "properties": {
            "component": {
              "type": "string",
              "description": "Component name e.g. 'router', 'tools', 'agent', 'tui'"
            },
            "summary": {
              "type": "string",
              "description": "One-line summary of the current state of this component"
            },
            "detail": {
              "type": "string",
              "description": "Full explanation of what was done and why"
            },
            "related_files": {
              "type": "array",
              "items": { "type": "string" },
              "description": "File paths touched this turn"
            },
            "tags": {
              "type": "array",
              "items": { "type": "string" }
            },
            "change_entry": {
              "type": "object",
              "description": "Record of the specific change made this turn",
              "properties": {
                "file": { "type": "string" },
                "what": { "type": "string" },
                "why":  { "type": "string" }
              }
            },
            "open_issues": {
              "type": "array",
              "items": { "type": "string" },
              "description": "Unresolved issues to carry forward"
            }
          },
          "required": ["component", "summary"]
        }
      }
    ]"#).unwrap()
}

pub async fn agent_turn(
    user_msg: &str,
    messages: &mut Vec<serde_json::Value>,
    cwd: &str,
    max_steps: usize,
    session_id: &str,
    on_event: Option<tokio::sync::mpsc::Sender<crate::providers::StreamEvent>>,
) -> anyhow::Result<()> {
    let mut max_steps = max_steps;
    if let Ok(val) = std::env::var("MIMICODE_MAX_STEPS") {
        if let Ok(n) = val.parse::<usize>() {
            max_steps = n.max(1);
        }
    }

    crate::logger::log("user_message", serde_json::json!({
        "chars": user_msg.len(),
        "resumed": !messages.is_empty()
    }));

    messages.push(serde_json::json!({"role": "user", "content": user_msg}));

    let turn_choice = route_turn(user_msg);
    crate::logger::log("model_route", serde_json::json!({
        "model": turn_choice.model,
        "reason": turn_choice.reason,
        "has_guidance": !turn_choice.guidance.is_empty()
    }));
    let repo_map = ""; // placeholder until repomap.rs is built
    let system = build_system(cwd, repo_map);
    let step_system = augment_system_prompt(&system, &turn_choice.guidance);

    for step in 0..max_steps {
        let assistant = call_claude_streaming(
            messages,
            &step_system,
            &tool_definitions(),
            &turn_choice.model,
            true,
            on_event.clone(),
            None,
        ).await?;

        let tool_uses: Vec<(String, String, Value)> = assistant["content"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|b| {
                if b["type"] == "tool_use" {
                    Some((
                        b["id"].as_str().unwrap_or("").to_string(),
                        b["name"].as_str().unwrap_or("").to_string(),
                        b["input"].clone(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        messages.push(assistant);

        if tool_uses.is_empty() {
            crate::logger::log("turn_end", serde_json::json!({
                "steps": step + 1,
                "reason": "no_tool_use"
            }));
            return Ok(());
        }

        let mut result_blocks = Vec::new();
        for (id, name, input) in tool_uses {
            crate::logger::log("tool_call", serde_json::json!({
                "name": name,
                "args_keys": input.as_object()
                    .map(|o| o.keys().collect::<Vec<_>>())
                    .unwrap_or_default()
            }));
            let result = dispatch(&name, &input, cwd, session_id).await;
            crate::logger::log("tool_result", serde_json::json!({
                "name": name,
                "is_error": result.is_error,
                "bytes": result.output.len()
            }));
            result_blocks.push(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": result.output,
                "is_error": result.is_error,
            }));
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": result_blocks
        }));
    }

    crate::logger::log("turn_end", serde_json::json!({
        "steps": max_steps,
        "reason": "max_steps"
    }));
    anyhow::bail!("reached {max_steps}-step limit without a final response")
}

pub async fn agent_turn_streaming(
    user_msg: &str,
    history: Arc<Mutex<Vec<Message>>>,
    system: &str,
    cwd: &str,
    tx: mpsc::Sender<crate::tui::app::StreamEvent>,
) {
    use crate::tui::app::{StatusInfo, StreamEvent};

    let turn_choice = route_turn(user_msg);
    eprintln!("[model_route] model={} reason={}", turn_choice.model, turn_choice.reason);
    let step_system = augment_system_prompt(system, &turn_choice.guidance);

    let mut hist = history.lock().await;
    hist.push(Message { role: "user".into(), content: MessageContent::Text(user_msg.into()) });

    let mut tokens_in: u64 = 0;
    let mut tokens_out: u64 = 0;

    for _ in 0..MAX_STEPS {
        let hist_json: Vec<Value> = hist.iter().map(|m| serde_json::to_value(m).unwrap()).collect();
        let reply_val = match call_claude(&hist_json, &step_system, &tool_definitions(), &turn_choice.model, true).await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                let _ = tx.send(StreamEvent::Done(StatusInfo {
                    session_id: String::new(),
                    model: turn_choice.model.clone(),
                    turn: 0,
                    tokens_in,
                    tokens_out,
                })).await;
                return;
            }
        };

        let usage = crate::providers::get_last_usage();
        tokens_in += usage.tokens_in;
        tokens_out += usage.tokens_out;

        let reply: Message = serde_json::from_value(reply_val).unwrap_or_else(|_| Message {
            role: "assistant".into(),
            content: MessageContent::Blocks(vec![]),
        });

        let (texts, tool_uses) = split_content(&reply.content);

        for text in texts {
            if !text.is_empty() {
                let _ = tx.send(StreamEvent::Token(text)).await;
            }
        }

        hist.push(reply);

        if tool_uses.is_empty() {
            let _ = tx.send(StreamEvent::Done(StatusInfo {
                session_id: String::new(),
                model: turn_choice.model.clone(),
                turn: 0,
                tokens_in,
                tokens_out,
            })).await;
            return;
        }

        let mut result_blocks = Vec::new();
        for (id, name, input) in tool_uses {
            let _ = tx.send(StreamEvent::ToolCallStart(name.clone())).await;
            let result = dispatch(&name, &input, cwd, "").await;
            let _ = tx.send(StreamEvent::ToolCallResult(name.clone(), result.output.clone())).await;
            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content: result.output,
                is_error: result.is_error,
            });
        }

        hist.push(Message {
            role: "user".into(),
            content: MessageContent::Blocks(result_blocks),
        });
    }

    let _ = tx.send(StreamEvent::Error(format!("reached {MAX_STEPS}-step limit"))).await;
    let _ = tx.send(StreamEvent::Done(StatusInfo {
        session_id: String::new(),
        model: turn_choice.model.clone(),
        turn: 0,
        tokens_in,
        tokens_out,
    })).await;
}

fn split_content(content: &MessageContent) -> (Vec<String>, Vec<(String, String, Value)>) {
    match content {
        MessageContent::Blocks(blocks) => {
            let mut texts = Vec::new();
            let mut tools = Vec::new();
            for b in blocks {
                match b {
                    ContentBlock::Text { text } => texts.push(text.clone()),
                    ContentBlock::ToolUse { id, name, input } => {
                        tools.push((id.clone(), name.clone(), input.clone()));
                    }
                    _ => {}
                }
            }
            (texts, tools)
        }
        MessageContent::Text(t) => (vec![t.clone()], vec![]),
    }
}

async fn dispatch(name: &str, input: &Value, cwd: &str, session_id: &str) -> ToolResult {
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

        "memory_write" => {
            let cwd_str = cwd;
            let output = crate::mimi_memory::handle_memory_write(session_id, input, cwd_str);
            ToolResult::ok(output)
        }

        other => ToolResult::err(format!("[error] unknown tool: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_always_has_date_and_cwd() {
        let system = build_system("/home/jake/project", "");
        assert!(system.contains("Current date:"));
        assert!(system.contains("Current working directory: /home/jake/project"));
        assert!(system.contains("You are a coding agent"));
    }

    #[test]
    fn test_build_system_no_repo_map_when_empty() {
        let system = build_system("/tmp", "");
        assert!(!system.contains("Repository map"));
    }

    #[test]
    fn test_build_system_appends_repo_map_when_given() {
        let system = build_system("/tmp", "src/main.rs: main, run");
        assert!(system.contains("Repository map"));
        assert!(system.contains("src/main.rs: main, run"));
    }

    #[test]
    fn test_build_system_no_rules_section_when_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let system = build_system(tmp.path().to_str().unwrap(), "");
        assert!(!system.contains("Behavioral rules"));
    }

    #[test]
    fn test_build_system_appends_rules_when_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let mimi_dir = tmp.path().join(".mimi");
        std::fs::create_dir_all(&mimi_dir).unwrap();
        std::fs::write(mimi_dir.join("RULES.md"), "always use rg").unwrap();
        let system = build_system(tmp.path().to_str().unwrap(), "");
        assert!(system.contains("Behavioral rules"));
        assert!(system.contains("always use rg"));
    }

    #[test]
    fn test_build_system_appends_memory_when_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let mimi_dir = tmp.path().join(".mimi");
        std::fs::create_dir_all(&mimi_dir).unwrap();
        std::fs::write(mimi_dir.join("MEMORY.md"), "router uses keyword matching").unwrap();
        let system = build_system(tmp.path().to_str().unwrap(), "");
        assert!(system.contains("## Memory"));
        assert!(system.contains("router uses keyword matching"));
    }

    #[test]
    fn test_load_rules_returns_empty_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_rules(tmp.path().to_str().unwrap()), "");
    }

    #[test]
    fn test_load_memory_returns_empty_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_memory(tmp.path().to_str().unwrap()), "");
    }

    #[test]
    fn test_assembly_order() {
        let tmp = tempfile::tempdir().unwrap();
        let mimi_dir = tmp.path().join(".mimi");
        std::fs::create_dir_all(&mimi_dir).unwrap();
        std::fs::write(mimi_dir.join("RULES.md"), "rule content").unwrap();
        std::fs::write(mimi_dir.join("MEMORY.md"), "memory content").unwrap();
        let system = build_system(tmp.path().to_str().unwrap(), "");
        let rules_pos = system.find("Behavioral rules").unwrap();
        let memory_pos = system.find("## Memory").unwrap();
        assert!(rules_pos < memory_pos);
    }
}
