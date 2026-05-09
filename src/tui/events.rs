use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::tui::app::App;

pub enum AppAction {
    None,
    Submit(String),
    Quit,
    ScrollUp,
    ScrollDown,
    ClearInput,
}

pub fn handle_event(app: &mut App, event: Event) -> AppAction {
    let Event::Key(key) = event else {
        return AppAction::None;
    };

    // Windows fires both Press and Release; ignore everything except Press.
    if key.kind != KeyEventKind::Press {
        return AppAction::None;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Quit always works regardless of waiting state
    if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d')) {
        return AppAction::Quit;
    }

    if app.is_waiting {
        return AppAction::None;
    }

    match key.code {
        KeyCode::Enter => {
            let text = app.clear_input();
            if text.trim().is_empty() {
                AppAction::None
            } else {
                AppAction::Submit(text)
            }
        }
        KeyCode::Esc => {
            app.input.clear();
            AppAction::ClearInput
        }
        KeyCode::Backspace => {
            app.input.pop();
            AppAction::None
        }
        KeyCode::PageUp | KeyCode::Char('k') => AppAction::ScrollUp,
        KeyCode::PageDown | KeyCode::Char('j') => AppAction::ScrollDown,
        KeyCode::Char(c) => {
            app.input.push(c);
            AppAction::None
        }
        _ => AppAction::None,
    }
}
