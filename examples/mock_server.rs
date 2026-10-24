//! A fake OpenRGB SDK server with a handful of virtual devices, for developing
//! the TUI without real hardware.
//!
//! ```sh
//! cargo run --example mock_server            # listens on 127.0.0.1:6742
//! cargo run --example mock_server -- 6743    # custom port
//! ```

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use openrgb_tui::protocol::*;

const SERVER_PROTOCOL: u32 = 4;

struct State {
    devices: Vec<Controller>,
    profiles: Vec<(String, Vec<Controller>)>,
}

fn mode(name: &str, flags: u32, color_mode: ColorMode, colors: Vec<Rgb>) -> Mode {
    let has = |f| flags & f != 0;
    Mode {
        name: name.into(),
        value: 0,
        flags,
        speed_min: 0,
        speed_max: if has(mode_flags::HAS_SPEED) { 100 } else { 0 },
        brightness_min: 0,
        brightness_max: if has(mode_flags::HAS_BRIGHTNESS) {
            100
        } else {
            0
        },
        colors_min: if colors.is_empty() { 0 } else { 1 },
        colors_max: colors.len().max(if colors.is_empty() { 0 } else { 4 }) as u32,
        speed: 50,
        brightness: 100,
        direction: Direction::Right,
        color_mode,
        colors,
    }
}

fn standard_modes() -> Vec<Mode> {
    use mode_flags::*;
    vec![
        mode("Direct", HAS_PER_LED_COLOR, ColorMode::PerLed, vec![]),
        mode(
            "Static",
            HAS_MODE_SPECIFIC_COLOR | HAS_BRIGHTNESS | MANUAL_SAVE,
            ColorMode::ModeSpecific,
            vec![Rgb::new(0, 140, 255)],
        ),
        mode(
            "Breathing",
            HAS_SPEED | HAS_BRIGHTNESS | HAS_MODE_SPECIFIC_COLOR | HAS_RANDOM_COLOR | MANUAL_SAVE,
            ColorMode::ModeSpecific,
            vec![Rgb::new(255, 0, 90), Rgb::new(0, 200, 255)],
        ),
        mode(
            "Spectrum Cycle",
            HAS_SPEED | HAS_BRIGHTNESS,
            ColorMode::None,
            vec![],
        ),
        mode(
            "Rainbow Wave",
            HAS_SPEED | HAS_BRIGHTNESS | HAS_DIRECTION_LR | HAS_DIRECTION_UD,
            ColorMode::None,
            vec![],
        ),
        mode("Off", 0, ColorMode::None, vec![]),
    ]
}

fn linear(name: &str, n: u32, resizable: bool) -> Zone {
    Zone {
        name: name.into(),
        kind: ZoneType::Linear,
        leds_min: if resizable { 0 } else { n },
        leds_max: if resizable { 120 } else { n },
        leds_count: n,
        matrix: None,
        segments: vec![],
    }
}

fn device(kind: DeviceType, name: &str, vendor: &str, zones: Vec<Zone>, active: i32) -> Controller {
    let count: u32 = zones.iter().map(|z| z.leds_count).sum();
    let leds = zones
        .iter()
        .flat_map(|z| (0..z.leds_count).map(move |i| format!("{} LED {}", z.name, i + 1)))
        .enumerate()
        .map(|(i, name)| Led {
            name,
            value: i as u32,
        })
        .collect();
    let colors = (0..count)
        .map(|i| Rgb::from_hsv(i as f32 * 360.0 / count.max(1) as f32, 0.9, 1.0))
        .collect();
    Controller {
        kind,
        name: name.into(),
        vendor: vendor.into(),
        description: format!("Virtual {} for UI development", kind.label().to_lowercase()),
        version: "1.2.3".into(),
        serial: format!("MOCK-{:04X}", name.len() * 7919),
        location: "mock://localhost".into(),
        active_mode: active,
        modes: standard_modes(),
        zones,
        leds,
        colors,
    }
}

