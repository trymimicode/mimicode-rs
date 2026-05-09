pub mod bash;
pub mod read;
pub mod write;

use std::path::PathBuf;
use std::sync::Arc;

use std::sync::LazyLock;

use dashmap::DashMap;
use tokio::sync::Mutex;

pub static FILE_LOCKS: LazyLock<DashMap<PathBuf, Arc<Mutex<()>>>> =
    LazyLock::new(DashMap::new);

#[derive(Debug, Clone)]
pub struct DiffInfo {
    pub path: String,
    pub old_content: String,
    pub new_content: String,
    /// "write" or "edit"
    pub operation: String,
    pub is_new_file: bool,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
    pub truncated: bool,
    pub timed_out: bool,
    pub diff_info: Option<DiffInfo>,
}

impl ToolResult {
    pub fn ok(output: String) -> Self {
        Self {
            output,
            is_error: false,
            truncated: false,
            timed_out: false,
            diff_info: None,
        }
    }

    pub fn err(output: String) -> Self {
        Self {
            output,
            is_error: true,
            truncated: false,
            timed_out: false,
            diff_info: None,
        }
    }
}
