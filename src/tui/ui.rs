use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::app::{App, ChatMessage, MessageType};
use crate::tui::commands;

const HINT: &str =
    "Enter: send  ↑↓: history  Tab: complete  PgUp/Dn: scroll  Ctrl+C: cancel  Ctrl+D: quit  Ctrl+Y: copy";

pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(4), // top border + input line + hint line + bottom border
            Constraint::Length(1),
        ])
        .split(area);

    // ── Message list ─────────────────────────────────────────────────────────
    let all_lines: Vec<Line<'static>> = app.messages.iter().flat_map(message_lines).collect();
    let total = all_lines.len();
    let visible_height = chunks[0].height.saturating_sub(2) as usize;
    let max_offset = total.saturating_sub(visible_height);

    app.total_lines = total;
    app.scroll_offset = app.scroll_offset.min(max_offset);

    let offset = app.scroll_offset;
    let visible: Vec<Line<'static>> = all_lines.into_iter().skip(offset).collect();

    let msg_title = if offset > 0 {
        "mimicode [↑ PgUp/PgDn to scroll]"
    } else {
        "mimicode"
    };
    frame.render_widget(
        Paragraph::new(visible)
            .block(Block::default().borders(Borders::ALL).title(msg_title)),
        chunks[0],
    );

    // ── Autocomplete overlay ──────────────────────────────────────────────────
    if app.input.starts_with('/') {
        let completions = commands::completions(&app.input);
        if !completions.is_empty() {
            let comp_h = (completions.len() as u16 + 2).min(chunks[0].height);
            let comp_w = 54_u16.min(area.width);
            let comp_y = chunks[1]
                .y
                .saturating_sub(comp_h)
                .max(chunks[0].y);
            let comp_rect = Rect {
                x: chunks[1].x,
                y: comp_y,
                width: comp_w,
                height: comp_h,
            };

            let sel = app.autocomplete_index.unwrap_or(0);
            let lines: Vec<Line<'static>> = completions
                .iter()
                .enumerate()
                .map(|(i, (cmd, desc))| {
                    let style = if i == sel {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    Line::from(Span::styled(format!("{:<14}{}", cmd, desc), style))
                })
                .collect();

            frame.render_widget(Clear, comp_rect);
            frame.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                ),
                comp_rect,
            );
        }
    }

    // ── Input area ───────────────────────────────────────────────────────────
    let hint_span = Span::styled(
        HINT,
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
    );

    let (border_color, top_line) = if app.is_waiting {
        (
            Color::Yellow,
            Line::from(Span::styled(
                "waiting…",
                Style::default().add_modifier(Modifier::DIM),
            )),
        )
    } else {
        (Color::Blue, Line::from(app.input.clone()))
    };

    let input_text = Text::from(vec![top_line, Line::from(hint_span)]);
    let input_block = Block::default()
        .borders(Borders::ALL)
        .title("input")
        .border_style(Style::default().fg(border_color));

    frame.render_widget(Paragraph::new(input_text).block(input_block), chunks[1]);

    // Cursor sits on the input line; hidden while waiting.
    if !app.is_waiting {
        let inner_width = chunks[1].width.saturating_sub(2);
        let col = (app.input.len() as u16).min(inner_width.saturating_sub(1));
        frame.set_cursor_position((chunks[1].x + 1 + col, chunks[1].y + 1));
    }

    // ── Status bar ───────────────────────────────────────────────────────────
    let s = &app.status;
    let status_line = format!(
        " {} | {} | turn {} | in {} out {}",
        s.session_id, s.model, s.turn, s.tokens_in, s.tokens_out,
    );
    frame.render_widget(
        Paragraph::new(status_line).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn message_lines(msg: &ChatMessage) -> Vec<Line<'static>> {
    let (prefix, style): (&str, Style) = match msg.message_type {
        MessageType::User => (
            "> ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        MessageType::Assistant => ("", Style::default().fg(Color::White)),
        MessageType::ToolCall => (
            "⚙ ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
        ),
        MessageType::ToolResult => (
            "  → ",
            Style::default().fg(Color::Green).add_modifier(Modifier::DIM),
        ),
        MessageType::Error => (
            "✗ ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        MessageType::System => (
            "• ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::DIM),
        ),
    };

    let mut lines: Vec<Line<'static>> = if msg.content.is_empty() {
        vec![Line::from(Span::styled(prefix.to_string(), style))]
    } else {
        let indent = prefix.chars().count();
        msg.content
            .lines()
            .enumerate()
            .map(|(i, line)| {
                let text = if i == 0 {
                    format!("{}{}", prefix, line)
                } else {
                    format!("{:indent$}{}", "", line)
                };
                Line::from(Span::styled(text, style))
            })
            .collect()
    };

    lines.push(Line::default()); // blank separator between messages
    lines
}
