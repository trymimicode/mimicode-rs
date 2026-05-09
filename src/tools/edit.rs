use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::{DiffInfo, ToolResult, FILE_LOCKS};

#[derive(serde::Deserialize)]
pub(crate) struct EditOp {
    pub old_text: String,
    pub new_text: String,
}

pub(crate) async fn edit(
    path: &str,
    cwd: &str,
    old_text: Option<&str>,
    new_text: Option<&str>,
    edits: Option<Vec<EditOp>>,
) -> ToolResult {
    // --- validate and normalize to ops ---
    let has_single = old_text.is_some() || new_text.is_some();
    let has_batch = edits.is_some();

    if has_single && has_batch {
        return ToolResult::err(
            "provide either (old_text, new_text) or edits[], not both".to_string(),
        );
    }

    let ops: Vec<EditOp> = if has_batch {
        let batch = edits.unwrap();
        if batch.is_empty() {
            return ToolResult::err("no edits given".to_string());
        }
        for (i, op) in batch.iter().enumerate() {
            if op.old_text.is_empty() {
                return ToolResult::err(format!("edits[{i}]: old_text is empty"));
            }
            if op.new_text.is_empty() {
                return ToolResult::err(format!("edits[{i}]: new_text is empty"));
            }
            if op.old_text == op.new_text {
                return ToolResult::err(format!("edits[{i}]: old_text and new_text are identical"));
            }
        }
        batch
    } else {
        match (old_text, new_text) {
            (Some(old), Some(new)) => vec![EditOp {
                old_text: old.to_string(),
                new_text: new.to_string(),
            }],
            _ => return ToolResult::err("no edits given".to_string()),
        }
    };

    // --- resolve path and acquire lock ---
    let resolved = resolve(path, cwd);

    let lock: Arc<Mutex<()>> = FILE_LOCKS
        .entry(resolved.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;

    // --- read file ---
    if !resolved.exists() {
        return ToolResult::err(format!("[error] not found: {path}"));
    }

    let raw = match tokio::fs::read(&resolved).await {
        Ok(b) => b,
        Err(e) => return ToolResult::err(format!("[error] {e}")),
    };
    let original_content = match String::from_utf8(raw) {
        Ok(s) => s,
        Err(_) => return ToolResult::err(format!("[error] binary file: {path}")),
    };

    // --- apply edits sequentially to buffer ---
    let mut buffer = original_content.clone();
    let mut line_numbers: Vec<usize> = Vec::with_capacity(ops.len());

    for (i, op) in ops.iter().enumerate() {
        let count = buffer.matches(op.old_text.as_str()).count();

        if count == 0 {
            let ctx = if i > 0 {
                format!(" ({i} prior edit{} applied to buffer)", if i == 1 { "" } else { "s" })
            } else {
                String::new()
            };
            return ToolResult::err(format!(
                "[error] edits[{i}]: old_text not found in {path}{ctx}"
            ));
        }

        if count > 1 {
            return ToolResult::err(format!(
                "[error] edits[{i}]: old_text matches {count} times; make it unique with more surrounding context"
            ));
        }

        let match_idx = buffer.find(op.old_text.as_str()).unwrap();
        let line_number = buffer[..match_idx].chars().filter(|&c| c == '\n').count() + 1;
        line_numbers.push(line_number);

        buffer = buffer.replacen(op.old_text.as_str(), op.new_text.as_str(), 1);
    }

    // --- write atomically (only reached if all edits succeeded) ---
    if let Err(e) = tokio::fs::write(&resolved, &buffer).await {
        return ToolResult::err(format!("[error] {e}"));
    }

    let output = if line_numbers.len() == 1 {
        format!("edited {path} at line {}", line_numbers[0])
    } else {
        let lines = line_numbers
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("edited {path}: {} changes at lines {lines}", line_numbers.len())
    };

    ToolResult {
        output,
        is_error: false,
        truncated: false,
        timed_out: false,
        diff_info: Some(DiffInfo {
            path: path.to_string(),
            old_content: original_content,
            new_content: buffer,
            operation: "edit".to_string(),
            is_new_file: false,
        }),
    }
}

fn resolve(path: &str, cwd: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    let joined = if p.is_absolute() { p.to_path_buf() } else { Path::new(cwd).join(p) };
    joined.canonicalize().unwrap_or(joined)
}
