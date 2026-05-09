use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use chrono::Local;

#[allow(dead_code)]
struct SessionState {
    session_id: String,
    log_path: PathBuf,
    file: File,
}

static ACTIVE_SESSION: LazyLock<Mutex<Option<SessionState>>> =
    LazyLock::new(|| Mutex::new(None));

pub struct Session {
    pub id: String,
    pub path: PathBuf,
}

pub fn start_session(requested_id: Option<&str>) -> Session {
    let id = match requested_id {
        Some(id) => id.to_string(),
        None => Local::now().format("%Y%m%d_%H%M%S").to_string(),
    };

    let sessions_dir = std::env::current_dir().unwrap().join("sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let log_path = sessions_dir.join(format!("{id}.jsonl"));

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("failed to open session log");

    let mut guard = ACTIVE_SESSION.lock().unwrap();
    *guard = Some(SessionState {
        session_id: id.clone(),
        log_path: log_path.clone(),
        file,
    });

    Session { id, path: log_path }
}

pub fn log(kind: &str, data: serde_json::Value) {
    let mut guard = ACTIVE_SESSION.lock().unwrap();
    let Some(state) = guard.as_mut() else { return };

    let timestamp = Local::now().to_rfc3339();
    let entry = serde_json::json!({
        "kind": kind,
        "ts": timestamp,
        "data": data
    });

    let Ok(line) = serde_json::to_string(&entry) else { return };
    let Ok(_) = state.file.write_all(line.as_bytes()) else { return };
    let Ok(_) = state.file.write_all(b"\n") else { return };
    let _ = state.file.flush();
}

pub fn log_dir() -> PathBuf {
    std::env::current_dir().unwrap().join("sessions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_session_with_explicit_id() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();
        let sess = start_session(Some("mytest"));
        assert_eq!(sess.id, "mytest");
        assert!(sess.path.to_string_lossy().contains("mytest"));
    }

    #[test]
    fn test_log_writes_line() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();
        let sess = start_session(Some("logtest"));
        log("test_event", serde_json::json!({"foo": "bar"}));
        let contents = std::fs::read_to_string(&sess.path).unwrap();
        assert!(contents.contains("test_event"));
        assert!(contents.contains("foo"));
    }

    #[test]
    fn test_log_before_start_does_nothing() {
        *ACTIVE_SESSION.lock().unwrap() = None;
        log("orphan_event", serde_json::json!({}));
    }

    #[test]
    fn test_generated_id_is_timestamp_shaped() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();
        let sess = start_session(None);
        assert_eq!(sess.id.len(), 15);
        assert_eq!(sess.id.chars().nth(8), Some('_'));
    }
}