fn keyboard() -> Controller {
    const ROWS: [&[&str]; 6] = [
        &[
            "Esc", "", "F1", "F2", "F3", "F4", "", "F5", "F6", "F7", "F8", "", "F9", "F10", "F11",
            "F12",
        ],
        &[
            "`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "Bksp", "", "",
        ],
        &[
            "Tab", "Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]", "\\", "", "",
        ],
        &[
            "Caps", "A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'", "Enter", "", "", "",
        ],
        &[
            "Shift", "Z", "X", "C", "V", "B", "N", "M", ",", ".", "/", "RShift", "", "", "Up", "",
        ],
        &[
            "Ctrl", "Win", "Alt", "", "", "Space", "", "", "RAlt", "Fn", "Menu", "RCtrl", "",
            "Left", "Down", "Right",
        ],
    ];
    let width = 16;
    let mut map = Vec::new();
    let mut names = Vec::new();
    for row in ROWS {
        for col in 0..width {
            let key = row.get(col).copied().unwrap_or("");
            if key.is_empty() {
                map.push(MatrixMap::EMPTY);
            } else {
                map.push(names.len() as u32);
                names.push(format!("Key: {key}"));
            }
        }
    }
    let n = names.len() as u32;
    let mut c = device(
        DeviceType::Keyboard,
        "Aurora K7 Mechanical",
        "Lumen",
        vec![
            Zone {
                name: "Keyboard".into(),
                kind: ZoneType::Matrix,
                leds_min: n,
                leds_max: n,
                leds_count: n,
                matrix: Some(MatrixMap {
                    height: ROWS.len() as u32,
                    width: width as u32,
                    map,
                }),
                segments: vec![],
            },
            linear("Underglow", 18, false),
        ],
        0,
    );
    for (l, name) in c.leds.iter_mut().zip(names) {
        l.name = name;
    }
    c
}

fn initial_devices() -> Vec<Controller> {
    let ram = |n: u32| {
        device(
            DeviceType::Dram,
            &format!("Vengeance DDR5 DIMM {n}"),
            "Corsair",
            vec![linear("DRAM", 10, false)],
            2,
        )
    };
    vec![
        keyboard(),
        device(
            DeviceType::Mouse,
            "Glide Pro Wireless",
            "Lumen",
            vec![
                linear("Logo", 1, false),
                linear("Scroll Wheel", 1, false),
                linear("Side Strip", 8, false),
            ],
            1,
        ),
        device(
            DeviceType::Motherboard,
            "ROG STRIX X870-E",
            "ASUS",
            vec![
                linear("Aura Mainboard", 6, false),
                linear("ARGB Header 1", 30, true),
                linear("ARGB Header 2", 0, true),
            ],
            4,
        ),
        ram(1),
        ram(2),
        device(
            DeviceType::Gpu,
            "GeForce RTX 5080",
            "NVIDIA",
            vec![linear("Logo", 1, false), linear("Front Bar", 12, false)],
            3,
        ),
        device(
            DeviceType::Cooler,
            "Kraken Elite 360",
            "NZXT",
            vec![
                linear("Pump Ring", 24, false),
                linear("Fan 1", 8, false),
                linear("Fan 2", 8, false),
                linear("Fan 3", 8, false),
            ],
            4,
        ),
        device(
            DeviceType::LedStrip,
            "Desk Strip",
            "WLED",
            vec![linear("Strip", 60, true)],
            0,
        ),
    ]
}

fn read_packet(s: &mut TcpStream) -> std::io::Result<(Header, Vec<u8>)> {
    let mut hb = [0u8; HEADER_LEN];
    s.read_exact(&mut hb)?;
    let h = Header::parse(&hb).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut body = vec![0; h.size as usize];
    s.read_exact(&mut body)?;
    Ok((h, body))
}

fn cstr(b: &[u8]) -> String {
    String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap_or(&[])).into_owned()
}

