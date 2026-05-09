use std::path::Path;

use chrono::Local;

const MIMI_DIR: &str = ".mimi";
const MEMORY_FILE: &str = "MEMORY.md";
const RULES_FILE: &str = "RULES.md";
const MAX_MEMORY_LINES: usize = 200;

fn mimi_read(path: &Path) -> String {
    if path.exists() {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    }
}

fn mimi_write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).ok();
    std::fs::write(path, text).ok();
}

fn cap(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MAX_MEMORY_LINES {
        return text.to_string();
    }
    lines[lines.len() - MAX_MEMORY_LINES..].join("\n")
}

fn upsert_component(content: &str, name: &str, block: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let header = format!("## {name}");

    let start = lines.iter().enumerate().find(|(_, line)| {
        match line.trim().strip_prefix(header.as_str()) {
            None => false,
            Some(rest) => rest.is_empty() || rest.starts_with(' ') || rest.starts_with('['),
        }
    }).map(|(i, _)| i);

    match start {
        None => {
            if content.trim().is_empty() {
                format!("{block}\n")
            } else {
                format!("{content}\n{block}\n")
            }
        }
        Some(start_idx) => {
            let end = lines[start_idx + 1..]
                .iter()
                .position(|l| l.starts_with("## "))
                .map(|i| start_idx + 1 + i)
                .unwrap_or(lines.len());

            let mut result_lines: Vec<&str> = Vec::new();
            result_lines.extend_from_slice(&lines[..start_idx]);
            result_lines.extend(block.lines());
            result_lines.push("");
            result_lines.extend_from_slice(&lines[end..]);
            result_lines.join("\n")
        }
    }
}

pub fn load_memory(cwd: &str) -> String {
    let path = Path::new(cwd).join(MIMI_DIR).join(MEMORY_FILE);
    mimi_read(&path)
}

pub fn load_rules(cwd: &str) -> String {
    let path = Path::new(cwd).join(MIMI_DIR).join(RULES_FILE);
    mimi_read(&path)
}

pub fn handle_memory_write(
    _session_id: &str,
    args: &serde_json::Value,
    cwd: &str,
) -> String {
    let component = args["component"].as_str().unwrap_or("").trim();
    let summary   = args["summary"].as_str().unwrap_or("").trim();
    if component.is_empty() || summary.is_empty() {
        return "[memory_write] error: component and summary are required".to_string();
    }

    let detail = args["detail"].as_str().unwrap_or("").trim();
    let files: Vec<&str> = args["related_files"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let tags: Vec<&str> = args["tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let change = args.get("change_entry");

    let today = Local::now().date_naive().to_string();
    let mut block = format!("## {component} [{today}]\n**summary:** {summary}");

    if !files.is_empty() {
        block.push_str(&format!("\n**files:** {}", files.join(", ")));
    }
    if !tags.is_empty() {
        block.push_str(&format!("\n**tags:** {}", tags.join(", ")));
    }
    if let Some(c) = change.filter(|v| v.is_object()) {
        let file = c["file"].as_str().unwrap_or("");
        let what = c["what"].as_str().unwrap_or("");
        let why  = c["why"].as_str().unwrap_or("");
        block.push_str(&format!("\n**change:** {file} \u{2014} {what} \u{2014} {why}"));
    }
    if !detail.is_empty() {
        block.push_str(&format!("\n{detail}"));
    }

    let memory_path = Path::new(cwd).join(MIMI_DIR).join(MEMORY_FILE);
    let current = mimi_read(&memory_path);
    let updated = upsert_component(&current, component, &block);
    mimi_write(&memory_path, &cap(&updated));

    format!("[memory_write] saved component '{component}'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_memory_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_memory(tmp.path().to_str().unwrap()), "");
    }

    #[test]
    fn test_load_rules_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_rules(tmp.path().to_str().unwrap()), "");
    }

    #[test]
    fn test_load_memory_returns_content() {
        let tmp = tempfile::tempdir().unwrap();
        let mimi = tmp.path().join(".mimi");
        std::fs::create_dir_all(&mimi).unwrap();
        std::fs::write(mimi.join("MEMORY.md"), "some memory content").unwrap();
        assert_eq!(load_memory(tmp.path().to_str().unwrap()), "some memory content");
    }

    #[test]
    fn test_handle_memory_write_missing_required_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let result = handle_memory_write(
            "sess1",
            &serde_json::json!({"component": "router"}),
            tmp.path().to_str().unwrap(),
        );
        assert!(result.contains("error"));
    }

    #[test]
    fn test_handle_memory_write_creates_memory_file() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_str().unwrap();
        let args = serde_json::json!({
            "component": "router",
            "summary": "routes by keyword matching"
        });
        let result = handle_memory_write("sess1", &args, cwd);
        assert!(result.contains("router"));
        let memory = load_memory(cwd);
        assert!(memory.contains("## router"));
        assert!(memory.contains("routes by keyword matching"));
    }

    #[test]
    fn test_handle_memory_write_full_block() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_str().unwrap();
        let args = serde_json::json!({
            "component": "tools",
            "summary": "four tools implemented",
            "detail": "bash read write edit all working",
            "related_files": ["src/tools/bash.rs", "src/tools/edit.rs"],
            "tags": ["tools", "core"],
            "change_entry": {
                "file": "src/tools/edit.rs",
                "what": "added atomicity",
                "why": "prevent partial writes"
            }
        });
        handle_memory_write("sess1", &args, cwd);
        let memory = load_memory(cwd);
        assert!(memory.contains("**files:** src/tools/bash.rs, src/tools/edit.rs"));
        assert!(memory.contains("**tags:** tools, core"));
        assert!(memory.contains("**change:** src/tools/edit.rs \u{2014} added atomicity \u{2014} prevent partial writes"));
        assert!(memory.contains("bash read write edit all working"));
    }

    #[test]
    fn test_upsert_replaces_existing_component() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_str().unwrap();
        let args1 = serde_json::json!({"component": "router", "summary": "first summary"});
        let args2 = serde_json::json!({"component": "router", "summary": "updated summary"});
        handle_memory_write("s", &args1, cwd);
        handle_memory_write("s", &args2, cwd);
        let memory = load_memory(cwd);
        assert_eq!(memory.matches("## router").count(), 1);
        assert!(memory.contains("updated summary"));
        assert!(!memory.contains("first summary"));
    }

    #[test]
    fn test_upsert_preserves_other_components() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().to_str().unwrap();
        handle_memory_write("s", &serde_json::json!({"component": "router", "summary": "router summary"}), cwd);
        handle_memory_write("s", &serde_json::json!({"component": "tools",  "summary": "tools summary"}), cwd);
        handle_memory_write("s", &serde_json::json!({"component": "router", "summary": "router updated"}), cwd);
        let memory = load_memory(cwd);
        assert!(memory.contains("router updated"));
        assert!(memory.contains("tools summary"));
    }

    #[test]
    fn test_cap_trims_to_200_lines() {
        let big = (0..250).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let capped = cap(&big);
        assert_eq!(capped.lines().count(), 200);
        assert!(capped.contains("line 249"));
        assert!(!capped.contains("line 0"));
    }

    #[test]
    fn test_cap_unchanged_under_limit() {
        let small = "line 1\nline 2\nline 3";
        assert_eq!(cap(small), small);
    }
}
