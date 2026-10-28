// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! USBMUX v2 handshake and outbound TCP byte streams over a claimed bulk pipe.
use super::usb_mux_frame_buffer::{be16, be32, UsbMuxFrame, UsbMuxFrameBuffer};
use crate::{invalid, DuplexStream, Receive, Result};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    sync::{Arc, Condvar, Mutex, Weak},
    thread,
    time::{Duration, Instant},
};
pub const LOCKDOWN_PORT: u16 = 62078;
/// Claimed bulk pipe with the source's distinct handshake and ordinary write budgets.
pub trait UsbMuxPipe: DuplexStream {
    fn send_with_timeout(&self, data: &[u8], timeout: Duration) -> Result<()>;
}
impl UsbMuxPipe for super::iphone_usb_host::Iap2UsbSession {
    fn send_with_timeout(&self, data: &[u8], timeout: Duration) -> Result<()> {
        Ok(self.write(data, timeout)?)
    }
}

struct State {
    closed: bool,
    failure: Option<String>,
    sequence: u16,
    ack: u16,
    next_port: u16,
    connections: BTreeMap<u16, Arc<Iap2UsbMuxTcpConnection>>,
}
pub struct Iap2UsbMuxHost {
    pipe: Arc<dyn UsbMuxPipe>,
    state: Mutex<State>,
    write: Mutex<()>,
    frames: Mutex<UsbMuxFrameBuffer>,
    reader: Mutex<Option<thread::JoinHandle<()>>>,
}
impl Iap2UsbMuxHost {
    /// Performs VERSION then SETUP before starting the reader. Owns the supplied pipe.
    pub fn open(
        pipe: Arc<dyn UsbMuxPipe>,
        read_timeout: Duration,
        diagnostic: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Arc<Self>> {
        if read_timeout.is_zero() || read_timeout > Duration::from_secs(30) {
            return Err(invalid("read timeout must be in 1..30000 ms"));
        }
        let host = Arc::new(Self {
            pipe,
            state: Mutex::new(State {
                closed: false,
                failure: None,
                sequence: 0,
                ack: 0,
                next_port: 1,
                connections: BTreeMap::new(),
            }),
            write: Mutex::new(()),
            frames: Mutex::new(UsbMuxFrameBuffer::new(diagnostic)),
            reader: Mutex::new(None),
        });
        if let Err(e) = host.begin() {
            let _ = host.close();
            return Err(e);
        }
        let weak = Arc::downgrade(&host);
        *host.reader.lock().unwrap() = Some(
            thread::Builder::new()
                .name("iap2-usbmux-reader".into())
                .spawn(move || {
                    while let Some(host) = weak.upgrade() {
                        if host.state.lock().unwrap().closed {
                            break;
                        }
                        let result = host.take_frame(read_timeout).and_then(|f| {
                            if let Some(f) = f {
                                if f.protocol == 6 {
                                    host.dispatch(&f.payload)?;
                                }
                                Ok(())
                            } else {
                                Ok(())
                            }
                        });
                        if let Err(e) = result {
                            host.fail(e.to_string());
                            break;
                        }
                    }
                })?,
        );
        Ok(host)
    }
    fn begin(&self) -> Result<()> {
        let mut version = vec![0; 20];
        version[4..8].copy_from_slice(&20u32.to_be_bytes());
        version[8..12].copy_from_slice(&2u32.to_be_bytes());
        self.pipe
            .send_with_timeout(&version, Duration::from_secs(60))?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut stale = 0;
        loop {
            let f = self
                .take_frame(deadline.saturating_duration_since(Instant::now()))?
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Timed out waiting for the USBMUX version reply",
                    )
                })?;
            if f.protocol == 0 && f.length == 20 && f.word8 == 2 {
                break;
            }
            stale += 1;
            if f.protocol != 6 || stale > 32 {
                return Err(invalid(format!(
                    "Invalid USBMUX version reply: proto={} length={} version={}",
                    f.protocol, f.length, f.word8
                )));
            }
        }
        self.send_frame(2, &[7])
    }
    pub fn connect(
        self: &Arc<Self>,
        destination_port: u16,
        timeout: Duration,
    ) -> Result<Arc<Iap2UsbMuxTcpConnection>> {
        if destination_port == 0 || timeout.is_zero() {
            return Err(invalid("destination port and timeout must be positive"));
        }
        let connection = {
            let mut s = self.state.lock().unwrap();
            check_host(&s)?;
            let mut port = None;
            for _ in 0..u16::MAX {
                let p = s.next_port;
                s.next_port = if p == u16::MAX { 1 } else { p + 1 };
                if !s.connections.contains_key(&p) {
                    port = Some(p);
                    break;
                }
            }
            let port = port.ok_or_else(|| invalid("USBMUX has no free TCP source ports"))?;
            let c = Arc::new(Iap2UsbMuxTcpConnection {
                host: Arc::downgrade(self),
                source_port: port,
                destination_port,
                state: Mutex::new(TcpState {
                    seq: 0,
                    ack: 0,
                    connected: false,
                    closed: false,
                    failure: None,
                    received: VecDeque::new(),
                    queued: 0,
                }),
                changed: Condvar::new(),
                write: Mutex::new(()),
            });
            s.connections.insert(port, c.clone());
            c
        };
        let result = (|| {
            connection.control(2)?;
            let deadline = Instant::now() + timeout;
            let mut s = connection.state.lock().unwrap();
            while !s.connected && !s.closed {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("USBMUX TCP connection to port {destination_port} timed out"),
                    ));
                }
                s = connection.changed.wait_timeout(s, remaining).unwrap().0;
            }
            check_tcp(&s)?;
            Ok(())
        })();
        if let Err(e) = result {
            connection.abort();
            return Err(e);
        }
        Ok(connection)
    }
    pub fn close(&self) -> Result<()> {
        let connections = {
            let mut s = self.state.lock().unwrap();
            s.closed = true;
            std::mem::take(&mut s.connections)
        };
        for c in connections.values() {
            c.close_from_host(None);
        }
        let result = self.pipe.close();
        if let Some(reader) = self.reader.lock().unwrap().take() {
            if reader.thread().id() != thread::current().id() {
                let _ = reader.join();
            }
        }
        result
    }
    fn fail(&self, message: String) {
        let connections = {
            let mut s = self.state.lock().unwrap();
            if s.closed {
                return;
            }
            s.closed = true;
            s.failure = Some(message.clone());
            std::mem::take(&mut s.connections)
        };
        for c in connections.values() {
            c.close_from_host(Some(message.clone()));
        }
        let _ = self.pipe.close();
    }
    fn take_frame(&self, timeout: Duration) -> Result<Option<UsbMuxFrame>> {
        let deadline = Instant::now() + timeout;
        loop {
            check_host(&self.state.lock().unwrap())?;
            if let Some(f) = self.frames.lock().unwrap().take_frame()? {
                self.state.lock().unwrap().ack = f.sequence;
                return Ok(Some(f));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            match self.pipe.recv(65536, remaining)? {
                Receive::Data(b) => self.frames.lock().unwrap().append(&b),
                Receive::Timeout => return Ok(None),
                Receive::Eof => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "USBMUX pipe ended",
                    ))
                }
            }
        }
    }
    fn send_frame(&self, protocol: u32, payload: &[u8]) -> Result<()> {
        let _lock = self.write.lock().unwrap();
        let (seq, ack) = {
            let s = self.state.lock().unwrap();
            check_host(&s)?;
            (s.sequence, s.ack)
        };
        let mut f = Vec::with_capacity(16 + payload.len());
        f.extend_from_slice(&protocol.to_be_bytes());
        f.extend_from_slice(&((16 + payload.len()) as u32).to_be_bytes());
        f.extend_from_slice(&0xfeedfaceu32.to_be_bytes());
        f.extend_from_slice(&seq.to_be_bytes());
        f.extend_from_slice(&ack.to_be_bytes());
        f.extend_from_slice(payload);
        if let Err(e) = self.pipe.send_with_timeout(&f, Duration::from_secs(2)) {
            self.fail(e.to_string());
            return Err(e);
        }
        let mut s = self.state.lock().unwrap();
        s.sequence = s.sequence.wrapping_add(1);
        Ok(())
    }
    fn send_tcp(
        &self,
        c: &Iap2UsbMuxTcpConnection,
        seq: u32,
        ack: u32,
        flags: u8,
        data: &[u8],
    ) -> Result<()> {
        let mut b = vec![0; 20 + data.len()];
        b[..2].copy_from_slice(&c.source_port.to_be_bytes());
        b[2..4].copy_from_slice(&c.destination_port.to_be_bytes());
        b[4..8].copy_from_slice(&seq.to_be_bytes());
        b[8..12].copy_from_slice(&ack.to_be_bytes());
        b[12] = 0x50;
        b[13] = flags;
        b[14..16].copy_from_slice(&512u16.to_be_bytes());
        b[20..].copy_from_slice(data);
        self.send_frame(6, &b)
    }
    fn dispatch(&self, b: &[u8]) -> Result<()> {
        if b.len() < 20 {
            return Err(invalid("USBMUX TCP frame is shorter than its header"));
        }
        let h = (b[12] >> 4) as usize * 4;
        if h < 20 || h > b.len() {
            return Err(invalid("Invalid USBMUX TCP header length"));
        }
        let c = self
            .state
            .lock()
            .unwrap()
            .connections
            .get(&be16(b, 2))
            .cloned();
        if let Some(c) = c {
            c.on_packet(b[13], be32(b, 4), &b[h..])?;
        }
        Ok(())
    }
    fn remove(&self, port: u16) {
        self.state.lock().unwrap().connections.remove(&port);
    }
}
fn check_host(s: &State) -> Result<()> {
    if let Some(e) = &s.failure {
        Err(io::Error::other(e.clone()))
    } else if s.closed {
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "USBMUX host is closed",
        ))
    } else {
        Ok(())
    }
}
struct TcpState {
    seq: u32,
    ack: u32,
    connected: bool,
    closed: bool,
    failure: Option<String>,
    received: VecDeque<Vec<u8>>,
    queued: usize,
}
/// Thread-safe stream for one mux TCP connection. FIN preserves queued bytes; RST is an error.
pub struct Iap2UsbMuxTcpConnection {
    host: Weak<Iap2UsbMuxHost>,
    pub source_port: u16,
    pub destination_port: u16,
    state: Mutex<TcpState>,
    changed: Condvar,
    write: Mutex<()>,
}
impl Iap2UsbMuxTcpConnection {
    fn host(&self) -> Result<Arc<Iap2UsbMuxHost>> {
        self.host
            .upgrade()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "USBMUX host is closed"))
    }
    fn control(&self, flags: u8) -> Result<()> {
        let _lock = self.write.lock().unwrap();
        let (seq, ack) = {
            let s = self.state.lock().unwrap();
            (s.seq, s.ack)
        };
        self.host()?.send_tcp(self, seq, ack, flags, &[])
    }
    fn abort(&self) {
        self.close_from_host(None);
        let _ = self.control(0x14);
        if let Ok(h) = self.host() {
            h.remove(self.source_port);
        }
    }
    fn close_from_host(&self, failure: Option<String>) {
        let mut s = self.state.lock().unwrap();
        if !s.closed {
            s.closed = true;
            s.failure = failure;
            self.changed.notify_all();
        }
    }
    fn on_packet(&self, flags: u8, sequence: u32, payload: &[u8]) -> Result<()> {
        if flags & 4 != 0 {
            self.close_from_host(Some("USBMUX TCP connection was reset by the peer".into()));
            self.host()?.remove(self.source_port);
            return Ok(());
        }
        if flags & 0x12 == 0x12 {
            {
                let mut s = self.state.lock().unwrap();
                if s.closed {
                    return Ok(());
                }
                s.seq = s.seq.wrapping_add(1);
                s.ack = sequence.wrapping_add(1);
            }
            self.control(0x10)?;
            let mut s = self.state.lock().unwrap();
            if !s.closed {
                s.connected = true;
                self.changed.notify_all();
            }
            return Ok(());
        }
        if !payload.is_empty() {
            {
                let mut s = self.state.lock().unwrap();
                if s.closed {
                    return Ok(());
                }
                if s.queued + payload.len() > 4 * 1024 * 1024 {
                    return Err(invalid("USBMUX TCP receive queue exceeded its bounds"));
                }
                s.ack = s.ack.wrapping_add(payload.len() as u32);
                s.queued += payload.len();
                s.received.push_back(payload.to_vec());
                self.changed.notify_all();
            }
            self.control(0x10)?;
        }
        if flags & 1 != 0 {
            {
                let mut s = self.state.lock().unwrap();
                if s.closed {
                    return Ok(());
                }
                s.ack = s.ack.wrapping_add(1);
            }
            self.control(0x10)?;
            self.close_from_host(None);
            self.host()?.remove(self.source_port);
        }
        Ok(())
    }
}
fn check_tcp(s: &TcpState) -> Result<()> {
    if let Some(e) = &s.failure {
        Err(io::Error::new(io::ErrorKind::ConnectionReset, e.clone()))
    } else if !s.connected || s.closed {
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "USBMUX TCP connection is not open",
        ))
    } else {
        Ok(())
    }
}
impl DuplexStream for Iap2UsbMuxTcpConnection {
    fn send(&self, data: &[u8]) -> Result<()> {
        let _lock = self.write.lock().unwrap();
        check_tcp(&self.state.lock().unwrap())?;
        for chunk in data.chunks(16384) {
            let (seq, ack) = {
                let s = self.state.lock().unwrap();
                check_tcp(&s)?;
                (s.seq, s.ack)
            };
            self.host()?.send_tcp(self, seq, ack, 0x10, chunk)?;
            let mut s = self.state.lock().unwrap();
            s.seq = s.seq.wrapping_add(chunk.len() as u32);
        }
        Ok(())
    }
    fn recv(&self, max_bytes: usize, timeout: Duration) -> Result<Receive> {
        if max_bytes == 0 {
            return Err(invalid("maxBytes must be positive"));
        }
        let deadline = Instant::now() + timeout;
        let mut s = self.state.lock().unwrap();
        while s.received.is_empty() && !s.closed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(Receive::Timeout);
            }
            s = self.changed.wait_timeout(s, remaining).unwrap().0;
        }
        if let Some(e) = &s.failure {
            return Err(io::Error::new(io::ErrorKind::ConnectionReset, e.clone()));
        }
        if let Some(mut b) = s.received.pop_front() {
            if b.len() > max_bytes {
                let tail = b.split_off(max_bytes);
                s.received.push_front(tail);
            }
            s.queued -= b.len();
            Ok(Receive::Data(b))
        } else {
            Ok(Receive::Eof)
        }
    }
    fn close(&self) -> Result<()> {
        let connected = {
            let mut s = self.state.lock().unwrap();
            if s.closed {
                return Ok(());
            }
            s.closed = true;
            self.changed.notify_all();
            s.connected
        };
        if connected {
            let _ = self.control(0x11);
        }
        if let Ok(h) = self.host() {
            h.remove(self.source_port);
        }
        Ok(())
    }
}

impl Drop for Iap2UsbMuxHost {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
