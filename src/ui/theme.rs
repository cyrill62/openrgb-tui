//! Colors and styles. Text uses the terminal's own foreground so the app
//! reads well on light and dark terminal themes alike.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};

use openrgb_tui::protocol::Rgb;

pub const ACCENT: Color = Color::Rgb(0x8B, 0x5C, 0xF6);
pub const SUCCESS: Color = Color::Rgb(0x34, 0xD3, 0x99);
pub const WARN: Color = Color::Rgb(0xFB, 0xBF, 0x24);
pub const DANGER: Color = Color::Rgb(0xF8, 0x71, 0x71);
pub const DIM: Color = Color::Rgb(0x8B, 0x92, 0xA3);
pub const FAINT: Color = Color::Rgb(0x5C, 0x63, 0x73);
pub const BORDER: Color = Color::Rgb(0x3A, 0x40, 0x50);
/// Unlit LEDs are drawn in this color so they stay visible.
const LED_OFF: Color = Color::Rgb(0x22, 0x26, 0x30);

pub fn text() -> Style {
    Style::new()
}

pub fn dim() -> Style {
    Style::new().fg(DIM)
}

pub fn faint() -> Style {
    Style::new().fg(FAINT)
}

pub fn accent() -> Style {
    Style::new().fg(ACCENT)
}

pub fn strong() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

pub fn rgb(c: Rgb) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

pub fn led(c: Rgb) -> Color {
    if c.r.max(c.g).max(c.b) < 18 {
        LED_OFF
    } else {
        rgb(c)
    }
}

/// Black or white, whichever reads better on top of `c`.
pub fn contrast(c: Rgb) -> Color {
    let lum = 0.3 * c.r as f32 + 0.59 * c.g as f32 + 0.11 * c.b as f32;
    if lum > 150.0 {
        Color::Black
    } else {
        Color::White
    }
}

/// The most vivid LED color, used to tint device markers. (An average of a
/// rainbow would be grey.)
pub fn accent_of(colors: &[Rgb]) -> Rgb {
    let vividness = |c: &Rgb| {
        let max = c.r.max(c.g).max(c.b) as u32;
        let min = c.r.min(c.g).min(c.b) as u32;
        (max - min) * 2 + max
    };
    colors
        .iter()
        .copied()
        .max_by_key(vividness)
        .unwrap_or(Rgb::BLACK)
}

pub fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(if focused { ACCENT } else { BORDER }))
        .title(title)
}

/// "OpenRGB" with each letter in a different hue.
pub fn rainbow(text: &str, phase: f32) -> Line<'static> {
    let n = text.chars().count().max(1) as f32;
    Line::from(
        text.chars()
            .enumerate()
            .map(|(i, ch)| {
                let c = Rgb::from_hsv(i as f32 / n * 300.0 + phase, 0.75, 1.0);
                Span::styled(ch.to_string(), Style::new().fg(rgb(c)).bold())
            })
            .collect::<Vec<_>>(),
    )
}
