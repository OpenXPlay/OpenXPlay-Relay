// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::time::Instant;
use std::{sync::Arc, time::Duration};

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Condvar, Mutex,
};
#[derive(Default)]
struct ChannelState {
    commands: VecDeque<Vec<u8>>,
    command_bytes: usize,
    controls: VecDeque<Vec<u8>>,
    control_bytes: usize,
    ready: bool,
    peer_limit: Option<usize>,
    terminated: bool,
    failure: Option<String>,
    done: bool,
}
struct Shared {
    state: Mutex<ChannelState>,
    changed: Condvar,
    stream: Arc<dyn DuplexStream>,
    closed: AtomicBool,
}
impl Shared {
    fn close_stream(&self) -> Result<()> {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.stream.close()
        } else {
            Ok(())
        }
    }
    fn finish(&self, failure: Option<String>) {
        let mut s = self.state.lock().unwrap();
        s.terminated = true;
        s.ready = false;
        s.commands.clear();
        s.command_bytes = 0;
        s.controls.clear();
        s.control_bytes = 0;
        if s.failure.is_none() {
            s.failure = failure;
        }
        self.changed.notify_all();
    }
}
pub struct Iap2LinkChannel {
    shared: Arc<Shared>,
}
impl Iap2LinkChannel {
    pub fn open_wireless(
        stream: Arc<dyn DuplexStream>,
        config: Iap2LinkConfig,
    ) -> Result<Arc<Self>> {
        Self::open(stream, config, false)
    }
    pub fn open_tunnel(stream: Arc<dyn DuplexStream>, config: Iap2LinkConfig) -> Result<Arc<Self>> {
        Self::open(stream, config, true)
    }
    fn open(
        stream: Arc<dyn DuplexStream>,
        config: Iap2LinkConfig,
        initiator: bool,
    ) -> Result<Arc<Self>> {
        let shared = Arc::new(Shared {
            state: Mutex::new(ChannelState::default()),
            changed: Condvar::new(),
            stream,
            closed: AtomicBool::new(false),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("iap2-link".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Self::pump(&worker, config, initiator)
                }));
                let failure = match outcome {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => Some(e.to_string()),
                    Err(_) => Some("iAP2 link worker panicked".into()),
                };
                worker.finish(failure);
                if let Err(e) = worker.close_stream() {
                    let mut s = worker.state.lock().unwrap();
                    if s.failure.is_none() {
                        s.failure = Some(e.to_string());
                    }
                }
                let mut s = worker.state.lock().unwrap();
                s.done = true;
                worker.changed.notify_all();
            })?;
        Ok(Arc::new(Self { shared }))
    }
    fn pump(shared: &Shared, config: Iap2LinkConfig, initiator: bool) -> Result<()> {
        let started = Instant::now();
        let now = || started.elapsed().as_millis() as u64;
        let mut engine = Iap2LinkEngine::new(config);
        engine.start(initiator, now())?;
        let mut files = Iap2FileTransferReceiver::new(2 * 1024 * 1024);
        loop {
            if shared.state.lock().unwrap().terminated {
                return Ok(());
            }
            while engine.writable() {
                let command = {
                    let mut s = shared.state.lock().unwrap();
                    let c = s.commands.pop_front();
                    if let Some(c) = &c {
                        s.command_bytes -= c.len();
                        shared.changed.notify_all();
                    }
                    c
                };
                let Some(c) = command else {
                    break;
                };
                engine.send_control(&c, now())?;
            }
            engine.advance_time(now())?;
            let bytes = engine.take_output();
            if !bytes.is_empty() {
                shared.stream.send(&bytes)?;
            }
            while let Some(event) = engine.poll_event() {
                match event {
                    LinkEvent::Writable(value) => {
                        let mut s = shared.state.lock().unwrap();
                        if value {
                            s.peer_limit =
                                Some(engine.peer_synchronization().max_length as usize - 10);
                        }
                        s.ready = value && !s.terminated;
                        shared.changed.notify_all();
                    }
                    LinkEvent::Control(bytes) => {
                        let mut s = shared.state.lock().unwrap();
                        if s.controls.len() >= 64 || bytes.len() > 1048576 - s.control_bytes {
                            return Err(invalid("iAP2 received-control queue limit exceeded"));
                        }
                        s.control_bytes += bytes.len();
                        s.controls.push_back(bytes);
                        shared.changed.notify_all();
                    }
                    LinkEvent::Session(12, bytes) => {
                        let out = files.accept(&bytes)?;
                        for reply in out.replies {
                            engine.send_session(12, &reply, now())?;
                        }
                    }
                    LinkEvent::Session(_, _) => {}
                    LinkEvent::Dead(reason) => {
                        return Err(invalid(reason.unwrap_or_else(|| "iAP2 link ended".into())))
                    }
                }
            }
            if shared.state.lock().unwrap().terminated {
                return Ok(());
            }
            let wait = engine
                .next_deadline_millis()
                .map(|d| d.saturating_sub(now()))
                .unwrap_or(100)
                .clamp(1, 100);
            match shared.stream.recv(8192, Duration::from_millis(wait)) {
                Ok(Receive::Data(b)) => engine.feed(&b, now())?,
                Ok(Receive::Timeout) => {}
                Ok(Receive::Eof) => engine.feed_eof(),
                Err(e) => {
                    if shared.state.lock().unwrap().terminated {
                        return Ok(());
                    }
                    return Err(e);
                }
            }
        }
    }
    fn wait(
        &self,
        timeout: Duration,
        predicate: impl Fn(&ChannelState) -> bool,
    ) -> std::sync::MutexGuard<'_, ChannelState> {
        let s = self.shared.state.lock().unwrap();
        self.shared
            .changed
            .wait_timeout_while(
                s,
                timeout.min(Duration::from_millis(i64::MAX as u64 / 1_000_000)),
                |s| !predicate(s),
            )
            .unwrap()
            .0
    }
    fn check(s: &ChannelState) -> Result<()> {
        if let Some(f) = &s.failure {
            return Err(invalid(f.clone()));
        }
        if s.terminated {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "iAP2 channel is closed",
            ));
        }
        Ok(())
    }
    pub fn await_ready(&self, timeout: Duration) -> Result<bool> {
        let s = self.wait(timeout, |s| s.ready || s.terminated);
        Self::check(&s)?;
        Ok(s.ready)
    }
    pub fn peer_max_control_payload_bytes(&self) -> Option<usize> {
        self.shared.state.lock().unwrap().peer_limit
    }
    pub fn send_control(&self, bytes: &[u8], timeout: Duration) -> Result<bool> {
        if bytes.len() > 65525 {
            return Err(invalid("iAP2 control payload exceeds 65525 bytes"));
        }
        let mut s = self.wait(timeout, |s| {
            s.terminated || (s.commands.len() < 64 && bytes.len() <= 1048576 - s.command_bytes)
        });
        Self::check(&s)?;
        if s.commands.len() >= 64 || bytes.len() > 1048576 - s.command_bytes {
            return Ok(false);
        }
        s.command_bytes += bytes.len();
        s.commands.push_back(bytes.to_vec());
        self.shared.changed.notify_all();
        Ok(true)
    }
    pub fn recv_control(&self, timeout: Duration) -> Result<Receive> {
        let mut s = self.wait(timeout, |s| s.terminated || !s.controls.is_empty());
        Self::check(&s)?;
        if let Some(b) = s.controls.pop_front() {
            s.control_bytes -= b.len();
            Ok(Receive::Data(b))
        } else {
            Ok(Receive::Timeout)
        }
    }
    pub fn close(&self) -> Result<()> {
        self.shared.finish(None);
        let close = self.shared.close_stream();
        let s = self.wait(Duration::from_secs(1), |s| s.done);
        close?;
        if !s.done {
            return Err(invalid("iAP2 link worker did not stop after close"));
        }
        if let Some(f) = &s.failure {
            return Err(invalid(f.clone()));
        }
        Ok(())
    }
    pub fn is_closed(&self) -> bool {
        self.shared.state.lock().unwrap().terminated
    }
}
impl Drop for Iap2LinkChannel {
    fn drop(&mut self) {
        self.shared.finish(None);
        let _ = self.shared.close_stream();
    }
}

/// Protocol message, field, or framing constant.
pub const RECEIVE_CHUNK_BYTES: u64 = 8_192;
/// Protocol message, field, or framing constant.
pub const RECEIVE_POLL_MILLIS: u64 = 100;
/// Protocol message, field, or framing constant.
pub const MINIMUM_RECEIVE_POLL_MILLIS: u64 = 1;
/// Protocol message, field, or framing constant.
pub const CLOSE_JOIN_MILLIS: u64 = 1_000;
/// Protocol message, field, or framing constant.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;
/// Protocol message, field, or framing constant.
pub const MAX_PENDING_COMMANDS: u64 = 64;
/// Protocol message, field, or framing constant.
pub const MAX_PENDING_COMMAND_BYTES: u64 = 1_048_576;
/// Protocol message, field, or framing constant.
pub const MAX_PENDING_CONTROLS: u64 = 64;
/// Protocol message, field, or framing constant.
pub const MAX_PENDING_CONTROL_BYTES: u64 = 1_048_576;

/// Constant expression: `9223372036854775807 / NANOS_PER_MILLISECOND`.
pub const MAX_WAIT_MILLIS: f64 = 9223372036854.775;
