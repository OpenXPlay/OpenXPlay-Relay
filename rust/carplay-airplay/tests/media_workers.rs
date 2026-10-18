// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
use carplay_airplay::*;
use carplay_iap2::{DuplexStream, Receive};
use chacha20poly1305::{
    aead::{Aead, Payload},
    ChaCha20Poly1305, KeyInit,
};
use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
fn wait(f: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !f() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(f(), "condition timed out");
}
#[derive(Default)]
struct ByteStream {
    input: Mutex<VecDeque<Receive>>,
    changed: Condvar,
    closed: AtomicBool,
    consumed: AtomicUsize,
}
impl ByteStream {
    fn push(&self, bytes: Vec<u8>) {
        self.input.lock().unwrap().push_back(Receive::Data(bytes));
        self.changed.notify_all();
    }
    fn eof(&self) {
        self.input.lock().unwrap().push_back(Receive::Eof);
        self.changed.notify_all();
    }
}
impl DuplexStream for ByteStream {
    fn send(&self, _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn recv(&self, max: usize, timeout: Duration) -> Result<Receive> {
        let q = self.input.lock().unwrap();
        let (mut q, _) = self
            .changed
            .wait_timeout_while(q, timeout, |q| {
                q.is_empty() && !self.closed.load(Ordering::SeqCst)
            })
            .unwrap();
        if self.closed.load(Ordering::SeqCst) {
            return Ok(Receive::Eof);
        }
        match q.pop_front() {
            Some(Receive::Data(mut bytes)) => {
                if bytes.len() > max {
                    let tail = bytes.split_off(max);
                    q.push_front(Receive::Data(tail));
                }
                self.consumed.fetch_add(bytes.len(), Ordering::SeqCst);
                Ok(Receive::Data(bytes))
            }
            Some(v) => Ok(v),
            None => Ok(Receive::Timeout),
        }
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        self.changed.notify_all();
        Ok(())
    }
}
struct Listener {
    input: Mutex<VecDeque<Arc<dyn DuplexStream>>>,
    changed: Condvar,
    closed: AtomicBool,
    port: u16,
}
impl Listener {
    fn connect(&self, stream: Arc<ByteStream>) {
        self.input.lock().unwrap().push_back(stream);
        self.changed.notify_all();
    }
}
impl StreamListener for Listener {
    fn port(&self) -> Result<u16> {
        Ok(self.port)
    }
    fn accept(&self, timeout: Duration) -> Result<Option<Arc<dyn DuplexStream>>> {
        let q = self.input.lock().unwrap();
        let (mut q, _) = self
            .changed
            .wait_timeout_while(q, timeout, |q| {
                q.is_empty() && !self.closed.load(Ordering::SeqCst)
            })
            .unwrap();
        if self.closed.load(Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(q.pop_front())
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        self.changed.notify_all();
        Ok(())
    }
}
struct Datagram {
    input: Mutex<VecDeque<(Vec<u8>, SocketAddr)>>,
    sent: Mutex<Vec<(Vec<u8>, Option<SocketAddr>)>>,
    changed: Condvar,
    closed: AtomicBool,
    buffer: AtomicUsize,
    port: u16,
}
impl Datagram {
    fn new(port: u16) -> Self {
        Self {
            input: Mutex::new(VecDeque::new()),
            sent: Mutex::new(Vec::new()),
            changed: Condvar::new(),
            closed: AtomicBool::new(false),
            buffer: AtomicUsize::new(0),
            port,
        }
    }
    fn push(&self, data: Vec<u8>) {
        self.input
            .lock()
            .unwrap()
            .push_back((data, "127.0.0.1:1234".parse().unwrap()));
        self.changed.notify_all();
    }
}
impl DatagramTransport for Datagram {
    fn local_port(&self) -> Result<u16> {
        Ok(self.port)
    }
    fn set_receive_buffer_bytes(&self, size: usize) -> Result<()> {
        self.buffer.store(size, Ordering::SeqCst);
        Ok(())
    }
    fn recv(&self, max: usize, timeout: Duration) -> Result<Receive> {
        match self.recv_from(max, timeout)? {
            Some((bytes, _)) => Ok(Receive::Data(bytes)),
            None if self.closed.load(Ordering::SeqCst) => Ok(Receive::Eof),
            None => Ok(Receive::Timeout),
        }
    }
    fn recv_from(&self, max: usize, timeout: Duration) -> Result<Option<(Vec<u8>, SocketAddr)>> {
        let q = self.input.lock().unwrap();
        let (mut q, _) = self
            .changed
            .wait_timeout_while(q, timeout, |q| {
                q.is_empty() && !self.closed.load(Ordering::SeqCst)
            })
            .unwrap();
        Ok(q.pop_front().map(|(mut bytes, peer)| {
            bytes.truncate(max);
            (bytes, peer)
        }))
    }
    fn send(&self, data: &[u8]) -> Result<()> {
        self.sent.lock().unwrap().push((data.to_vec(), None));
        Ok(())
    }
    fn send_to(&self, data: &[u8], peer: SocketAddr) -> Result<()> {
        self.sent.lock().unwrap().push((data.to_vec(), Some(peer)));
        Ok(())
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        self.changed.notify_all();
        Ok(())
    }
}
#[derive(Default)]
struct Transport {
    servers: Mutex<Vec<Arc<Listener>>>,
    datagrams: Mutex<Vec<Arc<Datagram>>>,
}
impl Transport {
    fn server(&self) -> Arc<Listener> {
        self.servers.lock().unwrap().last().unwrap().clone()
    }
}
impl StreamTransport for Transport {
    fn listen(&self, _: IpAddr) -> Result<Arc<dyn StreamListener>> {
        let mut servers = self.servers.lock().unwrap();
        let server = Arc::new(Listener {
            input: Mutex::new(VecDeque::new()),
            changed: Condvar::new(),
            closed: AtomicBool::new(false),
            port: 10000 + servers.len() as u16,
        });
        servers.push(server.clone());
        Ok(server)
    }
    fn bind_datagram(&self, _: SocketAddr) -> Result<Arc<dyn DatagramTransport>> {
        let mut datagrams = self.datagrams.lock().unwrap();
        let d = Arc::new(Datagram::new(20000 + datagrams.len() as u16));
        datagrams.push(d.clone());
        Ok(d)
    }
}
fn sealed(key: &[u8; 32], counter: u64, body: &[u8], aad: &[u8]) -> Vec<u8> {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&counter.to_le_bytes());
    ChaCha20Poly1305::new(key.into())
        .encrypt((&nonce).into(), Payload { msg: body, aad })
        .unwrap()
}
fn audio_packet(key: &[u8; 32], seq: u16, ts: u32, payload: &[u8]) -> Vec<u8> {
    let mut counters = MicrophoneCounters {
        sequence: seq,
        timestamp: ts,
        nonce: seq as u64,
    };
    MicrophonePacketizer::seal_packet(key, 96, &mut counters, payload, 1024).unwrap()
}
fn buffered_packet(key: &[u8; 32], seq: u16, ts: u32, payload: &[u8]) -> Vec<u8> {
    let wire = audio_packet(key, seq, ts, payload);
    let mut body = ((wire.len() + 2) as u16).to_be_bytes().to_vec();
    body.extend(wire);
    body
}
#[derive(Default)]
struct AudioOutput {
    packets: Mutex<Vec<(u32, Vec<u8>)>>,
    starts: Mutex<Vec<u32>>,
    stops: AtomicUsize,
    errors: AtomicUsize,
}
impl AudioListener for AudioOutput {
    fn on_started(&self, _: &AudioFormat, sample: u32) -> Result<()> {
        self.starts.lock().unwrap().push(sample);
        Ok(())
    }
    fn on_rtp(&self, _: &AudioFormat, rtp: &[u8], sample: u32) -> Result<()> {
        self.packets.lock().unwrap().push((sample, rtp.to_vec()));
        Ok(())
    }
    fn on_stopped(&self) -> Result<()> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn on_packet(
        &self,
        _: &[u8],
        _: Option<&[u8]>,
        _: Option<u32>,
        error: Option<&std::io::Error>,
    ) {
        if error.is_some() {
            self.errors.fetch_add(1, Ordering::SeqCst);
        }
    }
}
fn format() -> AudioFormat {
    AudioStreamCodec::from_format_bits(0x800000, 103, "media").unwrap()
}
#[test]
fn audio_udp_keeps_reordered_samples_and_counts_authentication_errors() {
    let t = Arc::new(Transport::default());
    let o = Arc::new(AudioOutput::default());
    let key = [1; 32];
    let audio = AudioStream::new(t.clone(), format(), key.to_vec()).unwrap();
    assert_eq!(audio.listen(o.clone()).unwrap(), (20000, 20001));
    let d = t.datagrams.lock().unwrap()[0].clone();
    assert_eq!(d.buffer.load(Ordering::SeqCst), 512 * 1024);
    d.push(vec![0; 12]);
    let mut bad = audio_packet(&key, 1, 100, b"bad");
    bad[12] ^= 1;
    d.push(bad);
    for (seq, ts, p) in [
        (65535, u32::MAX - 100, b"aac".as_slice()),
        (0, 100, b"lpcm".as_slice()),
        (1, 50, b"opus".as_slice()),
    ] {
        d.push(audio_packet(&key, seq, ts, p));
    }
    wait(|| o.packets.lock().unwrap().len() == 3);
    assert_eq!(o.errors.load(Ordering::SeqCst), 2);
    assert_eq!(*o.starts.lock().unwrap(), [u32::MAX - 100]);
    let p = o.packets.lock().unwrap();
    assert_eq!(
        p.iter().map(|(ts, _)| *ts).collect::<Vec<_>>(),
        [u32::MAX - 100, 100, 50]
    );
    assert_eq!(&p[2].1[12..], b"opus");
    drop(p);
    audio.close().unwrap();
    assert!(t
        .datagrams
        .lock()
        .unwrap()
        .iter()
        .all(|s| s.closed.load(Ordering::SeqCst)));
}
#[derive(Default)]
struct ScreenOutput {
    codec: Mutex<Vec<VideoCodec>>,
    config: Mutex<Vec<Vec<u8>>>,
    frames: Mutex<Vec<(Vec<u8>, u64, u64)>>,
    closed: AtomicBool,
}
impl ScreenListener for ScreenOutput {
    fn on_codec(&self, c: VideoCodec) {
        self.codec.lock().unwrap().push(c);
    }
    fn on_config(&self, b: &[u8]) -> Result<()> {
        self.config.lock().unwrap().push(b.to_vec());
        Ok(())
    }
    fn on_frame(&self, b: &[u8], sender: u64, arrival: u64) -> Result<()> {
        self.frames
            .lock()
            .unwrap()
            .push((b.to_vec(), sender, arrival));
        Ok(())
    }
    fn on_closed(&self, _: Option<std::io::Error>) {
        self.closed.store(true, Ordering::SeqCst);
    }
}
fn screen_message(op: u8, body: &[u8], key: Option<&[u8; 32]>, counter: u64) -> Vec<u8> {
    let mut h = [0; 128];
    h[..4]
        .copy_from_slice(&((body.len() + if key.is_some() { 16 } else { 0 }) as u32).to_le_bytes());
    h[4] = op;
    h[8..16].copy_from_slice(&((3u64 << 32) | 0x40000000).to_le_bytes());
    let mut out = h.to_vec();
    out.extend(if let Some(key) = key {
        sealed(key, counter, body, &h)
    } else {
        body.to_vec()
    });
    out
}
fn screen_key(shared: &[u8; 32], id: u64) -> [u8; 32] {
    let mut key = [0; 32];
    hkdf::Hkdf::<sha2::Sha512>::new(Some(format!("DataStream-Salt{id}").as_bytes()), shared)
        .expand(b"DataStream-Output-Encryption-Key", &mut key)
        .unwrap();
    key
}
#[test]
fn screen_split_reads_clear_config_and_encrypted_access_units_keep_times() {
    let t = Arc::new(Transport::default());
    let shared = [9; 32];
    let s = ScreenStream::new(t.clone(), shared, 42).unwrap();
    let o = Arc::new(ScreenOutput::default());
    s.listen(o.clone()).unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let key = screen_key(&shared, 42);
    let mut bytes = screen_message(1, b"\0\0\0\0hvcC\x01\x02", None, 0);
    bytes.extend(screen_message(0, &[0, 0, 0, 2, 0x40, 1], Some(&key), 0));
    bytes.extend(screen_message(0, &[0, 0, 0, 2, 0x42, 1], Some(&key), 1));
    for chunk in bytes.chunks(37) {
        socket.push(chunk.to_vec());
    }
    wait(|| o.frames.lock().unwrap().len() == 2);
    assert_eq!(*o.codec.lock().unwrap(), [VideoCodec::H265]);
    assert_eq!(o.config.lock().unwrap()[0], [1, 2]);
    let f = o.frames.lock().unwrap();
    assert_eq!(f[0].0, [0, 0, 0, 1, 0x40, 1]);
    assert_eq!(f[0].1, 3_250_000_000);
    assert!(f[1].2 >= f[0].2);
    drop(f);
    s.close().unwrap();
    assert!(socket.closed.load(Ordering::SeqCst));
}
#[test]
fn screen_rejects_oversized_header_without_allocating_body() {
    let t = Arc::new(Transport::default());
    let s = ScreenStream::new(t.clone(), [0; 32], 1).unwrap();
    let o = Arc::new(ScreenOutput::default());
    s.listen(o.clone()).unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let mut header = vec![0; 128];
    header[..4].copy_from_slice(&(8 * 1024 * 1024 + 1u32).to_le_bytes());
    socket.push(header);
    wait(|| o.closed.load(Ordering::SeqCst));
    assert!(o.frames.lock().unwrap().is_empty());
    s.close().unwrap();
}
#[test]
fn screen_authentication_failure_terminates_without_delivering_frame() {
    let t = Arc::new(Transport::default());
    let s = ScreenStream::new(t.clone(), [1; 32], 1).unwrap();
    let o = Arc::new(ScreenOutput::default());
    s.listen(o.clone()).unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    socket.push(screen_message(0, b"nalu", Some(&[0; 32]), 0));
    wait(|| o.closed.load(Ordering::SeqCst));
    assert!(o.frames.lock().unwrap().is_empty());
    s.close().unwrap();
}
#[test]
fn buffered_split_and_coalesced_frames_wait_for_rate_then_pause_and_flush() {
    let t = Arc::new(Transport::default());
    let o = Arc::new(AudioOutput::default());
    let b = BufferedAudioStream::new(t.clone(), format(), vec![4; 32], o.clone()).unwrap();
    b.start().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let mut bytes = Vec::new();
    for i in 0..4 {
        bytes.extend(buffered_packet(
            &[4; 32],
            i,
            1000 + i as u32 * 1024,
            &[0x21, i as u8],
        ));
    }
    for chunk in bytes.chunks(11) {
        socket.push(chunk.to_vec());
    }
    std::thread::sleep(Duration::from_millis(30));
    assert!(o.packets.lock().unwrap().is_empty());
    assert!(b.anchor().unwrap().is_none());
    let now = (2_208_988_800u64 + 100) << 32;
    b.set_rate(Some(2024), 1, now).unwrap();
    wait(|| o.packets.lock().unwrap().len() == 3);
    assert_eq!(
        o.packets
            .lock()
            .unwrap()
            .iter()
            .map(|(s, _)| *s)
            .collect::<Vec<_>>(),
        [2024, 3048, 4072]
    );
    assert_eq!(o.starts.lock().unwrap().len(), 1);
    b.set_rate(None, 0, now).unwrap();
    assert_eq!(o.stops.load(Ordering::SeqCst), 1);
    b.flush(Some(9000)).unwrap();
    b.close().unwrap();
    assert!(socket.closed.load(Ordering::SeqCst));
    assert!(b.set_rate(Some(0), 1, now).unwrap().is_none());
}
#[test]
fn buffered_malformed_length_closes_without_reading_another_prefix() {
    let t = Arc::new(Transport::default());
    let o = Arc::new(AudioOutput::default());
    let b = BufferedAudioStream::new(t.clone(), format(), vec![0; 32], o.clone()).unwrap();
    b.start().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let mut bytes = vec![0, 37];
    bytes.extend(buffered_packet(&[0; 32], 1, 1000, b"aac"));
    socket.push(bytes);
    wait(|| socket.closed.load(Ordering::SeqCst));
    assert!(o.packets.lock().unwrap().is_empty());
    b.close().unwrap();
}
struct ReentrantAudio {
    stream: Mutex<Option<BufferedAudioStream>>,
    started: AtomicBool,
    rtp: AtomicUsize,
}
impl AudioListener for ReentrantAudio {
    fn on_started(&self, _: &AudioFormat, _: u32) -> Result<()> {
        let stream = self.stream.lock().unwrap().clone().unwrap();
        stream.set_rate(None, 0, 0)?;
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn on_rtp(&self, _: &AudioFormat, _: &[u8], _: u32) -> Result<()> {
        self.rtp.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
#[test]
fn reentrant_pause_during_start_cannot_deliver_stale_rtp() {
    let t = Arc::new(Transport::default());
    let o = Arc::new(ReentrantAudio {
        stream: Mutex::new(None),
        started: AtomicBool::new(false),
        rtp: AtomicUsize::new(0),
    });
    let b = BufferedAudioStream::new(t.clone(), format(), vec![0; 32], o.clone()).unwrap();
    *o.stream.lock().unwrap() = Some(b.clone());
    b.start().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    socket.push(buffered_packet(&[0; 32], 1, 1024, b"aac"));
    b.set_rate(Some(1024), 1, 0).unwrap();
    wait(|| o.started.load(Ordering::SeqCst));
    assert_eq!(o.rtp.load(Ordering::SeqCst), 0);
    b.close().unwrap();
    o.stream.lock().unwrap().take();
}
struct BlockingAudio {
    entered: AtomicBool,
    release: Mutex<bool>,
    changed: Condvar,
}
impl AudioListener for BlockingAudio {
    fn on_rtp(&self, _: &AudioFormat, _: &[u8], _: u32) -> Result<()> {
        self.entered.store(true, Ordering::SeqCst);
        let mut release = self.release.lock().unwrap();
        while !*release {
            release = self.changed.wait(release).unwrap();
        }
        Ok(())
    }
}
fn control_waits(method: u8) {
    let t = Arc::new(Transport::default());
    let o = Arc::new(BlockingAudio {
        entered: AtomicBool::new(false),
        release: Mutex::new(false),
        changed: Condvar::new(),
    });
    let b = BufferedAudioStream::new(t.clone(), format(), vec![0; 32], o.clone()).unwrap();
    b.start().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    socket.push(buffered_packet(&[0; 32], 1, 1024, b"aac"));
    b.set_rate(Some(1024), 1, 0).unwrap();
    wait(|| o.entered.load(Ordering::SeqCst));
    let done = Arc::new(AtomicBool::new(false));
    let d = done.clone();
    let other = b.clone();
    let control = std::thread::spawn(move || {
        match method {
            0 => {
                other.set_rate(None, 0, 0).unwrap();
            }
            1 => other.flush(Some(9999)).unwrap(),
            _ => other.close().unwrap(),
        }
        d.store(true, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_millis(20));
    assert!(!done.load(Ordering::SeqCst));
    *o.release.lock().unwrap() = true;
    o.changed.notify_all();
    control.join().unwrap();
    b.close().unwrap();
}
#[test]
fn pause_waits_for_in_flight_delivery() {
    control_waits(0);
}
#[test]
fn flush_waits_for_in_flight_delivery() {
    control_waits(1);
}
#[test]
fn close_waits_for_in_flight_delivery() {
    control_waits(2);
}
#[test]
fn pause_resume_retains_preload_frames() {
    let t = Arc::new(Transport::default());
    let o = Arc::new(AudioOutput::default());
    let b = BufferedAudioStream::new(t.clone(), format(), vec![0; 32], o.clone()).unwrap();
    b.start().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    for i in 0..3 {
        socket.push(buffered_packet(&[0; 32], i, i as u32 * 1024, b"aac"));
    }
    b.set_rate(Some(0), 1, 0).unwrap();
    wait(|| o.packets.lock().unwrap().len() == 3);
    b.set_rate(None, 0, 0).unwrap();
    b.set_rate(None, 1, 0).unwrap();
    wait(|| o.packets.lock().unwrap().len() == 6);
    assert_eq!(
        o.packets
            .lock()
            .unwrap()
            .iter()
            .map(|(s, _)| *s)
            .collect::<Vec<_>>(),
        [0, 1024, 2048, 0, 1024, 2048]
    );
    b.close().unwrap();
}
fn tunnel_packet(key: &[u8; 32], counter: u64, body: &[u8]) -> Vec<u8> {
    let prefix = (body.len() as u16).to_le_bytes();
    let mut bytes = prefix.to_vec();
    bytes.extend(sealed(key, counter, body, &prefix));
    bytes
}
fn package(body: &[u8]) -> Vec<u8> {
    let mut out = vec![0; 32];
    out[..4].copy_from_slice(&((32 + body.len()) as u32).to_be_bytes());
    out[16..20].copy_from_slice(b"comm");
    out.extend(body);
    out
}
#[test]
fn tunnel_split_frames_deliver_packages_and_input_eof_keeps_event_uplink() {
    let t = Arc::new(Transport::default());
    let tunnel = Arc::new(IapTunnel::with_key(t.clone(), [2; 32]));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let sends = sent.clone();
    let bridge = AirPlayIapTunnelStream::with_commands(
        tunnel.clone(),
        Arc::new(move |data, timeout| {
            assert_eq!(timeout, Duration::from_secs(10));
            sends.lock().unwrap().push(data.to_vec());
            Ok(true)
        }),
        Arc::new(|| panic!("EOF must not close session")),
    );
    bridge.listen().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let body = package(b"abcdef");
    let mut bytes = tunnel_packet(&[2; 32], 0, &body[..15]);
    bytes.extend(tunnel_packet(&[2; 32], 1, &body[15..]));
    for chunk in bytes.chunks(7) {
        socket.push(chunk.to_vec());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut received = Vec::new();
    while received.len() < 6 && Instant::now() < deadline {
        if let Receive::Data(data) = bridge.recv(2, Duration::from_millis(50)).unwrap() {
            received.extend(data);
        }
    }
    assert_eq!(received, b"abcdef");
    socket.eof();
    wait(|| socket.closed.load(Ordering::SeqCst));
    bridge.send(b"response").unwrap();
    assert_eq!(*sent.lock().unwrap(), [b"response".to_vec()]);
    assert_eq!(bridge.recv(10, Duration::ZERO).unwrap(), Receive::Timeout);
    bridge.close().unwrap();
    assert_eq!(bridge.recv(10, Duration::ZERO).unwrap(), Receive::Eof);
    assert!(bridge
        .send(b"closed")
        .unwrap_err()
        .to_string()
        .contains("AirPlay iAP tunnel is closed"));
}
#[test]
fn tunnel_auth_failure_is_terminal_and_wakes_receive() {
    let t = Arc::new(Transport::default());
    let tunnel = Arc::new(IapTunnel::with_key(t.clone(), [2; 32]));
    let failed = Arc::new(AtomicBool::new(false));
    let f = failed.clone();
    let bridge = AirPlayIapTunnelStream::with_commands(
        tunnel,
        Arc::new(|_, _| Ok(true)),
        Arc::new(move || {
            f.store(true, Ordering::SeqCst);
        }),
    );
    bridge.listen().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let mut bytes = tunnel_packet(&[2; 32], 0, &package(b"iap"));
    bytes[4] ^= 1;
    socket.push(bytes);
    wait(|| failed.load(Ordering::SeqCst));
    assert!(bridge
        .recv(10, Duration::from_secs(1))
        .unwrap_err()
        .to_string()
        .starts_with("AirPlay iAP tunnel failed:"));
    bridge.close().unwrap();
}
#[test]
fn ntp_replies_echo_origin_and_return_source_address() {
    let d = Arc::new(Datagram::new(3333));
    let clock = NtpClock::new(d.clone());
    assert_eq!(clock.listen().unwrap(), 3333);
    assert!(clock.listen().is_err());
    let mut request = vec![0; 32];
    request[..4].copy_from_slice(&[0x80, 210, 0, 7]);
    request[24..].copy_from_slice(&0x123456789abcdef0u64.to_be_bytes());
    d.push(request);
    wait(|| d.sent.lock().unwrap().len() == 1);
    let sent = d.sent.lock().unwrap();
    assert_eq!(&sent[0].0[..4], &[0x80, 211, 0, 7]);
    assert_eq!(&sent[0].0[8..16], &0x123456789abcdef0u64.to_be_bytes());
    assert_eq!(sent[0].1, Some("127.0.0.1:1234".parse().unwrap()));
    drop(sent);
    clock.close().unwrap();
}

#[test]
fn buffered_byte_budget_applies_tcp_backpressure_and_flush_releases_receiver() {
    let t = Arc::new(Transport::default());
    let output = Arc::new(AudioOutput::default());
    // A high test clock puts the byte cap below the pacing floor; it isolates byte backpressure.
    let mut f = format();
    f.sample_rate = 480000;
    f.rtp_clock_rate = 480000;
    let b = BufferedAudioStream::new(t.clone(), f, vec![0; 32], output).unwrap();
    b.start().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let payload = vec![0x21; 4096];
    let rtp_size = 12 + payload.len();
    let capacity = 8 * 1024 * 1024 / rtp_size;
    let frame_size = 2 + 12 + payload.len() + 16 + 8;
    let mut bytes = Vec::new();
    for n in 0..capacity + 10 {
        bytes.extend(buffered_packet(
            &[0; 32],
            n as u16,
            n as u32 * 1024,
            &payload,
        ));
    }
    for chunk in bytes.chunks(4096) {
        socket.push(chunk.to_vec());
    }
    // Decrypting 8 MB takes seconds in a debug build and longer on a loaded runner, so wait as
    // long as the receiver makes progress rather than for a fixed time. A receiver that stops
    // early stalls below the cap; one without backpressure reads past it.
    let mut progress = (0, Instant::now());
    loop {
        let consumed = socket.consumed.load(Ordering::SeqCst);
        if consumed >= (capacity + 1) * frame_size {
            break;
        }
        if consumed != progress.0 {
            progress = (consumed, Instant::now());
        } else if progress.1.elapsed() > Duration::from_secs(5) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        socket.consumed.load(Ordering::SeqCst),
        (capacity + 1) * frame_size
    );
    // Long enough for a receiver without backpressure to read on, even on a loaded runner.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        socket.consumed.load(Ordering::SeqCst),
        (capacity + 1) * frame_size
    );
    b.flush(Some((capacity as u32 + 10) * 1024)).unwrap();
    wait(|| socket.consumed.load(Ordering::SeqCst) == (capacity + 10) * frame_size);
    b.close().unwrap();
}
#[test]
fn tunnel_pending_limit_is_terminal_instead_of_unbounded() {
    let t = Arc::new(Transport::default());
    let tunnel = Arc::new(IapTunnel::with_key(t.clone(), [2; 32]));
    let bridge =
        AirPlayIapTunnelStream::with_commands(tunnel, Arc::new(|_, _| Ok(true)), Arc::new(|| {}));
    bridge.listen().unwrap();
    let socket = Arc::new(ByteStream::default());
    t.server().connect(socket.clone());
    let body = package(&vec![0x55; 1024 * 1024 + 1]);
    for (counter, chunk) in body.chunks(16384).enumerate() {
        socket.push(tunnel_packet(&[2; 32], counter as u64, chunk));
    }
    wait(|| socket.closed.load(Ordering::SeqCst));
    assert!(bridge
        .recv(1, Duration::ZERO)
        .unwrap_err()
        .to_string()
        .contains("pending-data limit exceeded"));
    bridge.close().unwrap();
}
