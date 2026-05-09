use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn render(input: &str) -> Vec<Line<'static>> {
    let opts = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_MATH;
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
    item_indent_width: usize,
    in_table: bool,
    table_in_header: bool,
    table_alignments: Vec<Alignment>,
    table_rows: Vec<(Vec<String>, bool)>,
    table_current_row: Vec<String>,
    table_current_cell: String,
}

// ── Styles ────────────────────────────────────────────────────────────────────

fn border_style() -> Style { Style::default().fg(Color::DarkGray) }

fn lang_style(lang: &str) -> Style {
    let lower = lang.to_lowercase();
    match lower.as_str() {
        "rust" | "rs"                                         => Style::default().fg(Color::LightRed),
        "python" | "py"                                       => Style::default().fg(Color::LightBlue),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx"
                                                              => Style::default().fg(Color::LightYellow),
        "bash" | "sh" | "shell" | "zsh" | "fish"             => Style::default().fg(Color::LightGreen),
        "json"                                                => Style::default().fg(Color::Cyan),
        "toml" | "yaml" | "yml"                               => Style::default().fg(Color::LightCyan),
        "sql"                                                 => Style::default().fg(Color::Magenta),
        "html" | "xml" | "svg"                                => Style::default().fg(Color::Green),
        "css" | "scss" | "sass"                               => Style::default().fg(Color::LightMagenta),
        "go"                                                  => Style::default().fg(Color::Cyan),
        "c" | "cpp" | "c++" | "cc"                           => Style::default().fg(Color::LightRed),
        "java" | "kotlin" | "kt"                              => Style::default().fg(Color::LightYellow),
        "ruby" | "rb"                                         => Style::default().fg(Color::Red),
        _                                                     => Style::default().fg(Color::Yellow),
    }
}

fn heading_style(level: HeadingLevel) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match level {
        HeadingLevel::H1 => base.fg(Color::Cyan),
        HeadingLevel::H2 => base.fg(Color::LightCyan),
        HeadingLevel::H3 => base.fg(Color::LightYellow),
        _                => base.fg(Color::DarkGray),
    }
}

fn table_sep(left: char, mid: char, cross: char, right: char, widths: &[usize]) -> Line<'static> {
    let mut s = String::from(left);
    for (i, &w) in widths.iter().enumerate() {
        for _ in 0..w + 2 { s.push(mid); }
        if i + 1 < widths.len() { s.push(cross); }
    }
    s.push(right);
    Line::from(Span::styled(s, border_style()))
}

fn pad_cell(s: &str, width: usize, align: Alignment) -> String {
    let len = s.chars().count();
    let pad = width.saturating_sub(len);
    match align {
        Alignment::Right  => format!("{}{}", " ".repeat(pad), s),
        Alignment::Center => {
            let left = pad / 2;
            format!("{}{}{}", " ".repeat(left), s, " ".repeat(pad - left))
        }
        _ => format!("{}{}", s, " ".repeat(pad)),
    }
}

// ── Syntax highlighting ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default)]
enum HlState {
    #[default]
    Normal,
    InBlockComment,
    InStringDouble,
    InStringBacktick,
}

#[derive(Clone, Copy)]
enum Tok { Keyword, Type, Str, Comment, Number, Plain }

fn tok_style(t: Tok) -> Style {
    match t {
        Tok::Keyword => Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
        Tok::Type    => Style::default().fg(Color::Cyan),
        Tok::Str     => Style::default().fg(Color::LightGreen),
        Tok::Comment => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        Tok::Number  => Style::default().fg(Color::Yellow),
        Tok::Plain   => Style::default().fg(Color::White),
    }
}

fn push_tok(spans: &mut Vec<Span<'static>>, text: &str, t: Tok) {
    if !text.is_empty() {
        spans.push(Span::styled(text.to_string(), tok_style(t)));
    }
}

