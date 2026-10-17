// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use carplay_iap2::{
    mfi::MfiAuthenticator, DuplexStream, Iap2Session, Iap2WirelessCarPlayEndpoint,
    Iap2WirelessSecurity, Receive,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// `VideoCodec` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
}
/// Platform interface for `ScreenListener`. Implementations must support bounded shutdown.
pub trait ScreenListener: Send + Sync {
    fn on_closed(&self, _cause: Option<std::io::Error>) {}
    /// Errors propagate to the lifecycle owner.
    fn on_codec(&self, codec: VideoCodec);
    /// Errors propagate to the lifecycle owner.
    fn on_config(&self, codec_data: &[u8]) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_frame(&self, nalu_bytes: &[u8], sender_nanos: u64, arrival_nanos: u64) -> Result<()>;
}
/// Platform interface for `StreamListener`. Implementations must support bounded shutdown.
pub trait StreamListener: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn port(&self) -> Result<u16>;
    /// Errors propagate to the lifecycle owner.
    fn accept(&self, timeout: Duration) -> Result<Option<Arc<dyn DuplexStream>>>;
    /// Errors propagate to the lifecycle owner.
    fn close(&self) -> Result<()>;
}
/// Platform interface for `StreamTransport`. Implementations must support bounded shutdown.
pub trait StreamTransport: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    /// Listen on a local ephemeral port, admitting this remote peer. An unspecified peer admits any.
    fn listen(&self, peer: std::net::IpAddr) -> Result<Arc<dyn StreamListener>>;
    /// Errors propagate to the lifecycle owner.
    /// Bind a local ephemeral port. A nonzero peer port is the send destination; zero receives from that host.
    fn bind_datagram(&self, peer: std::net::SocketAddr) -> Result<Arc<dyn DatagramTransport>>;
}
/// Platform interface for `DatagramTransport`. Implementations must support bounded shutdown.
pub trait DatagramTransport: Send + Sync {
    /// Request kernel receive buffering; platforms may cap the request.
    fn set_receive_buffer_bytes(&self, _bytes: usize) -> Result<()> {
        Ok(())
    }
    /// Errors propagate to the lifecycle owner.
    fn local_port(&self) -> Result<u16>;
    /// Errors propagate to the lifecycle owner.
    fn recv(&self, max_bytes: usize, timeout: Duration) -> Result<Receive>;
    /// Errors propagate to the lifecycle owner.
    fn send(&self, data: &[u8]) -> Result<()>;
    /// Send a reply to a datagram source, including NTP timing requests.
    fn send_to(&self, data: &[u8], peer: std::net::SocketAddr) -> Result<()>;
    /// Receive source address for timing and retransmission control.
    fn recv_from(
        &self,
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<Option<(Vec<u8>, std::net::SocketAddr)>>;
    /// Errors propagate to the lifecycle owner.
    fn close(&self) -> Result<()>;
}
/// State owned by `ScreenStream`; construct through its factory.
pub struct ScreenStream {
    transport: Arc<dyn StreamTransport>,
    key: [u8; 32],
    worker: Arc<Worker>,
    diagnostic: Arc<dyn Fn(String) + Send + Sync>,
}
impl ScreenStream {
    pub fn new(
        transport: Arc<dyn StreamTransport>,
        shared_secret: [u8; 32],
        stream_connection_id: u64,
    ) -> Result<Self> {
        use hkdf::Hkdf;
        let mut key = [0; 32];
        Hkdf::<sha2::Sha512>::new(
            Some(format!("DataStream-Salt{stream_connection_id}").as_bytes()),
            &shared_secret,
        )
        .expand(b"DataStream-Output-Encryption-Key", &mut key)
        .map_err(|_| invalid("screen key derivation failed"))?;
        Ok(Self::with_key(transport, key))
    }
    pub fn with_diagnostic(mut self, diagnostic: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        self.diagnostic = diagnostic;
        self
    }

