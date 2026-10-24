//! Background thread that owns the network connection, so the UI never blocks.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::Duration;

use crate::client::Client;
use crate::protocol::{Controller, Mode, Rgb, pkt};

const CLIENT_NAME: &str = "OpenRGB TUI";
const IDLE_POLL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum Command {
    Connect {
        host: String,
        port: u16,
    },
    Disconnect,
    Refresh,
    UpdateMode {
        dev: u32,
        idx: usize,
        mode: Mode,
        save: bool,
    },
    SetCustomMode {
        dev: u32,
    },
    UpdateLeds {
        dev: u32,
        colors: Vec<Rgb>,
    },
    UpdateZoneLeds {
        dev: u32,
        zone: u32,
        colors: Vec<Rgb>,
    },
    ResizeZone {
        dev: u32,
        zone: u32,
        size: u32,
    },
    LoadProfile(String),
    SaveProfile(String),
    DeleteProfile(String),
}

#[derive(Debug)]
pub enum Event {
    Connected { proto: u32, server_proto: u32 },
    Disconnected(Option<String>),
    Devices(Vec<Controller>),
    Device(u32, Box<Controller>),
    Profiles(Vec<String>),
    Error(String),
}

pub struct Handle {
    tx: Sender<Command>,
    pub rx: Receiver<Event>,
}

impl Handle {
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
}

pub fn spawn(repaint: impl Fn() + Send + 'static) -> Handle {
    let (tx, cmd_rx) = channel();
    let (ev_tx, rx) = channel();
    thread::Builder::new()
        .name("openrgb-worker".into())
        .spawn(move || {
            Worker {
                rx: cmd_rx,
                tx: ev_tx,
                repaint: Box::new(repaint),
                client: None,
                devices: Vec::new(),
            }
            .run()
        })
        .expect("spawn worker thread");
    Handle { tx, rx }
}

struct Worker {
    rx: Receiver<Command>,
    tx: Sender<Event>,
    repaint: Box<dyn Fn() + Send>,
    client: Option<Client>,
    devices: Vec<Controller>,
}

impl Worker {
    fn emit(&self, e: Event) {
        let _ = self.tx.send(e);
        (self.repaint)();
    }

