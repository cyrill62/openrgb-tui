mod color;
mod device;
mod pages;
mod settings;
mod theme;
mod widgets;

use std::collections::{BTreeSet, HashMap};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget, Wrap};
use ratatui::{DefaultTerminal, Frame};

use openrgb_tui::protocol::{ColorMode, Controller, Mode, Rgb};
use openrgb_tui::worker::{self, Command, Event};

use color::{Hsv, Row};
pub use settings::Settings;
use widgets::Col;

#[derive(Clone, Debug, PartialEq)]
pub enum Conn {
    Disconnected(Option<String>),
    Connecting,
    Connected { proto: u32, server_proto: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Device(usize),
    Profiles,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceTab {
    Lighting,
    Leds,
    Zones,
    Info,
}

impl DeviceTab {
    pub const ALL: [Self; 4] = [Self::Lighting, Self::Leds, Self::Zones, Self::Info];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lighting => "Lighting",
            Self::Leds => "LEDs",
            Self::Zones => "Zones",
            Self::Info => "Info",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Content,
}

/// Which color an inline picker edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickTarget {
    /// The "sync all devices" color on the overview.
    Sync,
    /// The paint color, applied live to the current device.
    Paint,
}

/// Something on a page the keyboard can focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Host,
    Port,
    AutoConnect,
    Connect,
    Disconnect,
    Rescan,
    Pick(PickTarget, Row),
    ApplyAll,
    AllOff,
    OpenDevice(usize),
    Mode(usize),
    Speed,
    Brightness,
    Direction,
    ColorMode,
    ApplyPaint,
    TurnOff,
    ModeColor(usize),
    AddColor,
    RemoveColor,
    SaveToDevice,
    ZoneFill(usize),
    ZoneSize(usize),
    InfoRow(usize),
    NewProfile,
    Profile(usize),
}

/// A clickable screen region, recorded while drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Page(Page),
    Tab(DeviceTab),
    Field(Field),
    /// A slider or gradient; the rect maps the click position to a value.
    Rail(Field, Rect),
    PopupRow(Row),
    PopupRail(Row, Rect),
    Led(usize),
}

/// What the color popup edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTarget {
    /// The paint color on the LEDs tab; applies live to the selection.
    Paint(usize),
    ModeColor {
        dev: usize,
        idx: usize,
    },
    ZoneFill {
        dev: usize,
        zone: usize,
    },
}

