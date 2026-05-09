//! Boot intro: animated “m” + morphing trailing text, then settle into the header.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntroState {
    Playing { started: Instant },
    Done,
}

impl IntroState {
    pub fn new_playing() -> Self {
        Self::Playing {
            started: Instant::now(),
        }
    }

    pub fn skip(&mut self) {
        *self = Self::Done;
    }

    pub fn is_playing(&self) -> bool {
        matches!(self, Self::Playing { .. })
    }
}

const MS_EMPTY: u64 = 320;
const MS_M_SLIDE: u64 = 520;
const MS_MINIMAL_BUILD: u64 = 820;
const MS_HOLD_WORD: u64 = 700;
const MS_MORPH: u64 = 480;
const MS_FLY: u64 = 720;

const TRAILINGS: &[&str] = &["inimal", "emory-oriented", "ade-with-love", "imicode"];

const HEADER_LABEL: &str = " mimicode";

/// Full-screen intro. Returns `true` if this frame is only the intro (caller should skip the rest).
pub fn draw(frame: &mut ratatui::Frame, area: Rect, header_row: Rect, intro: &mut IntroState) -> bool {
    let IntroState::Playing { started } = *intro else {
        return false;
    };

    let elapsed = started.elapsed();
    if elapsed >= total_duration() {
        *intro = IntroState::Done;
        return false;
    }

    frame.render_widget(Clear, area);

    let w = area.width as i32;
    let h = area.height as i32;
    let cx = area.x as i32 + w / 2;
    let cy = area.y as i32 + h / 2;

    let mut t = elapsed.as_millis() as u64;

    if t < MS_EMPTY {
        return true;
    }
    t -= MS_EMPTY;

    // --- "m" slides up ---
    if t < MS_M_SLIDE {
        let u = smoothstep(0.0, 1.0, t as f32 / MS_M_SLIDE as f32);
        let y0 = area.y as i32 + h + 1;
        let y1 = cy;
        let y = lerp_i(y0, y1, u);
        draw_m_only(frame, area, cx, y);
        return true;
    }
    t -= MS_M_SLIDE;

    // --- Build "minimal": m drifts left, trailing letters fade in ---
    if t < MS_MINIMAL_BUILD {
        let u = smoothstep(0.0, 1.0, t as f32 / MS_MINIMAL_BUILD as f32);
        let trail = TRAILINGS[0];
        let full_w = (1 + trail.chars().count()) as i32;
        let target_mx = cx - full_w / 2;
        let mx = lerp_i(cx, target_mx, u);

        let n = trail.chars().count();
        let reveal_u = smoothstep(0.0, 1.0, u);
        let visible_count = ((reveal_u * n as f32).ceil() as usize).min(n);

        let mut spans = vec![Span::styled(
            "m",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )];
        for (i, ch) in trail.chars().enumerate().take(visible_count) {
            let stagger = reveal_u * (n + 2) as f32 - i as f32;
            let fa = stagger.clamp(0.0, 1.0);
            spans.push(Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(fade_fg(fa))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        draw_spans(frame, area, mx, cy, spans);
        return true;
    }
    t -= MS_MINIMAL_BUILD;

    // --- Hold "minimal", then morph through remaining trailings ---
    if t < MS_HOLD_WORD {
        let trail = TRAILINGS[0];
        let mx = centered_m_x(cx, trail);
        draw_word(frame, area, mx, cy, trail, 1.0);
        return true;
    }
    t -= MS_HOLD_WORD;

    for word_i in 1..TRAILINGS.len() {
        if t < MS_MORPH {
            let u = smoothstep(0.0, 1.0, t as f32 / MS_MORPH as f32);
            morph_trailing(frame, area, cx, cy, word_i - 1, word_i, u);
            return true;
        }
        t -= MS_MORPH;

        if t < MS_HOLD_WORD {
            let trail = TRAILINGS[word_i];
            let mx = centered_m_x(cx, trail);
            draw_word(frame, area, mx, cy, trail, 1.0);
            return true;
        }
        t -= MS_HOLD_WORD;
    }

    // --- Fly full header label to the top-left header slot ---
    let trail = TRAILINGS[TRAILINGS.len() - 1];
    let mx0 = centered_m_x(cx, trail);
    let my0 = cy;
    let tx1 = header_row.x as i32;
    let ty1 = header_row.y as i32;

    if t < MS_FLY {
        let u = smoothstep(0.0, 1.0, t as f32 / MS_FLY as f32);
        // Hold ended with `m` at `mx0`. Header row renders ` mimicode` with a leading space at `tx1`,
        // so align by interpolating the left edge from `mx0 - 1` (virtual space before `m`) to `tx1`.
        let left = lerp_i(mx0 - 1, tx1, u);
        let my = lerp_i(my0, ty1, u);

        let mut spans: Vec<Span<'static>> = Vec::new();
        for ch in HEADER_LABEL.chars() {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ));
        }
        draw_spans(frame, area, left, my, spans);
        return true;
    }

    *intro = IntroState::Done;
    false
}

