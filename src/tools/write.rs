use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::{DiffInfo, ToolResult, FILE_LOCKS};

pub(crate) async fn write(path: &str, content: &str, cwd: &str) -> ToolResult {
    let resolved = resolve(path, cwd);

    // acquire per-path lock — clone Arc before awaiting to drop the DashMap ref
    let lock: Arc<Mutex<()>> = FILE_LOCKS
        .entry(resolved.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _guard = lock.lock().await;

    let is_new_file = !resolved.exists();

    let old_content = if is_new_file {
        String::new()
    } else {
        tokio::fs::read(&resolved)
            .await
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default()
    };

    if let Some(parent) = resolved.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return ToolResult::err(format!("[error] {e}"));
        }
    }

    if let Err(e) = tokio::fs::write(&resolved, content).await {
        return ToolResult::err(format!("[error] {e}"));
    }

    ToolResult {
        output: format!("wrote {} bytes to {}", content.len(), path),
        is_error: false,
        truncated: false,
        timed_out: false,
        diff_info: Some(DiffInfo {
            path: path.to_string(),
            old_content,
            new_content: content.to_string(),
            operation: "write".to_string(),
            is_new_file,
        }),
    }
}

fn resolve(path: &str, cwd: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    let joined = if p.is_absolute() { p.to_path_buf() } else { Path::new(cwd).join(p) };
    joined.canonicalize().unwrap_or(joined)
}