fn serve(mut s: TcpStream, state: Arc<Mutex<State>>) -> std::io::Result<()> {
    let mut proto = 0;
    loop {
        let (h, body) = read_packet(&mut s)?;
        let mut r = Reader::new(&body);
        let mut st = state.lock().unwrap();
        let dev = h.device as usize;
        let reply = match h.id {
            pkt::REQUEST_PROTOCOL_VERSION => {
                let client = r.u32().unwrap_or(0);
                proto = client.min(SERVER_PROTOCOL);
                Some(SERVER_PROTOCOL.to_le_bytes().to_vec())
            }
            pkt::SET_CLIENT_NAME => {
                println!("client connected: {}", cstr(&body));
                None
            }
            pkt::REQUEST_CONTROLLER_COUNT => Some((st.devices.len() as u32).to_le_bytes().to_vec()),
            pkt::REQUEST_CONTROLLER_DATA => {
                let p = r.u32().unwrap_or(0).min(SERVER_PROTOCOL);
                st.devices.get(dev).map(|c| encode_controller(c, p))
            }
            pkt::REQUEST_PROFILE_LIST => {
                let names: Vec<String> = st.profiles.iter().map(|p| p.0.clone()).collect();
                Some(encode_profile_list(&names))
            }
            pkt::REQUEST_SAVE_PROFILE => {
                let name = cstr(&body);
                let snapshot = st.devices.clone();
                st.profiles.retain(|p| p.0 != name);
                st.profiles.push((name, snapshot));
                None
            }
            pkt::REQUEST_LOAD_PROFILE => {
                let name = cstr(&body);
                if let Some(p) = st.profiles.iter().find(|p| p.0 == name) {
                    st.devices = p.1.clone();
                }
                None
            }
            pkt::REQUEST_DELETE_PROFILE => {
                let name = cstr(&body);
                st.profiles.retain(|p| p.0 != name);
                None
            }
            pkt::RGBCONTROLLER_UPDATELEDS => {
                if let Some(c) = st.devices.get_mut(dev) {
                    let _ = r.u32();
                    if let Ok(colors) = r.list(Reader::color) {
                        for (dst, src) in c.colors.iter_mut().zip(colors) {
                            *dst = src;
                        }
                    }
                }
                None
            }
            pkt::RGBCONTROLLER_UPDATEZONELEDS => {
                if let Some(c) = st.devices.get_mut(dev) {
                    let _ = r.u32();
                    let zone = r.u32().unwrap_or(0) as usize;
                    let start = c.zone_offsets().get(zone).copied().unwrap_or(0);
                    if let Ok(colors) = r.list(Reader::color) {
                        for (dst, src) in c.colors.iter_mut().skip(start).zip(colors) {
                            *dst = src;
                        }
                    }
                }
                None
            }
            pkt::RGBCONTROLLER_SETCUSTOMMODE => {
                if let Some(c) = st.devices.get_mut(dev) {
                    c.active_mode = c.direct_mode_index().unwrap_or(0) as i32;
                }
                None
            }
            pkt::RGBCONTROLLER_UPDATEMODE | pkt::RGBCONTROLLER_SAVEMODE => {
                if let Some(c) = st.devices.get_mut(dev) {
                    let _ = r.u32();
                    let idx = r.i32().unwrap_or(-1);
                    if let (Some(m), Ok(new)) = (
                        usize::try_from(idx).ok().and_then(|i| c.modes.get_mut(i)),
                        read_mode(&mut r, proto),
                    ) {
                        m.speed = new.speed;
                        m.brightness = new.brightness;
                        m.direction = new.direction;
                        m.color_mode = new.color_mode;
                        m.colors = new.colors;
                        c.active_mode = idx;
                    }
                }
                None
            }
            pkt::RGBCONTROLLER_RESIZEZONE => {
                if let Some(c) = st.devices.get_mut(dev) {
                    let zone = r.i32().unwrap_or(-1);
                    let size = r.i32().unwrap_or(0).max(0) as u32;
                    if let Some(zi) = usize::try_from(zone).ok().filter(|&z| z < c.zones.len()) {
                        let start = c.zone_offsets()[zi];
                        let old = c.zones[zi].leds_count as usize;
                        let z = &mut c.zones[zi];
                        z.leds_count = size.clamp(z.leds_min, z.leds_max);
                        let new = z.leds_count as usize;
                        let zname = z.name.clone();
                        c.colors.splice(
                            start..start + old,
                            (0..new).map(|i| Rgb::from_hsv(i as f32 * 12.0, 1.0, 1.0)),
                        );
                        c.leds.splice(
                            start..start + old,
                            (0..new).map(|i| Led {
                                name: format!("{zname} LED {}", i + 1),
                                value: 0,
                            }),
                        );
                    }
                }
                None
            }
            other => {
                println!("unhandled packet id {other}");
                None
            }
        };
        drop(st);
        if let Some(payload) = reply {
            s.write_all(&encode_packet(h.device, h.id, &payload))?;
        }
    }
}

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(6742);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("mock OpenRGB server listening on 127.0.0.1:{port}");
    let state = Arc::new(Mutex::new(State {
        devices: initial_devices(),
        profiles: vec![("Default".into(), initial_devices())],
    }));
    for s in listener.incoming() {
        let s = s?;
        let state = state.clone();
        thread::spawn(move || {
            if let Err(e) = serve(s, state) {
                println!("client disconnected: {e}");
            }
        });
    }
    Ok(())
}
