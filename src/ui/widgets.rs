//! Line-based building blocks shared by the pages.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use openrgb_tui::protocol::Rgb;

use super::theme;
use super::{Field, Hit};

/// Lays out a scrollable column one line at a time.
///
/// Rows are numbered in "virtual" coordinates; only the ones inside the
/// visible window come back as screen rects. The column also records where
/// each focusable field landed, so the app can scroll to the focused one and
/// route mouse clicks.
pub struct Col {
    area: Rect,
    y: i32,
    scroll: i32,
    pub hits: Vec<(Rect, Hit)>,
    /// Virtual row of every field, in layout order.
    pub field_rows: Vec<(Field, i32)>,
}

impl Col {
    pub fn new(area: Rect, scroll: i32) -> Self {
        Self {
            area,
            y: 0,
            scroll,
            hits: Vec::new(),
            field_rows: Vec::new(),
        }
    }

    /// Total virtual height used so far.
    pub fn height(&self) -> i32 {
        self.y
    }

    pub fn line(&mut self) -> Option<Rect> {
        let y = self.y - self.scroll;
        self.y += 1;
        (y >= 0 && y < self.area.height as i32)
            .then(|| Rect::new(self.area.x, self.area.y + y as u16, self.area.width, 1))
    }

    pub fn gap(&mut self) {
        self.y += 1;
    }

    /// A line holding a single field, clickable as a whole.
    pub fn field(&mut self, f: Field) -> Option<Rect> {
        self.field_rows.push((f, self.y));
        let r = self.line();
        if let Some(r) = r {
            self.hits.push((r, Hit::Field(f)));
        }
        r
    }

    /// A line holding several fields side by side (e.g. a row of buttons).
    /// The caller registers a hit for each one.
    pub fn fields(&mut self, fs: &[Field]) -> Option<Rect> {
        for &f in fs {
            self.field_rows.push((f, self.y));
        }
        self.line()
    }

    pub fn hit(&mut self, r: Rect, h: Hit) {
        self.hits.push((r, h));
    }

    pub fn section(&mut self, buf: &mut Buffer, title: &str) {
        if self.height() > 0 {
            self.gap();
        }
        if let Some(r) = self.line() {
            section(buf, r, title);
        }
    }
}

pub fn section(buf: &mut Buffer, r: Rect, title: &str) {
    let rule = "─".repeat((r.width as usize).saturating_sub(title.chars().count() + 1));
    Line::from(vec![
        Span::styled(
            title.to_uppercase(),
            theme::dim().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {rule}"), theme::faint()),
    ])
    .render(r, buf);
}

/// Marker + label at the start of a form row; returns the rest of the line.
pub fn label(buf: &mut Buffer, r: Rect, text: &str, focused: bool) -> Rect {
    let w = super::color::LABEL_W.min(r.width);
    Line::from(vec![
        Span::styled(if focused { "▸ " } else { "  " }, theme::accent()),
        Span::styled(
            text.to_string(),
            if focused {
                theme::accent().add_modifier(Modifier::BOLD)
            } else {
                theme::dim()
            },
        ),
    ])
    .render(Rect::new(r.x, r.y, w, 1), buf);
    Rect::new(r.x + w, r.y, r.width - w, 1)
}

/// A strip of LED colors, resampled to `r`'s width.
pub fn led_bar(buf: &mut Buffer, r: Rect, colors: &[Rgb]) {
    if colors.is_empty() || r.width == 0 {
        return;
    }
    for i in 0..r.width {
        let led = (i as usize * colors.len()) / r.width as usize;
        if let Some(cell) = buf.cell_mut((r.x + i, r.y)) {
            cell.set_char('▀').set_fg(theme::led(colors[led]));
        }
    }
}

/// Renders a slider rail; returns its rect for mouse hits.
pub fn slider(
    buf: &mut Buffer,
    r: Rect,
    v: u32,
    (min, max): (u32, u32),
    suffix: &str,
    focused: bool,
) -> Rect {
    let text = format!(" {v}{suffix}");
    let w = r.width.saturating_sub(text.len() as u16 + 1).min(40);
    let span = max.abs_diff(min).max(1) as f32;
    let t = (v.abs_diff(min) as f32 / span).clamp(0.0, 1.0);
    let filled = (t * w as f32).round() as u16;
    let on = if focused {
        theme::accent()
    } else {
        Style::new().fg(theme::DIM)
    };
    Line::from(vec![
        Span::styled("━".repeat(filled as usize), on),
        Span::styled("─".repeat((w - filled) as usize), theme::faint()),
        Span::styled(
            text,
            if focused {
                theme::strong()
            } else {
                theme::text()
            },
        ),
    ])
    .render(r, buf);
    Rect::new(r.x, r.y, w, 1)
}

/// A horizontal choice like `Left │ Right`, with the current one highlighted.
pub fn choice(buf: &mut Buffer, r: Rect, options: &[&str], current: usize, focused: bool) {
    let mut spans = vec![Span::styled(
        if focused { "‹ " } else { "  " },
        theme::accent(),
    )];
    for (i, o) in options.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", theme::faint()));
        }
        spans.push(if i == current {
            Span::styled(
                *o,
                Style::new()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
        } else {
            Span::styled(*o, theme::dim())
        });
    }
    if focused {
        spans.push(Span::styled(" ›", theme::accent()));
    }
    Line::from(spans).render(r, buf);
}

pub fn button(label: &str, focused: bool) -> Span<'static> {
    let style = if focused {
        Style::new()
            .fg(ratatui::style::Color::White)
            .bg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::ACCENT)
    };
    Span::styled(format!("[ {label} ]"), style)
}

/// Draws buttons left to right on one line, registering a hit for each.
pub fn buttons(
    col: &mut Col,
    buf: &mut Buffer,
    r: Rect,
    items: &[(Field, &str)],
    focus: Option<Field>,
) {
    let mut x = r.x + 2;
    for &(f, label) in items {
        let s = button(label, focus == Some(f));
        let w = (s.width() as u16).min((r.x + r.width).saturating_sub(x));
        let br = Rect::new(x, r.y, w, 1);
        s.render(br, buf);
        col.hit(br, Hit::Field(f));
        x += w + 2;
    }
}

pub fn checkbox(buf: &mut Buffer, r: Rect, label: &str, on: bool, focused: bool) {
    Line::from(vec![
        Span::styled(if focused { "▸ " } else { "  " }, theme::accent()),
        Span::styled(if on { "[x] " } else { "[ ] " }, theme::accent()),
        Span::styled(
            label.to_string(),
            if focused {
                theme::strong()
            } else {
                theme::text()
            },
        ),
    ])
    .render(r, buf);
}

/// Text value, or the live edit buffer with a caret while editing.
pub fn text_value(
    buf: &mut Buffer,
    r: Rect,
    value: &str,
    editing: Option<&str>,
    placeholder: &str,
) {
    let line = match editing {
        Some(s) => Line::from(vec![
            Span::styled(s.to_string(), theme::accent().add_modifier(Modifier::BOLD)),
            Span::styled("▏", theme::accent()),
        ]),
        None if value.is_empty() => Line::styled(placeholder.to_string(), theme::faint()),
        None => Line::raw(value.to_string()),
    };
    line.render(r, buf);
}

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
