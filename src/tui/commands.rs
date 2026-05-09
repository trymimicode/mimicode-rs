use std::cmp::Reverse;
use std::io::Write;
use std::process::{Command, Stdio};

use super::app::{App, ChatMessage, MessageType};

pub const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/clear",    "erase chat history"),
    ("/copy",     "copy last response to clipboard  (Ctrl+Y)"),
    ("/cwd",      "show or change working directory"),
    ("/exit",     "quit"),
    ("/help",     "list all commands"),
    ("/new",      "start a fresh session"),
    ("/route",    "show current model"),
    ("/sessions", "list recent sessions"),
    ("/usage",    "show token usage"),
];

pub fn completions(prefix: &str) -> Vec<(&'static str, &'static str)> {
    SLASH_COMMANDS
        .iter()
        .filter(|(cmd, _)| cmd.starts_with(prefix))
        .copied()
        .collect()
}

fn sys(app: &mut App, text: impl Into<String>) {
    app.push_message(ChatMessage {
        role: "system".into(),
        content: text.into(),
        message_type: MessageType::System,
    });
}

fn push_err(app: &mut App, text: impl Into<String>) {
    app.push_message(ChatMessage {
        role: "error".into(),
        content: text.into(),
        message_type: MessageType::Error,
    });
}

/// Returns `true` if the app should quit.
pub fn execute(cmd_line: &str, app: &mut App) -> bool {
    let trimmed = cmd_line.trim();
    let (cmd, arg) = match trimmed.find(char::is_whitespace) {
        Some(i) => (&trimmed[..i], trimmed[i..].trim()),
        None => (trimmed, ""),
    };

    match cmd {
        "/help" => {
            let lines: Vec<String> = SLASH_COMMANDS
                .iter()
                .map(|(c, d)| format!("{:<12} {}", c, d))
                .collect();
            sys(app, lines.join("\n"));
        }

        "/clear" => {
            app.messages.clear();
            app.total_lines = 0;
        }

        "/exit" => return true,

        "/new" => {
            app.messages.clear();
            app.total_lines = 0;
            app.status.turn = 0;
            app.status.tokens_in = 0;
            app.status.tokens_out = 0;
            app.api_history.blocking_lock().clear();
            let new_id = super::generate_session_id();
            app.status.session_id = new_id.clone();
            sys(app, format!("New session: {}", new_id));
        }

        "/sessions" => {
            let dir = crate::logger::log_dir();
            match std::fs::read_dir(&dir) {
                Err(_) => push_err(app, format!("Cannot read: {}", dir.display())),
                Ok(entries) => {
                    let mut files: Vec<_> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path().extension().map(|x| x == "jsonl").unwrap_or(false)
                        })
                        .collect();
                    files.sort_by_key(|e| {
                        Reverse(e.metadata().and_then(|m| m.modified()).ok())
                    });
                    if files.is_empty() {
                        sys(app, "No sessions found.");
                    } else {
                        let lines: Vec<String> = files
                            .iter()
                            .take(20)
                            .map(|e| {
                                e.path()
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned()
                            })
                            .collect();
                        sys(app, format!("Recent sessions:\n{}", lines.join("\n")));
                    }
                }
            }
        }

        "/cwd" => {
            if arg.is_empty() {
                sys(app, format!("cwd: {}", app.cwd));
            } else {
                let base = std::path::Path::new(&app.cwd);
                let target = base.join(arg);
                match target.canonicalize() {
                    Ok(p) if p.is_dir() => {
                        if let Err(e) = std::env::set_current_dir(&p) {
                            push_err(app, format!("Cannot set cwd: {e}"));
                        } else {
                            app.cwd = p.to_string_lossy().into_owned();
                            sys(app, format!("cwd: {}", app.cwd));
                        }
                    }
                    Ok(_) => push_err(app, format!("Not a directory: {arg}")),
                    Err(e) => push_err(app, format!("Cannot resolve path: {e}")),
                }
            }
        }

        "/usage" => {
            let s = &app.status;
            sys(
                app,
                format!(
                    "session {}  turn {}  in {} tok  out {} tok",
                    s.session_id, s.turn, s.tokens_in, s.tokens_out
                ),
            );
        }

        "/route" => {
            sys(app, format!("model: {}", app.status.model));
        }

        "/copy" => {
            let text = app
                .messages
                .iter()
                .rev()
                .find(|m| m.message_type == MessageType::Assistant)
                .map(|m| m.content.clone());
            match text {
                None => push_err(app, "No assistant message to copy."),
                Some(t) => {
                    if clipboard_write(&t) {
                        sys(app, "Copied to clipboard.");
                    } else {
                        push_err(app, "Clipboard unavailable.");
                    }
                }
            }
        }

        _ => push_err(app, format!("Unknown command: {cmd}  (try /help)")),
    }

    false
}

pub fn clipboard_write(text: &str) -> bool {
    #[cfg(target_os = "windows")]
    let (prog, args): (&str, &[&str]) = ("clip", &[]);
    #[cfg(target_os = "macos")]
    let (prog, args): (&str, &[&str]) = ("pbcopy", &[]);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let (prog, args): (&str, &[&str]) = ("xclip", &["-selection", "clipboard"]);

    let Ok(mut child) = Command::new(prog).args(args).stdin(Stdio::piped()).spawn() else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}
