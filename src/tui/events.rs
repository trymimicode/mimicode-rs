use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};

use super::app::App;

pub enum AppAction {
    None,
    Submit(String),
    Command(String),
    Quit,
    Cancel,
    ScrollUp,
    ScrollDown,
    ClearInput,
    CopyLast,
}

pub fn handle_event(app: &mut App, event: Event) -> AppAction {
    // Mouse scroll wheel.
    if let Event::Mouse(mouse) = event {
        return match mouse.kind {
            MouseEventKind::ScrollUp => AppAction::ScrollUp,
            MouseEventKind::ScrollDown => AppAction::ScrollDown,
            _ => AppAction::None,
        };
    }

    let Event::Key(key) = event else {
        return AppAction::None;
    };

    // Windows fires both Press and Release; only handle Press.
    if key.kind != KeyEventKind::Press {
        return AppAction::None;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Ctrl+D always quits.
    if ctrl && key.code == KeyCode::Char('d') {
        return AppAction::Quit;
    }

    // Ctrl+C: cancel running agent if waiting, otherwise quit.
    if ctrl && key.code == KeyCode::Char('c') {
        return if app.is_waiting {
            AppAction::Cancel
        } else {
            AppAction::Quit
        };
    }

    // Ctrl+Y: copy last assistant message to clipboard.
    if ctrl && key.code == KeyCode::Char('y') {
        return AppAction::CopyLast;
    }

    // While waiting, keyboard input is blocked (mouse scroll still works above).
    if app.is_waiting {
        return AppAction::None;
    }

    let completions = app.current_completions();
    let has_completions = !completions.is_empty();

    match key.code {
        KeyCode::Enter => {
            let text = app.clear_input();
            if text.trim().is_empty() {
                return AppAction::None;
            }
            if text.starts_with('/') {
                AppAction::Command(text)
            } else {
                app.history_push(&text);
                AppAction::Submit(text)
            }
        }

        // Tab accepts the highlighted autocomplete entry.
        KeyCode::Tab if has_completions => {
            let idx = app.autocomplete_index.unwrap_or(0).min(completions.len() - 1);
            app.input = completions[idx].0.to_string();
            app.autocomplete_index = Some(idx);
            AppAction::None
        }

        // Up: navigate autocomplete (if visible), scroll chat if input empty, else history.
        KeyCode::Up => {
            if has_completions {
                let len = completions.len();
                let next = match app.autocomplete_index {
                    None | Some(0) => len - 1,
                    Some(i) => i - 1,
                };
                app.autocomplete_index = Some(next);
                AppAction::None
            } else if app.input.is_empty() {
                AppAction::ScrollUp
            } else {
                app.history_prev();
                AppAction::None
            }
        }

        // Down: navigate autocomplete (if visible), scroll chat if input empty, else history.
        KeyCode::Down => {
            if has_completions {
                let len = completions.len();
                let next = match app.autocomplete_index {
                    None => 0,
                    Some(i) => (i + 1) % len,
                };
                app.autocomplete_index = Some(next);
                AppAction::None
            } else if app.input.is_empty() {
                AppAction::ScrollDown
            } else {
                app.history_next();
                AppAction::None
            }
        }

        KeyCode::Esc => {
            app.input.clear();
            app.autocomplete_index = None;
            app.history_index = None;
            AppAction::ClearInput
        }

        KeyCode::Backspace => {
            app.input.pop();
            // Keep autocomplete index in bounds after deletion.
            let updated = app.current_completions();
            if updated.is_empty() {
                app.autocomplete_index = None;
            } else if let Some(i) = app.autocomplete_index {
                app.autocomplete_index = Some(i.min(updated.len() - 1));
            }
            AppAction::None
        }

        KeyCode::Char(c) => {
            app.input.push(c);
            // Auto-select first completion when autocomplete becomes active.
            let updated = app.current_completions();
            if updated.is_empty() {
                app.autocomplete_index = None;
            } else if app.autocomplete_index.is_none() {
                app.autocomplete_index = Some(0);
            }
            AppAction::None
        }

        _ => AppAction::None,
    }
}
