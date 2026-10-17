// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
// Test input: the simulated iPhone replays the committed clips through the real controller, input
// and Stream connection to a fake Stream consumer.
use carplay_airplay::VideoCodec;
use carplay_iap2::mfi::MfiAuthenticator;
use openxplay_relay::replay_input::{
    clip::{self, Clip, ClipLibrary},
    player::KeyframeModel,
    script::ContextScript,
    InputCounts, TestInput,
};
use openxplay_relay::*;
use oxp_wire::{Header, Kind, Message};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}
const H264_CLIP: &str = "replay-h264-high.h264";
const H265_CLIP: &str = "replay-h265-main.h265";
fn wait_for(mut test: impl FnMut() -> bool) {
    let start = Instant::now();
    while !test() {
        assert!(start.elapsed() < Duration::from_secs(5), "Timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn format_of(clip: &Clip) -> oxp_core::model::VideoFormat {
    let config = &clip.units[clip.config_before(clip.first_key).unwrap()];
    let record = clip::decoder_config(clip.codec, &clip.read(config).unwrap()).unwrap();
    let codec = match clip.codec {
        VideoCodec::H264 => oxp_core::model::VideoCodec::H264,
        VideoCodec::H265 => oxp_core::model::VideoCodec::H265,
    };
    oxp_core::model::VideoFormat::create(0, codec, record.into(), None).unwrap()
}

#[test]
fn committed_clips_are_single_idr_720p_high_and_main_with_matching_indexes() {
    for (file, codec, codec_string) in [
        (H264_CLIP, VideoCodec::H264, "avc1.640020"),
        (H265_CLIP, VideoCodec::H265, "hvc1.1.6.L120.90"),
    ] {
        assert!(std::fs::metadata(data(file)).unwrap().len() < 200_000);
        let indexed = Clip::open(&data(file)).unwrap();
        let raw = Clip::open_raw(&data(file)).unwrap();
        for clip in [&indexed, &raw] {
            assert_eq!(clip.codec, codec);
            assert_eq!(clip.frames, 90);
            assert_eq!(clip.units.iter().filter(|u| u.key).count(), 1);
            assert_eq!(clip.first_key, 1);
            assert!((clip.peak_fps - 30.0).abs() < 0.1, "{}", clip.peak_fps);
            let format = format_of(clip);
            assert_eq!(format.codec_string, codec_string);
            assert_eq!((format.width, format.height), (1280, 720));
        }
        assert!(indexed
            .units
            .iter()
            .zip(&raw.units)
            .all(|(a, b)| a.ranges == b.ranges && a.at_ns == b.at_ns && a.kind == b.kind));
        // The index file opens the same clip.
        let via_index = Clip::open(&data(&format!("{file}.jsonl"))).unwrap();
        assert_eq!(via_index.units.len(), indexed.units.len());
    }
}

#[test]
fn library_selection_and_cli_options() {
    let library = ClipLibrary::open(&[data("")]).unwrap();
    assert_eq!(library.clips.len(), 2);
    assert_eq!(
        library.select(VideoCodec::H264, 60).unwrap().name,
        H264_CLIP
    );
    assert_eq!(
        library.select(VideoCodec::H265, 30).unwrap().name,
        H265_CLIP
    );
    let only = ClipLibrary::open(&[data(H264_CLIP)]).unwrap();
    assert_eq!(
        only.select(VideoCodec::H265, 60).unwrap().codec,
        VideoCodec::H264
    );

    let values = |pairs: &[(&str, String)]| {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    let replay = format!("replay:{}", data("").display());
    assert!(TestInput::from_options(&values(&[])).unwrap().is_none());
    for bad in [
        values(&[("--test-keyframe", "immediate".into())]),
        values(&[("--test-input", data("").display().to_string())]),
        values(&[("--test-input", "replay:/nonexistent/clips".into())]),
        values(&[
            ("--test-input", replay.clone()),
            ("--test-keyframe", "never".into()),
        ]),
        values(&[
            ("--test-input", replay.clone()),
            ("--test-context", "/nonexistent.json".into()),
        ]),
    ] {
        let error = TestInput::from_options(&bad).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{error}");
    }
    let state = std::env::temp_dir().join(format!("oxp-replay-options-{}", std::process::id()));
    let options = Options::parse(&[
        "--state-dir".into(),
        state.display().to_string(),
        "--test-input".into(),
        replay,
        "--test-keyframe".into(),
        "h265:ignore-first,delay:120".into(),
        "--test-context".into(),
        "demo".into(),
    ])
    .unwrap();
    assert!(format!("{:?}", options.test_input).contains("replay:"));
    let _ = std::fs::remove_dir_all(state);
}

struct Hotspot;
impl WirelessHotspotManager for Hotspot {
    fn start(&self, _: Duration) -> openxplay_relay::Result<WirelessHotspotInfo> {
        Ok(WirelessHotspotInfo {
            ssid: "test".into(),
            passphrase: "password".into(),
            channel: 36,
            security: carplay_iap2::Iap2WirelessSecurity::WpaWpa2,
            host_address: Some("10.42.0.1".parse().unwrap()),
            bssid: Some("AA:BB:CC:DD:EE:FF".into()),
            access_point_bssid: Some("AA:BB:CC:DD:EE:FF".into()),
            interface_name: Some("wlan0".into()),
        })
    }
    fn validate_ready(&self) -> openxplay_relay::Result<()> {
        Ok(())
    }
    fn on_car_play_confirmed(&self) -> openxplay_relay::Result<()> {
        Ok(())
    }
    fn close(&self) -> openxplay_relay::Result<()> {
        Ok(())
    }
}
/// The real platform's hotspot side; the test input must not use anything else.
struct HotspotOnly;
fn unused<T>() -> openxplay_relay::Result<T> {
    Err(std::io::Error::other("unused by the test input"))
}
impl WirelessPlatform for HotspotOnly {
    fn check_bluetooth_radio(&self) -> openxplay_relay::Result<()> {
        unused()
    }
    fn prepare_mfi(&self) -> openxplay_relay::Result<MfiResource> {
        unused()
    }
    fn create_hotspot(&self) -> openxplay_relay::Result<Arc<dyn WirelessHotspotManager>> {
        Ok(Arc::new(Hotspot))
    }
    fn start_pairing(&self) -> openxplay_relay::Result<Option<Arc<dyn CloseResource>>> {
        unused()
    }
    fn select_iphone(&self) -> openxplay_relay::Result<BondedDevice> {
        unused()
    }
    fn create_bluetooth(
        &self,
        _: &BondedDevice,
    ) -> openxplay_relay::Result<Arc<dyn BluetoothConnection>> {
        unused()
    }
    fn create_network(
        &self,
        _: WirelessHotspotInfo,
        _: Arc<dyn MfiAuthenticator>,
        _: NetworkCallbacks,
    ) -> openxplay_relay::Result<Arc<dyn NetworkAttachment>> {
        unused()
    }
    fn run_bootstrap(
        &self,
        _: Arc<dyn BluetoothConnection>,
        _: Arc<dyn PendingNetwork>,
        _: Arc<dyn MfiAuthenticator>,
        _: BootstrapCallbacks,
    ) -> openxplay_relay::Result<()> {
        unused()
    }
}

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;
/// The relay shell around the test input: input, Stream connection to a fake consumer, and the
/// controller the shell builds and replaces.
struct Relay {
    input: Arc<DiPlayInput>,
    events: Arc<JsonEvents>,
    logs: Arc<Mutex<Vec<Value>>>,
    test: TestInput,
    stream: StreamConnection,
    peer: UnixStream,
    messages: mpsc::Receiver<Message>,
    controller: Option<(Arc<LinuxCarPlayController>, std::thread::JoinHandle<()>)>,
    socket: PathBuf,
}
impl Relay {
    fn new(
        name: &str,
        keyframes: &str,
        clock: Option<Clock>,
        script: Option<ContextScript>,
    ) -> Self {
        let logs = Arc::new(Mutex::new(Vec::new()));
        let copy = logs.clone();
        let events = Arc::new(JsonEvents::new(Arc::new(move |line| {
            copy.lock()
                .unwrap()
                .push(serde_json::from_str(&line).unwrap())
        })));
        let input = Arc::new(match (clock, &script) {
            (Some(clock), _) => DiPlayInput::with_clock(events.clone(), clock),
            (None, script) => DiPlayInput::with_plugins(events.clone(), script.is_some()),
        });
        let socket =
            std::env::temp_dir().join(format!("oxp-replay-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        let stream = StreamConnection::new(socket.clone(), input.clone(), events.clone());
        stream.start().unwrap();
        let (peer, _) = listener.accept().unwrap();
        let mut reader = peer.try_clone().unwrap();
        let (tx, messages) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(Some(m)) = oxp_wire::read(&mut reader) {
                if tx.send(m).is_err() {
                    break;
                }
            }
        });
        let library = ClipLibrary::open(&[data(H264_CLIP), data(H265_CLIP)]).unwrap();
        let test = TestInput::new(
            "replay:tests/data",
            library,
            KeyframeModel::parse(keyframes).unwrap(),
            script,
        );
        let mut relay = Self {
            input,
            events,
            logs,
            test,
            stream,
            peer,
            messages,
            controller: None,
            socket,
        };
        relay.start_controller();
        relay
    }
    /// What the relay shell does when it builds a controller.
    fn start_controller(&mut self) {
        self.input.configure_session();
        let platform = self.test.platform(
            Arc::new(HotspotOnly),
            self.input.clone(),
            Arc::new(RecordingMediaSink::discard(self.events.clone())),
            self.events.clone(),
        );
        let controller = Arc::new(LinuxCarPlayController::new(
            platform,
            self.events.clone(),
            Duration::from_secs(5),
        ));
        let c = controller.clone();
        let worker = std::thread::spawn(move || {
            let _ = c.run();
        });
        self.controller = Some((controller, worker));
    }
    fn stop_controller(&mut self) {
        if let Some((controller, worker)) = self.controller.take() {
            controller.close().unwrap();
            worker.join().unwrap();
        }
        self.input.stop().unwrap();
    }
    /// The shell's tick: a pending codec or frame-rate change replaces the controller.
    fn tick(&mut self) -> bool {
        let input = self.input.clone();
        let restarted = input.apply_requested_restart(|| self.stop_controller());
        if restarted {
            self.start_controller();
        }
        restarted
    }
    fn send(&self, m: &Message) {
        let mut writer = &self.peer;
        oxp_wire::write(&mut writer, m).unwrap();
    }
    fn expect(&self, what: &str, want: impl Fn(&Message) -> bool) -> Message {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(m) if want(&m) => return m,
                Ok(_) => {}
                Err(_) => panic!("Timed out waiting for {what}"),
            }
        }
    }
    fn session(&self, phase: &str) {
        self.expect(phase, |m| {
            matches!(m, Message::Json(v) if v["type"] == "session" && v["session"]["phase"] == phase)
        });
    }
    fn format(&self, codec: &str) -> Value {
        match self.expect(codec, |m| {
            matches!(m, Message::Json(v) if v["type"] == "video-format" && v["format"]["codec"] == codec)
        }) {
            Message::Json(v) => v["format"].clone(),
            _ => unreachable!(),
        }
    }
    fn frame(&self) -> Header {
        match self.expect(
            "a frame",
            |m| matches!(m, Message::Binary { header, .. } if header.kind == Kind::Video),
        ) {
            Message::Binary { header, .. } => header,
            _ => unreachable!(),
        }
    }
    fn key_frame(&self) {
        self.expect("a key frame", |m| {
            matches!(m, Message::Binary { header, .. }
                if header.kind == Kind::Video && header.flags & oxp_wire::KEYFRAME != 0)
        });
    }
    fn logged(&self, prefix: &str) -> bool {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["message"].as_str().is_some_and(|m| m.starts_with(prefix)))
    }
    fn last_event(&self, name: &str) -> Value {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|v| v["event"] == name)
            .cloned()
            .unwrap_or(Value::Null)
    }
    fn capabilities(&self, hevc: bool, rate: &str) {
        let mut caps: Value = serde_json::from_str(include_str!(
            "../../../../OpenXPlay-Stream/spec/vectors/wire/capabilities.json"
        ))
        .unwrap();
        if hevc {
            caps["decoders"] = json!([{"codecString":"hvc1.1.6.L120.90","hardware":true}]);
        }
        caps["displays"]["0"]["frameRate"] = rate.into();
        self.send(&Message::Json(caps));
    }
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.stop_controller();
        let _ = self.stream.close();
        let _ = std::fs::remove_file(&self.socket);
    }
}
fn binary(kind: Kind, payload: Vec<u8>) -> Message {
    Message::Binary {
        header: Header {
            kind,
            stream: 0,
            flags: 0,
            sequence: 0,
            pts_us: 0,
            sender_clock_us: 0,
        },
        payload,
    }
}
fn touch(pressed: bool) -> Message {
    binary(
        Kind::Touch,
        oxp_wire::encode_touch(&[oxp_wire::Contact {
            id: 0,
            x: 0.25,
            y: 0.75,
            pressed,
        }])
        .unwrap(),
    )
}

