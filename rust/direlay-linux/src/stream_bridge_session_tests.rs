// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use super::*;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

#[derive(Default)]
struct Control {
    keys: Mutex<Vec<u8>>,
}
impl SessionControl for Control {
    fn send_touch(&self, _: &[AirPlayContact]) -> Result<bool> {
        Ok(true)
    }
    fn send_media_button(&self, index: u8) -> Result<bool> {
        self.keys.lock().unwrap().push(index);
        Ok(true)
    }
}
struct Fixture {
    events: Arc<JsonEvents>,
    input: Arc<DiPlayInput>,
    rx: Receiver<Message>,
}
impl Fixture {
    fn new() -> Self {
        let events = Arc::new(JsonEvents::new(Arc::new(|_| {})));
        let input = Arc::new(DiPlayInput::new(events.clone()));
        let (tx, rx) = mpsc::sync_channel(128);
        input.state.lock().unwrap().output = Some(tx);
        Self { events, input, rx }
    }
    fn bridge(&self, control: Arc<dyn SessionControl>) -> StreamBridgeSink {
        StreamBridgeSink::new(
            self.input.clone(),
            Arc::new(RecordingMediaSink::discard(self.events.clone())),
            control,
            self.events.clone(),
        )
    }
    fn current(&self) -> u64 {
        self.input.state.lock().unwrap().current.unwrap()
    }
    fn declaration(&self, key: &str) -> Message {
        self.input.state.lock().unwrap().declarations[key].clone()
    }
    fn phase(&self, id: u64, phase: &str) {
        let Message::Json(v) = self.declaration("0-session") else {
            panic!("missing session declaration")
        };
        assert_eq!(v["session"]["id"], id.to_string());
        assert_eq!(v["session"]["phase"], phase);
    }
    fn drain(&self) -> Vec<Message> {
        self.rx.try_iter().collect()
    }
    fn picture(&self, expected: bool) {
        let facts = self.input.runtime_path_facts();
        assert_eq!(facts.picture_on_car, expected);
        let status = serde_json::from_str(include_str!(
            "../../../admin-web/tests/fixtures/linux/status-idle.json"
        ))
        .unwrap();
        let path = crate::path_input::build(
            &facts,
            &crate::settings::Settings::default(),
            &status,
            0,
            true,
            true,
        );
        assert_eq!(path["live"]["pictureOnCar"], expected);
    }
}
fn configure(bridge: &StreamBridgeSink) {
    bridge.on_video_codec(110, VideoCodec::H264).unwrap();
    bridge
        .on_video_config(
            110,
            &[
                1, 0x42, 0xc0, 0x1e, 0xff, 0xe1, 0, 9, 0x67, 0x42, 0xc0, 0x1e, 0xf4, 5, 1, 0xec,
                0x80, 1, 0, 4, 0x68, 0xce, 0x3c, 0x80,
            ],
        )
        .unwrap();
}
fn frame(bridge: &StreamBridgeSink, byte: u8) {
    bridge
        .on_video_frame(110, &[0, 0, 1, 0x65, byte], 0, 1_000_000)
        .unwrap();
}
fn video_payloads(messages: &[Message]) -> Vec<Vec<u8>> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::Binary { header, payload } if header.kind == Kind::Video => {
                Some(payload.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn old_control_close_and_late_callbacks_preserve_new_frames_and_format() {
    let f = Fixture::new();
    let old = f.bridge(Arc::new(Control::default()));
    old.on_screen_stream_active(110, true);
    configure(&old);
    frame(&old, 0x88);
    let old_id = f.current();
    let new = f.bridge(Arc::new(Control::default()));
    let new_id = f.current();
    assert_ne!(old_id, new_id);
    f.phase(new_id, "CONNECTING");
    new.on_screen_stream_active(110, true);
    configure(&new);
    frame(&new, 0x99);
    let format = f.declaration("1-video-0");
    assert_eq!(video_payloads(&f.drain()).len(), 2);

    old.on_screen_stream_active(110, false);
    old.on_video_codec(110, VideoCodec::H265).unwrap();
    configure(&old);
    frame(&old, 0xaa);
    old.end().unwrap();
    old.on_screen_stream_active(110, true);
    assert!(f.drain().is_empty());
    assert_eq!(f.declaration("1-video-0"), format);
    f.phase(new_id, "ACTIVE");
    frame(&new, 0xbb);
    assert_eq!(video_payloads(&f.drain()), [vec![0, 0, 0, 2, 0x65, 0xbb]]);
}

#[test]
fn closing_old_control_without_a_screen_preserves_new_frames() {
    let f = Fixture::new();
    let old = f.bridge(Arc::new(Control::default()));
    let new = f.bridge(Arc::new(Control::default()));
    let id = f.current();
    new.on_screen_stream_active(110, true);
    configure(&new);
    frame(&new, 0x88);
    old.end().unwrap();
    frame(&new, 0x99);
    assert_eq!(video_payloads(&f.drain()).len(), 2);
    f.phase(id, "ACTIVE");
}

#[test]
fn current_control_close_ends_projection_and_rejects_late_frames() {
    let f = Fixture::new();
    let bridge = f.bridge(Arc::new(Control::default()));
    let id = f.current();
    bridge.on_screen_stream_active(110, true);
    configure(&bridge);
    frame(&bridge, 0x88);
    f.drain();
    bridge.on_screen_stream_active(110, false);
    bridge.end().unwrap();
    f.phase(id, "ENDED");
    assert!(f.input.state.lock().unwrap().current.is_none());
    let messages = f.drain();
    assert!(messages.iter().any(
        |m| matches!(m, Message::Json(v) if v["type"] == "video-format" && v["format"].is_null())
    ));
    frame(&bridge, 0x99);
    assert!(f.drain().is_empty());
    assert!(!f.input.key(7, true).unwrap());
    assert!(!f.input.touch(0, &[]).unwrap());
    f.picture(false);
}

#[test]
fn saved_old_recovery_handler_and_late_registration_are_fenced() {
    let f = Fixture::new();
    let old = f.bridge(Arc::new(Control::default()));
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let handler: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    old.set_video_recovery_handler(110, handler.clone());
    let saved = f.input.state.lock().unwrap().recovery[&0].clone();
    saved();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let new = f.bridge(Arc::new(Control::default()));
    let id = f.current();
    let (tx, rx) = mpsc::channel();
    new.set_video_recovery_handler(
        110,
        Arc::new(move || {
            let _ = tx.send(());
        }),
    );
    let current = f.input.state.lock().unwrap().recovery[&0].clone();
    saved();
    old.set_video_recovery_handler(110, handler);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(Arc::ptr_eq(
        &current,
        &f.input.state.lock().unwrap().recovery[&0]
    ));
    assert!(f.input.request_keyframe(0).unwrap());
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(f.current(), id);

    new.on_screen_stream_active(110, false);
    saved();
    old.on_screen_stream_active(110, true);
    assert!(f.input.state.lock().unwrap().current.is_none());
    new.on_screen_stream_active(110, true);
    assert_ne!(f.current(), id);
    assert!(f.input.request_keyframe(0).unwrap());
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

fn audio() -> (AudioStreamId, AudioFormat) {
    (
        AudioStreamId {
            kind: 100,
            audio_type: "alert".into(),
        },
        AudioFormat {
            codec: AudioCodecKind::Lpcm,
            sample_rate: 16000,
            channels: 1,
            payload_type: 96,
            audio_type: "alert".into(),
            rtp_clock_rate: 16000,
        },
    )
}
fn audio_packet(f: &Fixture, bridge: &StreamBridgeSink, sample: u8) {
    let (id, format) = audio();
    let mut rtp = vec![0; 12];
    rtp.extend_from_slice(&[0x12, sample]);
    bridge.on_audio_rtp(&id, &format, &rtp, 0).unwrap();
    loop {
        if let Message::Binary { header, payload } =
            f.rx.recv_timeout(Duration::from_secs(2)).unwrap()
        {
            if header.kind == Kind::Audio {
                assert_eq!(payload, [sample, 0x12]);
                break;
            }
        }
    }
}

#[test]
fn same_control_reconfiguration_keeps_active_session_audio_control_and_keys() {
    let f = Fixture::new();
    let control = Arc::new(Control::default());
    let bridge = f.bridge(control.clone());
    bridge.on_screen_stream_active(110, true);
    configure(&bridge);
    frame(&bridge, 0x88);
    let id = f.current();
    let (audio_id, audio_format) = audio();
    bridge
        .on_audio_started(&audio_id, &audio_format, 0)
        .unwrap();
    audio_packet(&f, &bridge, 0x34);
    let stream = AudioPolicy::stream(&audio_id).unwrap();
    let declaration = f.declaration(&format!("2-audio-{stream}"));
    let run = bridge.state.lock().unwrap().audio[&stream].1.clone();
    let keys = f.input.state.lock().unwrap().keys.clone().unwrap();
    f.drain();

    // A replacement screen on this control reports active again, without an inactive callback.
    bridge.on_screen_stream_active(110, true);
    bridge.set_video_recovery_handler(110, Arc::new(|| {}));
    configure(&bridge);
    frame(&bridge, 0x99);
    assert_eq!(f.current(), id);
    f.phase(id, "ACTIVE");
    assert_eq!(f.declaration(&format!("2-audio-{stream}")), declaration);
    assert!(Arc::ptr_eq(
        &run,
        &bridge.state.lock().unwrap().audio[&stream].1
    ));
    {
        let state = f.input.state.lock().unwrap();
        assert!(state.active);
        let expected: Arc<dyn SessionControl> = control.clone();
        assert!(Arc::ptr_eq(state.control.as_ref().unwrap(), &expected));
        assert!(Arc::ptr_eq(state.keys.as_ref().unwrap(), &keys));
    }
    assert!(f.input.key(7, true).unwrap());
    assert!(f.input.key(7, false).unwrap());
    assert_eq!(*control.keys.lock().unwrap(), [3]);
    let messages = f.drain();
    assert_eq!(video_payloads(&messages), [vec![0, 0, 0, 2, 0x65, 0x99]]);
    assert!(!messages.iter().any(
        |m| matches!(m, Message::Json(v) if v["type"] == "session" || v["type"] == "audio-format")
    ));
    audio_packet(&f, &bridge, 0x56);
    bridge.end().unwrap();
}

#[test]
fn real_teardown_then_reopen_on_same_control_starts_a_new_session() {
    let f = Fixture::new();
    let bridge = f.bridge(Arc::new(Control::default()));
    bridge.on_screen_stream_active(110, true);
    configure(&bridge);
    let first = f.current();
    bridge.on_screen_stream_active(110, false);
    f.phase(first, "ENDED");
    f.drain();
    bridge.on_screen_stream_active(110, true);
    let second = f.current();
    assert_ne!(first, second);
    f.phase(second, "CONNECTING");
    frame(&bridge, 0x88);
    assert!(video_payloads(&f.drain()).is_empty());
    configure(&bridge);
    frame(&bridge, 0x99);
    f.phase(second, "ACTIVE");
    assert_eq!(video_payloads(&f.drain()).len(), 1);
}

// Only SETUP acceptance is needed to obtain real, opaque playback tokens through public APIs.
struct TokenHost(AtomicUsize);
impl carplay_iap2::DuplexStream for TokenHost {
    fn send(&self, _: &[u8]) -> Result<()> {
        panic!("unexpected socket write")
    }
    fn recv(&self, _: usize, _: Duration) -> Result<carplay_iap2::Receive> {
        panic!("unexpected socket read")
    }
    fn close(&self) -> Result<()> {
        Ok(())
    }
}
impl PairingStore for TokenHost {
    fn save(&self, _: &str, _: &[u8; 32]) -> Result<()> {
        panic!("unexpected pairing write")
    }
    fn get(&self, _: &str) -> Result<Option<[u8; 32]>> {
        Ok(None)
    }
    fn clear(&self) -> Result<()> {
        Ok(())
    }
}
impl MfiAuthenticator for TokenHost {
    fn protocol_major(&self) -> Result<u8> {
        panic!("unexpected authentication")
    }
    fn read_certificate(&self, _: usize) -> Result<Vec<u8>> {
        panic!("unexpected authentication")
    }
    fn sign_challenge(&self, _: &[u8]) -> Result<Vec<u8>> {
        panic!("unexpected authentication")
    }
    fn baa_certificates(&self) -> Result<carplay_iap2::mfi::BaaCertificatePair> {
        panic!("unexpected authentication")
    }
}
impl AirPlaySessionListener for TokenHost {
    fn on_video_frame_rendered(&self, _: &AirPlaySession) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl AirPlayMediaHandler for TokenHost {
    fn on_screen(&self, _: &AirPlaySession, _: u16, _: &BplistValue) -> Result<Option<u16>> {
        Ok(Some(1))
    }
    fn on_audio(&self, _: &AirPlaySession, _: u16, _: &BplistValue) -> Result<Option<BplistValue>> {
        panic!("unexpected audio setup")
    }
    fn on_buffered_audio(
        &self,
        _: &AirPlaySession,
        _: &BplistValue,
    ) -> Result<Option<BplistValue>> {
        panic!("unexpected audio setup")
    }
    fn on_buffered_audio_control(
        &self,
        _: &AirPlaySession,
        _: &str,
        _: &BplistValue,
    ) -> Result<Option<BplistValue>> {
        panic!("unexpected audio control")
    }
    fn on_data_stream(&self, _: &AirPlaySession, _: &BplistValue) -> Result<Option<BplistValue>> {
        panic!("unexpected data setup")
    }
    fn on_feedback(&self, _: &AirPlaySession) -> Result<Option<BplistValue>> {
        Ok(None)
    }
    fn on_setup_response_sent(&self, _: &AirPlaySession) -> Result<()> {
        Ok(())
    }
    fn on_teardown(&self, _: &AirPlaySession, _: u16) -> Result<()> {
        Ok(())
    }
    fn on_session_closed(&self, _: &AirPlaySession) -> Result<()> {
        Ok(())
    }
}
fn playback_control() -> (Arc<AirPlaySession>, Arc<TokenHost>) {
    let host = Arc::new(TokenHost(AtomicUsize::new(0)));
    let control = AirPlaySession::new(
        host.clone(),
        AirPlayConfig::linux_receiver("bridge-test".into(), "02:00:00:00:00:01".into()),
        Arc::new(AirPlayIdentity::generate().unwrap()),
        host.clone(),
        host.clone(),
        host.clone(),
        host.clone(),
    )
    .unwrap();
    let body = BplistValue::dict([(
        "streams",
        BplistValue::Array(vec![BplistValue::dict([("type", 110u64.into())])]),
    )]);
    let response = control
        .handle_request(&RtspRequest {
            method: "SETUP".into(),
            path: "/".into(),
            protocol: "RTSP/1.0".into(),
            headers: Default::default(),
            body: BplistCodec::encode(&body).unwrap(),
        })
        .unwrap();
    assert_eq!(response.status, 200);
    assert!(control.playback_token().is_some());
    (control, host)
}
fn stats(id: u64) -> Value {
    json!({"type":"stats","screen":0,"session":id.to_string()})
}

#[test]
fn replacement_requires_its_own_playback_proof_and_resets_path_picture() {
    let f = Fixture::new();
    let (old_control, old_host) = playback_control();
    let old = f.bridge(old_control.clone());
    old.on_screen_stream_active(110, true);
    configure(&old);
    frame(&old, 0x88);
    let old_id = f.current();
    let old_token = f.input.state.lock().unwrap().playback.clone().unwrap();
    assert!(f.input.control(&Message::Json(stats(old_id))).unwrap());
    assert_eq!(old_host.0.load(Ordering::SeqCst), 1);
    f.picture(true);

    let (new_control, new_host) = playback_control();
    let new = f.bridge(new_control.clone());
    let new_id = f.current();
    assert_ne!(old_id, new_id);
    f.phase(new_id, "CONNECTING");
    assert!(!f
        .input
        .state
        .lock()
        .unwrap()
        .declarations
        .contains_key("1-video-0"));
    f.picture(false);
    assert!(!f.input.receiver_playback(&stats(old_id)));
    assert!(!f.input.receiver_playback(&stats(new_id)));
    new.on_screen_stream_active(110, true);
    configure(&new);
    frame(&new, 0x99);
    f.phase(new_id, "ACTIVE");
    f.picture(false);
    assert!(!f.input.receiver_playback(&stats(old_id)));
    assert!(!new_control.report_video_frame_rendered(&old_token));
    // A delayed token captured by the old bridge cannot prove the replacement, even with its id.
    let current_token = f.input.state.lock().unwrap().playback.replace(old_token);
    assert!(!f.input.receiver_playback(&stats(new_id)));
    f.picture(false);
    f.input.state.lock().unwrap().playback = current_token;
    old.end().unwrap();
    frame(&old, 0xaa);
    assert!(f.input.receiver_playback(&stats(new_id)));
    assert_eq!(old_host.0.load(Ordering::SeqCst), 1);
    assert_eq!(new_host.0.load(Ordering::SeqCst), 1);
    f.picture(true);
    new.end().unwrap();
    assert!(!f.input.receiver_playback(&stats(new_id)));
    f.picture(false);
}