    pub fn listen(&self, listener: Arc<dyn ScreenListener>) -> Result<u16> {
        let server = self.transport.listen("::".parse().unwrap())?;
        let port = server.port()?;
        self.worker.add_server(server.clone())?;
        let worker = self.worker.clone();
        let key = self.key;
        let diagnostic = self.diagnostic.clone();
        self.worker.spawn("airplay-screen", move || {
            let stats = StreamReceiveStats::with_clock(
                "video",
                Arc::new(crate::stream_receive_stats::monotonic_nanos),
                diagnostic,
            );
            let result = (|| -> Result<()> {
                let Some(socket) = accept_one(&worker, &server)? else {
                    return Ok(());
                };
                let mut counter = 0u64;
                while !worker.closed() {
                    stats.reading();
                    let Some(header) = read_exact(&worker, &socket, 128)? else {
                        break;
                    };
                    let length = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
                    if length > MAX_BODY as usize {
                        break;
                    }
                    let Some(body) = read_exact(&worker, &socket, length)? else {
                        break;
                    };
                    let arrival = crate::stream_receive_stats::monotonic_nanos();
                    stats.received(128 + length, arrival);
                    match header[4] {
                        0 => {
                            let start = Instant::now();
                            let mut payload =
                                ScreenCodec::decrypt_frame(&key, counter, &header, &body)?;
                            if body.len() >= 16 {
                                counter = counter.wrapping_add(1);
                                stats.decrypted(start.elapsed().as_nanos() as u64, body.len());
                            }
                            ScreenCodec::length_prefixed_to_annex_b(&mut payload)?;
                            if !worker.closed() {
                                listener.on_frame(
                                    &payload,
                                    ScreenCodec::sender_nanos(&header),
                                    arrival,
                                )?;
                            }
                        }
                        1 => {
                            let (codec, config) = ScreenCodec::detect_config(&body)?;
                            if !worker.closed() {
                                listener.on_codec(codec);
                                listener.on_config(&config)?;
                            }
                        }
                        _ => {}
                    }
                    stats.processed();
                }
                Ok(())
            })();
            stats.flush(true);
            if !worker.closed() {
                listener.on_closed(result.err());
            }
            let _ = worker.close();
        })?;
        Ok(port)
    }