fn is_kw(word: &str, lang: &str) -> bool {
    match lang {
        "rust" | "rs" => matches!(word,
            "as"|"async"|"await"|"break"|"const"|"continue"|"crate"|"dyn"|"else"|"enum"|
            "extern"|"false"|"fn"|"for"|"if"|"impl"|"in"|"let"|"loop"|"match"|"mod"|
            "move"|"mut"|"pub"|"ref"|"return"|"self"|"Self"|"static"|"struct"|"super"|
            "trait"|"true"|"type"|"unsafe"|"use"|"where"|"while"|"Box"|"Vec"|"Option"|
            "Result"|"Some"|"None"|"Ok"|"Err"|"String"|"str"|"i8"|"i16"|"i32"|"i64"|
            "i128"|"u8"|"u16"|"u32"|"u64"|"u128"|"f32"|"f64"|"bool"|"char"|"usize"|"isize"
        ),
        "python" | "py" => matches!(word,
            "and"|"as"|"assert"|"async"|"await"|"break"|"class"|"continue"|"def"|"del"|
            "elif"|"else"|"except"|"False"|"finally"|"for"|"from"|"global"|"if"|"import"|
            "in"|"is"|"lambda"|"None"|"nonlocal"|"not"|"or"|"pass"|"raise"|"return"|
            "True"|"try"|"while"|"with"|"yield"|"self"|"cls"|"int"|"str"|"float"|"bool"|
            "list"|"dict"|"tuple"|"set"|"print"|"len"|"range"|"type"|"super"
        ),
        "javascript"|"js"|"typescript"|"ts"|"jsx"|"tsx" => matches!(word,
            "abstract"|"async"|"await"|"break"|"case"|"catch"|"class"|"const"|"continue"|
            "debugger"|"default"|"delete"|"do"|"else"|"export"|"extends"|"false"|"finally"|
            "for"|"from"|"function"|"if"|"import"|"in"|"instanceof"|"let"|"new"|"null"|
            "of"|"return"|"static"|"super"|"switch"|"this"|"throw"|"true"|"try"|"type"|
            "typeof"|"undefined"|"var"|"void"|"while"|"with"|"yield"|"interface"|
            "implements"|"enum"|"namespace"|"declare"|"readonly"|"as"|"keyof"|"never"|
            "any"|"unknown"|"number"|"string"|"boolean"
        ),
        "go" => matches!(word,
            "break"|"case"|"chan"|"const"|"continue"|"default"|"defer"|"else"|"fallthrough"|
            "for"|"func"|"go"|"goto"|"if"|"import"|"interface"|"map"|"package"|"range"|
            "return"|"select"|"struct"|"switch"|"type"|"var"|"nil"|"true"|"false"|
            "int"|"int8"|"int16"|"int32"|"int64"|"uint"|"uint8"|"uint16"|"uint32"|"uint64"|
            "float32"|"float64"|"bool"|"string"|"byte"|"rune"|"error"|"make"|"new"|"len"|
            "cap"|"append"|"copy"|"close"|"delete"|"panic"|"recover"
        ),
        "c"|"cpp"|"c++"|"cc" => matches!(word,
            "auto"|"break"|"case"|"char"|"const"|"continue"|"default"|"do"|"double"|
            "else"|"enum"|"extern"|"float"|"for"|"goto"|"if"|"inline"|"int"|"long"|
            "register"|"return"|"short"|"signed"|"sizeof"|"static"|"struct"|"switch"|
            "typedef"|"union"|"unsigned"|"void"|"volatile"|"while"|"class"|"delete"|
            "new"|"namespace"|"nullptr"|"operator"|"private"|"protected"|"public"|
            "template"|"this"|"throw"|"try"|"catch"|"true"|"false"|"virtual"|"using"|
            "override"|"final"|"bool"|"string"
        ),
        "java"|"kotlin"|"kt" => matches!(word,
            "abstract"|"assert"|"boolean"|"break"|"byte"|"case"|"catch"|"char"|"class"|
            "const"|"continue"|"default"|"do"|"double"|"else"|"enum"|"extends"|"false"|
            "final"|"finally"|"float"|"for"|"goto"|"if"|"implements"|"import"|"instanceof"|
            "int"|"interface"|"long"|"native"|"new"|"null"|"package"|"private"|"protected"|
            "public"|"return"|"short"|"static"|"super"|"switch"|"synchronized"|"this"|
            "throw"|"throws"|"transient"|"true"|"try"|"void"|"volatile"|"while"|
            "fun"|"val"|"var"|"when"|"object"|"data"|"sealed"|"open"|"override"|
            "companion"|"by"|"in"|"is"|"as"|"typealias"|"suspend"|"lateinit"
        ),
        "ruby"|"rb" => matches!(word,
            "alias"|"and"|"begin"|"break"|"case"|"class"|"def"|"do"|"else"|"elsif"|
            "end"|"ensure"|"false"|"for"|"if"|"in"|"module"|"next"|"nil"|"not"|"or"|
            "redo"|"rescue"|"retry"|"return"|"self"|"super"|"then"|"true"|"undef"|
            "unless"|"until"|"when"|"while"|"yield"
        ),
        "bash"|"sh"|"shell"|"zsh"|"fish" => matches!(word,
            "if"|"then"|"else"|"elif"|"fi"|"case"|"esac"|"for"|"while"|"do"|"done"|
            "in"|"function"|"return"|"exit"|"echo"|"export"|"local"|"readonly"|"shift"|
            "break"|"continue"|"true"|"false"|"until"|"select"|"source"|"alias"|"unset"|"set"
        ),
        "sql" => matches!(word,
            "SELECT"|"FROM"|"WHERE"|"JOIN"|"INNER"|"LEFT"|"RIGHT"|"OUTER"|"ON"|"AS"|
            "AND"|"OR"|"NOT"|"IN"|"EXISTS"|"BETWEEN"|"LIKE"|"IS"|"NULL"|"TRUE"|"FALSE"|
            "INSERT"|"INTO"|"VALUES"|"UPDATE"|"SET"|"DELETE"|"CREATE"|"TABLE"|"INDEX"|
            "DROP"|"ALTER"|"ADD"|"COLUMN"|"PRIMARY"|"KEY"|"FOREIGN"|"REFERENCES"|"UNIQUE"|
            "ORDER"|"BY"|"GROUP"|"HAVING"|"LIMIT"|"OFFSET"|"DISTINCT"|"COUNT"|"SUM"|
            "AVG"|"MIN"|"MAX"|"CASE"|"WHEN"|"THEN"|"ELSE"|"END"|"WITH"|"UNION"|"ALL"|
            "select"|"from"|"where"|"join"|"inner"|"left"|"right"|"outer"|"on"|"as"|
            "and"|"or"|"not"|"in"|"exists"|"between"|"like"|"is"|"null"|"true"|"false"|
            "insert"|"into"|"values"|"update"|"set"|"delete"|"create"|"table"|"index"|
            "drop"|"alter"|"add"|"column"|"primary"|"key"|"foreign"|"references"|"unique"|
            "order"|"by"|"group"|"having"|"limit"|"offset"|"distinct"|"count"|"sum"|
            "avg"|"min"|"max"|"case"|"when"|"then"|"else"|"end"|"with"|"union"|"all"
        ),
        _ => false,
    }
}

