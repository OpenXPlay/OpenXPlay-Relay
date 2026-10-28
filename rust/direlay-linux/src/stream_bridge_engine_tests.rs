// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use super::*;
use carplay_iap2::{DuplexStream, Receive};
use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    sync::{Condvar, Weak},
    time::Instant,
};

const WAIT: Duration = Duration::from_secs(2);
const AVC: &[u8] = &[
    1, 0x42, 0xc0, 0x1e, 0xff, 0xe1, 0, 9, 0x67, 0x42, 0xc0, 0x1e, 0xf4, 5, 1, 0xec, 0x80, 1, 0, 4,
    0x68, 0xce, 0x3c, 0x80,
];

enum ReadStep {
    Bytes(VecDeque<u8>),
    Barrier(mpsc::SyncSender<()>),
    Eof,
}
#[derive(Default)]
struct PipeState {
    reads: VecDeque<ReadStep>,
    closed: bool,
}
struct Pipe {
    port: u16,
    state: Mutex<PipeState>,
    changed: Condvar,
}
impl Pipe {
    fn write(&self, bytes: Vec<u8>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return;
        }
        state.reads.push_back(ReadStep::Bytes(bytes.into()));
        // The next read acknowledges that the preceding media callbacks have returned.
        state.reads.push_back(ReadStep::Barrier(tx));
        self.changed.notify_all();
        drop(state);
        rx.recv_timeout(WAIT)
            .expect("media worker did not finish delivery");
    }
    fn eof(&self) {
        self.state.lock().unwrap().reads.push_back(ReadStep::Eof);
        self.changed.notify_all();
    }
    fn closed(&self) -> bool {
        self.state.lock().unwrap().closed
    }
    fn read(&self, max: usize, timeout: Duration) -> Result<Receive> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().unwrap();
        loop {
            if state.closed {
                return Ok(Receive::Eof);
            }
            match state.reads.pop_front() {
                Some(ReadStep::Bytes(mut bytes)) => {
                    let data = bytes.drain(..max.min(bytes.len())).collect();
                    if !bytes.is_empty() {
                        state.reads.push_front(ReadStep::Bytes(bytes));
                    }
                    return Ok(Receive::Data(data));
                }
                Some(ReadStep::Barrier(tx)) => {
                    let _ = tx.send(());
                }
                Some(ReadStep::Eof) => return Ok(Receive::Eof),
                None => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Ok(Receive::Timeout);
                    }
                    state = self.changed.wait_timeout(state, remaining).unwrap().0;
                }
            }
        }
    }
    fn stop(&self) -> Result<()> {
        self.state.lock().unwrap().closed = true;
        self.changed.notify_all();
        Ok(())
    }
}
impl DuplexStream for Pipe {
    fn send(&self, _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn recv(&self, max: usize, timeout: Duration) -> Result<Receive> {
        self.read(max, timeout)
    }
    fn close(&self) -> Result<()> {
        self.stop()
    }
}
impl DatagramTransport for Pipe {
    fn local_port(&self) -> Result<u16> {
        Ok(self.port)
    }
    fn recv(&self, max: usize, timeout: Duration) -> Result<Receive> {
        self.read(max, timeout)
    }
    fn send(&self, _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn send_to(&self, _: &[u8], _: SocketAddr) -> Result<()> {
        Ok(())
    }
    fn recv_from(&self, _: usize, _: Duration) -> Result<Option<(Vec<u8>, SocketAddr)>> {
        panic!("unexpected timing transport")
    }
    fn close(&self) -> Result<()> {
        self.stop()
    }
}
struct Acceptor(Mutex<Option<Arc<Pipe>>>, Arc<Pipe>);
impl StreamListener for Acceptor {
    fn port(&self) -> Result<u16> {
        Ok(self.1.port)
    }
    fn accept(&self, _: Duration) -> Result<Option<Arc<dyn DuplexStream>>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .take()
            .map(|p| p as Arc<dyn DuplexStream>))
    }
    fn close(&self) -> Result<()> {
        self.1.stop()
    }
}
#[derive(Default)]
struct Transport(Mutex<BTreeMap<u16, Arc<Pipe>>>);
impl Transport {
    fn allocate(&self) -> Arc<Pipe> {
        let mut pipes = self.0.lock().unwrap();
        let port = 10000 + pipes.len() as u16;
        let pipe = Arc::new(Pipe {
            port,
            state: Mutex::new(PipeState::default()),
            changed: Condvar::new(),
        });
        pipes.insert(port, pipe.clone());
        pipe
    }
    fn pipe(&self, port: u64) -> Arc<Pipe> {
        self.0.lock().unwrap()[&(port as u16)].clone()
    }
}
impl StreamTransport for Transport {
    fn listen(&self, _: IpAddr) -> Result<Arc<dyn StreamListener>> {
        let pipe = self.allocate();
        Ok(Arc::new(Acceptor(Mutex::new(Some(pipe.clone())), pipe)))
    }
    fn bind_datagram(&self, _: SocketAddr) -> Result<Arc<dyn DatagramTransport>> {
        Ok(self.allocate())
    }
}

