// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
use carplay_airplay::air_play_session::{setup_enabled_features, teardown_stream_types};
use carplay_airplay::*;
use carplay_iap2::{mfi::BaaCertificatePair, DuplexStream, Receive};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
use BplistValue as V;
#[derive(Default)]
struct Socket {
    input: Mutex<VecDeque<Vec<u8>>>,
    output: Mutex<VecDeque<Vec<u8>>>,
    wake: Condvar,
    closed: AtomicBool,
    failed: AtomicBool,
}
impl Socket {
    fn offer(&self, v: Vec<u8>) {
        self.input.lock().unwrap().push_back(v);
        self.wake.notify_all();
    }
    fn take(&self) -> Vec<u8> {
        let start = Instant::now();
        loop {
            if let Some(v) = self.output.lock().unwrap().pop_front() {
                return v;
            }
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "Timed out waiting for session response"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl DuplexStream for Socket {
    fn send(&self, v: &[u8]) -> Result<()> {
        self.output.lock().unwrap().push_back(v.to_vec());
        Ok(())
    }
    fn recv(&self, _: usize, timeout: Duration) -> Result<Receive> {
        let mut q = self.input.lock().unwrap();
        if q.is_empty()
            && !self.closed.load(Ordering::Acquire)
            && !self.failed.load(Ordering::Acquire)
        {
            q = self.wake.wait_timeout(q, timeout).unwrap().0;
        }
        if self.failed.load(Ordering::Acquire) {
            return Err(std::io::Error::other("control read failed"));
        }
        Ok(if let Some(v) = q.pop_front() {
            Receive::Data(v)
        } else if self.closed.load(Ordering::Acquire) {
            Receive::Eof
        } else {
            Receive::Timeout
        })
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        self.wake.notify_all();
        Ok(())
    }
}
#[derive(Default)]
struct Store(Mutex<BTreeMap<String, [u8; 32]>>);
impl PairingStore for Store {
    fn save(&self, id: &str, k: &[u8; 32]) -> Result<()> {
        self.0.lock().unwrap().insert(id.into(), *k);
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<[u8; 32]>> {
        Ok(self.0.lock().unwrap().get(id).copied())
    }
    fn clear(&self) -> Result<()> {
        self.0.lock().unwrap().clear();
        Ok(())
    }
}
struct Auth;
impl MfiAuthenticator for Auth {
    fn protocol_major(&self) -> Result<u8> {
        Ok(3)
    }
    fn read_certificate(&self, _: usize) -> Result<Vec<u8>> {
        Ok(vec![1])
    }
    fn sign_challenge(&self, _: &[u8]) -> Result<Vec<u8>> {
        Ok(vec![2; 64])
    }
    fn baa_certificates(&self) -> Result<BaaCertificatePair> {
        Err(std::io::Error::other("unused"))
    }
}
#[derive(Default)]
struct Events {
    active: AtomicUsize,
    ended: AtomicUsize,
    commands: Mutex<Vec<String>>,
    ui: AtomicUsize,
    milestones: Mutex<Vec<String>>,
    rendered: AtomicUsize,
    throw_milestone: AtomicBool,
    authentication_failures: AtomicUsize,
    session_errors: Mutex<Vec<u64>>,
}
impl AirPlaySessionListener for Events {
    fn on_session_error(&self, session: &AirPlaySession, _: &str) {
        self.session_errors.lock().unwrap().push(session.id());
    }
    fn on_authentication_failed(&self, _: &AirPlaySession) {
        self.authentication_failures.fetch_add(1, Ordering::Relaxed);
    }
    fn on_startup_milestone(&self, _: &AirPlaySession, name: &str) {
        assert!(
            !self.throw_milestone.load(Ordering::Relaxed),
            "host observer failed"
        );
        self.milestones.lock().unwrap().push(name.into());
    }
    fn on_video_frame_rendered(&self, _: &AirPlaySession) {
        self.rendered.fetch_add(1, Ordering::Relaxed);
    }
    fn on_tcp_accepted(&self, _: AirPlayTcpAccepted) {}
    fn on_session_active(&self, _: Arc<AirPlaySession>) {
        self.active.fetch_add(1, Ordering::Relaxed);
    }
    fn on_session_ended(&self, _: Arc<AirPlaySession>) {
        self.ended.fetch_add(1, Ordering::Relaxed);
    }
    fn on_transport_error(&self, _: &str) {}
    fn on_debug_log(&self, _: &str) {}
    fn on_host_ui_requested(&self, _: &AirPlaySession) {
        self.ui.fetch_add(1, Ordering::Relaxed);
    }
    fn on_command(&self, _: &AirPlaySession, kind: &str, _: &V) {
        self.commands.lock().unwrap().push(kind.into());
    }
}
#[derive(Default)]
struct Media {
    buffered: AtomicUsize,
    controls: AtomicUsize,
    closed: AtomicUsize,
    teardown: Mutex<Vec<u16>>,
    reject: AtomicBool,
}
impl AirPlayMediaHandler for Media {
    fn on_screen(&self, _: &AirPlaySession, _: u16, _: &V) -> Result<Option<u16>> {
        Ok(Some(1234))
    }
    fn on_audio(&self, _: &AirPlaySession, kind: u16, _: &V) -> Result<Option<V>> {
        Ok(Some(V::dict([
            ("type", V::from(kind as u64)),
            ("dataPort", 1235u64.into()),
        ])))
    }
    fn on_buffered_audio(&self, _: &AirPlaySession, _: &V) -> Result<Option<V>> {
        self.buffered.fetch_add(1, Ordering::Relaxed);
        Ok((!self.reject.load(Ordering::Relaxed))
            .then(|| V::dict([("type", 103u64.into()), ("dataPort", 1236u64.into())])))
    }
    fn on_buffered_audio_control(&self, _: &AirPlaySession, _: &str, _: &V) -> Result<Option<V>> {
        self.controls.fetch_add(1, Ordering::Relaxed);
        Ok(Some(V::dict([("rate", V::Real(0.0))])))
    }
    fn on_data_stream(&self, _: &AirPlaySession, _: &V) -> Result<Option<V>> {
        Ok(None)
    }
    fn on_feedback(&self, _: &AirPlaySession) -> Result<Option<V>> {
        Ok(Some(V::dict([("streams", V::Array(vec![]))])))
    }
    fn on_setup_response_sent(&self, _: &AirPlaySession) -> Result<()> {
        Ok(())
    }
    fn on_teardown(&self, _: &AirPlaySession, kind: u16) -> Result<()> {
        self.teardown.lock().unwrap().push(kind);
        Ok(())
    }
    fn on_session_closed(&self, _: &AirPlaySession) -> Result<()> {
        self.closed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
struct Fixture {
    session: Arc<AirPlaySession>,
    media: Arc<Media>,
    events: Arc<Events>,
    socket: Arc<Socket>,
    store: Arc<Store>,
}
fn fixture(config: AirPlayConfig) -> Fixture {
    let key = AirPlayCrypto::ed25519_generate().unwrap();
    let identity = Arc::new(AirPlayIdentity {
        private_key: key.private_key,
        public_key: key.public_key,
        pairing_id: "receiver".into(),
    });
    let (socket, store, media, events) = (
        Arc::new(Socket::default()),
        Arc::new(Store::default()),
        Arc::new(Media::default()),
        Arc::new(Events::default()),
    );
    let session = AirPlaySession::new(
        socket.clone(),
        config,
        identity,
        store.clone(),
        Arc::new(Auth),
        media.clone(),
        events.clone(),
    )
    .unwrap();
    Fixture {
        session,
        media,
        events,
        socket,
        store,
    }
}
fn config() -> AirPlayConfig {
    AirPlayConfig::linux_receiver("dev".into(), "bt".into())
}
fn request(method: &str, path: &str, body: Vec<u8>) -> RtspRequest {
    RtspRequest {
        method: method.into(),
        path: path.into(),
        protocol: "RTSP/1.0".into(),
        headers: Default::default(),
        body,
    }
}
fn call(f: &Fixture, method: &str, path: &str, body: &V) -> RtspResponse {
    f.session
        .handle_request(&request(method, path, BplistCodec::encode(body).unwrap()))
        .unwrap()
}
fn streams(kinds: &[u64]) -> V {
    V::dict([(
        "streams",
        V::Array(
            kinds
                .iter()
                .map(|kind| V::dict([("type", V::from(*kind))]))
                .collect(),
        ),
    )])
}
#[test]
fn disabled_buffered_output_declines_before_media_handler() {
    for disable in [false, true] {
        let mut c = config();
        c.main_buffered_audio = disable;
        c.disable_audio_output = disable;
        let f = fixture(c);
        let response = call(&f, "SETUP", "/", &streams(&[103]));
        assert_eq!(response.status, 200);
        assert_eq!(f.media.buffered.load(Ordering::Relaxed), 0);
        let body = BplistCodec::decode(&response.body).unwrap();
        assert_eq!(body.get("streams").unwrap().as_array().unwrap().len(), 0);
    }
}
#[test]
fn malformed_buffered_control_rejected_before_handler() {
    let mut c = config();
    c.main_buffered_audio = true;
    let f = fixture(c);
    for body in [
        vec![],
        b"bad".to_vec(),
        BplistCodec::encode(&V::Array(vec![])).unwrap(),
    ] {
        assert_eq!(
            f.session
                .handle_request(&request("SETRATE", "/", body))
                .unwrap()
                .status,
            400
        );
    }
    assert_eq!(f.media.controls.load(Ordering::Relaxed), 0);
    assert_eq!(
        f.session
            .handle_request(&request("GETANCHOR", "/", vec![]))
            .unwrap()
            .status,
        200
    );
    assert_eq!(f.media.controls.load(Ordering::Relaxed), 1);
}
#[test]
fn rejected_buffered_replacement_preserves_existing_registration() {
    let mut c = config();
    c.main_buffered_audio = true;
    let f = fixture(c);
    call(&f, "SETUP", "/", &streams(&[103]));
    f.media.reject.store(true, Ordering::Relaxed);
    let rejected = call(&f, "SETUP", "/", &streams(&[103]));
    assert!(BplistCodec::decode(&rejected.body)
        .unwrap()
        .get("streams")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    call(&f, "TEARDOWN", "/", &streams(&[103]));
    assert_eq!(*f.media.teardown.lock().unwrap(), vec![103]);
}
#[test]
fn teardown_type_isolation_and_screen_tokens() {
    let ordered = fixture(config());
    call(&ordered, "SETUP", "/", &streams(&[110, 100, 101, 102]));
    ordered
        .session
        .handle_request(&request("TEARDOWN", "/", vec![]))
        .unwrap();
    assert_eq!(
        *ordered.media.teardown.lock().unwrap(),
        vec![110, 100, 101, 102]
    );

    let f = fixture(config());
    call(&f, "SETUP", "/", &streams(&[110, 100, 101, 102]));
    let first = f.session.main_screen_session_token().unwrap();
    call(&f, "TEARDOWN", "/", &streams(&[100]));
    assert_eq!(*f.media.teardown.lock().unwrap(), vec![100]);
    assert_eq!(f.session.main_screen_session_token(), Some(first));
    call(&f, "TEARDOWN", "/", &streams(&[110]));
    assert!(f.session.main_screen_session_token().is_none());
    call(&f, "SETUP", "/", &streams(&[110]));
    assert_ne!(f.session.main_screen_session_token(), Some(first));
    f.session
        .handle_request(&request("TEARDOWN", "/", vec![]))
        .unwrap();
    assert_eq!(
        *f.media.teardown.lock().unwrap(),
        vec![100, 110, 101, 102, 110]
    );
}
#[test]
fn excluded_alternate_screen_is_not_accepted() {
    let f = fixture(config());
    let response = call(&f, "SETUP", "/", &streams(&[111]));
    assert!(BplistCodec::decode(&response.body)
        .unwrap()
        .get("streams")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn malformed_setup_is_400_and_feedback_is_plist() {
    let f = fixture(config());
    assert_eq!(
        f.session
            .handle_request(&request("SETUP", "/", b"bad".to_vec()))
            .unwrap()
            .status,
        400
    );
    let response = f
        .session
        .handle_request(&request("POST", "/feedback", vec![]))
        .unwrap();
    assert!(BplistCodec::decode(&response.body)
        .unwrap()
        .get("streams")
        .is_some());
}
#[test]
fn commands_dispatch_to_listener_and_record_marks_active() {
    let f = fixture(config());
    call(
        &f,
        "POST",
        "/command",
        &V::dict([
            ("type", "requestUI".into()),
            ("params", V::dict([("url", "oem://".into())])),
        ]),
    );
    assert_eq!(f.events.ui.load(Ordering::Relaxed), 1);
    assert_eq!(*f.events.commands.lock().unwrap(), vec!["requestUI"]);
    f.session
        .handle_request(&request("RECORD", "/", vec![]))
        .unwrap();
    assert_eq!(f.events.active.load(Ordering::Relaxed), 1);
}
#[test]
fn host_ui_excludes_video_playback_and_remote_control_but_keeps_command_observation() {
    let f = fixture(config());
    for params in [
        V::Dictionary(vec![]),
        V::dict([("url", "oem://".into())]),
        V::dict([("url", "videoplayback:".into())]),
    ] {
        call(
            &f,
            "POST",
            "/command",
            &V::dict([("type", "requestUI".into()), ("params", params)]),
        );
    }
    assert_eq!(f.events.ui.load(Ordering::Relaxed), 2);
    assert_eq!(
        *f.events.commands.lock().unwrap(),
        ["requestUI", "requestUI", "requestUI"]
    );
    let mut remote = request(
        "POST",
        "/command",
        BplistCodec::encode(&V::dict([
            ("type", "requestUI".into()),
            (
                "params",
                V::dict([(
                    "data",
                    V::Data(BplistCodec::encode(&V::Dictionary(vec![])).unwrap()),
                )]),
            ),
        ]))
        .unwrap(),
    );
    remote.headers.insert("x-apple-streamid".into(), "3".into());
    assert_eq!(f.session.handle_request(&remote).unwrap().status, 200);
    assert_eq!(f.events.ui.load(Ordering::Relaxed), 2);
    assert_eq!(f.events.commands.lock().unwrap().len(), 3);
}

#[test]
fn external_playback_proof_rejects_replaced_streams_other_phones_and_closed_sessions() {
    let f = fixture(config());
    assert!(f.session.playback_token().is_none());
    call(&f, "SETUP", "/", &streams(&[110]));
    let first = f.session.playback_token().unwrap();
    assert!(f.session.report_video_frame_rendered(&first));
    call(&f, "SETUP", "/", &streams(&[110]));
    assert!(!f.session.report_video_frame_rendered(&first));
    let second = f.session.playback_token().unwrap();
    assert!(f.session.report_video_frame_rendered(&second));
    let other = fixture(config());
    call(&other, "SETUP", "/", &streams(&[110]));
    assert!(!other.session.report_video_frame_rendered(&second));
    let current = other.session.playback_token().unwrap();
    assert!(other.session.report_video_frame_rendered(&current));
    call(&f, "TEARDOWN", "/", &streams(&[110]));
    assert!(!f.session.report_video_frame_rendered(&second));
    other.session.close().unwrap();
    assert!(!other.session.report_video_frame_rendered(&current));
    assert!(other.session.playback_token().is_none());
    assert_eq!(f.events.rendered.load(Ordering::Relaxed), 2);
    assert_eq!(other.events.rendered.load(Ordering::Relaxed), 1);
}

#[test]
fn startup_milestones_only_follow_accepted_protocol_steps_and_observer_panics_are_isolated() {
    let f = fixture(config());
    let peer = AirPlayCrypto::x25519_generate().unwrap();
    for body in [vec![], [vec![1], peer.public_key.to_vec()].concat()] {
        f.session
            .handle_request(&request("POST", "/auth-setup", body))
            .unwrap();
    }
    call(&f, "SETUP", "/", &streams(&[111]));
    call(&f, "SETUP", "/", &streams(&[110]));
    assert_eq!(
        *f.events.milestones.lock().unwrap(),
        ["auth_setup_accepted", "setup_main_screen"]
    );
    f.events.throw_milestone.store(true, Ordering::Relaxed);
    assert_eq!(call(&f, "SETUP", "/", &streams(&[110])).status, 200);
    assert!(f.session.playback_token().is_some());
    f.events.throw_milestone.store(false, Ordering::Relaxed);
    // Control SETUP is observed before transport allocation, even when the host has supplied none.
    assert!(f
        .session
        .handle_request(&request(
            "SETUP",
            "/",
            BplistCodec::encode(&V::Dictionary(vec![])).unwrap()
        ))
        .is_err());
    assert_eq!(
        f.events.milestones.lock().unwrap().last().unwrap(),
        "setup_control"
    );
    f.session.close().unwrap();
}

fn wait_for_end(f: &Fixture) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while f.events.ended.load(Ordering::Acquire) == 0 {
        assert!(Instant::now() < deadline, "control worker did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn transport_milestones_distinguish_peer_eof_and_io_failure() {
    for failed in [false, true] {
        let f = fixture(config());
        if failed {
            f.socket.failed.store(true, Ordering::Release);
        } else {
            f.socket.close().unwrap();
        }
        f.session.start().unwrap();
        wait_for_end(&f);
        assert_eq!(
            *f.events.milestones.lock().unwrap(),
            [if failed {
                "transport_closed_io_failed"
            } else {
                "transport_closed_peer_eof"
            }]
        );
    }
}
#[test]
fn a_control_failure_reaches_the_listener_with_its_session() {
    let f = fixture(config());
    let other = fixture(config());
    assert_ne!(f.session.id(), other.session.id());
    f.socket.failed.store(true, Ordering::Release);
    f.session.start().unwrap();
    wait_for_end(&f);
    assert_eq!(*f.events.session_errors.lock().unwrap(), [f.session.id()]);
    // A clean end is no failure.
    other.socket.close().unwrap();
    other.session.start().unwrap();
    wait_for_end(&other);
    assert!(other.events.session_errors.lock().unwrap().is_empty());
}
#[test]
fn failed_pair_setup_and_auth_setup_are_reported_but_a_failed_pair_verify_is_not() {
    let f = fixture(config());
    let failures = || f.events.authentication_failures.load(Ordering::Relaxed);
    // An unknown controller fails pair-verify; the iPhone then pairs again.
    let verify = request("POST", "/pair-verify", tlv(&[(6, vec![3])]));
    assert_eq!(f.session.handle_request(&verify).unwrap().status, 200);
    assert_eq!(failures(), 0);
    let setup = request("POST", "/pair-setup", tlv(&[(6, vec![1])]));
    assert_eq!(f.session.handle_request(&setup).unwrap().status, 200);
    assert_eq!(failures(), 0);
    let setup = request("POST", "/pair-setup", tlv(&[(6, vec![3])]));
    assert_eq!(f.session.handle_request(&setup).unwrap().status, 200);
    assert_eq!(failures(), 1);
    let auth = request("POST", "/auth-setup", vec![1; 5]);
    assert_eq!(f.session.handle_request(&auth).unwrap().status, 400);
    assert_eq!(failures(), 2);
}
#[test]
fn close_notifies_once_and_terminates_waiting_input() {
    let f = fixture(config());
    f.session.start().unwrap();
    f.session.close().unwrap();
    f.session.close().unwrap();
    assert_eq!(f.media.closed.load(Ordering::Relaxed), 1);
    assert_eq!(f.events.ended.load(Ordering::Relaxed), 1);
    assert!(!f
        .session
        .send_iap_message(&[1], Duration::from_millis(100))
        .unwrap());
    assert!(!f.session.send_touch(&[]).unwrap());
    assert_eq!(
        *f.events.milestones.lock().unwrap(),
        ["transport_closed_local"]
    );
}
#[test]
fn feature_negotiation_and_data_teardown_match_source() {
    let mut c = config();
    c.hevc = true;
    c.main_buffered_audio = true;
    let proposed = vec!["mainBuffered".into(), "videoPlayback".into()];
    assert_eq!(
        setup_enabled_features(&c, Some(&proposed)),
        vec!["hevc", "iAPChannel", "viewAreas", "mainBuffered"]
    );
    assert_eq!(teardown_stream_types(None), None);
    let data = V::dict([(
        "streams",
        V::Array(vec![V::dict([
            ("type", 130u64.into()),
            ("streamID", 9u64.into()),
        ])]),
    )]);
    assert_eq!(teardown_stream_types(Some(&data)), Some(vec![]));
}
fn tlv(items: &[(u8, Vec<u8>)]) -> Vec<u8> {
    Tlv8Codec::encode(
        &items
            .iter()
            .map(|(k, v)| Tlv8Item {
                kind: *k,
                value: v.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn exchange(f: &Fixture, body: &[u8]) -> Vec<u8> {
    let head = format!(
        "POST /pair-verify RTSP/1.0\r\nCSeq: 3\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    f.socket.offer([head.as_bytes(), body].concat());
    f.socket.take()
}
fn derive(shared: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    AirPlayCrypto::hkdf_sha512(shared, salt, info, 32)
        .unwrap()
        .try_into()
        .unwrap()
}
#[test]
fn pair_verify_response_is_plaintext_then_control_encryption_activates() {
    let f = fixture(config());
    let controller = AirPlayCrypto::ed25519_generate().unwrap();
    f.store.save("controller", &controller.public_key).unwrap();
    f.session.start().unwrap();
    let ephemeral = AirPlayCrypto::x25519_generate().unwrap();
    let wire = exchange(
        &f,
        &tlv(&[(6, vec![1]), (3, ephemeral.public_key.to_vec())]),
    );
    assert!(wire.starts_with(b"RTSP/1.0 200 OK"));
    let parsed = RtspMessage::parse_messages(&wire).unwrap();
    let m2 = Tlv8Codec::decode(&parsed.messages[0].body).unwrap();
    let own: [u8; 32] = m2[&3].clone().try_into().unwrap();
    let shared = AirPlayCrypto::x25519_shared(&ephemeral.private_key, &own).unwrap();
    let key = derive(
        &shared,
        b"Pair-Verify-Encrypt-Salt",
        b"Pair-Verify-Encrypt-Info",
    );
    let signature = AirPlayCrypto::ed25519_sign(
        &controller.private_key,
        &[ephemeral.public_key.as_slice(), b"controller", &own].concat(),
    )
    .unwrap();
    let sub = tlv(&[(1, b"controller".to_vec()), (10, signature.to_vec())]);
    let sealed =
        AirPlayCrypto::chacha_seal(&key, &AirPlayCrypto::nonce_label("PV-Msg03"), &sub, &[])
            .unwrap();
    let wire = exchange(&f, &tlv(&[(6, vec![3]), (5, sealed)]));
    assert!(wire.starts_with(b"RTSP/1.0 200 OK"));
    let parsed = RtspMessage::parse_messages(&wire).unwrap();
    assert_eq!(parsed.messages[0].body, tlv(&[(6, vec![4])]));
    let mut cipher = ControlCipher::new(
        derive(&shared, b"Control-Salt", b"Control-Read-Encryption-Key"),
        derive(&shared, b"Control-Salt", b"Control-Write-Encryption-Key"),
    );
    let encrypted = cipher
        .encrypt(b"GET /info RTSP/1.0\r\nCSeq: 4\r\n\r\n")
        .unwrap();
    f.socket.offer(encrypted[..1].to_vec());
    f.socket.offer(encrypted[1..].to_vec());
    let wire = f.socket.take();
    assert!(!wire.starts_with(b"RTSP/"));
    let plain = cipher.decrypt(&wire).unwrap().data;
    let parsed = RtspMessage::parse_messages(&plain).unwrap();
    let info = BplistCodec::decode(&parsed.messages[0].body).unwrap();
    assert_eq!(
        info.get("sourceVersion").and_then(V::as_str),
        Some("950.7.1")
    );
    assert_eq!(f.session.controller_id().as_deref(), Some("controller"));
    assert_eq!(*f.events.milestones.lock().unwrap(), ["pair_verified"]);
    let mut corrupt = cipher
        .encrypt(b"GET /info RTSP/1.0\r\nCSeq: 5\r\n\r\n")
        .unwrap();
    *corrupt.last_mut().unwrap() ^= 1;
    f.socket.offer(corrupt);
    wait_for_end(&f);
    assert_eq!(
        *f.events.milestones.lock().unwrap(),
        ["pair_verified", "transport_closed_decrypt_failed"]
    );
    f.session.close().unwrap();
}