pub enum Popup {
    Color {
        target: ColorTarget,
        hsv: Hsv,
        row: Row,
    },
    ConfirmDelete(String),
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputTarget {
    Host,
    Port,
    NewProfile,
    Hex(PickTarget),
    PopupHex,
}

pub struct Input {
    pub target: InputTarget,
    pub buf: String,
}

struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

pub struct App {
    worker: worker::Handle,
    pub settings: Settings,
    pub conn: Conn,
    pub devices: Vec<Controller>,
    pub profiles: Vec<String>,
    pub page: Page,
    pub tab: DeviceTab,
    pub focus: Focus,
    /// Focused field on the current page, with its index as a fallback for
    /// when the field disappears (e.g. after switching modes).
    field: Option<Field>,
    field_idx: usize,
    scroll: i32,
    /// Selected LED indices (controller-wide) on the current device.
    pub selection: BTreeSet<usize>,
    pub led_cursor: usize,
    led_scroll: i32,
    led_cells: Vec<device::LedCell>,
    pub sync_color: Hsv,
    /// Pending zone sizes, keyed by (device, zone).
    pub resize: HashMap<(usize, usize), u32>,
    pub input: Option<Input>,
    pub popup: Option<Popup>,
    popup_rect: Rect,
    toasts: Vec<Toast>,
    hits: Vec<(Rect, Hit)>,
    field_rows: Vec<(Field, i32)>,
    started: Instant,
    quit: bool,
    /// Set when drawing changed the scroll position, so the frame is redrawn.
    redraw: bool,
}

impl App {
    /// `server` overrides the saved address (`host` or `host:port`) and connects.
    pub fn new(server: Option<String>) -> Self {
        let mut settings = Settings::load();
        if let Some(server) = &server {
            match server.rsplit_once(':') {
                Some((h, p)) if p.parse::<u16>().is_ok() => {
                    settings.host = h.to_owned();
                    settings.port = p.parse().unwrap();
                }
                _ => settings.host = server.clone(),
            }
        }
        let mut app = Self {
            worker: worker::spawn(|| {}),
            conn: Conn::Disconnected(None),
            devices: Vec::new(),
            profiles: Vec::new(),
            page: Page::Overview,
            tab: DeviceTab::Lighting,
            focus: Focus::Sidebar,
            field: None,
            field_idx: 0,
            scroll: 0,
            selection: BTreeSet::new(),
            led_cursor: 0,
            led_scroll: 0,
            led_cells: Vec::new(),
            sync_color: settings.paint,
            resize: HashMap::new(),
            input: None,
            popup: None,
            popup_rect: Rect::default(),
            toasts: Vec::new(),
            hits: Vec::new(),
            field_rows: Vec::new(),
            started: Instant::now(),
            quit: false,
            redraw: false,
            settings,
        };
        if app.settings.auto_connect || server.is_some() {
            app.connect();
        }
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.quit {
            self.pump();
            terminal.draw(|f| self.draw(f))?;
            if std::mem::take(&mut self.redraw) {
                terminal.draw(|f| self.draw(f))?;
            }
            // The timeout doubles as the animation and worker-event tick.
            if event::poll(Duration::from_millis(50))? {
                loop {
                    match event::read()? {
                        TermEvent::Key(k) if k.kind != KeyEventKind::Release => self.on_key(k),
                        TermEvent::Mouse(m) => self.on_mouse(m),
                        _ => {}
                    }
                    if self.quit || !event::poll(Duration::ZERO)? {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn send(&self, c: Command) {
        self.worker.send(c);
    }

    pub fn connect(&mut self) {
        self.conn = Conn::Connecting;
        self.send(Command::Connect {
            host: self.settings.host.trim().to_owned(),
            port: self.settings.port,
        });
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast {
            text: text.into(),
            error,
            until: Instant::now() + Duration::from_millis(3500),
        });
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.conn, Conn::Connected { .. })
    }

    pub fn proto(&self) -> u32 {
        match self.conn {
            Conn::Connected { proto, .. } => proto,
            _ => 0,
        }
    }

    /// Seconds since start, for animations.
    pub fn time(&self) -> f32 {
        self.started.elapsed().as_secs_f32()
    }

    /// The focused field, if the content area has focus.
    pub fn focused(&self) -> Option<Field> {
        if self.focus == Focus::Content || !self.is_connected() {
            self.field
        } else {
            None
        }
    }

    /// The edit buffer, if `target` is being typed into.
    pub fn editing(&self, target: InputTarget) -> Option<&str> {
        self.input
            .as_ref()
            .filter(|i| i.target == target)
            .map(|i| i.buf.as_str())
    }

    pub fn current_device(&self) -> Option<usize> {
        match self.page {
            Page::Device(i) if i < self.devices.len() => Some(i),
            _ => None,
        }
    }

    // -- Device operations shared by pages ---------------------------------

    pub fn set_mode(&mut self, dev: usize, idx: usize, save: bool) {
        let Some(d) = self.devices.get_mut(dev) else {
            return;
        };
        let Some(mode) = d.modes.get(idx).cloned() else {
            return;
        };
        d.active_mode = idx as i32;
        self.send(Command::UpdateMode {
            dev: dev as u32,
            idx,
            mode,
            save,
        });
    }

    pub fn update_active_mode(&mut self, dev: usize, f: impl FnOnce(&mut Mode)) {
        let Some(d) = self.devices.get_mut(dev) else {
            return;
        };
        let Some(idx) = usize::try_from(d.active_mode).ok() else {
            return;
        };
        if let Some(m) = d.modes.get_mut(idx) {
            f(m);
            self.set_mode(dev, idx, false);
        }
    }

    /// Puts the device into its per-LED mode if it isn't already.
    fn ensure_direct(&mut self, dev: usize) {
        let Some(d) = self.devices.get_mut(dev) else {
            return;
        };
        if d.active_mode().is_some_and(Mode::is_direct_like) {
            return;
        }
        if let Some(i) = d.direct_mode_index() {
            d.active_mode = i as i32;
        }
        self.send(Command::SetCustomMode { dev: dev as u32 });
    }

    pub fn set_leds(&mut self, dev: usize, colors: Vec<Rgb>) {
        self.ensure_direct(dev);
        if let Some(d) = self.devices.get_mut(dev) {
            d.colors = colors.clone();
        }
        self.send(Command::UpdateLeds {
            dev: dev as u32,
            colors,
        });
    }

    pub fn fill_device(&mut self, dev: usize, c: Rgb) {
        if let Some(n) = self.devices.get(dev).map(|d| d.colors.len()) {
            self.set_leds(dev, vec![c; n]);
        }
    }

    pub fn fill_zone(&mut self, dev: usize, zone: usize, c: Rgb) {
        let Some(d) = self.devices.get(dev) else {
            return;
        };
        let Some(z) = d.zones.get(zone) else {
            return;
        };
        let start = d.zone_offsets()[zone].min(d.colors.len());
        let end = (start + z.leds_count as usize).min(d.colors.len());
        let mut colors = d.colors.clone();
        colors[start..end].fill(c);
        self.set_leds(dev, colors);
    }

    /// Paints the selected LEDs, or the one under the cursor if none are selected.
    pub fn paint_selection(&mut self, dev: usize, c: Rgb) {
        let Some(d) = self.devices.get(dev) else {
            return;
        };
        let mut colors = d.colors.clone();
        if self.selection.is_empty() {
            if let Some(x) = colors.get_mut(self.led_cursor) {
                *x = c;
            }
        } else {
            for &i in &self.selection {
                if let Some(x) = colors.get_mut(i) {
                    *x = c;
                }
            }
        }
        self.set_leds(dev, colors);
    }

    fn apply_all(&mut self, c: Rgb) {
        for i in 0..self.devices.len() {
            if self.devices[i].direct_mode_index().is_some() {
                self.fill_device(i, c);
            }
        }
    }

    // -- Event pump ----------------------------------------------------------

    fn pump(&mut self) {
        while let Ok(ev) = self.worker.rx.try_recv() {
            match ev {
                Event::Connected {
                    proto,
                    server_proto,
                } => {
                    self.conn = Conn::Connected {
                        proto,
                        server_proto,
                    };
                    let msg = format!("Connected to {}:{}", self.settings.host, self.settings.port);
                    self.toast(msg, false);
                }
                Event::Disconnected(err) => {
                    let was = self.is_connected();
                    self.conn = Conn::Disconnected(err.clone());
                    self.devices.clear();
                    self.profiles.clear();
                    self.page = Page::Overview;
                    self.focus = Focus::Content;
                    self.field = None;
                    if was {
                        let msg = err.unwrap_or_else(|| "Disconnected".into());
                        self.toast(msg, true);
                    }
                }
                Event::Devices(d) => {
                    if self.devices.len() != d.len() {
                        self.selection.clear();
                    }
                    self.devices = d;
                    if let Page::Device(i) = self.page
                        && i >= self.devices.len()
                    {
                        self.set_page(Page::Overview);
                    }
                }
                Event::Device(i, c) => {
                    if let Some(d) = self.devices.get_mut(i as usize) {
                        if d.colors.len() != c.colors.len() {
                            self.selection.clear();
                        }
                        *d = *c;
                    }
                }
                Event::Profiles(p) => self.profiles = p,
                Event::Error(e) => {
                    if self.conn == Conn::Connecting {
                        self.conn = Conn::Disconnected(Some(e.clone()));
                    }
                    self.toast(e, true);
                }
            }
        }
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
    }

    // -- Navigation ------------------------------------------------------------

    fn nav_pages(&self) -> Vec<Page> {
        let mut v = vec![Page::Overview, Page::Profiles];
        v.extend((0..self.devices.len()).map(Page::Device));
        v.push(Page::Settings);
        v
    }

    pub fn set_page(&mut self, p: Page) {
        if p == self.page {
            return;
        }
        if matches!(p, Page::Device(_)) {
            self.selection.clear();
            self.led_cursor = 0;
            self.led_scroll = 0;
        }
        self.page = p;
        self.reset_fields();
    }

    fn set_tab(&mut self, t: DeviceTab) {
        if t != self.tab {
            self.tab = t;
            self.reset_fields();
        }
    }

    fn reset_fields(&mut self) {
        self.field = None;
        self.field_idx = 0;
        self.scroll = 0;
        self.field_rows.clear();
    }

    fn move_field(&mut self, delta: i32) {
        if self.field_rows.is_empty() {
            return;
        }
        let max = self.field_rows.len() as i32 - 1;
        self.field_idx = (self.field_idx as i32 + delta).clamp(0, max) as usize;
        self.field = Some(self.field_rows[self.field_idx].0);
    }

    fn focus_field(&mut self, f: Field) {
        self.focus = Focus::Content;
        self.field = Some(f);
        if let Some(i) = self.field_rows.iter().position(|(x, _)| *x == f) {
            self.field_idx = i;
        }
    }

    /// Whether the neighbouring field in `delta` direction shares a line with
    /// the focused one (a row of buttons).
    fn same_line(&self, delta: i32) -> bool {
        let i = self.field_idx as i32 + delta;
        let (Some(a), Some(b)) = (
            self.field_rows.get(self.field_idx),
            usize::try_from(i).ok().and_then(|i| self.field_rows.get(i)),
        ) else {
            return false;
        };
        a.1 == b.1
    }

    // -- Keyboard --------------------------------------------------------------

    fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.input.is_some() {
            self.input_key(k);
            return;
        }
        if self.popup.is_some() {
            self.popup_key(k);
            return;
        }
        match k.code {
            KeyCode::Char('q') => {
                self.quit = true;
                return;
            }
            KeyCode::Char('?') => {
                self.popup = Some(Popup::Help);
                return;
            }
            _ => {}
        }
        if !self.is_connected() {
            self.form_key(k);
            return;
        }
        match k.code {
            KeyCode::Char('r') => {
                self.send(Command::Refresh);
                self.toast("Rescanning devices", false);
                return;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::Content,
                    Focus::Content => Focus::Sidebar,
                };
                return;
            }
            _ => {}
        }
        if let Page::Device(_) = self.page {
            let i = DeviceTab::ALL
                .iter()
                .position(|t| *t == self.tab)
                .unwrap_or(0);
            let tab = match k.code {
                KeyCode::Char(']') => Some((i + 1) % 4),
                KeyCode::Char('[') => Some((i + 3) % 4),
                KeyCode::Char(c @ '1'..='4') => Some(c as usize - '1' as usize),
                _ => None,
            };
            if let Some(t) = tab {
                self.set_tab(DeviceTab::ALL[t]);
                self.focus = Focus::Content;
                return;
            }
        }
        match self.focus {
            Focus::Sidebar => self.sidebar_key(k),
            Focus::Content => {
                let handled = if self.tab == DeviceTab::Leds
                    && let Some(dev) = self.current_device()
                {
                    device::leds_key(self, dev, k)
                } else {
                    self.form_key(k)
                };
                if !handled && matches!(k.code, KeyCode::Esc | KeyCode::Left | KeyCode::Char('h')) {
                    self.focus = Focus::Sidebar;
                }
            }
        }
    }

    fn sidebar_key(&mut self, k: KeyEvent) {
        let pages = self.nav_pages();
        let i = pages.iter().position(|p| *p == self.page).unwrap_or(0);
        let to = match k.code {
            KeyCode::Up | KeyCode::Char('k') => i.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => (i + 1).min(pages.len() - 1),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::End | KeyCode::Char('G') => pages.len() - 1,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => {
                self.focus = Focus::Content;
                return;
            }
            _ => return,
        };
        self.set_page(pages[to]);
    }

    /// Generic keys for pages made of fields. Returns whether the key was used.
    fn form_key(&mut self, k: KeyEvent) -> bool {
        let big = k
            .modifiers
            .intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL);
        if self.field.is_none() {
            self.move_field(0);
        }
        let f = self.field;
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_field(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_field(1),
            KeyCode::PageUp => self.move_field(-10),
            KeyCode::PageDown => self.move_field(10),
            KeyCode::Home | KeyCode::Char('g') => self.move_field(-10_000),
            KeyCode::End | KeyCode::Char('G') => self.move_field(10_000),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                if !f.is_some_and(|f| self.adjust(f, -1, big || k.code == KeyCode::Char('H'))) {
                    if self.same_line(-1) {
                        self.move_field(-1);
                    } else {
                        return false;
                    }
                }
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                if !f.is_some_and(|f| self.adjust(f, 1, big || k.code == KeyCode::Char('L')))
                    && self.same_line(1)
                {
                    self.move_field(1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(f) = f {
                    self.activate(f);
                }
            }
            KeyCode::Delete | KeyCode::Char('d') => {
                if let Some(Field::Profile(i)) = f
                    && let Some(name) = self.profiles.get(i)
                {
                    self.popup = Some(Popup::ConfirmDelete(name.clone()));
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if matches!(
                    f,
                    Some(Field::ModeColor(_) | Field::AddColor | Field::RemoveColor)
                ) {
                    self.activate(Field::AddColor);
                }
            }
            KeyCode::Char('-') => {
                if matches!(
                    f,
                    Some(Field::ModeColor(_) | Field::AddColor | Field::RemoveColor)
                ) {
                    self.activate(Field::RemoveColor);
                }
            }
            _ => return false,
        }
        true
    }

    fn pick_mut(&mut self, t: PickTarget) -> &mut Hsv {
        match t {
            PickTarget::Sync => &mut self.sync_color,
            PickTarget::Paint => &mut self.settings.paint,
        }
    }

    /// Called after an inline picker changed.
    fn picked(&mut self, t: PickTarget) {
        if t == PickTarget::Paint
            && let Some(dev) = self.current_device()
        {
            self.fill_device(dev, self.settings.paint.rgb());
        }
    }

    /// Left/right on a field. Returns `false` if the field doesn't take them.
    fn adjust(&mut self, f: Field, dir: i32, big: bool) -> bool {
        let dev = self.current_device();
        match f {
            Field::Pick(_, Row::Hex) => false,
            Field::Pick(t, row) => {
                if color::adjust(self.pick_mut(t), row, dir, big) {
                    self.picked(t);
                }
                true
            }
            Field::Port => {
                let step = if big { 100 } else { 1 };
                self.settings.port =
                    (self.settings.port as i32 + dir * step).clamp(1, 65535) as u16;
                true
            }
            Field::AutoConnect => {
                self.settings.auto_connect = !self.settings.auto_connect;
                true
            }
            Field::Speed | Field::Brightness => {
                if let Some(dev) = dev {
                    self.step_param(dev, f, dir, big);
                }
                true
            }
            Field::Direction | Field::ColorMode => {
                if let Some(dev) = dev {
                    self.cycle(dev, f, dir);
                }
                true
            }
            Field::ZoneSize(z) => {
                if let Some(dev) = dev
                    && let Some(zone) = self.devices[dev].zones.get(z)
                {
                    let (min, max) = (zone.leds_min, zone.leds_max);
                    let cur = *self.resize.get(&(dev, z)).unwrap_or(&zone.leds_count);
                    let step = if big { 10 } else { 1 };
                    let v = (cur as i64 + dir as i64 * step).clamp(min as i64, max as i64) as u32;
                    self.resize.insert((dev, z), v);
                }
                true
            }
            _ => false,
        }
    }

    fn step_param(&mut self, dev: usize, f: Field, dir: i32, big: bool) {
        let Some(m) = self.devices[dev].active_mode() else {
            return;
        };
        let (v, min, max) = if f == Field::Speed {
            (m.speed, m.speed_min, m.speed_max)
        } else {
            (m.brightness, m.brightness_min, m.brightness_max)
        };
        let (lo, hi) = (min.min(max), min.max(max));
        let span = hi - lo;
        let step = if big {
            (span / 10).max(1)
        } else {
            (span / 50).max(1)
        };
        // Right always moves toward `max`, which some devices put below `min`.
        let toward = if max >= min { dir } else { -dir };
        let nv = (v as i64 + toward as i64 * step as i64).clamp(lo as i64, hi as i64) as u32;
        self.set_param(dev, f, nv);
    }

    fn set_param(&mut self, dev: usize, f: Field, v: u32) {
        let cur = self.devices[dev].active_mode().map(|m| {
            if f == Field::Speed {
                m.speed
            } else {
                m.brightness
            }
        });
        if cur == Some(v) {
            return;
        }
        self.update_active_mode(dev, |m| {
            if f == Field::Speed {
                m.speed = v;
            } else {
                m.brightness = v;
            }
        });
    }

    fn cycle(&mut self, dev: usize, f: Field, dir: i32) {
        let Some(m) = self.devices[dev].active_mode() else {
            return;
        };
        let step = |len: usize, i: usize| (i as i32 + dir).rem_euclid(len as i32) as usize;
        if f == Field::Direction {
            let dirs = m.directions();
            if dirs.is_empty() {
                return;
            }
            let i = dirs.iter().position(|d| *d == m.direction).unwrap_or(0);
            let d = dirs[step(dirs.len(), i)];
            self.update_active_mode(dev, |m| m.direction = d);
        } else {
            let opts = device::color_modes(m);
            if opts.len() < 2 {
                return;
            }
            let i = opts
                .iter()
                .position(|(c, _)| *c == m.color_mode)
                .unwrap_or(0);
            let cm = opts[step(opts.len(), i)].0;
            self.update_active_mode(dev, |m| m.color_mode = cm);
        }
    }

    /// Enter on a field.
    fn activate(&mut self, f: Field) {
        let dev = self.current_device();
        match f {
            Field::Host => self.start_input(InputTarget::Host, self.settings.host.clone()),
            Field::Port => self.start_input(InputTarget::Port, self.settings.port.to_string()),
            Field::NewProfile => self.start_input(InputTarget::NewProfile, String::new()),
            Field::Pick(t, Row::Hex) => {
                let hex = self.pick_mut(t).rgb().hex();
                self.start_input(InputTarget::Hex(t), hex);
            }
            Field::Pick(t, Row::Preset) => {
                if color::adjust(self.pick_mut(t), Row::Preset, 1, false) {
                    self.picked(t);
                }
            }
            Field::AutoConnect => self.settings.auto_connect = !self.settings.auto_connect,
            Field::Connect => self.connect(),
            Field::Disconnect => self.send(Command::Disconnect),
            Field::Rescan => {
                self.send(Command::Refresh);
                self.toast("Rescanning devices", false);
            }
            Field::ApplyAll => {
                let c = self.sync_color.rgb();
                self.apply_all(c);
                self.toast(format!("Applied {} to all devices", c.hex()), false);
            }
            Field::AllOff => self.apply_all(Rgb::BLACK),
            Field::OpenDevice(i) => {
                self.set_page(Page::Device(i));
                self.focus = Focus::Content;
            }
            Field::Profile(i) => {
                if let Some(name) = self.profiles.get(i).cloned() {
                    self.send(Command::LoadProfile(name.clone()));
                    self.toast(format!("Loaded \u{201C}{name}\u{201D}"), false);
                }
            }
            Field::InfoRow(i) => {
                if let Some(dev) = dev
                    && let Some((_, v)) = device::info_rows(&self.devices[dev]).get(i)
                    && !v.is_empty()
                {
                    copy_to_clipboard(v);
                    self.toast("Copied to clipboard", false);
                }
            }
            _ => {
                if let Some(dev) = dev {
                    self.activate_device(dev, f);
                }
            }
        }
    }

    fn activate_device(&mut self, dev: usize, f: Field) {
        let paint = self.settings.paint;
        match f {
            Field::Mode(i) => self.set_mode(dev, i, false),
            Field::Direction | Field::ColorMode => self.cycle(dev, f, 1),
            Field::ApplyPaint => self.fill_device(dev, paint.rgb()),
            Field::TurnOff => self.fill_device(dev, Rgb::BLACK),
            Field::ModeColor(idx) => {
                let c = self.devices[dev]
                    .active_mode()
                    .and_then(|m| m.colors.get(idx).copied());
                if let Some(c) = c {
                    self.popup = Some(Popup::Color {
                        target: ColorTarget::ModeColor { dev, idx },
                        hsv: Hsv::from_rgb(c),
                        row: Row::Hue,
                    });
                }
            }
            Field::AddColor | Field::RemoveColor => {
                let Some(m) = self.devices[dev].active_mode() else {
                    return;
                };
                if m.color_mode != ColorMode::ModeSpecific {
                    return;
                }
                let len = m.colors.len() as u32;
                if f == Field::AddColor && len < m.colors_max {
                    let c = m.colors.last().copied().unwrap_or(Rgb::WHITE);
                    self.update_active_mode(dev, |m| m.colors.push(c));
                } else if f == Field::RemoveColor && len > m.colors_min.max(1) {
                    self.update_active_mode(dev, |m| {
                        m.colors.pop();
                    });
                }
            }
            Field::SaveToDevice => {
                if let Ok(idx) = usize::try_from(self.devices[dev].active_mode) {
                    self.set_mode(dev, idx, true);
                    self.toast("Saved to device", false);
                }
            }
            Field::ZoneFill(zone) => {
                self.popup = Some(Popup::Color {
                    target: ColorTarget::ZoneFill { dev, zone },
                    hsv: paint,
                    row: Row::Hue,
                });
            }
            Field::ZoneSize(z) => {
                let Some(zone) = self.devices[dev].zones.get(z) else {
                    return;
                };
                if let Some(&size) = self.resize.get(&(dev, z))
                    && size != zone.leds_count
                {
                    self.send(Command::ResizeZone {
                        dev: dev as u32,
                        zone: z as u32,
                        size,
                    });
                    self.toast(format!("Resized {} to {size} LEDs", zone.name), false);
                    self.resize.remove(&(dev, z));
                }
            }
            _ => {}
        }
    }

    /// Sets a slider or gradient from a click at fraction `t` along its rail.
    fn set_fraction(&mut self, f: Field, t: f32) {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u32, b: u32| (a as f32 + (b as f32 - a as f32) * t).round() as u32;
        let dev = self.current_device();
        match f {
            Field::Pick(p, row) => {
                if color::set_fraction(self.pick_mut(p), row, t) {
                    self.picked(p);
                }
            }
            Field::Speed | Field::Brightness => {
                let Some(dev) = dev else { return };
                let Some(m) = self.devices[dev].active_mode() else {
                    return;
                };
                let v = if f == Field::Speed {
                    lerp(m.speed_min, m.speed_max)
                } else {
                    lerp(m.brightness_min, m.brightness_max)
                };
                self.set_param(dev, f, v);
            }
            Field::ZoneSize(z) => {
                if let Some(dev) = dev
                    && let Some(zone) = self.devices[dev].zones.get(z)
                {
                    self.resize
                        .insert((dev, z), lerp(zone.leds_min, zone.leds_max));
                }
            }
            _ => {}
        }
    }

    fn start_input(&mut self, target: InputTarget, buf: String) {
        self.input = Some(Input { target, buf });
    }

    fn input_key(&mut self, k: KeyEvent) {
        let Some(input) = self.input.as_mut() else {
            return;
        };
        match k.code {
            KeyCode::Esc => self.input = None,
            KeyCode::Enter => {
                if let Some(i) = self.input.take() {
                    self.commit(i);
                }
            }
            KeyCode::Backspace => {
                input.buf.pop();
            }
            KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => input.buf.clear(),
            KeyCode::Char(c) if !c.is_control() => {
                let ok = match input.target {
                    InputTarget::Port => c.is_ascii_digit() && input.buf.len() < 5,
                    InputTarget::Hex(_) | InputTarget::PopupHex => {
                        (c.is_ascii_hexdigit() || c == '#') && input.buf.len() < 7
                    }
                    _ => input.buf.chars().count() < 64,
                };
                if ok {
                    input.buf.push(c);
                }
            }
            _ => {}
        }
    }

    fn commit(&mut self, input: Input) {
        let text = input.buf.trim().to_owned();
        match input.target {
            InputTarget::Host => {
                if !text.is_empty() {
                    self.settings.host = text;
                    self.connect();
                }
            }
            InputTarget::Port => match text.parse::<u16>() {
                Ok(p) if p > 0 => self.settings.port = p,
                _ => self.toast("Port must be between 1 and 65535", true),
            },
            InputTarget::NewProfile => {
                if !text.is_empty() {
                    let verb = if self.profiles.contains(&text) {
                        "Updated"
                    } else {
                        "Saved"
                    };
                    self.toast(format!("{verb} profile \u{201C}{text}\u{201D}"), false);
                    self.send(Command::SaveProfile(text));
                }
            }
            InputTarget::Hex(t) => match Rgb::from_hex(&text) {
                Some(c) => {
                    *self.pick_mut(t) = Hsv::from_rgb(c);
                    self.picked(t);
                }
                None => self.toast("Not a hex color (expected #RRGGBB)", true),
            },
            InputTarget::PopupHex => match Rgb::from_hex(&text) {
                Some(c) => {
                    if let Some(Popup::Color { hsv, .. }) = &mut self.popup {
                        *hsv = Hsv::from_rgb(c);
                    }
                    self.apply_popup_color();
                }
                None => self.toast("Not a hex color (expected #RRGGBB)", true),
            },
        }
    }

    // -- Popups ----------------------------------------------------------------

    fn popup_key(&mut self, k: KeyEvent) {
        let big = k
            .modifiers
            .intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL);
        match self.popup.as_mut() {
            Some(Popup::Color { hsv, row, .. }) => {
                let i = color::ROWS.iter().position(|r| r == row).unwrap_or(0);
                let dir: i32 = match k.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        *row = color::ROWS[i.saturating_sub(1)];
                        0
                    }
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                        *row = color::ROWS[(i + 1).min(color::ROWS.len() - 1)];
                        0
                    }
                    KeyCode::Left | KeyCode::Char('h') => -1,
                    KeyCode::Right | KeyCode::Char('l') => 1,
                    KeyCode::Char('H') => -2,
                    KeyCode::Char('L') => 2,
                    KeyCode::Enter if *row == Row::Hex => {
                        let hex = hsv.rgb().hex();
                        self.start_input(InputTarget::PopupHex, hex);
                        return;
                    }
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('c') => {
                        self.popup = None;
                        return;
                    }
                    _ => 0,
                };
                if dir != 0 && color::adjust(hsv, *row, dir.signum(), big || dir.abs() > 1) {
                    self.apply_popup_color();
                }
            }
            Some(Popup::ConfirmDelete(name)) => {
                if matches!(k.code, KeyCode::Char('y') | KeyCode::Enter) {
                    let name = name.clone();
                    self.send(Command::DeleteProfile(name.clone()));
                    self.toast(format!("Deleted \u{201C}{name}\u{201D}"), false);
                }
                self.popup = None;
            }
            Some(Popup::Help) | None => self.popup = None,
        }
    }

    fn apply_popup_color(&mut self) {
        let Some(Popup::Color { target, hsv, .. }) = &self.popup else {
            return;
        };
        let (target, hsv) = (*target, *hsv);
        let c = hsv.rgb();
        match target {
            ColorTarget::Paint(dev) => {
                self.settings.paint = hsv;
                if !self.selection.is_empty() {
                    self.paint_selection(dev, c);
                }
            }
            ColorTarget::ModeColor { dev, idx } => self.update_active_mode(dev, |m| {
                if let Some(x) = m.colors.get_mut(idx) {
                    *x = c;
                }
            }),
            ColorTarget::ZoneFill { dev, zone } => {
                self.settings.paint = hsv;
                self.fill_zone(dev, zone, c);
            }
        }
    }

    // -- Mouse -----------------------------------------------------------------

    fn on_mouse(&mut self, m: MouseEvent) {
        let pos = ratatui::layout::Position::new(m.column, m.row);
        let drag = match m.kind {
            MouseEventKind::Down(MouseButton::Left) => false,
            MouseEventKind::Drag(MouseButton::Left) => true,
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let d = if m.kind == MouseEventKind::ScrollDown {
                    1
                } else {
                    -1
                };
                if self.popup.is_none() && self.input.is_none() {
                    if self.tab == DeviceTab::Leds && self.current_device().is_some() {
                        self.led_scroll = (self.led_scroll + d * 2).max(0);
                    } else {
                        self.focus = Focus::Content;
                        self.move_field(d);
                    }
                }
                return;
            }
            _ => return,
        };
        if !drag {
            self.input = None;
        }
        if self.popup.is_some() && !self.popup_rect.contains(pos) {
            if !drag {
                self.popup = None;
            }
            return;
        }
        let Some(&(_, hit)) = self.hits.iter().rev().find(|(r, _)| r.contains(pos)) else {
            return;
        };
        let frac = |r: Rect| (m.column.saturating_sub(r.x)) as f32 / (r.width.max(2) - 1) as f32;
        let toggle = m
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        match hit {
            Hit::Rail(f, r) => {
                self.focus_field(f);
                self.set_fraction(f, frac(r));
            }
            Hit::PopupRail(row, r) => {
                if let Some(Popup::Color { hsv, row: cur, .. }) = &mut self.popup {
                    *cur = row;
                    if color::set_fraction(hsv, row, frac(r)) {
                        self.apply_popup_color();
                    }
                }
            }
            Hit::Led(led) => {
                self.focus = Focus::Content;
                self.led_cursor = led;
                if drag {
                    self.selection.insert(led);
                } else if toggle {
                    if !self.selection.remove(&led) {
                        self.selection.insert(led);
                    }
                } else {
                    self.selection.clear();
                    self.selection.insert(led);
                }
            }
            _ if drag => {}
            Hit::Page(p) => {
                self.set_page(p);
                self.focus = Focus::Sidebar;
            }
            Hit::Tab(t) => {
                self.set_tab(t);
                self.focus = Focus::Content;
            }
            Hit::Field(f) => {
                self.focus_field(f);
                self.activate(f);
            }
            Hit::PopupRow(row) => {
                if let Some(Popup::Color { hsv, row: cur, .. }) = &mut self.popup {
                    *cur = row;
                    if row == Row::Hex {
                        let hex = hsv.rgb().hex();
                        self.start_input(InputTarget::PopupHex, hex);
                    }
                }
            }
        }
    }

    // -- Drawing -----------------------------------------------------------------

    fn draw(&mut self, f: &mut Frame) {
        let [main, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(f.area());
        self.hits.clear();

        let laid = if self.is_connected() {
            let [side, content] =
                Layout::horizontal([Constraint::Length(30), Constraint::Fill(1)]).areas(main);
            self.draw_sidebar(f.buffer_mut(), side);
            self.draw_content(f.buffer_mut(), content)
        } else {
            let (col, h) = pages::connect_screen(self, f.buffer_mut(), main);
            Some((col, h))
        };
        if let Some((col, view_h)) = laid {
            self.hits.extend(col.hits.iter().copied());
            self.settle_fields(&col, view_h);
        }

        self.draw_status(f.buffer_mut(), status);
        self.draw_popup(f.buffer_mut(), main);
        self.draw_toasts(f.buffer_mut(), main);
    }

    /// Reconciles focus with the fields just laid out, and scrolls the
    /// focused one into view.
    fn settle_fields(&mut self, col: &Col, view_h: u16) {
        self.field_rows = col.field_rows.clone();
        if self.field_rows.is_empty() {
            self.field = None;
            self.field_idx = 0;
            return;
        }
        match self
            .field
            .and_then(|f| self.field_rows.iter().position(|(x, _)| *x == f))
        {
            Some(i) => self.field_idx = i,
            None => {
                self.field_idx = self.field_idx.min(self.field_rows.len() - 1);
                self.field = Some(self.field_rows[self.field_idx].0);
            }
        }
        let h = view_h as i32;
        let before = self.scroll;
        if self.focused().is_some() {
            let y = self.field_rows[self.field_idx].1;
            if self.field_idx == 0 {
                self.scroll = 0;
            } else if y < self.scroll + 1 {
                self.scroll = y - 1;
            } else if y >= self.scroll + h - 1 {
                self.scroll = y - h + 2;
            }
        }
        self.scroll = self.scroll.clamp(0, (col.height() - h).max(0));
        self.redraw |= self.scroll != before;
    }

    fn draw_sidebar(&mut self, buf: &mut ratatui::buffer::Buffer, area: Rect) {
        let focused = self.focus == Focus::Sidebar && self.popup.is_none();
        let mut title = theme::rainbow(" OpenRGB", 0.0);
        title.push_span(Span::styled(" TUI ", theme::dim()));
        let block = theme::panel(title, focused);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.height < 6 {
            return;
        }

        let item = |buf: &mut ratatui::buffer::Buffer, r: Rect, label: &str, selected: bool| {
            let style = if selected && focused {
                Style::new()
                    .fg(ratatui::style::Color::White)
                    .bg(theme::ACCENT)
                    .bold()
            } else if selected {
                theme::accent().bold()
            } else {
                theme::text()
            };
            Line::from(vec![
                Span::styled(if selected { "▌" } else { " " }, theme::accent()),
                Span::styled(format!(" {label:<w$}", w = r.width as usize - 2), style),
            ])
            .render(r, buf);
        };
        let row = |y: u16| Rect::new(inner.x, inner.y + y, inner.width, 1);

        for (i, (p, label)) in [(Page::Overview, "Overview"), (Page::Profiles, "Profiles")]
            .into_iter()
            .enumerate()
        {
            let r = row(i as u16);
            item(buf, r, label, self.page == p);
            self.hits.push((r, Hit::Page(p)));
        }

        widgets::section(
            buf,
            Rect {
                width: inner.width - 1,
                ..row(3)
            },
            "Devices",
        );
        Span::styled(format!("{:>2}", self.devices.len()), theme::faint())
            .render(Rect::new(inner.right() - 2, inner.y + 3, 2, 1), buf);

        // Devices take two lines each; the bottom four lines are reserved.
        let top = 4;
        let bottom = 4;
        let slots = (inner.height.saturating_sub(top + bottom) / 2) as usize;
        let sel = self.current_device();
        let first = sel.map_or(0, |s| (s + 1).saturating_sub(slots));
        for (slot, (i, d)) in self
            .devices
            .iter()
            .enumerate()
            .skip(first)
            .take(slots)
            .enumerate()
        {
            let y = top + slot as u16 * 2;
            let selected = sel == Some(i);
            let r = row(y);
            let tint = theme::led(theme::accent_of(&d.colors));
            let name_style = if selected && focused {
                Style::new()
                    .fg(ratatui::style::Color::White)
                    .bg(theme::ACCENT)
                    .bold()
            } else if selected {
                theme::accent().bold()
            } else {
                theme::text()
            };
            let w = inner.width.saturating_sub(4) as usize;
            Line::from(vec![
                Span::styled(if selected { "▌" } else { " " }, theme::accent()),
                Span::styled("● ", Style::new().fg(tint)),
                Span::styled(format!("{:<w$.w$}", d.name), name_style),
            ])
            .render(r, buf);
            let mode = d.active_mode().map(|m| m.name.as_str()).unwrap_or("—");
            Line::from(vec![
                "   ".into(),
                Span::styled(format!("{} · {mode}", d.kind.label()), theme::dim()),
            ])
            .render(row(y + 1), buf);
            self.hits
                .push((Rect { height: 2, ..r }, Hit::Page(Page::Device(i))));
        }
        if self.devices.is_empty() {
            Span::styled("   No devices", theme::faint()).render(row(top), buf);
        }

        let r = row(inner.height - 4);
        item(buf, r, "Settings", self.page == Page::Settings);
        self.hits.push((r, Hit::Page(Page::Settings)));
        if let Conn::Connected { proto, .. } = self.conn {
            Line::from(vec![
                Span::styled(" ● ", Style::new().fg(theme::SUCCESS)),
                Span::styled("Connected", theme::strong()),
            ])
            .render(row(inner.height - 2), buf);
            Span::styled(
                format!(
                    "   {}:{} · SDK v{proto}",
                    self.settings.host, self.settings.port
                ),
                theme::dim(),
            )
            .render(row(inner.height - 1), buf);
        }
    }

    /// Draws the page; returns the laid-out column (if the page uses one)
    /// and its visible height.
    fn draw_content(
        &mut self,
        buf: &mut ratatui::buffer::Buffer,
        area: Rect,
    ) -> Option<(Col, u16)> {
        let focused = self.focus == Focus::Content && self.popup.is_none();
        let title = match self.page {
            Page::Overview => {
                let leds: usize = self.devices.iter().map(|d| d.colors.len()).sum();
                format!(" Overview · {} devices · {leds} LEDs ", self.devices.len())
            }
            Page::Device(i) => format!(" {} ", self.devices.get(i).map_or("", |d| d.name.as_str())),
            Page::Profiles => " Profiles ".into(),
            Page::Settings => " Settings ".into(),
        };
        let block = theme::panel(Span::styled(title, theme::strong()), focused);
        let inner = block.inner(area).inner(ratatui::layout::Margin::new(1, 0));
        block.render(area, buf);

        let mut body = inner;
        if let Some(dev) = self.current_device() {
            body = device::header(self, buf, inner, dev);
            if self.tab == DeviceTab::Leds {
                let laid = device::draw_leds(self, buf, body, dev);
                self.hits.extend(laid.hits);
                self.led_cells = laid.cells;
                if let Some(y) = laid.cursor_y {
                    let before = self.led_scroll;
                    let h = laid.view_h as i32;
                    if y < self.led_scroll + 1 {
                        self.led_scroll = (y - 1).max(0);
                    } else if y >= self.led_scroll + h {
                        self.led_scroll = y - h + 1;
                    }
                    self.led_scroll = self.led_scroll.clamp(0, (laid.total_h - h).max(0));
                    self.redraw |= before != self.led_scroll;
                }
                self.field_rows.clear();
                return None;
            }
        }

        let mut col = Col::new(body, self.scroll);
        match self.page {
            Page::Overview => pages::overview(self, buf, &mut col),
            Page::Device(dev) => device::page(self, buf, &mut col, dev),
            Page::Profiles => pages::profiles(self, buf, &mut col),
            Page::Settings => pages::settings(self, buf, &mut col),
        }
        if col.height() > body.height as i32 {
            let pct = self.scroll * 100 / (col.height() - body.height as i32).max(1);
            Span::styled(format!(" {pct}% "), theme::faint())
                .render(Rect::new(area.right() - 7, area.bottom() - 1, 6, 1), buf);
        }
        Some((col, body.height))
    }

    fn draw_status(&self, buf: &mut ratatui::buffer::Buffer, area: Rect) {
        let (dot, label) = match &self.conn {
            Conn::Connected { .. } => (
                theme::SUCCESS,
                format!("{}:{}", self.settings.host, self.settings.port),
            ),
            Conn::Connecting => (
                theme::WARN,
                format!("Connecting to {}…", self.settings.host),
            ),
            Conn::Disconnected(_) => (theme::DANGER, "Offline".into()),
        };
        Line::from(vec![
            Span::styled(" ● ", Style::new().fg(dot)),
            Span::styled(label, theme::dim()),
        ])
        .render(area, buf);

        let hints: &[(&str, &str)] = if self.input.is_some() {
            &[("Enter", "confirm"), ("Esc", "cancel"), ("Ctrl-U", "clear")]
        } else if let Some(p) = &self.popup {
            match p {
                Popup::Color { .. } => &[
                    ("↑↓", "row"),
                    ("←→", "adjust"),
                    ("Shift", "faster"),
                    ("Esc", "close"),
                ],
                Popup::ConfirmDelete(_) => &[("y", "delete"), ("n", "keep")],
                Popup::Help => &[("any key", "close")],
            }
        } else if !self.is_connected() {
            &[
                ("↑↓", "move"),
                ("Enter", "select"),
                ("?", "help"),
                ("q", "quit"),
            ]
        } else if self.focus == Focus::Sidebar {
            &[
                ("↑↓", "navigate"),
                ("Enter", "open"),
                ("Tab", "switch pane"),
                ("r", "rescan"),
                ("?", "help"),
                ("q", "quit"),
            ]
        } else if self.tab == DeviceTab::Leds && self.current_device().is_some() {
            &[
                ("arrows", "move"),
                ("Space", "select"),
                ("Enter", "paint"),
                ("c", "color"),
                ("[ ]", "tabs"),
                ("?", "help"),
            ]
        } else if self.current_device().is_some() {
            &[
                ("↑↓", "move"),
                ("←→", "adjust"),
                ("Enter", "select"),
                ("[ ]", "tabs"),
                ("Esc", "back"),
                ("?", "help"),
            ]
        } else {
            &[
                ("↑↓", "move"),
                ("←→", "adjust"),
                ("Enter", "select"),
                ("Esc", "back"),
                ("?", "help"),
            ]
        };
        let mut spans = Vec::new();
        for (k, v) in hints {
            spans.push(Span::styled(
                *k,
                theme::accent().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!(" {v}  "), theme::dim()));
        }
        Line::from(spans).right_aligned().render(area, buf);
    }

    fn draw_popup(&mut self, buf: &mut ratatui::buffer::Buffer, area: Rect) {
        let Some(popup) = &self.popup else {
            return;
        };
        match popup {
            Popup::Color { target, hsv, row } => {
                let title = match target {
                    ColorTarget::Paint(_) => " Paint color ",
                    ColorTarget::ModeColor { .. } => " Mode color ",
                    ColorTarget::ZoneFill { .. } => " Fill zone ",
                };
                let r = widgets::centered(area, 64, 10);
                self.popup_rect = r;
                Clear.render(r, buf);
                let block = theme::panel(Span::styled(title, theme::strong()), true);
                let inner = block.inner(r).inner(ratatui::layout::Margin::new(1, 1));
                block.render(r, buf);
                let editing = self.editing(InputTarget::PopupHex).map(str::to_owned);
                let (hsv, row) = (*hsv, *row);
                for (i, &rw) in color::ROWS.iter().enumerate() {
                    let lr = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                    let rail = color::render_row(buf, lr, hsv, rw, rw == row, editing.as_deref());
                    self.hits.push((lr, Hit::PopupRow(rw)));
                    if rw.is_rail() {
                        self.hits.push((rail, Hit::PopupRail(rw, rail)));
                    }
                }
                let note = match target {
                    ColorTarget::Paint(_) if self.selection.is_empty() => {
                        "Select LEDs to paint them live, or press Enter on the map."
                    }
                    ColorTarget::Paint(_) => "Changes apply live to the selected LEDs.",
                    _ => "Changes apply live.",
                };
                Span::styled(note, theme::faint()).render(
                    Rect::new(inner.x + 2, inner.bottom() - 1, inner.width - 2, 1),
                    buf,
                );
            }
            Popup::ConfirmDelete(name) => {
                let r = widgets::centered(area, 52, 5);
                self.popup_rect = r;
                Clear.render(r, buf);
                let block = theme::panel(Span::styled(" Delete profile ", theme::strong()), true)
                    .border_style(Style::new().fg(theme::DANGER));
                let inner = block.inner(r).inner(ratatui::layout::Margin::new(1, 0));
                block.render(r, buf);
                Paragraph::new(vec![
                    Line::from(format!("Delete \u{201C}{name}\u{201D} from the server?")),
                    Line::from(vec![
                        Span::styled("y", theme::accent().bold()),
                        Span::styled(" delete   ", theme::dim()),
                        Span::styled("n", theme::accent().bold()),
                        Span::styled(" keep", theme::dim()),
                    ]),
                ])
                .wrap(Wrap { trim: true })
                .render(
                    Rect {
                        y: inner.y,
                        ..inner
                    },
                    buf,
                );
            }
            Popup::Help => {
                let r = widgets::centered(area, 72, 26);
                self.popup_rect = r;
                Clear.render(r, buf);
                let block = theme::panel(Span::styled(" Keys ", theme::strong()), true);
                let inner = block.inner(r).inner(ratatui::layout::Margin::new(1, 0));
                block.render(r, buf);
                Paragraph::new(help_lines()).render(inner, buf);
            }
        }
    }

    fn draw_toasts(&self, buf: &mut ratatui::buffer::Buffer, area: Rect) {
        let mut y = area.bottom().saturating_sub(1);
        for t in self.toasts.iter().rev().take(3) {
            let (icon, c) = if t.error {
                ("✖", theme::DANGER)
            } else {
                ("✔", theme::SUCCESS)
            };
            let w = (t.text.chars().count() as u16 + 6)
                .min(area.width.saturating_sub(2))
                .min(60);
            if y < area.y + 3 {
                break;
            }
            y -= 3;
            let r = Rect::new(area.right().saturating_sub(w + 2), y, w, 3);
            Clear.render(r, buf);
            let block = theme::panel("", false).border_style(Style::new().fg(c));
            let inner = block.inner(r);
            block.render(r, buf);
            Line::from(vec![
                Span::styled(format!("{icon} "), Style::new().fg(c)),
                Span::raw(t.text.clone()),
            ])
            .render(inner, buf);
        }
    }
}