#[derive(Default)]
struct ControlTarget(Mutex<Weak<AirPlaySession>>);
impl SessionControl for ControlTarget {
    fn playback_token(&self) -> Option<PlaybackToken> {
        self.0
            .lock()
            .unwrap()
            .upgrade()
            .and_then(|s| s.playback_token())
    }
    fn report_video_frame_rendered(&self, token: &PlaybackToken) -> bool {
        self.0
            .lock()
            .unwrap()
            .upgrade()
            .is_some_and(|s| s.report_video_frame_rendered(token))
    }
    fn send_touch(&self, contacts: &[AirPlayContact]) -> Result<bool> {
        self.0
            .lock()
            .unwrap()
            .upgrade()
            .map_or(Ok(false), |s| s.send_touch(contacts))
    }
}
#[derive(Clone, Copy)]
enum Shape {
    PerControlFactory,
    SharedEngine,
}
#[derive(Clone)]
struct Media {
    engine: Arc<CarPlayMediaEngine>,
    bridge: Arc<StreamBridgeSink>,
    transport: Arc<Transport>,
    control: Arc<ControlTarget>,
}
struct Peer {
    session: Arc<AirPlaySession>,
    observer: Arc<TokenHost>,
    media: Media,
}
impl Peer {
    fn request(&self, method: &str, path: &str, body: Vec<u8>) -> RtspResponse {
        let response = self
            .session
            .handle_request(&RtspRequest {
                method: method.into(),
                path: path.into(),
                protocol: "RTSP/1.0".into(),
                headers: Default::default(),
                body,
            })
            .unwrap();
        assert_eq!(response.status, 200);
        response
    }
    fn setup(&self, stream: BplistValue) -> BplistValue {
        let response = self.request(
            "SETUP",
            "/",
            BplistCodec::encode(&BplistValue::dict([(
                "streams",
                BplistValue::Array(vec![stream]),
            )]))
            .unwrap(),
        );
        let body = BplistCodec::decode(&response.body).unwrap();
        let streams = body.get("streams").unwrap().as_array().unwrap();
        assert_eq!(streams.len(), 1, "engine declined media setup");
        streams[0].clone()
    }
    fn screen(&self, connection: u64) -> Arc<Pipe> {
        let reply = self.setup(BplistValue::dict([
            ("type", 110u64.into()),
            ("streamConnectionID", connection.into()),
        ]));
        self.media
            .transport
            .pipe(reply.get("dataPort").unwrap().as_u64().unwrap())
    }
    fn teardown(&self) {
        self.request(
            "TEARDOWN",
            "/",
            BplistCodec::encode(&BplistValue::dict([(
                "streams",
                BplistValue::Array(vec![BplistValue::dict([("type", 110u64.into())])]),
            )]))
            .unwrap(),
        );
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.session.close().unwrap();
    }
}
struct Harness {
    f: Fixture,
    shape: Shape,
    engines: Vec<Arc<CarPlayMediaEngine>>,
    shared: Option<Media>,
}
impl Harness {
    fn new(shape: Shape) -> Self {
        Self {
            f: Fixture::new(),
            shape,
            engines: Vec::new(),
            shared: None,
        }
    }
    fn peer(&mut self) -> Peer {
        let media = if let Some(media) = &self.shared {
            media.clone()
        } else {
            let control = Arc::new(ControlTarget::default());
            let bridge = Arc::new(self.f.bridge(control.clone()));
            let transport = Arc::new(Transport::default());
            let engine = Arc::new(CarPlayMediaEngine::with_screen_session_policy(
                bridge.clone(),
                transport.clone(),
                false,
                crate::carplay_policy::screen_session_policy(),
            ));
            self.engines.push(engine.clone());
            let media = Media {
                engine,
                bridge,
                transport,
                control,
            };
            if matches!(self.shape, Shape::SharedEngine) {
                self.shared = Some(media.clone());
            }
            media
        };
        let observer = Arc::new(TokenHost(AtomicUsize::new(0)));
        let session = AirPlaySession::new(
            observer.clone(),
            AirPlayConfig::linux_receiver("engine-bridge-test".into(), "02:00:00:00:00:01".into()),
            Arc::new(AirPlayIdentity::generate().unwrap()),
            observer.clone(),
            observer.clone(),
            media.engine.clone(),
            observer.clone(),
        )
        .unwrap();
        *media.control.0.lock().unwrap() = Arc::downgrade(&session);
        media
            .engine
            .bind_session(&session, IpAddr::from([127, 0, 0, 1]));
        let peer = Peer {
            session,
            observer,
            media,
        };
        // The first public pair-verify exchange supplies the media key, as in engine unit tests.
        let key = AirPlayCrypto::x25519_generate().unwrap();
        let response = peer.request(
            "POST",
            "/pair-verify",
            Tlv8Codec::encode(&[
                Tlv8Item {
                    kind: 6,
                    value: vec![1],
                },
                Tlv8Item {
                    kind: 3,
                    value: key.public_key.to_vec(),
                },
            ])
            .unwrap(),
        );
        assert_eq!(Tlv8Codec::decode(&response.body).unwrap()[&6], [2]);
        assert!(peer.session.shared_secret().is_some());
        peer
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        for engine in &self.engines {
            engine.close().unwrap();
        }
    }
}