    pub fn close(&self) -> Result<()> {
        self.worker.close()
    }
}

/// Protocol message, field, or framing constant.
pub const TAG: &str = "xcertplay-usb";
/// Protocol message, field, or framing constant.
pub const HEADER_LEN: u64 = 128;
/// Protocol message, field, or framing constant.
pub const OPCODE_OFFSET: u64 = 4;
/// Protocol message, field, or framing constant.
pub const OP_VIDEO_FRAME: u64 = 0;
/// Protocol message, field, or framing constant.
pub const OP_VIDEO_CONFIG: u64 = 1;
/// Protocol message, field, or framing constant.
pub const SENDER_TIME_OFFSET: u64 = 8;
/// Protocol message, field, or framing constant.
pub const TAG_SIZE: u64 = 16;

impl ScreenStream {}

/// Constant expression: `8 * 1024 * 1024`.
pub const MAX_BODY: u64 = 8388608;

/// Pure screen framing operations.
pub struct ScreenCodec {
    _private: (),
}
impl ScreenCodec {
    /// Convert the sender's unsigned 32.32 timestamp to nanoseconds.
    pub fn sender_nanos(header: &[u8]) -> u64 {
        if header.len() < 16 {
            return 0;
        }
        let raw = u64::from_le_bytes(header[8..16].try_into().unwrap());
        (raw >> 32) * 1_000_000_000 + (((raw & 0xffff_ffff) * 1_000_000_000) >> 32)
    }
    /// Authenticate the body using the entire header as associated data.
    pub fn decrypt_frame(
        key: &[u8; 32],
        counter: u64,
        header: &[u8],
        body: &[u8],
    ) -> Result<Vec<u8>> {
        if body.len() < 16 {
            return Ok(body.to_vec());
        }
        open_chacha(key, counter, body, header)
    }
    /// Validate all NAL lengths before replacing any with Annex B start codes.
    pub fn length_prefixed_to_annex_b(payload: &mut [u8]) -> Result<()> {
        if payload.len() < 4 || payload.starts_with(&[0, 0, 0, 1]) {
            return Ok(());
        }
        let mut offset = 0;
        while offset + 4 <= payload.len() {
            let n = u32::from_be_bytes(payload[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            if n == 0 || n > payload.len() - offset {
                return Ok(());
            }
            offset += n;
        }
        if offset != payload.len() {
            return Ok(());
        }
        offset = 0;
        while offset < payload.len() {
            let n = u32::from_be_bytes(payload[offset..offset + 4].try_into().unwrap()) as usize;
            payload[offset..offset + 4].copy_from_slice(&[0, 0, 0, 1]);
            offset += 4 + n;
        }
        Ok(())
    }
    /// Extract avcC or hvcC without decoding video.
    pub fn detect_config(payload: &[u8]) -> Result<(VideoCodec, Vec<u8>)> {
        if payload.len() >= 8 {
            for i in 4..=payload.len() - 4 {
                let codec = match &payload[i..i + 4] {
                    b"hvcC" => VideoCodec::H265,
                    b"avcC" => VideoCodec::H264,
                    _ => continue,
                };
                // The box size precedes the tag; other boxes (e.g. after hvcC) must not leak in.
                let size = u32::from_be_bytes(payload[i - 4..i].try_into().unwrap()) as usize;
                let end = if size >= 8 && i - 4 + size <= payload.len() {
                    i - 4 + size
                } else {
                    payload.len()
                };
                return Ok((codec, payload[i + 4..end].to_vec()));
            }
        }
        let avc = payload.len() >= 9
            && payload[5] & 0x1f >= 1
            && 8 + u16::from_be_bytes([payload[6], payload[7]]) as usize <= payload.len()
            && payload[8] & 0x1f == 7;
        Ok((
            if avc {
                VideoCodec::H264
            } else {
                VideoCodec::H265
            },
            payload.to_vec(),
        ))
    }
}

impl ScreenStream {
    pub(crate) fn with_key(transport: Arc<dyn StreamTransport>, key: [u8; 32]) -> Self {
        Self {
            transport,
            key,
            worker: Arc::new(Worker::default()),
            diagnostic: Arc::new(|line| eprintln!("{line}")),
        }
    }
}
pub(crate) fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}
pub(crate) fn open_chacha(
    key: &[u8; 32],
    counter: u64,
    body: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>> {
    use chacha20poly1305::{
        aead::{Aead, Payload},
        ChaCha20Poly1305, KeyInit,
    };
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&counter.to_le_bytes());
    ChaCha20Poly1305::new(key.into())
        .decrypt((&nonce).into(), Payload { msg: body, aad })
        .map_err(|_| invalid("ChaCha20-Poly1305 authentication failed"))
}
/// Shared ownership of portable transport workers. Close sockets before bounded joining.
#[derive(Default)]
pub(crate) struct Worker {
    closed: std::sync::atomic::AtomicBool,
    sockets: std::sync::Mutex<Vec<Arc<dyn DuplexStream>>>,
    servers: std::sync::Mutex<Vec<Arc<dyn StreamListener>>>,
    datagrams: std::sync::Mutex<Vec<Arc<dyn DatagramTransport>>>,
    threads: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>,
}
impl Worker {
    pub(crate) fn closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(crate) fn add_socket(&self, socket: Arc<dyn DuplexStream>) -> Result<()> {
        let mut sockets = self.sockets.lock().unwrap();
        if self.closed() {
            drop(sockets);
            socket.close()?;
            return Err(invalid("stream is closed"));
        }
        sockets.push(socket);
        Ok(())
    }
    pub(crate) fn remove_socket(&self, socket: &Arc<dyn DuplexStream>) {
        self.sockets
            .lock()
            .unwrap()
            .retain(|s| !Arc::ptr_eq(s, socket));
    }
    pub(crate) fn add_server(&self, server: Arc<dyn StreamListener>) -> Result<()> {
        let mut servers = self.servers.lock().unwrap();
        if self.closed() {
            drop(servers);
            server.close()?;
            return Err(invalid("stream is closed"));
        }
        servers.push(server);
        Ok(())
    }
    pub(crate) fn add_datagram(&self, socket: Arc<dyn DatagramTransport>) -> Result<()> {
        let mut sockets = self.datagrams.lock().unwrap();
        if self.closed() {
            drop(sockets);
            socket.close()?;
            return Err(invalid("stream is closed"));
        }
        sockets.push(socket);
        Ok(())
    }
    pub(crate) fn spawn(&self, name: &str, run: impl FnOnce() + Send + 'static) -> Result<()> {
        let mut threads = self.threads.lock().unwrap();
        if self.closed() {
            return Err(invalid("stream is closed"));
        }
        threads.push(
            std::thread::Builder::new()
                .name(name.into())
                .stack_size(256 * 1024)
                .spawn(run)?,
        );
        Ok(())
    }
    pub(crate) fn close(&self) -> Result<()> {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        let sockets = std::mem::take(&mut *self.sockets.lock().unwrap());
        let servers = std::mem::take(&mut *self.servers.lock().unwrap());
        let datagrams = std::mem::take(&mut *self.datagrams.lock().unwrap());
        for s in sockets {
            let _ = s.close();
        }
        for s in servers {
            let _ = s.close();
        }
        for s in datagrams {
            let _ = s.close();
        }
        let threads = std::mem::take(&mut *self.threads.lock().unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        for t in threads {
            if t.thread().id() == std::thread::current().id() {
                continue;
            }
            while !t.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
        Ok(())
    }
}
pub(crate) fn accept_one(
    worker: &Worker,
    server: &Arc<dyn StreamListener>,
) -> Result<Option<Arc<dyn DuplexStream>>> {
    while !worker.closed() {
        if let Some(socket) = server.accept(Duration::from_millis(100))? {
            worker.add_socket(socket.clone())?;
            return Ok(Some(socket));
        }
    }
    Ok(None)
}
pub(crate) fn read_exact(
    worker: &Worker,
    socket: &Arc<dyn DuplexStream>,
    size: usize,
) -> Result<Option<Vec<u8>>> {
    let mut output = Vec::with_capacity(size);
    while output.len() < size && !worker.closed() {
        match socket.recv(size - output.len(), Duration::from_millis(100))? {
            Receive::Data(bytes) => {
                if bytes.is_empty() {
                    return Ok(None);
                }
                if bytes.len() > size - output.len() {
                    return Err(invalid("transport exceeded requested read length"));
                }
                output.extend_from_slice(&bytes);
            }
            Receive::Timeout => {}
            Receive::Eof => return Ok(None),
        }
    }
    Ok(if worker.closed() { None } else { Some(output) })
}
