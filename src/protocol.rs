//! OpenRGB SDK network protocol: packet framing plus (de)serialization of
//! controller descriptions.
//!
//! Reference: <https://gitlab.com/CalcProgrammer1/OpenRGB/-/wikis/OpenRGB-SDK-Documentation>

use std::fmt;

pub const MAGIC: &[u8; 4] = b"ORGB";
pub const HEADER_LEN: usize = 16;

/// Highest protocol version this client understands.
pub const CLIENT_PROTOCOL_VERSION: u32 = 4;

pub mod pkt {
    pub const REQUEST_CONTROLLER_COUNT: u32 = 0;
    pub const REQUEST_CONTROLLER_DATA: u32 = 1;
    pub const REQUEST_PROTOCOL_VERSION: u32 = 40;
    pub const SET_CLIENT_NAME: u32 = 50;
    pub const DEVICE_LIST_UPDATED: u32 = 100;
    pub const REQUEST_PROFILE_LIST: u32 = 150;
    pub const REQUEST_SAVE_PROFILE: u32 = 151;
    pub const REQUEST_LOAD_PROFILE: u32 = 152;
    pub const REQUEST_DELETE_PROFILE: u32 = 153;
    pub const RGBCONTROLLER_RESIZEZONE: u32 = 1000;
    pub const RGBCONTROLLER_UPDATELEDS: u32 = 1050;
    pub const RGBCONTROLLER_UPDATEZONELEDS: u32 = 1051;
    pub const RGBCONTROLLER_UPDATESINGLELED: u32 = 1052;
    pub const RGBCONTROLLER_SETCUSTOMMODE: u32 = 1100;
    pub const RGBCONTROLLER_UPDATEMODE: u32 = 1101;
    pub const RGBCONTROLLER_SAVEMODE: u32 = 1102;
}

pub mod mode_flags {
    pub const HAS_SPEED: u32 = 1 << 0;
    pub const HAS_DIRECTION_LR: u32 = 1 << 1;
    pub const HAS_DIRECTION_UD: u32 = 1 << 2;
    pub const HAS_DIRECTION_HV: u32 = 1 << 3;
    pub const HAS_BRIGHTNESS: u32 = 1 << 4;
    pub const HAS_PER_LED_COLOR: u32 = 1 << 5;
    pub const HAS_MODE_SPECIFIC_COLOR: u32 = 1 << 6;
    pub const HAS_RANDOM_COLOR: u32 = 1 << 7;
    pub const MANUAL_SAVE: u32 = 1 << 8;
    pub const AUTOMATIC_SAVE: u32 = 1 << 9;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// OpenRGB packs colors as `0x00BBGGRR`.
    pub fn from_u32(v: u32) -> Self {
        Self::new(v as u8, (v >> 8) as u8, (v >> 16) as u8)
    }