#[test]
fn replay_reaches_the_stream_consumer_and_follows_codec_and_rate_restarts() {
    let mut relay = Relay::new("policy", "immediate", None, None);
    // Like the iPhone's stream: the session connects, declares H.264, then a key frame leads.
    relay.session("CONNECTING");
    let format = relay.format("H264");
    assert_eq!(format["codecString"], "avc1.640020");
    assert_eq!(
        (format["width"].as_u64(), format["height"].as_u64()),
        (Some(1280), Some(720))
    );
    assert_eq!(format["fpsHint"], 60);
    relay.session("ACTIVE");
    let first = relay.frame();
    assert_ne!(first.flags & oxp_wire::KEYFRAME, 0);
    let mut previous = first.pts_us;
    for _ in 0..5 {
        let frame = relay.frame();
        assert_eq!(frame.flags & oxp_wire::KEYFRAME, 0);
        assert!(frame.pts_us > previous);
        previous = frame.pts_us;
    }
    assert_eq!(relay.input.video_status()["codec"], "H264");
    assert_eq!(relay.last_event("stage")["stage"], "WirelessActive");
    // The way from the controller attempt to the ACTIVE session is logged as one line.
    let timing = relay.last_event("boot-timing");
    assert_eq!(timing["attempt"], 1);
    assert_eq!(timing["hotspotReused"], false);
    let at = |name: &str| timing[name].as_u64().unwrap();
    assert!(at("attemptStartMs") <= at("hotspotReadyMs"));
    assert!(at("btConnectStartMs") <= at("sessionConnectingMs"));
    assert!(at("hotspotReadyMs") <= at("sessionConnectingMs"));
    assert!(at("sessionConnectingMs") <= at("sessionActiveMs"));
    // The simulated iPhone skips iAP2.
    assert!(timing["authDoneMs"].is_null() && timing["wifiInfoSentMs"].is_null());

    // A receiver's key-frame request is answered with the clip's IDR and its config.
    relay.send(&Message::Json(json!({"type":"keyframe","screen":0})));
    relay.format("H264");
    relay.key_frame();
    assert!(relay.logged("Keyframe requested for screen 0 forwarded=true"));
    assert!(relay.logged("Replay answers a key-frame request in 0 ms"));

    // Touches and keys reach the simulated iPhone, are logged and counted.
    relay.send(&touch(true));
    relay.send(&touch(false));
    relay.send(&binary(Kind::Key, oxp_wire::encode_key(3, true).unwrap()));
    wait_for(|| {
        relay.test.counts()
            == InputCounts {
                touch_events: 2,
                presses: 1,
                keys: 1,
            }
    });
    assert!(relay.logged("Replay touch down contact=0 x=0.250 y=0.750"));
    assert!(relay.logged("Replay touch up contact=0"));
    assert!(relay.logged("Replay key code=3 down"));

    // A hardware HEVC receiver at 30 fps: one session restart applies codec and rate.
    relay.capabilities(true, "FPS_30");
    relay.send(&Message::Json(json!({"type":"restart"})));
    wait_for(|| relay.tick());
    relay.session("ENDED");
    relay.session("CONNECTING");
    let format = relay.format("H265");
    assert_eq!(format["codecString"], "hvc1.1.6.L120.90");
    assert_eq!(format["fpsHint"], 30);
    relay.key_frame();
    let session = relay.last_event("replay-session");
    assert_eq!(
        (session["codec"].as_str(), session["fps"].as_u64()),
        (Some("H265"), Some(30))
    );
    assert_eq!(session["clip"], H265_CLIP);
    assert_eq!(relay.input.video_status()["codec"], "H265");
    assert!(!relay.tick());

    // A later codec change waits for the policy's restart limiter, as with the iPhone.
    relay.capabilities(false, "FPS_30");
    wait_for(|| {
        relay.input.video_status()["reason"] == "hardware-hevc-unavailable; codec-change-pending"
    });
    assert!(!relay.tick());
}

