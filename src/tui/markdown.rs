use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn render(input: &str) -> Vec<Line<'static>> {
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let mut ctx = Ctx::default();
    for event in Parser::new_ext(input, opts) {
        ctx.process(event);
    }
    ctx.finish()
}

#[derive(Default)]
struct Ctx {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    bold: bool,
    italic: bool,
    strike: bool,
    heading: Option<HeadingLevel>,
    code_block: bool,
    code_lang: Option<String>,
    code_buf: String,
    blockquote: u32,
    list_ordered: Vec<bool>,
    item_nums: Vec<u64>,
    in_item: bool,
    item_prefix_done: bool,
}

// ── Styles ────────────────────────────────────────────────────────────────────

const BORDER: Style = Style::new().fg(Color::DarkGray);
const CODE_FG: Style = Style::new().fg(Color::Yellow);

fn heading_style(level: HeadingLevel) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match level {
        HeadingLevel::H1 => base.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
        HeadingLevel::H2 => base.fg(Color::LightCyan).add_modifier(Modifier::UNDERLINED),
        HeadingLevel::H3 => base.fg(Color::White),
        _ => base.fg(Color::DarkGray),
    }
}

// ── Ctx impl ──────────────────────────────────────────────────────────────────

impl Ctx {
    fn text_style(&self) -> Style {
        let mut s = if let Some(level) = self.heading {
            heading_style(level)
        } else {
            Style::default().fg(Color::White)
        };
        if self.bold { s = s.add_modifier(Modifier::BOLD); }
        if self.italic { s = s.add_modifier(Modifier::ITALIC); }
        if self.strike { s = s.add_modifier(Modifier::CROSSED_OUT); }
        s
    }

    fn flush(&mut self) {
        self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        self.item_prefix_done = false;
    }

    fn blank(&mut self) {
        self.lines.push(Line::default());
    }

    // ── Prefix emitters ───────────────────────────────────────────────────────

    fn emit_blockquote_prefix(&mut self) {
        if self.blockquote > 0 && self.spans.is_empty() {
            self.spans.push(Span::styled(
                "│ ".repeat(self.blockquote as usize),
                BORDER,
            ));
        }
    }

    fn emit_item_prefix(&mut self) {
        if self.in_item && !self.item_prefix_done {
            let depth = self.list_ordered.len();
            let indent = "  ".repeat(depth.saturating_sub(1));
            let marker = if *self.list_ordered.last().unwrap_or(&false) {
                format!("{}{}.  ", indent, self.item_nums.last().copied().unwrap_or(1))
            } else {
                format!("{}•  ", indent)
            };
            self.spans.push(Span::styled(marker, BORDER));
            self.item_prefix_done = true;
        }
    }

    // ── Event processing ──────────────────────────────────────────────────────

    fn process(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(t.into_string()),
            Event::Code(t) => self.inline_code(t.into_string()),
            Event::SoftBreak | Event::HardBreak => {
                if !self.spans.is_empty() {
                    self.flush();
                }
            }
            Event::Rule => {
                self.lines.push(Line::from(Span::styled("─".repeat(48), BORDER)));
                self.blank();
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => self.heading = Some(level),
            Tag::CodeBlock(kind) => {
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.into_string()),
                    _ => None,
                };
                self.code_block = true;
                self.code_buf.clear();
            }
            Tag::Strong => self.bold = true,
            Tag::Emphasis => self.italic = true,
            Tag::Strikethrough => self.strike = true,
            Tag::List(first) => {
                self.list_ordered.push(first.is_some());
                self.item_nums.push(first.unwrap_or(1));
            }
            Tag::Item => {
                self.in_item = true;
                self.item_prefix_done = false;
            }
            Tag::BlockQuote(_) => self.blockquote += 1,
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.flush();
                // Underline bar beneath H1 and H2.
                let bar = match self.heading {
                    Some(HeadingLevel::H1) => {
                        Some(("━".repeat(40), Style::default().fg(Color::Cyan)))
                    }
                    Some(HeadingLevel::H2) => {
                        Some(("─".repeat(32), Style::default().fg(Color::LightCyan)))
                    }
                    _ => None,
                };
                if let Some((s, style)) = bar {
                    self.lines.push(Line::from(Span::styled(s, style)));
                }
                self.blank();
                self.heading = None;
            }

            TagEnd::Paragraph => {
                if !self.spans.is_empty() {
                    self.flush();
                }
                self.blank();
            }

            TagEnd::CodeBlock => {
                let lang = self.code_lang.take().unwrap_or_default();
                let header = if lang.is_empty() {
                    " ┌─".to_string()
                } else {
                    format!(" ┌─ {} ", lang)
                };
                self.lines.push(Line::from(Span::styled(header, BORDER)));
                for line in self.code_buf.trim_end_matches('\n').lines() {
                    self.lines.push(Line::from(vec![
                        Span::styled(" │ ", BORDER),
                        Span::styled(line.to_string(), CODE_FG),
                    ]));
                }
                self.lines.push(Line::from(Span::styled(" └─", BORDER)));
                self.blank();
                self.code_block = false;
                self.code_buf.clear();
            }

            TagEnd::Strong => self.bold = false,
            TagEnd::Emphasis => self.italic = false,
            TagEnd::Strikethrough => self.strike = false,

            TagEnd::List(_) => {
                self.list_ordered.pop();
                self.item_nums.pop();
                if self.list_ordered.is_empty() {
                    self.blank();
                }
            }

            TagEnd::Item => {
                if !self.spans.is_empty() {
                    self.flush();
                }
                self.in_item = false;
                if let Some(n) = self.item_nums.last_mut() {
                    *n += 1;
                }
            }

            TagEnd::BlockQuote(_) => {
                if !self.spans.is_empty() {
                    self.flush();
                }
                self.blockquote = self.blockquote.saturating_sub(1);
                self.blank();
            }

            _ => {}
        }
    }

    fn text(&mut self, t: String) {
        if self.code_block {
            self.code_buf.push_str(&t);
            return;
        }

        self.emit_blockquote_prefix();
        self.emit_item_prefix();

        let style = self.text_style();

        // Text events can contain embedded newlines (e.g. inside blockquotes).
        let mut parts = t.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                self.spans.push(Span::styled(part.to_string(), style));
            }
            if parts.peek().is_some() {
                self.flush();
                self.emit_blockquote_prefix();
                self.emit_item_prefix();
            }
        }
    }

    fn inline_code(&mut self, t: String) {
        self.emit_item_prefix();
        self.spans.push(Span::styled(
            format!("`{}`", t),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if !self.spans.is_empty() {
            self.lines.push(Line::from(self.spans));
        }
        while self.lines.last().map(|l| l.spans.is_empty()).unwrap_or(false) {
            self.lines.pop();
        }
        self.lines
    }
}
