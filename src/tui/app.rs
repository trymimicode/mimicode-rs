use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::types::Message;

use super::splash::IntroState;

#[derive(Debug)]
pub enum StreamEvent {
    Token(String),
    ToolCallStart(String, String), // (name, args_summary)
    ToolCallResult(String, String), // (name, output)
    Done(StatusInfo),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    User,
    Assistant,
    ToolCall,
    ToolResult,
    Error,
    System,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub message_type: MessageType,
}

#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub session_id: String,
    pub model: String,
    pub turn: usize,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub scroll_offset: usize,
    pub total_lines: usize,
    pub is_waiting: bool,
    pub should_quit: bool,
    pub status: StatusInfo,
    pub stream_rx: Option<mpsc::Receiver<StreamEvent>>,
    pub cwd: String,
    // Shared API message history — also used by agent tasks.
    pub api_history: Arc<Mutex<Vec<Message>>>,
    // Input history navigation.
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub history_draft: String,
    // Selected autocomplete entry index.
    pub autocomplete_index: Option<usize>,
    // Handle for the running agent task (for cancellation).
    pub agent_handle: Option<JoinHandle<()>>,
    // Live tool activity display (above input box).
    pub tool_status: Option<String>,  // "⚙ bash  ls -la ."
    pub tool_result: Option<String>,  // first line of last result
    /// Startup intro animation (skipped on any key once the TUI is ready).
    pub intro: IntroState,
}

impl App {
    pub fn new(session_id: String, cwd: String, api_history: Arc<Mutex<Vec<Message>>>) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            scroll_offset: 0,
            total_lines: 0,
            is_waiting: false,
            should_quit: false,
            status: StatusInfo {
                session_id,
                model: "haiku".to_string(),
                turn: 0,
                tokens_in: 0,
                tokens_out: 0,
            },
            stream_rx: None,
            cwd,
            api_history,
            input_history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            autocomplete_index: None,
            agent_handle: None,
            tool_status: None,
            tool_result: None,
            intro: IntroState::new_playing(),
        }
    }

    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.scroll_offset = usize::MAX;
    }

    pub fn scroll_up(&mut self) {
        // Decrease skip-offset → reveal older lines above the current view.
        self.scroll_offset = self.scroll_offset.saturating_sub(3);
    }

    pub fn scroll_down(&mut self) {
        // Increase skip-offset → reveal newer lines below the current view.
        self.scroll_offset = self.scroll_offset.saturating_add(3).min(self.total_lines);
    }

    pub fn clear_input(&mut self) -> String {
        self.autocomplete_index = None;
        std::mem::take(&mut self.input)
    }

    pub fn start_stream(&mut self, rx: mpsc::Receiver<StreamEvent>) {
        self.stream_rx = Some(rx);
    }

    /// Push text to history and reset navigation state.
    pub fn history_push(&mut self, text: &str) {
        if !text.trim().is_empty()
            && self.input_history.last().map(|s| s.as_str()) != Some(text)
        {
            self.input_history.push(text.to_string());
        }
        self.history_index = None;
        self.history_draft.clear();
    }

    /// Navigate to an older history entry.
    pub fn history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_draft = self.input.clone();
                self.history_index = Some(self.input_history.len() - 1);
            }
            Some(0) => return,
            Some(i) => self.history_index = Some(i - 1),
        }
        if let Some(i) = self.history_index {
            self.input = self.input_history[i].clone();
        }
        self.autocomplete_index = None;
    }

    /// Navigate to a newer history entry, or back to the draft.
    pub fn history_next(&mut self) {
        match self.history_index {
            None => return,
            Some(i) if i + 1 >= self.input_history.len() => {
                self.history_index = None;
                self.input = std::mem::take(&mut self.history_draft);
            }
            Some(i) => {
                self.history_index = Some(i + 1);
                self.input = self.input_history[i + 1].clone();
            }
        }
        self.autocomplete_index = None;
    }

    /// Returns slash-command completions for the current input.
    pub fn current_completions(&self) -> Vec<(&'static str, &'static str)> {
        if self.input.starts_with('/') {
            super::commands::completions(&self.input)
        } else {
            vec![]
        }
    }

    /// Abort the running agent task and update UI state.
    pub fn cancel_agent(&mut self) {
        if let Some(handle) = self.agent_handle.take() {
            handle.abort();
        }
        self.is_waiting = false;
        self.stream_rx = None;
        self.push_message(ChatMessage {
            role: "system".into(),
            content: "Interrupted.".into(),
            message_type: MessageType::System,
        });
    }
}