fn line_comment_marker(lang: &str) -> &'static str {
    match lang {
        "python"|"py"|"ruby"|"rb"|"bash"|"sh"|"shell"|"zsh"|"fish"|"toml"|"yaml"|"yml" => "#",
        "sql" => "--",
        _ => "//",
    }
}

fn has_block_comments(lang: &str) -> bool {
    matches!(lang,
        "rust"|"rs"|"c"|"cpp"|"c++"|"cc"|"go"|"java"|"kotlin"|"kt"|
        "javascript"|"js"|"typescript"|"ts"|"jsx"|"tsx"|"css"|"scss"|"sass"|"swift"|"dart"|"php"
    )
}

// Scan from inside a string (after opening quote) to find closing quote.
// Returns (content_up_to_and_including_close, rest_after_close, found_close).
fn scan_past_quote(s: &str, q: char) -> (&str, &str, bool) {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (byte_pos, c) = chars[i];
        if c == '\\' {
            i += 2;
        } else if c == q {
            let end = byte_pos + c.len_utf8();
            return (&s[..end], &s[end..], true);
        } else {
            i += 1;
        }
    }
    (s, "", false)
}

fn highlight_line<'a>(line: &'a str, lang: &str, state: &mut HlState) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut s: &str = line;

    // Resume multi-line state from previous line
    match *state {
        HlState::InBlockComment => {
            if let Some(end) = s.find("*/") {
                push_tok(&mut spans, &s[..end + 2], Tok::Comment);
                s = &s[end + 2..];
                *state = HlState::Normal;
            } else {
                push_tok(&mut spans, s, Tok::Comment);
                return spans;
            }
        }
        HlState::InStringDouble => {
            let (content, rest, ended) = scan_past_quote(s, '"');
            push_tok(&mut spans, content, Tok::Str);
            if ended { *state = HlState::Normal; }
            s = rest;
            if s.is_empty() { return spans; }
        }
        HlState::InStringBacktick => {
            let (content, rest, ended) = scan_past_quote(s, '`');
            push_tok(&mut spans, content, Tok::Str);
            if ended { *state = HlState::Normal; }
            s = rest;
            if s.is_empty() { return spans; }
        }
        HlState::Normal => {}
    }

    let lc = line_comment_marker(lang);
    let bc = has_block_comments(lang);

    while !s.is_empty() {
        // Line comment
        if s.starts_with(lc) {
            push_tok(&mut spans, s, Tok::Comment);
            return spans;
        }

        // Block comment
        if bc && s.starts_with("/*") {
            if let Some(end) = s.find("*/") {
                push_tok(&mut spans, &s[..end + 2], Tok::Comment);
                s = &s[end + 2..];
                continue;
            } else {
                push_tok(&mut spans, s, Tok::Comment);
                *state = HlState::InBlockComment;
                return spans;
            }
        }

        let first = s.chars().next().unwrap();
        let first_len = first.len_utf8();

        // Double-quoted string (multi-line supported)
        if first == '"' {
            let after_open = &s[1..];
            let (content, rest, ended) = scan_past_quote(after_open, '"');
            let total = 1 + content.len();
            push_tok(&mut spans, &s[..total], Tok::Str);
            if !ended { *state = HlState::InStringDouble; }
            s = rest;
            continue;
        }

        // Backtick string (JS template literals, multi-line supported)
        if first == '`' {
            let after_open = &s[1..];
            let (content, rest, ended) = scan_past_quote(after_open, '`');
            let total = 1 + content.len();
            push_tok(&mut spans, &s[..total], Tok::Str);
            if !ended { *state = HlState::InStringBacktick; }
            s = rest;
            continue;
        }

        // Single-quoted string (no multi-line — avoids Rust lifetime false positives)
        if first == '\'' {
            let after_open = &s[1..];
            let (content, rest, ended) = scan_past_quote(after_open, '\'');
            if ended {
                let total = 1 + content.len();
                push_tok(&mut spans, &s[..total], Tok::Str);
                s = rest;
            } else {
                // No closing quote on this line — emit as plain to avoid polluting next line
                push_tok(&mut spans, "'", Tok::Plain);
                s = &s[1..];
            }
            continue;
        }

        // Whitespace
        if first.is_whitespace() {
            let len: usize = s.chars().take_while(|c| c.is_whitespace()).map(|c| c.len_utf8()).sum();
            push_tok(&mut spans, &s[..len], Tok::Plain);
            s = &s[len..];
            continue;
        }

        // Number
        if first.is_ascii_digit() {
            let len: usize = s.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                .map(|c| c.len_utf8())
                .sum();
            push_tok(&mut spans, &s[..len], Tok::Number);
            s = &s[len..];
            continue;
        }

        // Identifier / keyword
        if first.is_ascii_alphabetic() || first == '_' {
            let len: usize = s.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '?' || *c == '!')
                .map(|c| c.len_utf8())
                .sum();
            let word = &s[..len];
            let tok = if is_kw(word, lang) {
                Tok::Keyword
            } else if first.is_uppercase() {
                Tok::Type
            } else {
                Tok::Plain
            };
            push_tok(&mut spans, word, tok);
            s = &s[len..];
            continue;
        }

        // Operator / punctuation — single char
        push_tok(&mut spans, &s[..first_len], Tok::Plain);
        s = &s[first_len..];
    }

    spans
}

