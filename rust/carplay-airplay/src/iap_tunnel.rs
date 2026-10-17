// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::screen_stream::{accept_one, invalid, open_chacha, Worker};
use crate::*;
use carplay_iap2::{DuplexStream, Receive};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
pub trait IapTunnelListener: Send + Sync {
    fn on_connected(&self, stream: Arc<dyn DuplexStream>) -> Result<bool>;
    fn on_message(&self, bytes: &[u8]) -> Result<()>;
    fn on_closed(&self, _cause: Option<std::io::Error>) {}
}
#[derive(Clone)]
pub struct IapTunnel {
    inner: Arc<Inner>,
}
struct Inner {
    transport: Arc<dyn StreamTransport>,
    key: Option<[u8; 32]>,
    worker: Arc<Worker>,
    connected: Mutex<bool>,
    changed: Condvar,
}
impl IapTunnel {
    pub fn new(transport: Arc<dyn StreamTransport>) -> Self {
        Self::build(transport, None)
    }
    pub fn with_key(transport: Arc<dyn StreamTransport>, key: [u8; 32]) -> Self {
        Self::build(transport, Some(key))
    }
    fn build(transport: Arc<dyn StreamTransport>, key: Option<[u8; 32]>) -> Self {
        Self {
            inner: Arc::new(Inner {
                transport,
                key,
                worker: Arc::new(Worker::default()),
                connected: Mutex::new(false),
                changed: Condvar::new(),
            }),
        }
    }
    pub fn listen(&self, listener: Arc<dyn IapTunnelListener>) -> Result<u16> {
        let key = self
            .inner
            .key
            .ok_or_else(|| invalid("AirPlay iAP tunnel read key is not configured"))?;
        let server = self.inner.transport.listen("::".parse().unwrap())?;
        let port = server.port()?;
        self.inner.worker.add_server(server.clone())?;
        let this = self.clone();
        self.inner.worker.spawn("airplay-iap-tunnel", move || {
            while !this.inner.worker.closed() {
                let result = (|| -> Result<()> {
                    let Some(socket) = accept_one(&this.inner.worker, &server)? else {
                        return Ok(());
                    };
                    *this.inner.connected.lock().unwrap() = true;
                    this.inner.changed.notify_all();
                    // The raw socket is owned here. iAP relay attachment uses the full-duplex bridge.
                    let result = this.run(&key, &socket, &listener);
                    let _ = socket.close();
                    this.inner.worker.remove_socket(&socket);
                    result
                })();
                if let Err(e) = result {
                    if !this.inner.worker.closed() {
                        listener.on_closed(Some(e));
                    }
                    break;
                }
            }
        })?;
        Ok(port)
    }
    fn run(
        &self,
        key: &[u8; 32],
        socket: &Arc<dyn DuplexStream>,
        listener: &Arc<dyn IapTunnelListener>,
    ) -> Result<()> {
        let mut ciphertext = Vec::new();
        let mut plaintext = Vec::new();
        let mut counter = 0u64;
        while !self.inner.worker.closed() {
            match socket.recv(16384, Duration::from_millis(100))? {
                Receive::Timeout => continue,
                Receive::Eof => break,
                Receive::Data(data) => {
                    if data.is_empty() {
                        break;
                    }
                    ciphertext.extend_from_slice(&data);
                }
            }
            let mut offset = 0;
            while ciphertext.len() - offset >= 2 {
                let length =
                    u16::from_le_bytes([ciphertext[offset], ciphertext[offset + 1]]) as usize;
                let frame_length = 2 + length + 16;
                if ciphertext.len() - offset < frame_length {
                    break;
                }
                let opened = open_chacha(
                    key,
                    counter,
                    &ciphertext[offset + 2..offset + frame_length],
                    &ciphertext[offset..offset + 2],
                )?;
                counter = counter.wrapping_add(1);
                plaintext.extend_from_slice(&opened);
                offset += frame_length;
            }
            ciphertext.drain(..offset);
            let mut offset = 0;
            while plaintext.len() - offset >= 32 {
                let size =
                    u32::from_be_bytes(plaintext[offset..offset + 4].try_into().unwrap()) as usize;
                if !(32..=MAX_PACKAGE as usize).contains(&size) {
                    return Err(invalid("AirPlay iAP tunnel invalid package length"));
                }
                if plaintext.len() - offset < size {
                    break;
                }
                if &plaintext[offset + 16..offset + 20] == b"comm" && !self.inner.worker.closed() {
                    listener.on_message(&plaintext[offset + 32..offset + size])?;
                }
                offset += size;
            }
            plaintext.drain(..offset);
            if plaintext.len() > MAX_PACKAGE as usize {
                return Err(invalid("AirPlay iAP tunnel package limit exceeded"));
            }
        }
        Ok(())
    }
    pub fn await_peer_connection(&self, timeout: Duration) -> Result<bool> {
        let connected = self.inner.connected.lock().unwrap();
        let (connected, _) = self
            .inner
            .changed
            .wait_timeout_while(connected, timeout, |c| !*c && !self.inner.worker.closed())
            .unwrap();
        Ok(*connected && !self.inner.worker.closed())
    }
    pub fn close(&self) -> Result<()> {
        self.inner.changed.notify_all();
        let result = self.inner.worker.close();
        self.inner.changed.notify_all();
        result
    }
}
pub const FRAME_HEADER_LEN: u64 = 2;
pub const TAG_SIZE: u64 = 16;
pub const PACKAGE_HEADER_LEN: u64 = 32;
pub const MESSAGE_TYPE_OFFSET: u64 = 16;
pub const MSG_TYPE_COMM: u64 = 0x636f6d6d;
pub const MAX_PACKAGE: u64 = 4194304;
pub const READ_CHUNK_BYTES: u64 = 16384;
