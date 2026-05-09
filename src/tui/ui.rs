use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::app::{App, ChatMessage, MessageType};
use crate::tui::commands;

// Only non-obvious shortcuts live here.
const HEADER_HINTS: &str = "Ctrl+C: cancel · Ctrl+D: quit · Ctrl+Y: copy · /help";

pub fn draw(frame: &mut ratatui::Frame, app: &mut App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header: title + hints
            Constraint::Min(0),    // message area
            Constraint::Length(2), // activity: tool call + └─ result
            Constraint::Length(3), // input box
            Constraint::Length(1), // status bar
        ])
        .split(area);

    // ── Header ───────────────────────────────────────────────────────────────
    let header = Line::from(vec![
        Span::styled(" mimicode", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("    {}", HEADER_HINTS),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
        ),
    ]);
    frame.render_widget(Paragraph::new(header), chunks[0]);

    // ── Message area ─────────────────────────────────────────────────────────
    let all_lines: Vec<Line<'static>> = app.messages.iter().flat_map(message_lines).collect();
    let total = all_lines.len();
    let visible_height = chunks[1].height.saturating_sub(2) as usize;
    let max_offset = total.saturating_sub(visible_height);

    app.total_lines = total;
    app.scroll_offset = app.scroll_offset.min(max_offset);

    let offset = app.scroll_offset;
    let visible: Vec<Line<'static>> = all_lines.into_iter().skip(offset).collect();

    frame.render_widget(
        Paragraph::new(visible).block(Block::default().borders(Borders::ALL)),
        chunks[1],
    );

    // ── Activity area (above input) ───────────────────────────────────────────
    // Line 1: current or last tool call.
    // Line 2: └─ first line of result, indented to connect visually.
    let status_text = app.tool_status.as_deref().unwrap_or("");
    let result_text = app.tool_result.as_deref().unwrap_or("");

    let activity_lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            if status_text.is_empty() {
                String::new()
            } else {
                format!(" ⚙ {}", status_text)
            },
            Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
        )),
        Line::from(Span::styled(
            if result_text.is_empty() {
                String::new()
            } else {
                format!("   └─ {}", result_text)
            },
            Style::default().fg(Color::Green).add_modifier(Modifier::DIM),
        )),
    ];
    frame.render_widget(Paragraph::new(activity_lines), chunks[2]);

    // ── Autocomplete overlay ──────────────────────────────────────────────────
    if app.input.starts_with('/') {
        let completions = commands::completions(&app.input);
        if !completions.is_empty() {
            let comp_h = (completions.len() as u16 + 2).min(chunks[1].height);
            let comp_w = 54_u16.min(area.width);
            let comp_y = chunks[3].y.saturating_sub(comp_h).max(chunks[1].y);
            let comp_rect = Rect {
                x: chunks[3].x,
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

    // ── Input box ────────────────────────────────────────────────────────────
    let (border_color, input_line) = if app.is_waiting {
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

    frame.render_widget(
        Paragraph::new(Text::from(vec![input_line])).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        ),
        chunks[3],
    );

    // Cursor on the input line; hidden while waiting.
    if !app.is_waiting {
        let inner_width = chunks[3].width.saturating_sub(2);
        let col = (app.input.len() as u16).min(inner_width.saturating_sub(1));
        frame.set_cursor_position((chunks[3].x + 1 + col, chunks[3].y + 1));
    }

    // ── Status bar ───────────────────────────────────────────────────────────
    let s = &app.status;
    let status_line = format!(
        " {} | {} | turn {} | in {} out {}",
        s.session_id, s.model, s.turn, s.tokens_in, s.tokens_out,
    );
    frame.render_widget(
        Paragraph::new(status_line).style(Style::default().fg(Color::DarkGray)),
        chunks[4],
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

    lines.push(Line::default()); // blank separator
    lines
}