// ── Ctx impl ──────────────────────────────────────────────────────────────────

impl Ctx {
    fn text_style(&self) -> Style {
        let mut s = if let Some(level) = self.heading {
            heading_style(level)
        } else {
            Style::default().fg(Color::White)
        };
        if self.bold   { s = s.add_modifier(Modifier::BOLD); }
        if self.italic { s = s.add_modifier(Modifier::ITALIC); }
        if self.strike { s = s.add_modifier(Modifier::CROSSED_OUT); }
        s
    }

    fn flush(&mut self) {
        self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        self.item_prefix_done = false;
    }

    fn blank(&mut self) { self.lines.push(Line::default()); }

    fn emit_blockquote_prefix(&mut self) {
        if self.blockquote > 0 && self.spans.is_empty() {
            self.spans.push(Span::styled(
                "│ ".repeat(self.blockquote as usize),
                border_style(),
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
            self.item_indent_width = marker.chars().count();
            self.spans.push(Span::styled(marker, border_style()));
            self.item_prefix_done = true;
        }
    }

    // ── Event processing ──────────────────────────────────────────────────────

    fn process(&mut self, event: Event) {
        match event {
            Event::Start(tag)  => self.start(tag),
            Event::End(tag)    => self.end(tag),
            Event::Text(t)     => self.text(t.into_string()),
            Event::Code(t)     => self.inline_code(t.into_string()),
            Event::InlineMath(t) => {
                self.spans.push(Span::styled(
                    format!("${}$", t.into_string()),
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC),
                ));
            }
            Event::DisplayMath(t) => self.display_math(t.into_string()),
            Event::SoftBreak | Event::HardBreak => {
                if !self.in_table && !self.spans.is_empty() {
                    self.flush();
                }
            }
            Event::Rule => {
                self.lines.push(Line::from(Span::styled("─".repeat(48), border_style())));
                self.blank();
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                // Extra blank line before H1 for visual weight
                if matches!(level, HeadingLevel::H1) && !self.lines.is_empty() {
                    self.blank();
                }
                self.heading = Some(level);
                // Visual marker for H3+ to signal nesting depth without looking like raw markdown
                let prefix: Option<&'static str> = match level {
                    HeadingLevel::H3 => Some("▸ "),
                    HeadingLevel::H4 | HeadingLevel::H5 | HeadingLevel::H6 => Some("· "),
                    _ => None,
                };
                if let Some(p) = prefix {
                    self.spans.push(Span::styled(p.to_string(), border_style()));
                }
            }
            Tag::CodeBlock(kind) => {
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.into_string()),
                    _ => None,
                };
                self.code_block = true;
                self.code_buf.clear();
            }
            Tag::Strong        => self.bold   = true,
            Tag::Emphasis      => self.italic  = true,
            Tag::Strikethrough => self.strike  = true,
            Tag::List(first) => {
                if !self.spans.is_empty() { self.flush(); }
                self.list_ordered.push(first.is_some());
                self.item_nums.push(first.unwrap_or(1));
            }
            Tag::Item => {
                if !self.spans.is_empty() { self.flush(); }
                self.in_item = true;
                self.item_prefix_done = false;
            }
            Tag::BlockQuote(_) => self.blockquote += 1,
            Tag::Table(aligns) => {
                if !self.spans.is_empty() { self.flush(); }
                self.in_table = true;
                self.table_alignments = aligns;
                self.table_rows.clear();
                self.table_current_row.clear();
                self.table_current_cell.clear();
            }
            Tag::TableHead => {
                self.table_in_header = true;
                self.table_current_row.clear();
            }
            Tag::TableRow => {
                self.table_current_row.clear();
            }
            Tag::TableCell => {
                self.table_current_cell.clear();
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.flush();
                // Measure actual heading text width to draw a matching underline
                let text_width = self.lines.last()
                    .map(|l| l.spans.iter().map(|s| s.content.chars().count()).sum::<usize>())
                    .unwrap_or(4);
                let bar: Option<(&str, Style)> = match self.heading {
                    Some(HeadingLevel::H1) => Some(("━", Style::default().fg(Color::Cyan))),
                    Some(HeadingLevel::H2) => Some(("─", Style::default().fg(Color::LightCyan))),
                    _ => None,
                };
                if let Some((ch, style)) = bar {
                    let bar_str = ch.repeat(text_width.max(1));
                    self.lines.push(Line::from(Span::styled(bar_str, style)));
                }
                self.blank();
                self.heading = None;
            }

