use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use openrgb_tui::protocol::{ColorMode, Controller, Mode, Rgb, ZoneType, mode_flags};

use super::color::{Hsv, Row};
use super::pages::picker;
use super::widgets::{self, Col};
use super::{App, ColorTarget, DeviceTab, Field, Hit, PickTarget, Popup, theme};

/// Device summary, LED strip and tab bar. Returns the area below them.
pub fn header(app: &mut App, buf: &mut Buffer, area: Rect, dev: usize) -> Rect {
    let d = &app.devices[dev];
    let mut sub = vec![d.kind.label().to_string()];
    if !d.vendor.is_empty() {
        sub.insert(0, d.vendor.clone());
    }
    sub.push(format!("{} LEDs", d.colors.len()));
    sub.push(format!("{} zones", d.zones.len()));
    let line = |y: u16| Rect::new(area.x, area.y + y, area.width, 1);
    Span::styled(sub.join("  ·  "), theme::dim()).render(line(0), buf);
    widgets::led_bar(buf, line(1), &d.colors);

    let tabs = line(3);
    let mut x = tabs.x;
    for (i, t) in DeviceTab::ALL.into_iter().enumerate() {
        let label = format!(" {} {} ", i + 1, t.label());
        let w = label.chars().count() as u16;
        let style = if t == app.tab {
            Style::new()
                .fg(ratatui::style::Color::White)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            theme::dim()
        };
        let r = Rect::new(x, tabs.y, w.min(tabs.right().saturating_sub(x)), 1);
        Span::styled(label, style).render(r, buf);
        app.hits.push((r, Hit::Tab(t)));
        x += w + 1;
    }
    Rect::new(
        area.x,
        area.y + 5,
        area.width,
        area.height.saturating_sub(5),
    )
}

pub fn page(app: &App, buf: &mut Buffer, col: &mut Col, dev: usize) {
    match app.tab {
        DeviceTab::Lighting => lighting(app, buf, col, dev),
        DeviceTab::Zones => zones(app, buf, col, dev),
        DeviceTab::Info => info(app, buf, col, dev),
        DeviceTab::Leds => {}
    }
}

// ---------------------------------------------------------------------------
// Lighting tab
// ---------------------------------------------------------------------------

/// The color modes a mode offers, with their labels.
pub fn color_modes(m: &Mode) -> Vec<(ColorMode, &'static str)> {
    let mut opts = Vec::new();
    if m.has(mode_flags::HAS_PER_LED_COLOR) {
        opts.push((ColorMode::PerLed, "Per LED"));
    }
    if m.has(mode_flags::HAS_MODE_SPECIFIC_COLOR) {
        opts.push((ColorMode::ModeSpecific, "Custom"));
    }
    if m.has(mode_flags::HAS_RANDOM_COLOR) {
        opts.push((ColorMode::Random, "Random"));
    }
    opts
}

