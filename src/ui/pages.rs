use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use super::color::{self, Hsv};
use super::widgets::{self, Col};
use super::{App, Conn, Field, Hit, InputTarget, PickTarget, theme};

/// The five rows of an inline color picker.
pub fn picker(app: &App, buf: &mut Buffer, col: &mut Col, t: PickTarget, hsv: Hsv) {
    let editing = app.editing(InputTarget::Hex(t));
    for row in color::ROWS {
        let f = Field::Pick(t, row);
        if let Some(r) = col.field(f) {
            let rail = color::render_row(buf, r, hsv, row, app.focused() == Some(f), editing);
            if row.is_rail() {
                col.hit(rail, Hit::Rail(f, rail));
            }
        }
    }
}

fn server_fields(app: &App, buf: &mut Buffer, col: &mut Col) {
    let focus = app.focused();
    if let Some(r) = col.field(Field::Host) {
        let rest = widgets::label(buf, r, "Host", focus == Some(Field::Host));
        widgets::text_value(
            buf,
            rest,
            &app.settings.host,
            app.editing(InputTarget::Host),
            "",
        );
    }
    if let Some(r) = col.field(Field::Port) {
        let rest = widgets::label(buf, r, "Port", focus == Some(Field::Port));
        widgets::text_value(
            buf,
            rest,
            &app.settings.port.to_string(),
            app.editing(InputTarget::Port),
            "",
        );
    }
    if let Some(r) = col.field(Field::AutoConnect) {
        widgets::checkbox(
            buf,
            r,
            "Connect on startup",
            app.settings.auto_connect,
            focus == Some(Field::AutoConnect),
        );
    }
}

fn hint_for(app: &App, f: Field, r: Rect, buf: &mut Buffer, text: &str) {
    if app.focused() == Some(f) && app.input.is_none() {
        Line::styled(text.to_string(), theme::faint())
            .right_aligned()
            .render(r, buf);
    }
}

// ---------------------------------------------------------------------------
// Connect
// ---------------------------------------------------------------------------

