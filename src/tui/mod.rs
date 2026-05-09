pub mod app;
pub mod commands;
pub mod events;
pub mod ui;

use std::io::stdout;
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::{Mutex, mpsc};

use crate::types::{ContentBlock, Message, MessageContent};
use app::{App, ChatMessage, MessageType, StreamEvent};
use events::{AppAction, handle_event};

pub(super) fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{:x}", ms & 0x00ff_ffff)
}

fn cleanup() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen);
}

pub fn main(session_id: Option<String>) {
    let id = session_id.unwrap_or_else(generate_session_id);

    let session_path = crate::logger::log_dir().join(format!("{id}.jsonl"));
    let prior_values = crate::session::load_messages(&session_path);
    let prior: Vec<Message> = prior_values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let n_prior = prior.len();

    let api_history: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(prior.clone()));

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());

    let mut app = App::new(id.clone(), cwd, Arc::clone(&api_history));

    // Replay prior messages into the chat view.
    for msg in messages_to_chat(&prior) {
        app.messages.push(msg);
    }
    if n_prior > 0 {
        app.push_message(ChatMessage {
            role: "system".into(),
            content: format!("Resumed session {} ({} prior messages)", id, n_prior),
            message_type: MessageType::System,
        });
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        cleanup();
        original_hook(info);
    }));

    enable_raw_mode().expect("failed to enable raw mode");
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)
        .expect("failed to enter alternate screen");
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");

    loop {
        terminal.draw(|f| ui::draw(f, &mut app)).expect("draw failed");
        drain_stream(&mut app);

        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(ev) = event::read() {
                let action = handle_event(&mut app, ev);
                match action {
                    AppAction::Quit => break,

                    AppAction::Cancel => {
                        app.cancel_agent();
                    }

                    AppAction::Submit(text) => {
                        app.tool_status = None;
                        app.tool_result = None;
                        app.push_message(ChatMessage {
                            role: "user".into(),
                            content: text.clone(),
                            message_type: MessageType::User,
                        });
                        app.push_message(ChatMessage {
                            role: "assistant".into(),
                            content: String::new(),
                            message_type: MessageType::Assistant,
                        });
                        app.is_waiting = true;

                        let (tx, rx) = mpsc::channel(256);
                        app.start_stream(rx);

                        let hist = Arc::clone(&app.api_history);
                        let cwd_clone = app.cwd.clone();
                        let handle = tokio::spawn(async move {
                            crate::agent::agent_turn_streaming(
                                &text,
                                hist,
                                crate::SYSTEM,
                                &cwd_clone,
                                tx,
                            )
                            .await;
                        });
                        app.agent_handle = Some(handle);
                    }

                    AppAction::Command(text) => {
                        if commands::execute(&text, &mut app) {
                            break;
                        }
                    }

                    AppAction::CopyLast => {
                        let text = app
                            .messages
                            .iter()
                            .rev()
                            .find(|m| m.message_type == MessageType::Assistant)
                            .map(|m| m.content.clone());
                        match text {
                            None => app.push_message(ChatMessage {
                                role: "error".into(),
                                content: "No assistant message to copy.".into(),
                                message_type: MessageType::Error,
                            }),
                            Some(t) => {
                                let ok = commands::clipboard_write(&t);
                                app.push_message(ChatMessage {
                                    role: "system".into(),
                                    content: if ok {
                                        "Copied to clipboard.".into()
                                    } else {
                                        "Clipboard unavailable.".into()
                                    },
                                    message_type: if ok {
                                        MessageType::System
                                    } else {
                                        MessageType::Error
                                    },
                                });
                            }
                        }
                    }

                    AppAction::ScrollUp => app.scroll_up(),
                    AppAction::ScrollDown => app.scroll_down(),
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    cleanup();
}

fn messages_to_chat(messages: &[Message]) -> Vec<ChatMessage> {
    let mut out = Vec::new();
    for msg in messages {
        match (&msg.role as &str, &msg.content) {
            ("user", MessageContent::Text(text)) if !text.trim().is_empty() => {
                out.push(ChatMessage {
                    role: "user".into(),
                    content: text.clone(),
                    message_type: MessageType::User,
                });
            }
            ("assistant", MessageContent::Blocks(blocks)) => {
                let text: String = blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if !text.trim().is_empty() {
                    out.push(ChatMessage {
                        role: "assistant".into(),
                        content: text,
                        message_type: MessageType::Assistant,
                    });
                }
            }
            ("assistant", MessageContent::Text(text)) if !text.trim().is_empty() => {
                out.push(ChatMessage {
                    role: "assistant".into(),
                    content: text.clone(),
                    message_type: MessageType::Assistant,
                });
            }
            ("user", MessageContent::Blocks(blocks)) => {
                for block in blocks {
                    if let ContentBlock::ToolResult { content, .. } = block {
                        if !content.trim().is_empty() {
                            out.push(ChatMessage {
                                role: "tool".into(),
                                content: content.clone(),
                                message_type: MessageType::ToolResult,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn drain_stream(app: &mut App) {
    loop {
        let event = match app.stream_rx.as_mut().and_then(|rx| rx.try_recv().ok()) {
            Some(e) => e,
            None => break,
        };
        match event {
            StreamEvent::Token(chunk) => {
                if let Some(msg) = app
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| m.message_type == MessageType::Assistant)
                {
                    msg.content.push_str(&chunk);
                }
            }
            StreamEvent::ToolCallStart(name, args) => {
                let label = if args.is_empty() {
                    name.clone()
                } else {
                    format!("{name}  {args}")
                };
                app.tool_status = Some(label.clone());
                app.tool_result = None;
                app.push_message(ChatMessage {
                    role: "tool".into(),
                    content: format!("▶ {label}"),
                    message_type: MessageType::ToolCall,
                });
            }
            StreamEvent::ToolCallResult(name, output) => {
                let first = output.lines().next().unwrap_or("").trim();
                let preview = if first.len() > 70 {
                    format!("{}…", &first[..67])
                } else {
                    first.to_string()
                };
                app.tool_result = Some(preview.clone());
                app.push_message(ChatMessage {
                    role: "tool".into(),
                    content: format!("{name}: {preview}"),
                    message_type: MessageType::ToolResult,
                });
            }
            StreamEvent::Done(status) => {
                app.status.tokens_in += status.tokens_in;
                app.status.tokens_out += status.tokens_out;
                app.status.model = status.model;
                app.status.turn += 1;
                app.is_waiting = false;
                app.stream_rx = None;
                app.agent_handle = None;
                break;
            }
            StreamEvent::Error(e) => {
                app.push_message(ChatMessage {
                    role: "error".into(),
                    content: e,
                    message_type: MessageType::Error,
                });
                app.is_waiting = false;
                app.stream_rx = None;
                app.agent_handle = None;
                break;
            }
        }
    }
}