fn lighting(app: &App, buf: &mut Buffer, col: &mut Col, dev: usize) {
    let focus = app.focused();
    let d = &app.devices[dev];
    let active = usize::try_from(d.active_mode).ok();

    col.section(buf, "Mode");
    for (i, m) in d.modes.iter().enumerate() {
        let f = Field::Mode(i);
        let focused = focus == Some(f);
        let Some(r) = col.field(f) else { continue };
        let on = active == Some(i);
        Line::from(vec![
            Span::styled(if focused { "▸ " } else { "  " }, theme::accent()),
            Span::styled(
                if on { "◉ " } else { "○ " },
                if on { theme::accent() } else { theme::faint() },
            ),
            Span::styled(
                m.name.clone(),
                match (focused, on) {
                    (true, _) => theme::accent().bold(),
                    (false, true) => theme::strong(),
                    _ => theme::text(),
                },
            ),
            Span::styled(
                if m.is_direct_like() { "  per-LED" } else { "" },
                theme::faint(),
            ),
        ])
        .render(r, buf);
    }

    let Some(mode) = d.active_mode() else {
        return;
    };
    let dirs = mode.directions();
    let has_settings =
        mode.has(mode_flags::HAS_SPEED) || mode.has(mode_flags::HAS_BRIGHTNESS) || !dirs.is_empty();
    let opts = color_modes(mode);

    if has_settings {
        col.section(buf, "Parameters");
        let mut slider = |f: Field, label: &str, v: u32, range: (u32, u32)| {
            if let Some(r) = col.field(f) {
                let rest = widgets::label(buf, r, label, focus == Some(f));
                let rail = widgets::slider(buf, rest, v, range, "", focus == Some(f));
                col.hit(rail, Hit::Rail(f, rail));
            }
        };
        if mode.has(mode_flags::HAS_SPEED) {
            slider(
                Field::Speed,
                "Speed",
                mode.speed,
                (mode.speed_min, mode.speed_max),
            );
        }
        if mode.has(mode_flags::HAS_BRIGHTNESS) {
            slider(
                Field::Brightness,
                "Brightness",
                mode.brightness,
                (mode.brightness_min, mode.brightness_max),
            );
        }
        if !dirs.is_empty()
            && let Some(r) = col.field(Field::Direction)
        {
            let focused = focus == Some(Field::Direction);
            let rest = widgets::label(buf, r, "Direction", focused);
            let labels: Vec<&str> = dirs.iter().map(|d| d.label()).collect();
            let cur = dirs.iter().position(|d| *d == mode.direction).unwrap_or(0);
            widgets::choice(buf, rest, &labels, cur, focused);
        }
    }

    if !opts.is_empty() {
        col.section(buf, "Color");
        if opts.len() > 1
            && let Some(r) = col.field(Field::ColorMode)
        {
            let focused = focus == Some(Field::ColorMode);
            let rest = widgets::label(buf, r, "Colors", focused);
            let labels: Vec<&str> = opts.iter().map(|o| o.1).collect();
            let cur = opts
                .iter()
                .position(|o| o.0 == mode.color_mode)
                .unwrap_or(0);
            widgets::choice(buf, rest, &labels, cur, focused);
        }
        match mode.color_mode {
            ColorMode::PerLed => {
                picker(app, buf, col, PickTarget::Paint, app.settings.paint);
                if let Some(r) = col.fields(&[Field::ApplyPaint, Field::TurnOff]) {
                    widgets::buttons(
                        col,
                        buf,
                        r,
                        &[
                            (Field::ApplyPaint, "Apply color"),
                            (Field::TurnOff, "Turn off"),
                        ],
                        focus,
                    );
                }
                note(
                    col,
                    buf,
                    "Fills the whole device live. Use the LEDs tab to color single LEDs or keys.",
                );
            }
            ColorMode::ModeSpecific => mode_colors(app, buf, col, mode),
            ColorMode::Random => note(col, buf, "The device picks colors at random."),
            ColorMode::None => note(col, buf, "This mode uses built-in colors."),
        }
    }

    if mode.has(mode_flags::MANUAL_SAVE) && app.proto() >= 3 {
        col.gap();
        if let Some(r) = col.fields(&[Field::SaveToDevice]) {
            widgets::buttons(
                col,
                buf,
                r,
                &[(Field::SaveToDevice, "Save to device")],
                focus,
            );
        }
        note(
            col,
            buf,
            "Writes this mode to the device's onboard memory so it persists without OpenRGB.",
        );
    }
}

fn note(col: &mut Col, buf: &mut Buffer, text: &str) {
    if let Some(r) = col.line() {
        Span::styled(format!("  {text}"), theme::faint()).render(r, buf);
    }
}

fn mode_colors(app: &App, buf: &mut Buffer, col: &mut Col, mode: &Mode) {
    let focus = app.focused();
    for (i, &c) in mode.colors.iter().enumerate() {
        let f = Field::ModeColor(i);
        let focused = focus == Some(f);
        if let Some(r) = col.field(f) {
            let rest = widgets::label(buf, r, &format!("Color {}", i + 1), focused);
            let mut spans = vec![
                Span::styled("        ", Style::new().bg(theme::rgb(c))),
                Span::raw(format!(" {}", c.hex())),
            ];
            if focused {
                spans.push(Span::styled(
                    "   Enter edit · + add · - remove",
                    theme::faint(),
                ));
            }
            Line::from(spans).render(rest, buf);
        }
    }
    let len = mode.colors.len() as u32;
    let mut items = Vec::new();
    if len < mode.colors_max {
        items.push((Field::AddColor, "+ Add color"));
    }
    if len > mode.colors_min.max(1) {
        items.push((Field::RemoveColor, "- Remove last"));
    }
    if !items.is_empty() {
        let fs: Vec<Field> = items.iter().map(|i| i.0).collect();
        if let Some(r) = col.fields(&fs) {
            widgets::buttons(col, buf, r, &items, focus);
        }
    }
}