fn screen_packet(pipe: &Pipe, opcode: u8, body: &[u8]) {
    let mut packet = vec![0; 128];
    packet[..4].copy_from_slice(&(body.len() as u32).to_le_bytes());
    packet[4] = opcode;
    packet.extend_from_slice(body);
    pipe.write(packet);
}
fn config(pipe: &Pipe) {
    screen_packet(pipe, 1, AVC);
}
fn video(pipe: &Pipe, marker: u8) {
    screen_packet(pipe, 0, &[0, 0, 0, 2, 0x65, marker]);
}
fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "media lifecycle did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn overlap(shape: Shape, eof: bool) {
    let mut h = Harness::new(shape);
    let old = h.peer();
    let old_screen = old.screen(1);
    config(&old_screen);
    video(&old_screen, 0x11);
    let first = h.f.current();
    let new = h.peer();
    match shape {
        Shape::PerControlFactory => {
            assert_ne!(h.f.current(), first);
            h.f.phase(h.f.current(), "CONNECTING");
            assert!(!Arc::ptr_eq(&old.media.engine, &new.media.engine));
            assert!(!Arc::ptr_eq(&old.media.bridge, &new.media.bridge));
        }
        Shape::SharedEngine => {
            assert_eq!(h.f.current(), first);
            assert!(Arc::ptr_eq(&old.media.engine, &new.media.engine));
            assert!(Arc::ptr_eq(&old.media.bridge, &new.media.bridge));
        }
    }
    let new_screen = new.screen(2);
    config(&new_screen);
    video(&new_screen, 0x22);
    let current = h.f.current();
    assert_ne!(current, first);
    let format = h.f.declaration("1-video-0");
    h.f.drain();

    // Per-control workers can still receive old data; shared-engine takeover closes that socket.
    assert_eq!(old_screen.closed(), matches!(shape, Shape::SharedEngine));
    config(&old_screen);
    video(&old_screen, 0x33);
    if eof {
        old_screen.eof();
        if matches!(shape, Shape::PerControlFactory) {
            wait_until(|| old.session.is_closed() && old_screen.closed());
        }
    } else {
        old.teardown();
    }
    old.session.close().unwrap();
    config(&old_screen);
    video(&old_screen, 0x44);
    assert!(h.f.drain().is_empty());
    assert_eq!(h.f.current(), current);
    h.f.phase(current, "ACTIVE");
    assert_eq!(h.f.declaration("1-video-0"), format);
    assert!(!new.session.is_closed());
    video(&new_screen, 0x55);
    assert_eq!(video_payloads(&h.f.drain()), [vec![0, 0, 0, 2, 0x65, 0x55]]);
}
fn old_without_screen(shape: Shape) {
    let mut h = Harness::new(shape);
    let old = h.peer();
    let new = h.peer();
    let screen = new.screen(1);
    config(&screen);
    let current = h.f.current();
    h.f.drain();
    old.session.close().unwrap();
    h.f.phase(current, "ACTIVE");
    video(&screen, 0x66);
    assert_eq!(video_payloads(&h.f.drain()), [vec![0, 0, 0, 2, 0x65, 0x66]]);
}
fn close_current(shape: Shape) {
    let mut h = Harness::new(shape);
    let old = h.peer();
    let old_screen = old.screen(1);
    config(&old_screen);
    let new = h.peer();
    let screen = new.screen(2);
    config(&screen);
    video(&screen, 0x11);
    let current = h.f.current();
    h.f.drain();
    new.session.close().unwrap();
    h.f.phase(current, "ENDED");
    assert!(h.f.input.state.lock().unwrap().current.is_none());
    assert!(h.f.drain().iter().any(|m| matches!(m, Message::Json(v)
        if v["type"] == "video-format" && v["format"].is_null())));
    video(&screen, 0x22);
    config(&old_screen);
    video(&old_screen, 0x33);
    assert!(h.f.drain().is_empty());
    assert!(!h.f.input.receiver_playback(&stats(current)));
    h.f.picture(false);
}

