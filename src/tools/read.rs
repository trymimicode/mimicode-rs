use std::path::Path;

use tokio::io::AsyncReadExt;

use super::ToolResult;

pub(crate) async fn read(
    path: &str,
    cwd: &str,
    offset: usize,
    limit: Option<usize>,
) -> ToolResult {
    let resolved = resolve(path, cwd);

    if !resolved.exists() {
        return ToolResult::err(format!("[error] not found: {path}"));
    }
    if resolved.is_dir() {
        return ToolResult::err(format!("[error] is a directory: {path}"));
    }

    let mut file = match tokio::fs::File::open(&resolved).await {
        Ok(f) => f,
        Err(e) => return ToolResult::err(format!("[error] could not open {path}: {e}")),
    };

    // binary check
    let mut header = vec![0u8; 8192];
    let n = match file.read(&mut header).await {
        Ok(n) => n,
        Err(e) => return ToolResult::err(format!("[error] could not read {path}: {e}")),
    };
    if header[..n].contains(&0x00) {
        return ToolResult::err(format!("[error] binary file: {path}"));
    }

    let full = match tokio::fs::read(&resolved).await {
        Ok(b) => b,
        Err(e) => return ToolResult::err(format!("[error] could not read {path}: {e}")),
    };
    let text = String::from_utf8_lossy(&full);
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();

    if total == 0 {
        return ToolResult::ok("[empty file]".to_string());
    }

    let start = offset.saturating_sub(1);
    let cap = limit.unwrap_or(2000);
    let end = (start + cap).min(total);
    let selected = &lines[start..end];

    let mut output = selected
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:6}|{}", start + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    let truncated = end < total;
    if truncated {
        output.push_str(&format!(
            "\n[... showing lines {}-{} of {}; use offset/limit for more]",
            start + 1,
            end,
            total
        ));
    }

    ToolResult { output, is_error: false, truncated, timed_out: false, diff_info: None }
}

fn resolve(path: &str, cwd: &str) -> std::path::PathBuf {
    let p = Path::new(path);
    let joined = if p.is_absolute() { p.to_path_buf() } else { Path::new(cwd).join(p) };
    joined.canonicalize().unwrap_or(joined)
}