// ---------------------------------------------------------------------------
// LEDs tab
// ---------------------------------------------------------------------------

/// One drawable LED in map coordinates: `x` is a column offset into the map,
/// `y` a virtual row (before scrolling).
#[derive(Clone, Debug)]
pub struct LedCell {
    pub led: usize,
    pub zone: usize,
    pub x: u16,
    pub y: i32,
    pub w: u16,
}

struct MapLayout {
    cells: Vec<LedCell>,
    headers: Vec<(usize, i32)>,
    height: i32,
}

/// Matrix zones become a key grid; everything else a wrapped row of LEDs.
/// Each cell is followed by a one-column gap that holds the cursor brackets.
fn layout(d: &Controller, width: u16) -> MapLayout {
    let mut cells = Vec::new();
    let mut headers = Vec::new();
    let mut y = 0i32;
    let offsets = d.zone_offsets();
    let usable = width.saturating_sub(1).max(3);
    for (zi, z) in d.zones.iter().enumerate() {
        if z.leds_count == 0 {
            continue;
        }
        headers.push((zi, y));
        y += 1;
        let base = offsets[zi];
        match &z.matrix {
            Some(m) if z.kind == ZoneType::Matrix && m.width > 0 => {
                let cw = (usable / m.width as u16).clamp(2, 6);
                for row in 0..m.height {
                    for c in 0..m.width {
                        if let Some(i) = m.get(row, c)
                            && i < z.leds_count
                        {
                            cells.push(LedCell {
                                led: base + i as usize,
                                zone: zi,
                                x: 1 + c as u16 * cw,
                                y: y + row as i32,
                                w: cw - 1,
                            });
                        }
                    }
                }
                y += m.height as i32;
            }
            _ => {
                let cw = 3u16;
                let per_row = (usable / cw).max(1) as usize;
                let n = z.leds_count as usize;
                for i in 0..n {
                    cells.push(LedCell {
                        led: base + i,
                        zone: zi,
                        x: 1 + (i % per_row) as u16 * cw,
                        y: y + (i / per_row) as i32,
                        w: cw - 1,
                    });
                }
                y += n.div_ceil(per_row) as i32;
            }
        }
        y += 1;
    }
    cells.retain(|c| c.led < d.colors.len());
    MapLayout {
        cells,
        headers,
        height: y,
    }
}

pub struct LedsLaid {
    pub cells: Vec<LedCell>,
    pub hits: Vec<(Rect, Hit)>,
    pub cursor_y: Option<i32>,
    pub view_h: u16,
    pub total_h: i32,
}

fn key_label(name: &str) -> &str {
    name.strip_prefix("Key: ").unwrap_or(name)
}