#[test]
fn ignored_hevc_key_frame_requests_fall_back_to_the_h264_clip() {
    let offset = Arc::new(Mutex::new(Duration::ZERO));
    let o = offset.clone();
    let clock: Clock = Arc::new(move || Instant::now() + *o.lock().unwrap());
    let mut relay = Relay::new("fallback", "h265:ignore-all", Some(clock), None);
    relay.format("H264");
    relay.capabilities(true, "AUTO");
    wait_for(|| relay.tick());
    relay.format("H265");
    relay.key_frame();
    relay.send(&Message::Json(json!({"type":"keyframe","screen":0})));
    wait_for(|| relay.logged("Replay ignored a key-frame request (ignore-all)"));
    // The relay repeats the request every second; the iPhone ignores all of them.
    *offset.lock().unwrap() += Duration::from_secs(2);
    wait_for(|| {
        relay
            .logs
            .lock()
            .unwrap()
            .iter()
            .filter(|v| {
                v["message"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("Replay ignored"))
            })
            .count()
            >= 2
    });
    *offset.lock().unwrap() += Duration::from_secs(7);
    wait_for(|| relay.tick());
    relay.session("ENDED");
    relay.format("H264");
    relay.key_frame();
    assert!(relay.logged("video codec H264 reason=hevc-keyframe-timeout"));
    assert_eq!(
        relay.input.video_status()["reason"],
        "hevc-keyframe-timeout"
    );
    assert_eq!(relay.last_event("replay-session")["clip"], H264_CLIP);
}