fn help_lines() -> Vec<Line<'static>> {
    let groups: &[(&str, &[(&str, &str)])] = &[
        (
            "Anywhere",
            &[
                ("Tab", "switch between sidebar and page"),
                ("↑↓ / j k", "move"),
                ("Enter / Space", "open, select or press"),
                ("←→ / h l", "adjust a slider or choice (Shift: faster)"),
                ("Esc", "back to the sidebar"),
                ("r", "rescan devices"),
                ("q / Ctrl-C", "quit"),
            ],
        ),
        (
            "Device pages",
            &[
                ("[ ]  or  1–4", "switch tab: Lighting, LEDs, Zones, Info"),
                ("+ / -", "add or remove a mode color"),
            ],
        ),
        (
            "LEDs tab",
            &[
                ("arrows", "move the cursor (Shift: extend selection)"),
                ("Space", "select / unselect the LED under the cursor"),
                ("Enter", "paint selection (or the cursor LED)"),
                ("c", "change the paint color"),
                ("p", "pick the paint color from the cursor LED"),
                ("a / n / z", "select all / none / the cursor's zone"),
                ("f / o", "fill the device / turn the selection off"),
            ],
        ),
        ("Profiles", &[("Enter", "load"), ("d / Del", "delete")]),
    ];
    let mut lines = Vec::new();
    for (title, keys) in groups {
        lines.push(Line::styled(
            *title,
            theme::dim().add_modifier(Modifier::BOLD),
        ));
        for (k, v) in *keys {
            lines.push(Line::from(vec![
                Span::styled(format!("  {k:<16}"), theme::accent()),
                Span::raw(*v),
            ]));
        }
    }
    lines.push(Line::styled(
        "The mouse works too: click, drag over LEDs or along sliders.",
        theme::faint(),
    ));
    lines
}

/// Copies through the terminal (OSC 52), which also works over SSH.
fn copy_to_clipboard(text: &str) {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut enc = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                enc.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                enc.push('=');
            }
        }
    }
    let mut out = io::stdout();
    let _ = write!(out, "\x1b]52;c;{enc}\x07");
    let _ = out.flush();
}