pub fn draw_leds(app: &App, buf: &mut Buffer, area: Rect, dev: usize) -> LedsLaid {
    let d = &app.devices[dev];
    let mut hits = Vec::new();
    if d.colors.is_empty() {
        Span::styled("This device exposes no addressable LEDs.", theme::dim()).render(area, buf);
        return LedsLaid {
            cells: Vec::new(),
            hits,
            cursor_y: None,
            view_h: area.height,
            total_h: 0,
        };
    }
    let [bar, _, map] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);

    let lay = layout(d, map.width);
    let scroll = app.led_scroll;
    let visible = |y: i32| {
        let sy = y - scroll;
        (sy >= 0 && sy < map.height as i32).then(|| map.y + sy as u16)
    };

    let offsets = d.zone_offsets();
    for &(zi, y) in &lay.headers {
        if let Some(sy) = visible(y) {
            let z = &d.zones[zi];
            let start = offsets[zi];
            let sel = (start..start + z.leds_count as usize)
                .filter(|i| app.selection.contains(i))
                .count();
            let mut spans = vec![
                Span::styled(z.name.clone(), theme::strong()),
                Span::styled(format!("  {} LEDs", z.leds_count), theme::faint()),
            ];
            if sel > 0 {
                spans.push(Span::styled(format!(" · {sel} selected"), theme::accent()));
            }
            Line::from(spans).render(Rect::new(map.x, sy, map.width, 1), buf);
        }
    }

    let mut cursor_y = None;
    for cell in &lay.cells {
        if cell.led == app.led_cursor {
            cursor_y = Some(cell.y);
        }
        let Some(sy) = visible(cell.y) else { continue };
        let x = map.x + cell.x;
        if x + cell.w > map.right() {
            continue;
        }
        let c = d.colors[cell.led];
        let selected = app.selection.contains(&cell.led);
        let label = d.leds.get(cell.led).map_or("", |l| key_label(&l.name));
        let show_label = cell.w >= 3 && label.chars().count() <= cell.w as usize;
        let mut text: Vec<char> = vec![' '; cell.w as usize];
        if show_label {
            let lw = label.chars().count();
            let at = (cell.w as usize - lw) / 2;
            for (i, ch) in label.chars().enumerate() {
                text[at + i] = ch;
            }
        } else if selected {
            text[(cell.w as usize - 1) / 2] = '•';
        }
        let mut style = Style::new().bg(theme::led(c)).fg(theme::contrast(c));
        if selected {
            style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        }
        for (i, ch) in text.into_iter().enumerate() {
            if let Some(cl) = buf.cell_mut((x + i as u16, sy)) {
                cl.set_char(ch).set_style(style);
            }
        }
        if selected && show_label {
            // Underline alone is easy to miss on short labels; add a corner tick.
            if let Some(cl) = buf.cell_mut((x + cell.w - 1, sy))
                && cl.symbol() == " "
            {
                cl.set_char('•');
            }
        }
        if cell.led == app.led_cursor {
            let bracket = Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD);
            if let Some(cl) = buf.cell_mut((x - 1, sy)) {
                cl.set_char('[').set_style(bracket);
            }
            if let Some(cl) = buf.cell_mut((x + cell.w, sy)) {
                cl.set_char(']').set_style(bracket);
            }
        }
        hits.push((Rect::new(x, sy, cell.w, 1), Hit::Led(cell.led)));
    }

    paint_bar(app, buf, bar, d);
    LedsLaid {
        cells: lay.cells,
        hits,
        cursor_y,
        view_h: map.height,
        total_h: lay.height,
    }
}

/// Paint color, selection and cursor summary above the map.
fn paint_bar(app: &App, buf: &mut Buffer, area: Rect, d: &Controller) {
    let paint = app.settings.paint.rgb();
    let sel = app.selection.len();
    let mut spans = vec![
        Span::styled("Paint ", theme::dim()),
        Span::styled("      ", Style::new().bg(theme::rgb(paint))),
        Span::raw(format!(" {}   ", paint.hex())),
        if sel == 0 {
            Span::styled("No LEDs selected", theme::dim())
        } else {
            Span::styled(
                format!("{sel} of {} selected", d.colors.len()),
                theme::accent(),
            )
        },
    ];
    Line::from(std::mem::take(&mut spans)).render(Rect { height: 1, ..area }, buf);
    if let Some(l) = d.leds.get(app.led_cursor)
        && let Some(&c) = d.colors.get(app.led_cursor)
    {
        spans.extend([
            Span::styled(l.name.clone(), theme::text()),
            Span::raw(" "),
            Span::styled("  ", Style::new().bg(theme::led(c))),
            Span::styled(format!(" {}", c.hex()), theme::dim()),
        ]);
        Line::from(spans)
            .right_aligned()
            .render(Rect { height: 1, ..area }, buf);
    }
    let keys: &[(&str, &str)] = &[
        ("Space", "select"),
        ("Shift+arrows", "extend"),
        ("Enter", "paint"),
        ("c", "color"),
        ("p", "pick"),
        ("a/n/z", "all/none/zone"),
        ("f", "fill"),
        ("o", "off"),
    ];
    let mut spans = Vec::new();
    for (k, v) in keys {
        spans.push(Span::styled(*k, theme::accent()));
        spans.push(Span::styled(format!(" {v}  "), theme::faint()));
    }
    Line::from(spans).render(Rect::new(area.x, area.y + 1, area.width, 1), buf);
}

