//! Blocking OpenRGB SDK client. Owned by the worker thread.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::protocol::{self, Controller, Error, Header, Mode, Result, Rgb, Writer, pkt};

pub struct Client {
    stream: TcpStream,
    /// Negotiated protocol version: `min(client, server)`.
    pub proto: u32,
    pub server_proto: u32,
    /// Set when the server announced a device list change.
    pub devices_changed: bool,
}

impl Client {
    pub fn connect(host: &str, port: u16, name: &str) -> Result<Self> {
        let addr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| Error::Unexpected(format!("could not resolve {host}")))?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut c = Self {
            stream,
            proto: 0,
            server_proto: 0,
            devices_changed: false,
        };

        c.send(
            0,
            pkt::REQUEST_PROTOCOL_VERSION,
            &protocol::CLIENT_PROTOCOL_VERSION.to_le_bytes(),
        )?;
        // Servers predating protocol versioning never answer this request.
        c.server_proto = match c.recv(pkt::REQUEST_PROTOCOL_VERSION) {
            Ok(b) if b.len() >= 4 => u32::from_le_bytes(b[..4].try_into().unwrap()),
            Ok(_) => 0,
            Err(Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                0
            }
            Err(e) => return Err(e),
        };
        c.proto = c.server_proto.min(protocol::CLIENT_PROTOCOL_VERSION);
        c.send(0, pkt::SET_CLIENT_NAME, &protocol::nul_terminated(name))?;
        Ok(c)
    }

    fn send(&mut self, device: u32, id: u32, payload: &[u8]) -> Result<()> {
        self.stream
            .write_all(&protocol::encode_packet(device, id, payload))?;
        Ok(())
    }

    fn read_packet(&mut self) -> Result<(Header, Vec<u8>)> {
        let mut hb = [0u8; protocol::HEADER_LEN];
        self.stream.read_exact(&mut hb)?;
        let h = Header::parse(&hb)?;
        let mut body = vec![0u8; h.size as usize];
        self.stream.read_exact(&mut body)?;
        Ok((h, body))
    }

    /// Reads packets until one with `id` arrives, noting notifications on the way.
    fn recv(&mut self, id: u32) -> Result<Vec<u8>> {
        loop {
            let (h, body) = self.read_packet()?;
            if h.id == id {
                return Ok(body);
            }
            if h.id == pkt::DEVICE_LIST_UPDATED {
                self.devices_changed = true;
            }
        }
    }

    /// Non-blocking check for server-initiated packets.
    pub fn poll(&mut self) -> Result<()> {
        self.stream.set_nonblocking(true)?;
        let mut hb = [0u8; protocol::HEADER_LEN];
        let peeked = self.stream.peek(&mut hb);
        self.stream.set_nonblocking(false)?;
        match peeked {
            Ok(0) => Err(Error::Io(ErrorKind::ConnectionReset.into())),
            Ok(n) if n == protocol::HEADER_LEN => {
                let (h, _) = self.read_packet()?;
                if h.id == pkt::DEVICE_LIST_UPDATED {
                    self.devices_changed = true;
                }
                Ok(())
            }
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn controller_count(&mut self) -> Result<u32> {
        self.send(0, pkt::REQUEST_CONTROLLER_COUNT, &[])?;
        let b = self.recv(pkt::REQUEST_CONTROLLER_COUNT)?;
        protocol::Reader::new(&b).u32()
    }

    pub fn controller(&mut self, idx: u32) -> Result<Controller> {
        let payload = if self.proto >= 1 {
            self.proto.to_le_bytes().to_vec()
        } else {
            Vec::new()
        };
        self.send(idx, pkt::REQUEST_CONTROLLER_DATA, &payload)?;
        let b = self.recv(pkt::REQUEST_CONTROLLER_DATA)?;
        protocol::parse_controller(&b, self.proto)
    }

    pub fn controllers(&mut self) -> Result<Vec<Controller>> {
        self.devices_changed = false;
        let n = self.controller_count()?;
        (0..n).map(|i| self.controller(i)).collect()
    }

    pub fn update_mode(&mut self, dev: u32, idx: usize, mode: &Mode, save: bool) -> Result<()> {
        let mut w = Writer::default();
        w.i32(idx as i32);
        protocol::write_mode(&mut w, mode, self.proto);
        self.send(dev, pkt::RGBCONTROLLER_UPDATEMODE, &w.sized())?;
        if save && self.proto >= 3 {
            let mut w = Writer::default();
            w.i32(idx as i32);
            protocol::write_mode(&mut w, mode, self.proto);
            self.send(dev, pkt::RGBCONTROLLER_SAVEMODE, &w.sized())?;
        }
        Ok(())
    }

    pub fn set_custom_mode(&mut self, dev: u32) -> Result<()> {
        self.send(dev, pkt::RGBCONTROLLER_SETCUSTOMMODE, &[])
    }

    pub fn update_leds(&mut self, dev: u32, colors: &[Rgb]) -> Result<()> {
        let mut w = Writer::default();
        w.colors(colors);
        self.send(dev, pkt::RGBCONTROLLER_UPDATELEDS, &w.sized())
    }

    pub fn update_zone_leds(&mut self, dev: u32, zone: u32, colors: &[Rgb]) -> Result<()> {
        let mut w = Writer::default();
        w.u32(zone).colors(colors);
        self.send(dev, pkt::RGBCONTROLLER_UPDATEZONELEDS, &w.sized())
    }

    pub fn resize_zone(&mut self, dev: u32, zone: u32, size: u32) -> Result<()> {
        let mut w = Writer::default();
        w.i32(zone as i32).i32(size as i32);
        self.send(dev, pkt::RGBCONTROLLER_RESIZEZONE, &w.buf)
    }

    pub fn profiles(&mut self) -> Result<Vec<String>> {
        if self.proto < 2 {
            return Ok(Vec::new());
        }
        self.send(0, pkt::REQUEST_PROFILE_LIST, &[])?;
        let b = self.recv(pkt::REQUEST_PROFILE_LIST)?;
        protocol::parse_profile_list(&b)
    }

    pub fn profile_op(&mut self, id: u32, name: &str) -> Result<()> {
        self.send(0, id, &protocol::nul_terminated(name))
    }
}
