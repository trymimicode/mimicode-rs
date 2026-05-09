use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Render a markdown string into ratatui lines for display in the TUI.
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
    // Inline style toggles.
    bold: bool,
    italic: bool,
    strike: bool,
    // Block context.
    heading: Option<HeadingLevel>,
    code_block: bool,
    code_buf: String,
    blockquote: u32,
    // List tracking.
    list_ordered: Vec<bool>,  // true = ordered at each depth
    item_nums: Vec<u64>,      // current counter at each depth
    in_item: bool,
    item_prefix_done: bool,   // prevents emitting the bullet twice per item
}

impl Ctx {
    // ── Style helpers ─────────────────────────────────────────────────────────

    fn text_style(&self) -> Style {
        let mut s = Style::default().fg(Color::White);
        if self.bold || self.heading.is_some() {
            s = s.add_modifier(Modifier::BOLD);
        }
        if self.italic {
            s = s.add_modifier(Modifier::ITALIC);
        }
        if self.strike {
            s = s.add_modifier(Modifier::CROSSED_OUT);
        }
        if let Some(level) = self.heading {
            s = s.fg(match level {
                HeadingLevel::H1 => Color::Cyan,
                HeadingLevel::H2 => Color::LightCyan,
                _ => Color::White,
            });
        }
        s
    }

    // ── Line management ───────────────────────────────────────────────────────

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
            let s = "│ ".repeat(self.blockquote as usize);
            self.spans.push(Span::styled(s, Style::default().fg(Color::DarkGray)));
        }
    }

    fn emit_heading_prefix(&mut self) {
        if let Some(level) = self.heading {
            if self.spans.is_empty() {
                let prefix = match level {
                    HeadingLevel::H1 => "# ",
                    HeadingLevel::H2 => "## ",
                    HeadingLevel::H3 => "### ",
                    _ => "#### ",
                };
                self.spans.push(Span::styled(prefix, Style::default().fg(Color::DarkGray)));
            }
        }
    }

    fn emit_item_prefix(&mut self) {
        if self.in_item && !self.item_prefix_done {
            let depth = self.list_ordered.len();
            let indent = "  ".repeat(depth.saturating_sub(1));
            let dim = Style::default().fg(Color::DarkGray);
            let marker = if *self.list_ordered.last().unwrap_or(&false) {
                format!("{}{}.  ", indent, self.item_nums.last().copied().unwrap_or(1))
            } else {
                format!("{}•  ", indent)
            };
            self.spans.push(Span::styled(marker, dim));
            self.item_prefix_done = true;
        }
    }

    // ── Event dispatch ────────────────────────────────────────────────────────

    fn process(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.text(t.into_string()),
            Event::Code(t) => self.inline_code(t.into_string()),
            Event::SoftBreak => self.flush(),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(48),
                    Style::default().fg(Color::DarkGray),
                )));
                self.blank();
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => self.heading = Some(level),
            Tag::CodeBlock(_) => {
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
                self.blank();
                self.heading = None;
            }
            TagEnd::Paragraph => {
                if !self.spans.is_empty() {
                    self.flush();
                }
                // Only add blank line between paragraphs, not after the last one
                // (finish() strips trailing blanks).
                self.blank();
            }
            TagEnd::CodeBlock => {
                let style = Style::default().fg(Color::Yellow);
                for line in self.code_buf.trim_end_matches('\n').lines() {
                    self.lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        style,
                    )));
                }
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
        self.emit_heading_prefix();
        self.emit_item_prefix();

        let style = self.text_style();

        // A single Text event can contain embedded newlines (rare but possible).
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

    // ── Finalize ──────────────────────────────────────────────────────────────

    fn finish(mut self) -> Vec<Line<'static>> {
        if !self.spans.is_empty() {
            self.lines.push(Line::from(self.spans));
        }
        // Strip trailing blank lines — the caller adds a single separator.
        while self.lines.last().map(|l| l.spans.is_empty()).unwrap_or(false) {
            self.lines.pop();
        }
        self.lines
    }
}
