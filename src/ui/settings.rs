//! Preferences persisted between runs as a small `key = value` file.

use std::path::PathBuf;
use std::{env, fs, io};

use super::color::Hsv;

pub struct Settings {
    pub host: String,
    pub port: u16,
    pub auto_connect: bool,
    pub paint: Hsv,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 6742,
            auto_connect: true,
            paint: Hsv {
                h: 265.0,
                s: 0.65,
                v: 1.0,
            },
        }
    }
}

fn path() -> Option<PathBuf> {
    let base = if let Some(x) = env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(x)
    } else if cfg!(windows) {
        PathBuf::from(env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(env::var_os("HOME")?).join("Library/Application Support")
    } else {
        PathBuf::from(env::var_os("HOME")?).join(".config")
    };
    Some(base.join("openrgb-tui").join("settings.conf"))
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Self::default();
        let Some(text) = path().and_then(|p| fs::read_to_string(p).ok()) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "host" if !v.is_empty() => s.host = v.to_owned(),
                "port" => s.port = v.parse().unwrap_or(s.port),
                "auto_connect" => s.auto_connect = v == "true",
                "paint" => {
                    let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    if let [h, sat, val] = p[..] {
                        s.paint = Hsv { h, s: sat, v: val };
                    }
                }
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(p) = path() else {
            return Ok(());
        };
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir)?;
        }
        let Hsv { h, s, v } = self.paint;
        fs::write(
            p,
            format!(
                "host = {}\nport = {}\nauto_connect = {}\npaint = {h},{s},{v}\n",
                self.host, self.port, self.auto_connect
            ),
        )
    }
}