fn total_duration() -> Duration {
    let morphs = (MS_MORPH + MS_HOLD_WORD) * (TRAILINGS.len() - 1) as u64;
    Duration::from_millis(MS_EMPTY + MS_M_SLIDE + MS_MINIMAL_BUILD + MS_HOLD_WORD + morphs + MS_FLY + 40)
}

fn centered_m_x(cx: i32, trailing: &str) -> i32 {
    let full_w = (1 + trailing.chars().count()) as i32;
    cx - full_w / 2
}

fn draw_m_only(frame: &mut ratatui::Frame, area: Rect, mx: i32, my: i32) {
    let spans = vec![Span::styled(
        "m",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )];
    draw_spans(frame, area, mx, my, spans);
}

fn draw_word(frame: &mut ratatui::Frame, area: Rect, mx: i32, my: i32, trailing: &str, trail_opacity: f32) {
    let mut spans = vec![Span::styled(
        "m",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )];
    for ch in trailing.chars() {
        spans.push(Span::styled(
            ch.to_string(),
            Style::default()
                .fg(fade_fg(trail_opacity))
                .add_modifier(Modifier::BOLD),
        ));
    }
    draw_spans(frame, area, mx, my, spans);
}

fn morph_trailing(
    frame: &mut ratatui::Frame,
    area: Rect,
    cx: i32,
    cy: i32,
    from_idx: usize,
    to_idx: usize,
    u: f32,
) {
    let old_tr = TRAILINGS[from_idx];
    let new_tr = TRAILINGS[to_idx];

    let w_old = (1 + old_tr.chars().count()) as i32;
    let w_new = (1 + new_tr.chars().count()) as i32;
    let mx_old = cx - w_old / 2;
    let mx_new = cx - w_new / 2;
    let mx = lerp_i(mx_old, mx_new, smoothstep(0.0, 1.0, u));

    let mut spans = vec![Span::styled(
        "m",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )];

    const SPLIT: f32 = 0.42;
    if u < SPLIT {
        // Outgoing trailing dims out; `m` stays solid.
        let v = smoothstep(0.0, 1.0, u / SPLIT);
        let old_a = 1.0 - v;
        for ch in old_tr.chars() {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(fade_fg(old_a))
                    .add_modifier(Modifier::BOLD | Modifier::DIM),
            ));
        }
    } else {
        // Incoming trailing fades in left-to-right.
        let v = smoothstep(0.0, 1.0, (u - SPLIT) / (1.0 - SPLIT));
        let n = new_tr.chars().count();
        for (i, ch) in new_tr.chars().enumerate() {
            let stagger = v * (n + 2) as f32 - i as f32;
            let fa = stagger.clamp(0.0, 1.0);
            spans.push(Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(fade_fg(fa))
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    draw_spans(frame, area, mx, cy, spans);
}

fn draw_spans(frame: &mut ratatui::Frame, area: Rect, mx: i32, my: i32, spans: Vec<Span<'static>>) {
    let line: Line<'static> = Line::from(spans);
    let para = Paragraph::new(line);
    let x = mx.clamp(area.x as i32, area.x as i32 + area.width as i32 - 1).max(0) as u16;
    let y = my.clamp(area.y as i32, area.y as i32 + area.height as i32 - 1).max(0) as u16;
    let rect = Rect {
        x,
        y,
        width: area.width.saturating_sub(x.saturating_sub(area.x)),
        height: 1,
    };
    frame.render_widget(para, rect);
}

fn fade_fg(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    if t < 0.33 {
        Color::DarkGray
    } else if t < 0.66 {
        Color::Gray
    } else {
        Color::White
    }
}

fn lerp_i(a: i32, b: i32, t: f32) -> i32 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b - a) as f32 * t).round() as i32
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
