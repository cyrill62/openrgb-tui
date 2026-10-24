//! HSV color model and the keyboard-driven color picker.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use openrgb_tui::protocol::Rgb;

use super::theme;

/// HSV kept separately from RGB so the hue survives while saturation or
/// value is zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    pub fn rgb(self) -> Rgb {
        Rgb::from_hsv(self.h, self.s, self.v)
    }

    pub fn from_rgb(c: Rgb) -> Self {
        let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        let s = if max == 0.0 { 0.0 } else { d / max };
        Self { h, s, v: max }
    }
}

pub const PRESETS: [Rgb; 12] = [
    Rgb::new(255, 0, 0),
    Rgb::new(255, 96, 0),
    Rgb::new(255, 200, 0),
    Rgb::new(120, 255, 0),
    Rgb::new(0, 255, 90),
    Rgb::new(0, 255, 220),
    Rgb::new(0, 150, 255),
    Rgb::new(40, 40, 255),
    Rgb::new(140, 0, 255),
    Rgb::new(255, 0, 200),
    Rgb::new(255, 255, 255),
    Rgb::new(0, 0, 0),
];

/// One line of the picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Hue,
    Sat,
    Val,
    Preset,
    Hex,
}

pub const ROWS: [Row; 5] = [Row::Hue, Row::Sat, Row::Val, Row::Preset, Row::Hex];

impl Row {
    fn label(self) -> &'static str {
        match self {
            Row::Hue => "Hue",
            Row::Sat => "Saturation",
            Row::Val => "Brightness",
            Row::Preset => "Presets",
            Row::Hex => "Hex",
        }
    }

    /// Whether the row is a gradient that mouse clicks can set.
    pub fn is_rail(self) -> bool {
        matches!(self, Row::Hue | Row::Sat | Row::Val | Row::Preset)
    }
}

/// Nudges `row` by one step in `dir` (±1). Returns `true` if the color changed.
pub fn adjust(hsv: &mut Hsv, row: Row, dir: i32, big: bool) -> bool {
    let before = *hsv;
    let d = dir as f32;
    match row {
        Row::Hue => {
            hsv.h = (hsv.h + d * if big { 30.0 } else { 5.0 }).rem_euclid(360.0);
            wake(hsv);
        }
        Row::Sat => hsv.s = (hsv.s + d * if big { 0.1 } else { 0.02 }).clamp(0.0, 1.0),
        Row::Val => hsv.v = (hsv.v + d * if big { 0.1 } else { 0.02 }).clamp(0.0, 1.0),
        Row::Preset => {
            let n = PRESETS.len() as i32;
            let next = match PRESETS.iter().position(|&p| p == hsv.rgb()) {
                Some(i) => (i as i32 + dir).rem_euclid(n),
                None if dir > 0 => 0,
                None => n - 1,
            };
            *hsv = Hsv::from_rgb(PRESETS[next as usize]);
        }
        Row::Hex => {}
    }
    *hsv != before
}

/// Sets `row` from a position `t` in `0..=1` along its rail.
pub fn set_fraction(hsv: &mut Hsv, row: Row, t: f32) -> bool {
    let before = *hsv;
    let t = t.clamp(0.0, 1.0);
    match row {
        Row::Hue => {
            hsv.h = t * 359.9;
            wake(hsv);
        }
        Row::Sat => hsv.s = t,
        Row::Val => hsv.v = t,
        Row::Preset => {
            let i = (t * PRESETS.len() as f32).floor() as usize;
            *hsv = Hsv::from_rgb(PRESETS[i.min(PRESETS.len() - 1)]);
        }
        Row::Hex => {}
    }
    *hsv != before
}

/// Picking a hue on a grey or black color would otherwise show no change.
fn wake(hsv: &mut Hsv) {
    if hsv.s < 0.05 && hsv.v > 0.0 {
        hsv.s = 1.0;
    }
    if hsv.v < 0.05 {
        hsv.v = 1.0;
    }
}

pub const LABEL_W: u16 = 15;
const VALUE_W: u16 = 6;

/// Renders one picker row into a 1-line `area`. `hex_edit` replaces the hex
/// value while it is being typed. Returns the rail's rect for mouse hits.
pub fn render_row(
    buf: &mut Buffer,
    area: Rect,
    hsv: Hsv,
    row: Row,
    focused: bool,
    hex_edit: Option<&str>,
) -> Rect {
    let label = Line::from(vec![
        Span::styled(if focused { "▸ " } else { "  " }, theme::accent()),
        Span::styled(
            row.label(),
            if focused {
                theme::accent().add_modifier(Modifier::BOLD)
            } else {
                theme::dim()
            },
        ),
    ]);
    label.render(area, buf);

    let x0 = area.x + LABEL_W.min(area.width);
    let w = area.width.saturating_sub(LABEL_W + VALUE_W + 1);
    let rail = Rect::new(x0, area.y, w, 1);
    let at = |t: f32| match row {
        Row::Hue => Rgb::from_hsv(t * 360.0, 1.0, 1.0),
        Row::Sat => Rgb::from_hsv(hsv.h, t, 1.0),
        _ => Rgb::from_hsv(hsv.h, hsv.s, t),
    };
    match row {
        Row::Hue | Row::Sat | Row::Val => {
            let pos = match row {
                Row::Hue => hsv.h / 360.0,
                Row::Sat => hsv.s,
                _ => hsv.v,
            };
            let knob = (pos * w.saturating_sub(1) as f32).round() as u16;
            for i in 0..w {
                let t = if w > 1 {
                    i as f32 / (w - 1) as f32
                } else {
                    0.0
                };
                let c = at(t);
                if let Some(cell) = buf.cell_mut((x0 + i, area.y)) {
                    cell.set_style(Style::new().bg(theme::rgb(c)));
                    if i == knob {
                        cell.set_char('┃').set_fg(theme::contrast(c));
                    } else {
                        cell.set_char(' ');
                    }
                }
            }
            let value = match row {
                Row::Hue => format!("{:>4.0}°", hsv.h),
                Row::Sat => format!("{:>4.0}%", hsv.s * 100.0),
                _ => format!("{:>4.0}%", hsv.v * 100.0),
            };
            Span::styled(value, theme::dim())
                .render(Rect::new(x0 + w + 1, area.y, VALUE_W, 1), buf);
        }
        Row::Preset => {
            let current = hsv.rgb();
            let step = (w / PRESETS.len() as u16).clamp(1, 4);
            for (i, &p) in PRESETS.iter().enumerate() {
                let x = x0 + i as u16 * step;
                let cap = step.saturating_sub(1).max(1);
                for j in 0..cap {
                    if let Some(cell) = buf.cell_mut((x + j, area.y)) {
                        cell.set_style(Style::new().bg(theme::led(p)));
                        let mark = p == current && (j == cap / 2);
                        cell.set_char(if mark { '◆' } else { ' ' })
                            .set_fg(theme::contrast(p));
                    }
                }
            }
            return Rect::new(x0, area.y, step * PRESETS.len() as u16, 1);
        }
        Row::Hex => {
            let c = hsv.rgb();
            let mut spans = vec![
                Span::styled("        ", Style::new().bg(theme::rgb(c))),
                " ".into(),
            ];
            match hex_edit {
                Some(s) => {
                    spans.push(Span::styled(
                        s.to_string(),
                        theme::accent().add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled("▏", theme::accent()));
                }
                None => spans.push(Span::styled(c.hex(), theme::text())),
            }
            if focused && hex_edit.is_none() {
                spans.push(Span::styled("  Enter to type", theme::faint()));
            }
            Line::from(spans).render(rail, buf);
        }
    }
    rail
}
