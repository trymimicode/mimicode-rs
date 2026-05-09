use tokio::sync::mpsc;

#[derive(Debug)]
pub enum StreamEvent {
    Token(String),
    ToolCallStart(String),
    ToolCallResult(String, String),
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
}

impl App {
    pub fn new(session_id: String) -> Self {
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
        }
    }

    pub fn push_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.scroll_offset = usize::MAX; // draw clamps this to max valid offset
    }

    pub fn scroll_up(&mut self) {
        // draw will clamp to max_offset; total_lines is a safe upper bound
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(1)
            .min(self.total_lines);
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn clear_input(&mut self) -> String {
        std::mem::take(&mut self.input)
    }

    pub fn start_stream(&mut self, rx: mpsc::Receiver<StreamEvent>) {
        self.stream_rx = Some(rx);
    }
}
