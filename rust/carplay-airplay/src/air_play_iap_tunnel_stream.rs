// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use carplay_iap2::{DuplexStream, Receive};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct AirPlayIapTunnelStream {
    inner: Arc<Inner>,
}
struct Inner {
    tunnel: Arc<IapTunnel>,
    send: Arc<SendCommand>,
    failure: Arc<dyn Fn() + Send + Sync>,
    state: Mutex<State>,
    changed: Condvar,
}
type SendCommand = dyn Fn(&[u8], Duration) -> Result<bool> + Send + Sync;
#[derive(Default)]
struct State {
    pending: VecDeque<Vec<u8>>,
    bytes: usize,
    closed: bool,
    failure: Option<String>,
}
impl AirPlayIapTunnelStream {
    pub fn new(tunnel: Arc<IapTunnel>, session: Arc<AirPlaySession>) -> Self {
        let weak = Arc::downgrade(&session);
        let weak2 = weak.clone();
        Self::with_commands(
            tunnel,
            Arc::new(move |bytes, timeout| {
                weak.upgrade()
                    .ok_or_else(|| std::io::Error::other("AirPlay iAP session is closed"))?
                    .send_iap_message(bytes, timeout)
            }),
            Arc::new(move || {
                if let Some(s) = weak2.upgrade() {
                    let _ = s.close();
                }
            }),
        )
    }
    pub fn with_commands(
        tunnel: Arc<IapTunnel>,
        send: Arc<SendCommand>,
        failure: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                tunnel,
                send,
                failure,
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
            }),
        }
    }
    pub fn listen(&self) -> Result<u16> {
        self.inner
            .tunnel
            .listen(Arc::new(BridgeListener(self.clone())))
    }
    fn check(s: &State) -> Result<()> {
        if let Some(failure) = &s.failure {
            return Err(std::io::Error::other(format!(
                "AirPlay iAP tunnel failed: {failure}"
            )));
        }
        if s.closed {
            return Err(std::io::Error::other("AirPlay iAP tunnel is closed"));
        }
        Ok(())
    }
    fn offer(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let overflow = {
            let mut s = self.inner.state.lock().unwrap();
            if s.closed || s.failure.is_some() {
                return;
            }
            if s.bytes + bytes.len() > MAX_PENDING_BYTES as usize {
                s.failure = Some("AirPlay iAP tunnel pending-data limit exceeded".into());
                true
            } else {
                s.pending.push_back(bytes.to_vec());
                s.bytes += bytes.len();
                false
            }
        };
        self.inner.changed.notify_all();
        if overflow {
            let _ = self.inner.tunnel.close();
        }
    }
}
struct BridgeListener(AirPlayIapTunnelStream);
impl IapTunnelListener for BridgeListener {
    fn on_connected(&self, _stream: Arc<dyn DuplexStream>) -> Result<bool> {
        Ok(true)
    }
    fn on_message(&self, bytes: &[u8]) -> Result<()> {
        self.0.offer(bytes);
        Ok(())
    }
    fn on_closed(&self, cause: Option<std::io::Error>) {
        let should_close = {
            let mut s = self.0.inner.state.lock().unwrap();
            if s.closed || s.failure.is_some() {
                false
            } else {
                s.failure =
                    Some(cause.map_or("AirPlay iAP tunnel terminated".into(), |e| e.to_string()));
                true
            }
        };
        self.0.inner.changed.notify_all();
        if should_close {
            (self.0.inner.failure)();
        }
    }
}
impl DuplexStream for AirPlayIapTunnelStream {
    fn send(&self, data: &[u8]) -> Result<()> {
        Self::check(&self.inner.state.lock().unwrap())?;
        if !self
            .inner
            .tunnel
            .await_peer_connection(Duration::from_millis(PEER_CONNECT_TIMEOUT_MILLIS))?
        {
            return Err(std::io::Error::other(
                "CarPlay iAP tunnel peer did not connect",
            ));
        }
        Self::check(&self.inner.state.lock().unwrap())?;
        if !(self.inner.send)(data, Duration::from_millis(EVENT_READY_TIMEOUT_MILLIS))? {
            return Err(std::io::Error::other(
                "AirPlay event channel rejected an iAP message",
            ));
        }
        Ok(())
    }
    fn recv(&self, max_bytes: usize, timeout: Duration) -> Result<Receive> {
        if max_bytes == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "maxBytes must be positive",
            ));
        }
        let deadline = Instant::now().checked_add(timeout);
        let mut s = self.inner.state.lock().unwrap();
        loop {
            if let Some(mut head) = s.pending.pop_front() {
                s.bytes -= head.len();
                if head.len() > max_bytes {
                    let tail = head.split_off(max_bytes);
                    s.bytes += tail.len();
                    s.pending.push_front(tail);
                }
                return Ok(Receive::Data(head));
            }
            if let Some(failure) = &s.failure {
                return Err(std::io::Error::other(format!(
                    "AirPlay iAP tunnel failed: {failure}"
                )));
            }
            if s.closed {
                return Ok(Receive::Eof);
            }
            let remaining = deadline.map_or(Duration::MAX, |d| {
                d.saturating_duration_since(Instant::now())
            });
            if remaining.is_zero() {
                return Ok(Receive::Timeout);
            }
            s = self
                .inner
                .changed
                .wait_timeout(s, remaining.min(Duration::from_secs(30)))
                .unwrap()
                .0;
        }
    }
    fn close(&self) -> Result<()> {
        {
            let mut s = self.inner.state.lock().unwrap();
            if s.closed {
                return Ok(());
            }
            s.closed = true;
        }
        self.inner.changed.notify_all();
        self.inner.tunnel.close()
    }
}
pub const MAX_PENDING_BYTES: u64 = 1_048_576;
pub const PEER_CONNECT_TIMEOUT_MILLIS: u64 = 15000;
pub const EVENT_READY_TIMEOUT_MILLIS: u64 = 10000;
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;
