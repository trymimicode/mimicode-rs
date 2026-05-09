use std::path::{Path, PathBuf};
use serde_json::Value;

pub fn messages_path(session_path: &Path) -> PathBuf {
    session_path.with_extension("messages.json")
}

pub fn load_messages(session_path: &Path) -> Vec<Value> {
    let mp = messages_path(session_path);
    if !mp.exists() {
        return vec![];
    }
    let text = match std::fs::read_to_string(&mp) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Array(arr)) => arr,
        _ => vec![],
    }
}

pub fn save_messages(session_path: &Path, messages: &[Value]) {
    let mp = messages_path(session_path);
    let contents = serde_json::to_string_pretty(messages).unwrap();
    if let Err(e) = std::fs::write(&mp, contents) {
        eprintln!("[session] failed to save: {e}");
    }
}

pub fn generate_session_id() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

pub fn last_assistant_text(messages: &[Value]) -> String {
    for msg in messages.iter().rev() {
        if msg["role"] == "assistant" {
            if let Some(blocks) = msg["content"].as_array() {
                let parts: Vec<&str> = blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect();
                return parts.join("\n");
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_messages_path() {
        let p = Path::new("sessions/myproject.jsonl");
        assert_eq!(
            messages_path(p),
            PathBuf::from("sessions/myproject.messages.json")
        );
    }

    #[test]
    fn test_load_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ghost.jsonl");
        assert!(load_messages(&path).is_empty());
    }

    #[test]
    fn test_load_corrupt_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let mp = tmp.path().join("bad.messages.json");
        std::fs::write(&mp, "not valid json {{{{").unwrap();
        let path = tmp.path().join("bad.jsonl");
        assert!(load_messages(&path).is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.jsonl");
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "hi there"}
            ]}),
        ];
        save_messages(&path, &messages);
        let loaded = load_messages(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["role"], "user");
        assert_eq!(loaded[1]["content"][0]["text"], "hi there");
    }

    #[test]
    fn test_last_assistant_text() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "hi"},
                {"type": "text", "text": "there"}
            ]}),
        ];
        assert_eq!(last_assistant_text(&messages), "hi\nthere");
    }

    #[test]
    fn test_last_assistant_text_empty_when_no_assistant() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "hello"}),
        ];
        assert_eq!(last_assistant_text(&messages), "");
    }
}