pub fn connect_screen(app: &App, buf: &mut Buffer, area: Rect) -> (Col, u16) {
    let connecting = app.conn == Conn::Connecting;
    let r = widgets::centered(area, 62, 20);
    let block = theme::panel("", true);
    let inner = block.inner(r).inner(Margin::new(2, 1));
    block.render(r, buf);

    let phase = if connecting { app.time() * 240.0 } else { 0.0 };
    let line = |y: u16| Rect::new(inner.x, inner.y + y, inner.width, 1);
    theme::rainbow("OpenRGB", phase)
        .centered()
        .render(line(0), buf);
    Line::styled("Connect to a running OpenRGB SDK server", theme::dim())
        .centered()
        .render(line(1), buf);

    let form = Rect::new(inner.x, inner.y + 3, inner.width, inner.height - 3);
    let mut col = Col::new(form, 0);
    server_fields(app, buf, &mut col);
    col.gap();
    if let Some(r) = col.fields(&[Field::Connect]) {
        if connecting {
            const SPIN: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let s = SPIN[(app.time() * 12.0) as usize % SPIN.len()];
            Line::from(vec![
                Span::styled(format!("  {s} "), theme::accent()),
                Span::raw("Connecting…"),
            ])
            .render(r, buf);
        } else {
            widgets::buttons(
                &mut col,
                buf,
                r,
                &[(Field::Connect, "Connect")],
                app.focused(),
            );
        }
    }
    col.gap();

    let rest = Rect::new(
        form.x,
        form.y + col.height() as u16,
        form.width,
        form.height.saturating_sub(col.height() as u16),
    );
    let mut text = Vec::new();
    if let Conn::Disconnected(Some(err)) = &app.conn {
        text.push(Line::from(vec![
            Span::styled("✖ ", Style::new().fg(theme::DANGER)),
            Span::raw(err.clone()),
        ]));
        text.push(Line::raw(""));
    }
    text.push(Line::styled(
        "Start OpenRGB with --server, or enable the server in OpenRGB's \"SDK Server\" tab. \
         The default port is 6742.",
        theme::faint(),
    ));
    Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .render(rest, buf);
    (col, form.height)
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

pub fn overview(app: &App, buf: &mut Buffer, col: &mut Col) {
    let focus = app.focused();
    col.section(buf, "Sync all devices");
    picker(app, buf, col, PickTarget::Sync, app.sync_color);
    if let Some(r) = col.fields(&[Field::ApplyAll, Field::AllOff]) {
        widgets::buttons(
            col,
            buf,
            r,
            &[
                (Field::ApplyAll, "Apply to all"),
                (Field::AllOff, "All off"),
            ],
            focus,
        );
    }
    if let Some(r) = col.line() {
        Span::styled(
            "  Switches every device with per-LED control to its direct mode.",
            theme::faint(),
        )
        .render(r, buf);
    }

    col.section(buf, "Devices");
    if app.devices.is_empty()
        && let Some(r) = col.line()
    {
        Span::styled("  No devices reported by the server.", theme::faint()).render(r, buf);
    }
    for (i, d) in app.devices.iter().enumerate() {
        let f = Field::OpenDevice(i);
        let focused = focus == Some(f);
        if let Some(r) = col.field(f) {
            let tint = theme::led(theme::accent_of(&d.colors));
            let vendor = if d.vendor.is_empty() {
                d.kind.label().to_string()
            } else {
                format!("{} · {}", d.vendor, d.kind.label())
            };
            Line::from(vec![
                Span::styled(if focused { "▸ " } else { "  " }, theme::accent()),
                Span::styled("● ", Style::new().fg(tint)),
                Span::styled(
                    d.name.clone(),
                    if focused {
                        theme::accent().bold()
                    } else {
                        theme::strong()
                    },
                ),
                Span::styled(format!("  {vendor}"), theme::dim()),
            ])
            .render(r, buf);
            let mode = d.active_mode().map(|m| m.name.as_str()).unwrap_or("—");
            Line::from(vec![
                Span::styled(format!(" {mode} "), theme::accent()),
                Span::styled(format!(" {} LEDs ", d.colors.len()), theme::faint()),
            ])
            .right_aligned()
            .render(r, buf);
        }
        if let Some(r) = col.line() {
            let bar = Rect::new(r.x + 4, r.y, r.width.saturating_sub(4), 1);
            widgets::led_bar(buf, bar, &d.colors);
            col.hit(r, Hit::Field(f));
        }
    }
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

pub fn profiles(app: &App, buf: &mut Buffer, col: &mut Col) {
    let focus = app.focused();
    if app.proto() < 2 {
        col.section(buf, "Profiles unavailable");
        if let Some(r) = col.line() {
            Span::styled(
                "  This server's SDK protocol doesn't support profiles. Update OpenRGB to use them.",
                theme::dim(),
            )
            .render(r, buf);
        }
        return;
    }

    col.section(buf, "Save current lighting");
    if let Some(r) = col.field(Field::NewProfile) {
        let rest = widgets::label(buf, r, "Name", focus == Some(Field::NewProfile));
        widgets::text_value(
            buf,
            rest,
            "",
            app.editing(InputTarget::NewProfile),
            if focus == Some(Field::NewProfile) {
                "Press Enter to name a new profile"
            } else {
                "—"
            },
        );
    }
    if let Some(r) = col.line() {
        Span::styled(
            "  Saving under an existing name overwrites that profile. Profiles are stored by the server.",
            theme::faint(),
        )
        .render(r, buf);
    }

    col.section(buf, "Saved profiles");
    if app.profiles.is_empty()
        && let Some(r) = col.line()
    {
        Span::styled(
            "  No profiles yet. Save your current lighting to create one.",
            theme::faint(),
        )
        .render(r, buf);
    }
    for (i, name) in app.profiles.iter().enumerate() {
        let f = Field::Profile(i);
        let focused = focus == Some(f);
        if let Some(r) = col.field(f) {
            Line::from(vec![
                Span::styled(if focused { "▸ " } else { "  " }, theme::accent()),
                Span::styled("■ ", theme::accent()),
                Span::styled(
                    name.clone(),
                    if focused {
                        theme::accent().bold()
                    } else {
                        theme::text()
                    },
                ),
            ])
            .render(r, buf);
            hint_for(app, f, r, buf, "Enter load · d delete");
        }
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

pub fn settings(app: &App, buf: &mut Buffer, col: &mut Col) {
    col.section(buf, "SDK server");
    server_fields(app, buf, col);
    if let Some(r) = col.fields(&[Field::Connect, Field::Disconnect, Field::Rescan]) {
        widgets::buttons(
            col,
            buf,
            r,
            &[
                (Field::Connect, "Reconnect"),
                (Field::Disconnect, "Disconnect"),
                (Field::Rescan, "Rescan devices"),
            ],
            app.focused(),
        );
    }

    col.section(buf, "Server");
    if let Conn::Connected {
        proto,
        server_proto,
    } = app.conn
    {
        let rows = [
            (
                "Address",
                format!("{}:{}", app.settings.host, app.settings.port),
            ),
            ("Server protocol", format!("v{server_proto}")),
            ("Negotiated", format!("v{proto}")),
            ("Devices", app.devices.len().to_string()),
            ("Profiles", app.profiles.len().to_string()),
        ];
        for (k, v) in rows {
            if let Some(r) = col.line() {
                Line::from(vec![
                    Span::styled(format!("  {k:<18}"), theme::dim()),
                    Span::raw(v),
                ])
                .render(r, buf);
            }
        }
    }
}