    pub fn to_u32(self) -> u32 {
        self.r as u32 | (self.g as u32) << 8 | (self.b as u32) << 16
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 || !s.is_ascii() {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Self::new((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    /// `h` in degrees, `s` and `v` in `0..=1`.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(360.0) / 60.0;
        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        let f = |c: f32| ((c + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Self::new(f(r), f(g), f(b))
    }

    pub fn scale(self, k: f32) -> Self {
        let f = |c: u8| (c as f32 * k).round().clamp(0.0, 255.0) as u8;
        Self::new(f(self.r), f(self.g), f(self.b))
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self::new(f(self.r, other.r), f(self.g, other.g), f(self.b, other.b))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum DeviceType {
    Motherboard = 0,
    Dram,
    Gpu,
    Cooler,
    LedStrip,
    Keyboard,
    Mouse,
    MouseMat,
    Headset,
    HeadsetStand,
    Gamepad,
    Light,
    Speaker,
    Virtual,
    Storage,
    Case,
    Microphone,
    Accessory,
    Keypad,
    Laptop,
    Monitor,
    Unknown = 0xFF,
}

impl DeviceType {
    pub fn from_i32(v: i32) -> Self {
        use DeviceType::*;
        match v {
            0 => Motherboard,
            1 => Dram,
            2 => Gpu,
            3 => Cooler,
            4 => LedStrip,
            5 => Keyboard,
            6 => Mouse,
            7 => MouseMat,
            8 => Headset,
            9 => HeadsetStand,
            10 => Gamepad,
            11 => Light,
            12 => Speaker,
            13 => Virtual,
            14 => Storage,
            15 => Case,
            16 => Microphone,
            17 => Accessory,
            18 => Keypad,
            19 => Laptop,
            20 => Monitor,
            _ => Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        use DeviceType::*;
        match self {
            Motherboard => "Motherboard",
            Dram => "Memory",
            Gpu => "Graphics Card",
            Cooler => "Cooler",
            LedStrip => "LED Strip",
            Keyboard => "Keyboard",
            Mouse => "Mouse",
            MouseMat => "Mousemat",
            Headset => "Headset",
            HeadsetStand => "Headset Stand",
            Gamepad => "Gamepad",
            Light => "Light",
            Speaker => "Speaker",
            Virtual => "Virtual",
            Storage => "Storage",
            Case => "Case",
            Microphone => "Microphone",
            Accessory => "Accessory",
            Keypad => "Keypad",
            Laptop => "Laptop",
            Monitor => "Monitor",
            Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneType {
    Single,
    Linear,
    Matrix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left = 0,
    Right = 1,
    Up = 2,
    Down = 3,
    Horizontal = 4,
    Vertical = 5,
}

impl Direction {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Right,
            2 => Self::Up,
            3 => Self::Down,
            4 => Self::Horizontal,
            5 => Self::Vertical,
            _ => Self::Left,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Horizontal => "Horizontal",
            Self::Vertical => "Vertical",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    None = 0,
    PerLed = 1,
    ModeSpecific = 2,
    Random = 3,
}

impl ColorMode {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::PerLed,
            2 => Self::ModeSpecific,
            3 => Self::Random,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mode {
    pub name: String,
    pub value: i32,
    pub flags: u32,
    pub speed_min: u32,
    pub speed_max: u32,
    pub brightness_min: u32,
    pub brightness_max: u32,
    pub colors_min: u32,
    pub colors_max: u32,
    pub speed: u32,
    pub brightness: u32,
    pub direction: Direction,
    pub color_mode: ColorMode,
    pub colors: Vec<Rgb>,
}

impl Mode {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    pub fn directions(&self) -> Vec<Direction> {
        use Direction::*;
        let mut v = Vec::new();
        if self.has(mode_flags::HAS_DIRECTION_LR) {
            v.extend([Left, Right]);
        }
        if self.has(mode_flags::HAS_DIRECTION_UD) {
            v.extend([Up, Down]);
        }
        if self.has(mode_flags::HAS_DIRECTION_HV) {
            v.extend([Horizontal, Vertical]);
        }
        v
    }

    /// Whether this mode lets the host drive individual LED colors.
    pub fn is_direct_like(&self) -> bool {
        self.has(mode_flags::HAS_PER_LED_COLOR)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub name: String,
    pub kind: i32,
    pub start_idx: u32,
    pub leds_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatrixMap {
    pub height: u32,
    pub width: u32,
    /// Row-major; `u32::MAX` marks an empty cell, otherwise a zone-local LED index.
    pub map: Vec<u32>,
}

impl MatrixMap {
    pub const EMPTY: u32 = u32::MAX;

    pub fn get(&self, row: u32, col: u32) -> Option<u32> {
        let v = *self.map.get((row * self.width + col) as usize)?;
        (v != Self::EMPTY).then_some(v)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub name: String,
    pub kind: ZoneType,
    pub leds_min: u32,
    pub leds_max: u32,
    pub leds_count: u32,
    pub matrix: Option<MatrixMap>,
    pub segments: Vec<Segment>,
}

impl Zone {
    pub fn resizable(&self) -> bool {
        self.leds_min != self.leds_max
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Led {
    pub name: String,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Controller {
    pub kind: DeviceType,
    pub name: String,
    pub vendor: String,
    pub description: String,
    pub version: String,
    pub serial: String,
    pub location: String,
    pub active_mode: i32,
    pub modes: Vec<Mode>,
    pub zones: Vec<Zone>,
    pub leds: Vec<Led>,
    pub colors: Vec<Rgb>,
}

impl Controller {
    pub fn active_mode(&self) -> Option<&Mode> {
        usize::try_from(self.active_mode)
            .ok()
            .and_then(|i| self.modes.get(i))
    }

    /// Index of the first LED of each zone in the controller-wide LED list.
    pub fn zone_offsets(&self) -> Vec<usize> {
        let mut acc = 0usize;
        self.zones
            .iter()
            .map(|z| {
                let start = acc;
                acc += z.leds_count as usize;
                start
            })
            .collect()
    }

    /// Index of the mode most suitable for host-driven per-LED control
    /// ("Direct" if present, else any per-LED mode).
    pub fn direct_mode_index(&self) -> Option<usize> {
        self.modes
            .iter()
            .position(|m| m.name.eq_ignore_ascii_case("direct"))
            .or_else(|| {
                self.modes
                    .iter()
                    .position(|m| m.name.eq_ignore_ascii_case("custom") && m.is_direct_like())
            })
            .or_else(|| self.modes.iter().position(Mode::is_direct_like))
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Truncated,
    BadMagic,
    Unexpected(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Truncated => write!(f, "truncated packet"),
            Error::BadMagic => write!(f, "invalid packet header (not an OpenRGB server?)"),
            Error::Unexpected(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub device: u32,
    pub id: u32,
    pub size: u32,
}

impl Header {
    pub fn parse(b: &[u8; HEADER_LEN]) -> Result<Self> {
        if &b[0..4] != MAGIC {
            return Err(Error::BadMagic);
        }
        let u = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        Ok(Self {
            device: u(4),
            id: u(8),
            size: u(12),
        })
    }
}

pub fn encode_packet(device: u32, id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&device.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

// ---------------------------------------------------------------------------
// Reader / writer helpers
// ---------------------------------------------------------------------------

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let s = self.buf.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn color(&mut self) -> Result<Rgb> {
        Ok(Rgb::from_u32(self.u32()?))
    }

    /// Length-prefixed (u16), NUL-terminated string.
    pub fn string(&mut self) -> Result<String> {
        let len = self.u16()? as usize;
        let raw = self.take(len)?;
        let raw = raw.split(|&c| c == 0).next().unwrap_or(&[]);
        Ok(String::from_utf8_lossy(raw).into_owned())
    }

    /// Reads a u16 count, then that many items.
    pub fn list<T>(&mut self, mut f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let n = self.u16()? as usize;
        // Guard against absurd counts in corrupt data.
        let mut v = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            v.push(f(self)?);
        }
        Ok(v)
    }
}

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.u32(v as u32)
    }

    pub fn color(&mut self, c: Rgb) -> &mut Self {
        self.u32(c.to_u32())
    }

    pub fn string(&mut self, s: &str) -> &mut Self {
        self.u16(s.len() as u16 + 1);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        self
    }

    pub fn colors(&mut self, c: &[Rgb]) -> &mut Self {
        self.u16(c.len() as u16);
        for &c in c {
            self.color(c);
        }
        self
    }

    /// Prefixes the buffer with its own total length (including the prefix),
    /// as several OpenRGB packets require.
    pub fn sized(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.buf.len() + 4);
        out.extend_from_slice(&(self.buf.len() as u32 + 4).to_le_bytes());
        out.extend_from_slice(&self.buf);
        out
    }
}

// ---------------------------------------------------------------------------
// Controller data
// ---------------------------------------------------------------------------

pub fn read_mode(r: &mut Reader, proto: u32) -> Result<Mode> {
    let name = r.string()?;
    let value = r.i32()?;
    let flags = r.u32()?;
    let speed_min = r.u32()?;
    let speed_max = r.u32()?;
    let (brightness_min, brightness_max) = if proto >= 3 {
        (r.u32()?, r.u32()?)
    } else {
        (0, 0)
    };
    let colors_min = r.u32()?;
    let colors_max = r.u32()?;
    let speed = r.u32()?;
    let brightness = if proto >= 3 { r.u32()? } else { 0 };
    let direction = Direction::from_u32(r.u32()?);
    let color_mode = ColorMode::from_u32(r.u32()?);
    let colors = r.list(Reader::color)?;
    Ok(Mode {
        name,
        value,
        flags,
        speed_min,
        speed_max,
        brightness_min,
        brightness_max,
        colors_min,
        colors_max,
        speed,
        brightness,
        direction,
        color_mode,
        colors,
    })
}

pub fn write_mode(w: &mut Writer, m: &Mode, proto: u32) {
    w.string(&m.name)
        .i32(m.value)
        .u32(m.flags)
        .u32(m.speed_min)
        .u32(m.speed_max);
    if proto >= 3 {
        w.u32(m.brightness_min).u32(m.brightness_max);
    }
    w.u32(m.colors_min).u32(m.colors_max).u32(m.speed);
    if proto >= 3 {
        w.u32(m.brightness);
    }
    w.u32(m.direction as u32)
        .u32(m.color_mode as u32)
        .colors(&m.colors);
}

fn read_zone(r: &mut Reader, proto: u32) -> Result<Zone> {
    let name = r.string()?;
    let kind = match r.i32()? {
        0 => ZoneType::Single,
        2 => ZoneType::Matrix,
        _ => ZoneType::Linear,
    };
    let leds_min = r.u32()?;
    let leds_max = r.u32()?;
    let leds_count = r.u32()?;
    let matrix_len = r.u16()?;
    let matrix = if matrix_len > 0 {
        let height = r.u32()?;
        let width = r.u32()?;
        let n = (matrix_len as usize).saturating_sub(8) / 4;
        let map = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>>>()?;
        Some(MatrixMap { height, width, map })
    } else {
        None
    };
    let segments = if proto >= 4 {
        r.list(|r| {
            Ok(Segment {
                name: r.string()?,
                kind: r.i32()?,
                start_idx: r.u32()?,
                leds_count: r.u32()?,
            })
        })?
    } else {
        Vec::new()
    };
    Ok(Zone {
        name,
        kind,
        leds_min,
        leds_max,
        leds_count,
        matrix,
        segments,
    })
}

fn write_zone(w: &mut Writer, z: &Zone, proto: u32) {
    w.string(&z.name)
        .i32(z.kind as i32)
        .u32(z.leds_min)
        .u32(z.leds_max)
        .u32(z.leds_count);
    match &z.matrix {
        Some(m) => {
            w.u16((8 + m.map.len() * 4) as u16)
                .u32(m.height)
                .u32(m.width);
            for &v in &m.map {
                w.u32(v);
            }
        }
        None => {
            w.u16(0);
        }
    }
    if proto >= 4 {
        w.u16(z.segments.len() as u16);
        for s in &z.segments {
            w.string(&s.name)
                .i32(s.kind)
                .u32(s.start_idx)
                .u32(s.leds_count);
        }
    }
}

pub fn parse_controller(buf: &[u8], proto: u32) -> Result<Controller> {
    let mut r = Reader::new(buf);
    let _size = r.u32()?;
    let kind = DeviceType::from_i32(r.i32()?);
    let name = r.string()?;
    let vendor = if proto >= 1 {
        r.string()?
    } else {
        String::new()
    };
    let description = r.string()?;
    let version = r.string()?;
    let serial = r.string()?;
    let location = r.string()?;
    let num_modes = r.u16()?;
    let active_mode = r.i32()?;
    let modes = (0..num_modes)
        .map(|_| read_mode(&mut r, proto))
        .collect::<Result<Vec<_>>>()?;
    let zones = r.list(|r| read_zone(r, proto))?;
    let leds = r.list(|r| {
        Ok(Led {
            name: r.string()?,
            value: r.u32()?,
        })
    })?;
    let colors = r.list(Reader::color)?;
    Ok(Controller {
        kind,
        name,
        vendor,
        description,
        version,
        serial,
        location,
        active_mode,
        modes,
        zones,
        leds,
        colors,
    })
}

/// Inverse of [`parse_controller`]; used by the mock server and tests.
pub fn encode_controller(c: &Controller, proto: u32) -> Vec<u8> {
    let mut w = Writer::default();
    w.i32(c.kind as i32).string(&c.name);
    if proto >= 1 {
        w.string(&c.vendor);
    }
    w.string(&c.description)
        .string(&c.version)
        .string(&c.serial)
        .string(&c.location)
        .u16(c.modes.len() as u16)
        .i32(c.active_mode);
    for m in &c.modes {
        write_mode(&mut w, m, proto);
    }
    w.u16(c.zones.len() as u16);
    for z in &c.zones {
        write_zone(&mut w, z, proto);
    }
    w.u16(c.leds.len() as u16);
    for l in &c.leds {
        w.string(&l.name).u32(l.value);
    }
    w.colors(&c.colors);
    w.sized()
}

pub fn parse_profile_list(buf: &[u8]) -> Result<Vec<String>> {
    let mut r = Reader::new(buf);
    let _size = r.u32()?;
    r.list(Reader::string)
}

pub fn encode_profile_list(names: &[String]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u16(names.len() as u16);
    for n in names {
        w.string(n);
    }
    w.sized()
}

pub fn nul_terminated(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample() -> Controller {
        Controller {
            kind: DeviceType::Keyboard,
            name: "Test Keyboard".into(),
            vendor: "Acme".into(),
            description: "A keyboard".into(),
            version: "1.0".into(),
            serial: "XYZ".into(),
            location: "HID: /dev/hidraw0".into(),
            active_mode: 1,
            modes: vec![
                Mode {
                    name: "Direct".into(),
                    value: 0,
                    flags: mode_flags::HAS_PER_LED_COLOR,
                    speed_min: 0,
                    speed_max: 0,
                    brightness_min: 0,
                    brightness_max: 0,
                    colors_min: 0,
                    colors_max: 0,
                    speed: 0,
                    brightness: 0,
                    direction: Direction::Left,
                    color_mode: ColorMode::PerLed,
                    colors: vec![],
                },
                Mode {
                    name: "Breathing".into(),
                    value: 3,
                    flags: mode_flags::HAS_SPEED
                        | mode_flags::HAS_BRIGHTNESS
                        | mode_flags::HAS_MODE_SPECIFIC_COLOR,
                    speed_min: 10,
                    speed_max: 1,
                    brightness_min: 0,
                    brightness_max: 100,
                    colors_min: 1,
                    colors_max: 2,
                    speed: 5,
                    brightness: 80,
                    direction: Direction::Right,
                    color_mode: ColorMode::ModeSpecific,
                    colors: vec![Rgb::new(255, 0, 0), Rgb::new(0, 0, 255)],
                },
            ],
            zones: vec![Zone {
                name: "Keys".into(),
                kind: ZoneType::Matrix,
                leds_min: 3,
                leds_max: 3,
                leds_count: 3,
                matrix: Some(MatrixMap {
                    height: 2,
                    width: 2,
                    map: vec![0, 1, MatrixMap::EMPTY, 2],
                }),
                segments: vec![Segment {
                    name: "Seg".into(),
                    kind: 1,
                    start_idx: 0,
                    leds_count: 2,
                }],
            }],
            leds: (0..3)
                .map(|i| Led {
                    name: format!("Key {i}"),
                    value: i,
                })
                .collect(),
            colors: vec![Rgb::new(1, 2, 3); 3],
        }
    }

    #[test]
    fn color_packing() {
        let c = Rgb::new(0x12, 0x34, 0x56);
        assert_eq!(c.to_u32(), 0x0056_3412);
        assert_eq!(Rgb::from_u32(0x0056_3412), c);
        assert_eq!(Rgb::from_hex("#123456"), Some(c));
        assert_eq!(c.hex(), "#123456");
    }

    #[test]
    fn hsv() {
        assert_eq!(Rgb::from_hsv(0.0, 1.0, 1.0), Rgb::new(255, 0, 0));
        assert_eq!(Rgb::from_hsv(120.0, 1.0, 1.0), Rgb::new(0, 255, 0));
        assert_eq!(Rgb::from_hsv(240.0, 1.0, 1.0), Rgb::new(0, 0, 255));
    }

    #[test]
    fn controller_roundtrip_all_protocols() {
        for proto in 0..=CLIENT_PROTOCOL_VERSION {
            let mut c = sample();
            let bytes = encode_controller(&c, proto);
            let parsed = parse_controller(&bytes, proto).unwrap();
            if proto < 1 {
                c.vendor.clear();
            }
            if proto < 3 {
                for m in &mut c.modes {
                    m.brightness_min = 0;
                    m.brightness_max = 0;
                    m.brightness = 0;
                }
            }
            if proto < 4 {
                c.zones[0].segments.clear();
            }
            assert_eq!(parsed, c, "protocol {proto}");
        }
    }

    #[test]
    fn truncated_is_error() {
        let bytes = encode_controller(&sample(), 4);
        assert!(parse_controller(&bytes[..bytes.len() - 3], 4).is_err());
    }

    #[test]
    fn profiles_roundtrip() {
        let names = vec!["Gaming".to_string(), "Night".to_string()];
        assert_eq!(
            parse_profile_list(&encode_profile_list(&names)).unwrap(),
            names
        );
    }

    #[test]
    fn direct_mode_lookup() {
        assert_eq!(sample().direct_mode_index(), Some(0));
    }
}