#[derive(Clone, Copy)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// The LED reached by moving from `cursor` in `dir` on the drawn map.
fn step(cells: &[LedCell], cursor: usize, dir: Dir) -> Option<usize> {
    let Some(at) = cells.iter().position(|c| c.led == cursor) else {
        return cells.first().map(|c| c.led);
    };
    let cur = &cells[at];
    let center = |c: &LedCell| c.x as i32 * 2 + c.w as i32;
    let cx = center(cur);
    match dir {
        Dir::Left | Dir::Right => {
            let right = matches!(dir, Dir::Right);
            let same_row = cells
                .iter()
                .filter(|c| {
                    c.y == cur.y && c.zone == cur.zone && (c.x > cur.x) == right && c.x != cur.x
                })
                .min_by_key(|c| (c.x as i32 - cur.x as i32).abs());
            // At the end of a row, continue with the next LED in layout order.
            same_row.map(|c| c.led).or_else(|| {
                let i = if right {
                    at.checked_add(1)
                } else {
                    at.checked_sub(1)
                }?;
                cells.get(i).map(|c| c.led)
            })
        }
        Dir::Up | Dir::Down => {
            let down = matches!(dir, Dir::Down);
            cells
                .iter()
                .filter(|c| if down { c.y > cur.y } else { c.y < cur.y })
                .min_by_key(|c| ((c.y - cur.y).abs(), (center(c) - cx).abs()))
                .map(|c| c.led)
        }
    }
}

