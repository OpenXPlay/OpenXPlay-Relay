// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::time::Instant;
use std::{sync::Arc, time::Duration};

use std::collections::VecDeque;
use std::sync::Mutex;
struct Receiver {
    framer: Iap2CsmFramer,
    frames: VecDeque<Iap2Frame>,
    bytes: usize,
}
pub struct Iap2CsmChannel {
    link: Arc<Iap2LinkChannel>,
    send_lock: Mutex<()>,
    receive: Mutex<Receiver>,
    failure: Mutex<Option<String>>,
}
impl Iap2CsmChannel {
    pub fn open_wireless(stream: Arc<dyn DuplexStream>) -> Result<Arc<Self>> {
        Self::open(stream, false)
    }
    pub fn open_tunnel(stream: Arc<dyn DuplexStream>) -> Result<Arc<Self>> {
        Self::open(stream, true)
    }
    fn open(stream: Arc<dyn DuplexStream>, tunnel: bool) -> Result<Arc<Self>> {
        let c = Iap2LinkConfig {
            max_outgoing: 4,
            control_session_version: 2,
            zero_acknowledgements: tunnel,
            ..Default::default()
        };
        let link = if tunnel {
            Iap2LinkChannel::open_tunnel(stream, c)?
        } else {
            Iap2LinkChannel::open_wireless(stream, c)?
        };
        Ok(Arc::new(Self {
            link,
            send_lock: Mutex::new(()),
            receive: Mutex::new(Receiver {
                framer: Iap2CsmFramer::new(),
                frames: VecDeque::new(),
                bytes: 0,
            }),
            failure: Mutex::new(None),
        }))
    }
    fn fail<T>(&self, e: std::io::Error) -> Result<T> {
        let message = {
            let mut f = self.failure.lock().unwrap();
            f.get_or_insert_with(|| e.to_string()).clone()
        };
        let _ = self.link.close();
        Err(invalid(message))
    }
    fn check(&self) -> Result<()> {
        if let Some(f) = &*self.failure.lock().unwrap() {
            return Err(invalid(f.clone()));
        }
        if self.link.is_closed() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "CSM channel is closed",
            ));
        }
        Ok(())
    }
    fn require_timeout(timeout: Duration) -> Result<()> {
        if timeout > Duration::from_secs(300) {
            return Err(invalid("timeoutMillis must be in 0..300000"));
        }
        Ok(())
    }
    pub fn await_ready(&self, timeout: Duration) -> Result<bool> {
        Self::require_timeout(timeout)?;
        self.check()?;
        match self.link.await_ready(timeout) {
            Ok(v) => Ok(v),
            Err(e) => self.fail(e),
        }
    }
    pub fn send(&self, frame: &Iap2Frame, timeout: Duration) -> Result<()> {
        Self::require_timeout(timeout)?;
        let _guard = self.send_lock.lock().unwrap();
        self.check()?;
        let limit = self
            .link
            .peer_max_control_payload_bytes()
            .ok_or_else(|| invalid("CSM send requires a successful iAP2 awaitReady"))?;
        let chunks = Iap2CsmFramer::split_for_link(frame, limit)?;
        let started = Instant::now();
        for chunk in chunks {
            match self
                .link
                .send_control(&chunk, timeout.saturating_sub(started.elapsed()))
            {
                Ok(true) => {}
                Ok(false) => {
                    return self.fail(invalid(
                        "iAP2 could not queue the complete CSM frame before its timeout",
                    ))
                }
                Err(e) => return self.fail(e),
            }
        }
        Ok(())
    }
    pub fn recv(&self, timeout: Duration) -> Result<Option<Iap2Frame>> {
        Self::require_timeout(timeout)?;
        let mut r = self.receive.lock().unwrap();
        if let Some(frame) = r.frames.pop_front() {
            r.bytes -= frame.payload.len() + 6;
            return Ok(Some(frame));
        }
        let started = Instant::now();
        loop {
            self.check()?;
            let control = match self
                .link
                .recv_control(timeout.saturating_sub(started.elapsed()))
            {
                Ok(v) => v,
                Err(e) => return self.fail(e),
            };
            match control {
                Receive::Timeout => return Ok(None),
                Receive::Eof => return self.fail(invalid("iAP2 link ended")),
                Receive::Data(bytes) => {
                    for frame in r.framer.offer(&bytes)? {
                        let n = frame.payload.len() + 6;
                        if r.frames.len() >= 64 || n > 1048576 - r.bytes {
                            return self.fail(invalid("CSM received-frame queue limit exceeded"));
                        }
                        r.bytes += n;
                        r.frames.push_back(frame);
                    }
                }
            }
            if let Some(frame) = r.frames.pop_front() {
                r.bytes -= frame.payload.len() + 6;
                return Ok(Some(frame));
            }
            if started.elapsed() >= timeout {
                return Ok(None);
            }
        }
    }
    pub fn close(&self) -> Result<()> {
        self.link.close()
    }
    pub fn is_closed(&self) -> bool {
        self.link.is_closed() || self.failure.lock().unwrap().is_some()
    }
}

/// Protocol message, field, or framing constant.
pub const MAX_PENDING_FRAMES: u64 = 64;
/// Protocol message, field, or framing constant.
pub const MAX_PENDING_FRAME_BYTES: u64 = 1_048_576;
/// Protocol message, field, or framing constant.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;
/// Protocol message, field, or framing constant.
pub const DEFAULT_SEND_TIMEOUT_MILLIS: u64 = 5_000;

/// Constant expression: `5 * 60 * 1_000`.
pub const MAX_TIMEOUT_MILLIS: u64 = 300000;