#[test]
fn stream_restart_answers_and_context_script_reach_an_opted_in_consumer() {
    let script = ContextScript::parse(
        "script",
        r#"[{"nowPlaying":{"title":"Replay Song","playing":true}},
            {"afterMs":50,"call":"ringing","activity":{"phoneCall":true}}]"#,
    )
    .unwrap();
    let relay = Relay::new("context", "delay:150,stream-restart", None, Some(script));
    relay.send(&Message::hello("consumer", "fake", &oxp_wire::context_client("fake")).unwrap());
    relay.format("H264");
    relay.key_frame();
    let seen = std::cell::Cell::new((false, false));
    relay.expect("context", |m| {
        if let Message::Json(v) = m {
            let (mut title, mut call) = seen.get();
            title |= v["topic"] == "nowPlaying" && v["value"]["title"] == "Replay Song";
            call |= v["topic"] == "call" && v["value"]["state"] == "ringing";
            seen.set((title, call));
        }
        seen.get() == (true, true)
    });
    // The answer comes 150 ms later as a stream restart: the config again, then the key frame.
    let asked = Instant::now();
    relay.send(&Message::Json(json!({"type":"keyframe","screen":0})));
    relay.format("H264");
    relay.key_frame();
    assert!(asked.elapsed() >= Duration::from_millis(140));
    assert!(relay.logged("Replay answers a key-frame request in 150 ms with a stream restart"));
    assert!(relay.logged("Replay context step 2 of script: call ringing"));
}
