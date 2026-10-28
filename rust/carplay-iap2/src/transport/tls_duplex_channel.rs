// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! A rustls client carried by an owned DuplexStream, with no socket assumptions.
use crate::{invalid, DuplexStream, Receive, Result};
use std::{
    io::{self, Cursor, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
pub struct TlsDuplexChannel {
    underlying: Arc<dyn DuplexStream>,
    engine: Mutex<rustls::ClientConnection>,
    read: Mutex<()>,
    closed: AtomicBool,
}
impl TlsDuplexChannel {
    pub fn open(
        underlying: Arc<dyn DuplexStream>,
        engine: rustls::ClientConnection,
    ) -> Result<Arc<Self>> {
        let result = Arc::new(Self {
            underlying,
            engine: Mutex::new(engine),
            read: Mutex::new(()),
            closed: AtomicBool::new(false),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let handshake = (|| loop {
            {
                let mut engine = result.engine.lock().unwrap();
                result.flush(&mut engine)?;
                if !engine.is_handshaking() {
                    return Ok(());
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "TLS handshake timed out",
                ));
            }
            match result.underlying.recv(16384, remaining)? {
                Receive::Data(b) if !b.is_empty() => result.feed(&b)?,
                Receive::Timeout => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "TLS handshake timed out",
                    ))
                }
                _ => return Err(invalid("TLS transport ended during handshake")),
            }
        })();
        if let Err(e) = handshake {
            let _ = result.underlying.close();
            return Err(e);
        }
        Ok(result)
    }
    fn flush(&self, engine: &mut rustls::ClientConnection) -> Result<()> {
        let mut output = Vec::new();
        while engine.wants_write() {
            let n = engine.write_tls(&mut output)?;
            if n == 0 {
                return Err(invalid("TLS send made no progress"));
            }
        }
        if !output.is_empty() {
            self.underlying.send(&output)?;
        }
        Ok(())
    }
    fn feed(&self, bytes: &[u8]) -> Result<()> {
        let mut engine = self.engine.lock().unwrap();
        let mut input = Cursor::new(bytes);
        while (input.position() as usize) < bytes.len() {
            if engine.read_tls(&mut input)? == 0 {
                return Err(invalid("TLS receive made no progress"));
            }
            engine
                .process_new_packets()
                .map_err(|_| invalid("TLS channel failed"))?;
        }
        self.flush(&mut engine)
    }
    fn check(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "TLS channel is closed",
            ))
        } else {
            Ok(())
        }
    }
    fn fail<T>(&self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.closed.store(true, Ordering::Release);
            let _ = self.underlying.close();
        }
        result
    }
}
impl DuplexStream for TlsDuplexChannel {
    fn send(&self, data: &[u8]) -> Result<()> {
        self.check()?;
        self.fail((|| {
            let mut engine = self.engine.lock().unwrap();
            engine.writer().write_all(data)?;
            self.flush(&mut engine)
        })())
    }
    fn recv(&self, max_bytes: usize, timeout: Duration) -> Result<Receive> {
        if max_bytes == 0 {
            return Err(invalid("maxBytes must be positive"));
        }
        let _guard = self.read.lock().unwrap();
        let deadline = Instant::now() + timeout;
        self.fail((|| loop {
            {
                let mut engine = self.engine.lock().unwrap();
                let mut bytes = vec![0; max_bytes.min(1024 * 1024)];
                match engine.reader().read(&mut bytes) {
                    Ok(0) => {
                        drop(engine);
                        let _ = self.close();
                        return Ok(Receive::Eof);
                    }
                    Ok(n) => {
                        bytes.truncate(n);
                        return Ok(Receive::Data(bytes));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                    Err(e) => return Err(e),
                }
            }
            self.check()?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(Receive::Timeout);
            }
            match self.underlying.recv(16384, remaining)? {
                Receive::Data(b) if !b.is_empty() => self.feed(&b)?,
                Receive::Timeout => return Ok(Receive::Timeout),
                _ => return Err(invalid("TLS transport ended without close_notify")),
            }
        })())
    }
    fn close(&self) -> Result<()> {
        if !self.closed.swap(true, Ordering::AcqRel) {
            {
                let mut engine = self.engine.lock().unwrap();
                engine.send_close_notify();
                let _ = self.flush(&mut engine);
            }
            self.underlying.close()?;
        }
        Ok(())
    }
}
impl Drop for TlsDuplexChannel {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