    fn run(mut self) {
        loop {
            let mut batch = Vec::new();
            match self.rx.recv_timeout(IDLE_POLL) {
                Ok(c) => batch.push(c),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            batch.extend(self.rx.try_iter());

            for c in coalesce(batch) {
                if let Err(e) = self.handle(c) {
                    self.fail(e);
                }
            }
            if let Err(e) = self.tick() {
                self.fail(e);
            }
        }
    }

    fn fail(&mut self, e: crate::protocol::Error) {
        if matches!(e, crate::protocol::Error::Io(_)) {
            self.client = None;
            self.devices.clear();
            self.emit(Event::Disconnected(Some(e.to_string())));
        } else {
            self.emit(Event::Error(e.to_string()));
        }
    }

    fn client(&mut self) -> crate::protocol::Result<&mut Client> {
        self.client
            .as_mut()
            .ok_or_else(|| crate::protocol::Error::Unexpected("not connected".into()))
    }

    fn refresh(&mut self) -> crate::protocol::Result<()> {
        let c = self.client()?;
        let devices = c.controllers()?;
        let profiles = c.profiles()?;
        self.devices = devices.clone();
        self.emit(Event::Devices(devices));
        self.emit(Event::Profiles(profiles));
        Ok(())
    }

    fn refresh_device(&mut self, dev: u32) -> crate::protocol::Result<()> {
        let ctrl = self.client()?.controller(dev)?;
        if let Some(d) = self.devices.get_mut(dev as usize) {
            *d = ctrl.clone();
        }
        self.emit(Event::Device(dev, Box::new(ctrl)));
        Ok(())
    }

    fn handle(&mut self, cmd: Command) -> crate::protocol::Result<()> {
        match cmd {
            Command::Connect { host, port } => {
                self.client = None;
                let c = match Client::connect(&host, port, CLIENT_NAME) {
                    Ok(c) => c,
                    Err(e) => {
                        self.emit(Event::Disconnected(Some(format!(
                            "Could not reach {host}:{port} — {e}"
                        ))));
                        return Ok(());
                    }
                };
                let (proto, server_proto) = (c.proto, c.server_proto);
                self.client = Some(c);
                self.emit(Event::Connected {
                    proto,
                    server_proto,
                });
                self.refresh()?;
            }
            Command::Disconnect => {
                self.client = None;
                self.devices.clear();
                self.emit(Event::Disconnected(None));
            }
            Command::Refresh => self.refresh()?,
            Command::UpdateMode {
                dev,
                idx,
                mode,
                save,
            } => {
                // No refresh: echoing server state back would fight a slider being dragged.
                self.client()?.update_mode(dev, idx, &mode, save)?;
                if let Some(d) = self.devices.get_mut(dev as usize) {
                    d.active_mode = idx as i32;
                    if let Some(m) = d.modes.get_mut(idx) {
                        *m = mode;
                    }
                }
            }
            Command::SetCustomMode { dev } => {
                self.client()?.set_custom_mode(dev)?;
                self.refresh_device(dev)?;
            }
            Command::UpdateLeds { dev, colors } => {
                self.client()?.update_leds(dev, &colors)?;
                if let Some(d) = self.devices.get_mut(dev as usize) {
                    d.colors = colors;
                }
            }
            Command::UpdateZoneLeds { dev, zone, colors } => {
                self.client()?.update_zone_leds(dev, zone, &colors)?
            }
            Command::ResizeZone { dev, zone, size } => {
                self.client()?.resize_zone(dev, zone, size)?;
                self.refresh_device(dev)?;
            }
            Command::LoadProfile(name) => {
                self.client()?
                    .profile_op(pkt::REQUEST_LOAD_PROFILE, &name)?;
                // The server applies the profile asynchronously.
                thread::sleep(Duration::from_millis(250));
                self.refresh()?;
            }
            Command::SaveProfile(name) => {
                self.client()?
                    .profile_op(pkt::REQUEST_SAVE_PROFILE, &name)?;
                thread::sleep(Duration::from_millis(100));
                let p = self.client()?.profiles()?;
                self.emit(Event::Profiles(p));
            }
            Command::DeleteProfile(name) => {
                self.client()?
                    .profile_op(pkt::REQUEST_DELETE_PROFILE, &name)?;
                thread::sleep(Duration::from_millis(100));
                let p = self.client()?.profiles()?;
                self.emit(Event::Profiles(p));
            }
        }
        Ok(())
    }

    fn tick(&mut self) -> crate::protocol::Result<()> {
        let Some(client) = self.client.as_mut() else {
            return Ok(());
        };
        client.poll()?;
        if client.devices_changed {
            self.refresh()?;
        }
        Ok(())
    }
}

/// Drops LED updates superseded later in the same batch, so dragging a color
/// picker never builds up a backlog.
fn coalesce(batch: Vec<Command>) -> Vec<Command> {
    let mut out: Vec<Command> = Vec::with_capacity(batch.len());
    for c in batch {
        let superseded = |prev: &Command| match (prev, &c) {
            (Command::UpdateLeds { dev: a, .. }, Command::UpdateLeds { dev: b, .. }) => a == b,
            (
                Command::UpdateZoneLeds {
                    dev: a, zone: za, ..
                },
                Command::UpdateZoneLeds {
                    dev: b, zone: zb, ..
                },
            ) => a == b && za == zb,
            (
                Command::UpdateMode {
                    dev: a,
                    idx: ia,
                    save: false,
                    ..
                },
                Command::UpdateMode {
                    dev: b,
                    idx: ib,
                    save: false,
                    ..
                },
            ) => a == b && ia == ib,
            _ => false,
        };
        if let Some(last) = out.last()
            && superseded(last)
        {
            out.pop();
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_consecutive_led_updates() {
        let b = vec![
            Command::UpdateLeds {
                dev: 0,
                colors: vec![Rgb::BLACK],
            },
            Command::UpdateLeds {
                dev: 0,
                colors: vec![Rgb::WHITE],
            },
            Command::UpdateLeds {
                dev: 1,
                colors: vec![Rgb::BLACK],
            },
            Command::Refresh,
        ];
        let out = coalesce(b);
        assert_eq!(out.len(), 3);
        assert!(
            matches!(&out[0], Command::UpdateLeds { dev: 0, colors } if colors[0] == Rgb::WHITE)
        );
    }
}