            TagEnd::Paragraph => {
                if !self.spans.is_empty() { self.flush(); }
                self.blank();
            }

            TagEnd::CodeBlock => {
                let lang = self.code_lang.take().unwrap_or_default();
                let lang_lower = lang.to_lowercase();
                let header_col = lang_style(&lang_lower);
                let header = if lang.is_empty() {
                    Line::from(Span::styled(" ┌─ ".to_string(), border_style()))
                } else {
                    Line::from(vec![
                        Span::styled(" ┌─ ".to_string(), border_style()),
                        Span::styled(lang.clone(), header_col.add_modifier(Modifier::BOLD)),
                        Span::styled(" ".to_string(), border_style()),
                    ])
                };
                self.lines.push(header);

                let mut hl_state = HlState::Normal;
                for line in self.code_buf.trim_end_matches('\n').lines() {
                    let hl_spans = highlight_line(line, &lang_lower, &mut hl_state);
                    let mut line_spans = vec![Span::styled(" │ ".to_string(), border_style())];
                    if hl_spans.is_empty() {
                        // blank line inside code block
                        line_spans.push(Span::raw("".to_string()));
                    } else {
                        line_spans.extend(hl_spans);
                    }
                    self.lines.push(Line::from(line_spans));
                }

                self.lines.push(Line::from(Span::styled(" └─".to_string(), border_style())));
                self.blank();
                self.code_block = false;
                self.code_buf.clear();
            }