/// Keys for the LEDs tab. Returns whether the key was used.
pub fn leds_key(app: &mut App, dev: usize, k: KeyEvent) -> bool {
    let n = app.devices[dev].colors.len();
    if n == 0 {
        return false;
    }
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    let dir = match k.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Some(Dir::Up),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Some(Dir::Down),
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => Some(Dir::Left),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => Some(Dir::Right),
        _ => None,
    };
    if let Some(dir) = dir {
        let extend = shift || matches!(k.code, KeyCode::Char(c) if c.is_ascii_uppercase());
        let from = app.led_cursor;
        match step(&app.led_cells, from, dir) {
            Some(to) => {
                if extend {
                    app.selection.insert(from);
                    app.selection.insert(to);
                }
                app.led_cursor = to;
            }
            // Nothing further left: let the caller move focus to the sidebar.
            None if matches!(dir, Dir::Left) => return false,
            None => {}
        }
        return true;
    }

    let paint = app.settings.paint;
    match k.code {
        KeyCode::Char(' ') => {
            let c = app.led_cursor;
            if !app.selection.remove(&c) {
                app.selection.insert(c);
            }
        }
        KeyCode::Enter => app.paint_selection(dev, paint.rgb()),
        KeyCode::Char('c') => {
            app.popup = Some(Popup::Color {
                target: ColorTarget::Paint(dev),
                hsv: paint,
                row: Row::Hue,
            });
        }
        KeyCode::Char('p') => {
            if let Some(&c) = app.devices[dev].colors.get(app.led_cursor) {
                app.settings.paint = Hsv::from_rgb(c);
                app.toast(format!("Paint color set to {}", c.hex()), false);
            }
        }
        KeyCode::Char('a') => app.selection = (0..n).collect(),
        KeyCode::Char('n') => app.selection.clear(),
        KeyCode::Char('z') => {
            let d = &app.devices[dev];
            let offsets = d.zone_offsets();
            if let Some(zi) = (0..d.zones.len())
                .rev()
                .find(|&zi| offsets[zi] <= app.led_cursor)
            {
                let range = offsets[zi]..offsets[zi] + d.zones[zi].leds_count as usize;
                let all = range.clone().all(|i| app.selection.contains(&i));
                for i in range {
                    if all {
                        app.selection.remove(&i);
                    } else {
                        app.selection.insert(i);
                    }
                }
            }
        }
        KeyCode::Char('f') => app.fill_device(dev, paint.rgb()),
        KeyCode::Char('o') => app.paint_selection(dev, Rgb::BLACK),
        KeyCode::Home | KeyCode::Char('g') => {
            if let Some(c) = app.led_cells.first() {
                app.led_cursor = c.led;
            }
        }
        KeyCode::End | KeyCode::Char('G') => {
            if let Some(c) = app.led_cells.last() {
                app.led_cursor = c.led;
            }
        }
        KeyCode::Esc if !app.selection.is_empty() => app.selection.clear(),
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------------------
// Zones tab
// ---------------------------------------------------------------------------

fn zones(app: &App, buf: &mut Buffer, col: &mut Col, dev: usize) {
    let focus = app.focused();
    let d = &app.devices[dev];
    if d.zones.is_empty() {
        note(col, buf, "This device has no zones.");
        return;
    }
    let offsets = d.zone_offsets();
    for (zi, z) in d.zones.iter().enumerate() {
        if zi > 0 {
            col.gap();
        }
        let f = Field::ZoneFill(zi);
        let focused = focus == Some(f);
        if let Some(r) = col.field(f) {
            let kind = match z.kind {
                ZoneType::Single => "Single",
                ZoneType::Linear => "Linear",
                ZoneType::Matrix => "Matrix",
            };
            let mut s = format!("  {kind} · {} LEDs", z.leds_count);
            if let Some(m) = &z.matrix {
                s += &format!(" · {}×{}", m.width, m.height);
            }
            if !z.segments.is_empty() {
                s += &format!(" · {} segments", z.segments.len());
            }
            Line::from(vec![
                Span::styled(if focused { "▸ " } else { "  " }, theme::accent()),
                Span::styled(
                    z.name.clone(),
                    if focused {
                        theme::accent().bold()
                    } else {
                        theme::strong()
                    },
                ),
                Span::styled(s, theme::dim()),
            ])
            .render(r, buf);
            Line::from(widgets::button("Fill…", focused))
                .right_aligned()
                .render(r, buf);
        }
        if let Some(r) = col.line() {
            let start = offsets[zi].min(d.colors.len());
            let end = (start + z.leds_count as usize).min(d.colors.len());
            widgets::led_bar(
                buf,
                Rect::new(r.x + 2, r.y, r.width.saturating_sub(2), 1),
                &d.colors[start..end],
            );
        }
        if z.resizable() {
            let f = Field::ZoneSize(zi);
            let focused = focus == Some(f);
            if let Some(r) = col.field(f) {
                let size = *app.resize.get(&(dev, zi)).unwrap_or(&z.leds_count);
                let rest = widgets::label(buf, r, "Size", focused);
                let rail =
                    widgets::slider(buf, rest, size, (z.leds_min, z.leds_max), " LEDs", focused);
                col.hit(rail, Hit::Rail(f, rail));
                if size != z.leds_count {
                    Line::styled("Enter to resize ", theme::accent())
                        .right_aligned()
                        .render(r, buf);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Info tab
// ---------------------------------------------------------------------------

pub fn info_rows(d: &Controller) -> Vec<(&'static str, String)> {
    vec![
        ("Name", d.name.clone()),
        ("Vendor", d.vendor.clone()),
        ("Type", d.kind.label().to_string()),
        ("Description", d.description.clone()),
        ("Version", d.version.clone()),
        ("Serial", d.serial.clone()),
        ("Location", d.location.clone()),
        ("Modes", d.modes.len().to_string()),
        ("Zones", d.zones.len().to_string()),
        ("LEDs", d.colors.len().to_string()),
    ]
}

fn info(app: &App, buf: &mut Buffer, col: &mut Col, dev: usize) {
    let focus = app.focused();
    for (i, (k, v)) in info_rows(&app.devices[dev]).into_iter().enumerate() {
        let f = Field::InfoRow(i);
        let focused = focus == Some(f);
        if let Some(r) = col.field(f) {
            let rest = widgets::label(buf, r, k, focused);
            let mut spans = vec![if v.is_empty() {
                Span::styled("—", theme::faint())
            } else {
                Span::raw(v.clone())
            }];
            if focused && !v.is_empty() {
                spans.push(Span::styled("   Enter to copy", theme::faint()));
            }
            Line::from(spans).render(rest, buf);
        }
    }
}