fn reconfigure(shape: Shape) {
    let mut h = Harness::new(shape);
    let peer = h.peer();
    let first_screen = peer.screen(1);
    config(&first_screen);
    video(&first_screen, 0x11);
    let current = h.f.current();
    let reply = peer.setup(BplistValue::dict([
        ("type", 100u64.into()),
        ("streamConnectionID", 9u64.into()),
        ("audioType", "alert".into()),
        ("audioFormat", 0x10u64.into()),
    ]));
    let audio = peer
        .media
        .transport
        .pipe(reply.get("dataPort").unwrap().as_u64().unwrap());
    let key: [u8; 32] = AirPlayCrypto::hkdf_sha512(
        &peer.session.shared_secret().unwrap(),
        b"DataStream-Salt9",
        carplay_airplay::car_play_media_engine::DATASTREAM_OUTPUT_KEY.as_bytes(),
        32,
    )
    .unwrap()
    .try_into()
    .unwrap();
    let mut counters = MicrophoneCounters {
        sequence: 0,
        timestamp: 0,
        nonce: 0,
    };
    let mut send_audio = |sample: u8| {
        audio.write(
            MicrophonePacketizer::seal_packet(&key, 100, &mut counters, &[0x12, sample], 1)
                .unwrap(),
        );
        let deadline = Instant::now() + WAIT;
        loop {
            let message =
                h.f.rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap();
            if let Message::Binary { header, payload } = message {
                if header.kind == Kind::Audio {
                    assert_eq!(payload, [sample, 0x12]);
                    break;
                }
            }
        }
    };
    send_audio(0x34);
    let declaration = h.f.declaration("2-audio-1");
    let run = peer.media.bridge.state.lock().unwrap().audio[&1].1.clone();
    let control = h.f.input.state.lock().unwrap().control.clone().unwrap();
    let keys = h.f.input.state.lock().unwrap().keys.clone().unwrap();
    h.f.drain();

    let second_screen = peer.screen(2);
    assert!(first_screen.closed());
    first_screen.eof();
    video(&first_screen, 0x22);
    config(&second_screen);
    video(&second_screen, 0x33);
    assert_eq!(h.f.current(), current);
    h.f.phase(current, "ACTIVE");
    assert!(!peer.session.is_closed());
    assert_eq!(h.f.declaration("2-audio-1"), declaration);
    assert!(Arc::ptr_eq(
        &run,
        &peer.media.bridge.state.lock().unwrap().audio[&1].1
    ));
    {
        let state = h.f.input.state.lock().unwrap();
        assert!(Arc::ptr_eq(&control, state.control.as_ref().unwrap()));
        assert!(Arc::ptr_eq(&keys, state.keys.as_ref().unwrap()));
    }
    let messages = h.f.drain();
    assert_eq!(video_payloads(&messages), [vec![0, 0, 0, 2, 0x65, 0x33]]);
    assert!(!messages.iter().any(|m| matches!(m, Message::Json(v)
        if v["type"] == "session" || v["type"] == "audio-format")));
    send_audio(0x56);
}
fn playback(shape: Shape) {
    let mut h = Harness::new(shape);
    let old = h.peer();
    let screen = old.screen(1);
    config(&screen);
    video(&screen, 0x11);
    let first = h.f.current();
    let old_token = h.f.input.state.lock().unwrap().playback.clone().unwrap();
    assert!(h.f.input.receiver_playback(&stats(first)));
    assert_eq!(old.observer.0.load(Ordering::SeqCst), 1);
    h.f.picture(true);

    let new = h.peer();
    let screen = new.screen(2);
    config(&screen);
    let current = h.f.current();
    assert_ne!(current, first);
    h.f.phase(current, "ACTIVE");
    h.f.picture(false);
    assert!(!h.f.input.receiver_playback(&stats(first)));
    assert!(!h.f.input.receiver_playback(&stats(current)));
    assert!(!new.session.report_video_frame_rendered(&old_token));
    video(&screen, 0x22);
    assert!(!h.f.input.receiver_playback(&stats(first)));
    h.f.picture(false);
    // Even a delayed bridge token paired with the current Stream id cannot prove this session.
    let token = h.f.input.state.lock().unwrap().playback.replace(old_token);
    assert!(!h.f.input.receiver_playback(&stats(current)));
    h.f.picture(false);
    h.f.input.state.lock().unwrap().playback = token;
    assert_eq!(new.observer.0.load(Ordering::SeqCst), 0);
    assert!(h.f.input.receiver_playback(&stats(current)));
    assert_eq!(old.observer.0.load(Ordering::SeqCst), 1);
    assert_eq!(new.observer.0.load(Ordering::SeqCst), 1);
    h.f.picture(true);
}

macro_rules! engine_regressions {
    ($module:ident, $shape:expr) => {
        mod $module {
            use super::*;
            #[test]
            fn old_screen_eof_preserves_new_session() {
                overlap($shape, true);
            }
            #[test]
            fn old_teardown_and_late_frames_preserve_new_session() {
                overlap($shape, false);
            }
            #[test]
            fn old_control_without_screen_close_preserves_new_session() {
                old_without_screen($shape);
            }
            #[test]
            fn same_control_screen_reconfiguration_preserves_session_and_audio() {
                reconfigure($shape);
            }
            #[test]
            fn current_control_close_ends_projection() {
                close_current($shape);
            }
            #[test]
            fn playback_proof_belongs_to_each_stream_session() {
                playback($shape);
            }
        }
    };
}
engine_regressions!(per_control_factory, Shape::PerControlFactory);
engine_regressions!(shared_engine, Shape::SharedEngine);