            TagEnd::Strong      => self.bold   = false,
            TagEnd::Emphasis    => self.italic  = false,
            TagEnd::Strikethrough => self.strike = false,

            TagEnd::List(_) => {
                self.list_ordered.pop();
                self.item_nums.pop();
                if self.list_ordered.is_empty() { self.blank(); }
            }

            TagEnd::Item => {
                if !self.spans.is_empty() { self.flush(); }
                self.in_item = false;
                if let Some(n) = self.item_nums.last_mut() { *n += 1; }
            }

            TagEnd::BlockQuote(_) => {
                if !self.spans.is_empty() { self.flush(); }
                self.blockquote = self.blockquote.saturating_sub(1);
                self.blank();
            }

            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.table_current_cell);
                self.table_current_row.push(cell);
            }

            TagEnd::TableRow => {
                let row = std::mem::take(&mut self.table_current_row);
                if !row.is_empty() {
                    self.table_rows.push((row, self.table_in_header));
                }
            }

            TagEnd::TableHead => {
                if !self.table_current_row.is_empty() {
                    let row = std::mem::take(&mut self.table_current_row);
                    self.table_rows.push((row, true));
                }
                self.table_in_header = false;
            }

            TagEnd::Table => {
                self.in_table = false;
                let rows  = std::mem::take(&mut self.table_rows);
                let aligns = std::mem::take(&mut self.table_alignments);
                if rows.is_empty() { return; }

                let cols = rows.iter().map(|(r, _)| r.len()).max().unwrap_or(0);
                let mut col_widths: Vec<usize> = vec![1; cols];
                for (row, _) in &rows {
                    for (i, cell) in row.iter().enumerate() {
                        if i < cols {
                            col_widths[i] = col_widths[i].max(cell.chars().count());
                        }
                    }
                }

                self.lines.push(table_sep('┌', '─', '┬', '┐', &col_widths));
                for (row, is_header) in &rows {
                    let mut spans: Vec<Span<'static>> =
                        vec![Span::styled("│".to_string(), border_style())];
                    for (i, &w) in col_widths.iter().enumerate() {
                        let cell  = row.get(i).map(|s| s.as_str()).unwrap_or("");
                        let align = aligns.get(i).copied().unwrap_or(Alignment::None);
                        let cell_style = if *is_header {
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        spans.push(Span::styled(format!(" {} ", pad_cell(cell, w, align)), cell_style));
                        spans.push(Span::styled("│".to_string(), border_style()));
                    }
                    self.lines.push(Line::from(spans));
                    if *is_header {
                        self.lines.push(table_sep('├', '─', '┼', '┤', &col_widths));
                    }
                }
                self.lines.push(table_sep('└', '─', '┴', '┘', &col_widths));
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
        if self.in_table {
            self.table_current_cell.push_str(&t);
            return;
        }
        self.emit_blockquote_prefix();
        self.emit_item_prefix();
        let style = self.text_style();
        // H1 renders as uppercase for stronger visual weight in terminal
        let t = if self.heading == Some(HeadingLevel::H1) { t.to_uppercase() } else { t };
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
        if self.in_table {
            self.table_current_cell.push('`');
            self.table_current_cell.push_str(&t);
            self.table_current_cell.push('`');
            return;
        }
        self.emit_item_prefix();
        self.spans.push(Span::styled(
            format!("`{}`", t),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }

    fn display_math(&mut self, t: String) {
        if !self.spans.is_empty() { self.flush(); }
        self.lines.push(Line::from(Span::styled(" ┌─ math ".to_string(), border_style())));
        for line in t.trim_end_matches('\n').lines() {
            self.lines.push(Line::from(vec![
                Span::styled(" │ ".to_string(), border_style()),
                Span::styled(line.to_string(), Style::default().fg(Color::Magenta)),
            ]));
        }
        self.lines.push(Line::from(Span::styled(" └─".to_string(), border_style())));
        self.blank();
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
