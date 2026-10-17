// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Test input: a simulated iPhone that replays recorded or synthetic video into the relay.
//!
//! It stands in for Bluetooth, iAP2, MFi and AirPlay behind the controller's platform interface;
//! the hotspot, the Stream outputs, the codec and frame-rate policy with its session restarts, the
//! key-frame recovery and the admin page run unchanged. Each controller attempt is one CarPlay
//! session that replays a clip of the codec and frame rate the policy configured, through the same
//! `StreamBridgeSink` the AirPlay media engine feeds.
pub mod clip;
pub mod player;
pub mod script;

use crate::Result;
use crate::*;
use carplay_airplay::{AirPlayContact, MediaSink, SessionControl, VideoCodec};
use carplay_iap2::{
    mfi::{BaaCertificatePair, MfiAuthenticator},
    DuplexStream, Iap2WirelessCarPlayEndpoint, Receive,
};
use clip::{codec_name, ClipLibrary};
use player::{KeyframeModel, Output, Player, Request};
use script::ContextScript;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};

/// The main screen's stream type, as the AirPlay media engine reports it.
const SCREEN: u16 = 110;
const STATS_INTERVAL_NS: u64 = 30_000_000_000;

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}
fn now_ns() -> u64 {
    oxp_core::monotonic_nanos() as u64
}

/// Touches and keys the simulated iPhone received, across sessions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputCounts {
    /// Touch reports (every contact update).
    pub touch_events: u64,
    /// Contacts that went down.
    pub presses: u64,
    pub keys: u64,
}
#[derive(Default)]
struct Counters {
    touch_events: AtomicU64,
    presses: AtomicU64,
    keys: AtomicU64,
}
impl Counters {
    fn snapshot(&self) -> InputCounts {
        InputCounts {
            touch_events: self.touch_events.load(Ordering::Relaxed),
            presses: self.presses.load(Ordering::Relaxed),
            keys: self.keys.load(Ordering::Relaxed),
        }
    }
}
struct Config {
    spec: String,
    library: ClipLibrary,
    keyframes: KeyframeModel,
    script: Option<ContextScript>,
    counters: Counters,
    announced: AtomicBool,
}
/// The parsed `--test-input` options; cheap to clone.
#[derive(Clone)]
pub struct TestInput(Arc<Config>);
impl std::fmt::Debug for TestInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TestInput({})", self.0.spec)
    }
}
impl TestInput {
    /// `--test-input replay:PATH[,PATH...]` with the optional `--test-keyframe` and
    /// `--test-context`; None without `--test-input`.
    pub fn from_options(values: &BTreeMap<String, String>) -> Result<Option<Self>> {
        let Some(spec) = values.get("--test-input") else {
            if let Some(option) = ["--test-keyframe", "--test-context"]
                .into_iter()
                .find(|o| values.contains_key(*o))
            {
                return Err(invalid(format!("{option} needs --test-input")));
            }
            return Ok(None);
        };
        let list = spec
            .strip_prefix("replay:")
            .ok_or_else(|| invalid("--test-input must be replay:PATH[,PATH...]"))?;
        let paths = list
            .split(',')
            .filter(|p| !p.is_empty())
            .map(|p| -> Result<PathBuf> {
                let p = PathBuf::from(p);
                Ok(if p.is_absolute() {
                    p
                } else {
                    std::env::current_dir()?.join(p)
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if paths.is_empty() {
            return Err(invalid("--test-input replay: needs a file or directory"));
        }
        let keyframes = values
            .get("--test-keyframe")
            .map(|s| KeyframeModel::parse(s))
            .transpose()?
            .unwrap_or_default();
        let script = values
            .get("--test-context")
            .map(|s| ContextScript::load(s))
            .transpose()?;
        Ok(Some(Self::new(
            spec,
            ClipLibrary::open(&paths)?,
            keyframes,
            script,
        )))
    }
    pub fn new(
        spec: &str,
        library: ClipLibrary,
        keyframes: KeyframeModel,
        script: Option<ContextScript>,
    ) -> Self {
        Self(Arc::new(Config {
            spec: spec.into(),
            library,
            keyframes,
            script,
            counters: Counters::default(),
            announced: AtomicBool::new(false),
        }))
    }
    pub fn counts(&self) -> InputCounts {
        self.0.counters.snapshot()
    }
    /// Replaces the iPhone side of `inner`, which keeps providing the hotspot. Call it after the
    /// real platform consumed the pending session configuration (`DiPlayInput::configure_session`):
    /// the session replays the codec and frame rate applied there.
    pub fn platform(
        &self,
        inner: Arc<dyn WirelessPlatform>,
        input: Arc<DiPlayInput>,
        sink: Arc<dyn MediaSink>,
        events: Arc<JsonEvents>,
    ) -> Arc<dyn WirelessPlatform> {
        let config = &self.0;
        if !config.announced.swap(true, Ordering::AcqRel) {
            events.emit(
                "test-input",
                &serde_json::json!({"input":config.spec,
                    "keyframes":config.keyframes.to_string(),
                    "context":config.script.as_ref().map(|s| s.name.clone()),
                    "clips":config.library.clips.iter().map(|c| serde_json::json!({
                        "name":c.name,"codec":codec_name(c.codec),"frames":c.frames,
                        "durationMs":c.duration_ns / 1_000_000,
                        "peakFps":(c.peak_fps * 10.0).round() / 10.0})).collect::<Vec<_>>()}),
            );
            for note in &config.library.notes {
                events.log(&format!("Replay {note}"));
            }
        }
        let (codec, fps) = {
            let s = input.state.lock().unwrap();
            let codec = match s.applied_video.codec {
                oxp_core::model::VideoCodec::H264 => VideoCodec::H264,
                oxp_core::model::VideoCodec::H265 => VideoCodec::H265,
            };
            (codec, s.applied_fps)
        };
        Arc::new(ReplayPlatform {
            inner,
            plan: Arc::new(Plan {
                config: self.0.clone(),
                input,
                sink,
                events,
                codec,
                fps,
            }),
            network: Mutex::new(None),
        })
    }
}

/// One controller attempt: what its session replays and where it goes.
struct Plan {
    config: Arc<Config>,
    input: Arc<DiPlayInput>,
    sink: Arc<dyn MediaSink>,
    events: Arc<JsonEvents>,
    codec: VideoCodec,
    fps: u32,
}
struct ReplayPlatform {
    inner: Arc<dyn WirelessPlatform>,
    plan: Arc<Plan>,
    network: Mutex<Option<Arc<ReplayNetwork>>>,
}
impl WirelessPlatform for ReplayPlatform {
    fn check_bluetooth_radio(&self) -> Result<()> {
        Ok(())
    }
    fn prepare_mfi(&self) -> Result<MfiResource> {
        Ok(MfiResource {
            authenticator: Arc::new(NoMfi),
        })
    }
    fn create_hotspot(&self) -> Result<Arc<dyn WirelessHotspotManager>> {
        self.inner.create_hotspot()
    }
    fn prepare_hotspot(&self) {
        self.inner.prepare_hotspot()
    }
    fn planned_hotspot_ssid(&self) -> Option<String> {
        self.inner.planned_hotspot_ssid()
    }
    fn hotspot_reused(&self) -> bool {
        self.inner.hotspot_reused()
    }
    fn keep_hotspot_for_retry(&self) {
        self.inner.keep_hotspot_for_retry()
    }
    fn cancel_hotspot_retry(&self) -> Result<()> {
        self.inner.cancel_hotspot_retry()
    }
    // `hotspot_stations` keeps its default (unknown): no iPhone ever joins the AP here, and an
    // attempt must not take the AP down for lack of one.
    fn hotspot_down_remaining(&self) -> Duration {
        self.inner.hotspot_down_remaining()
    }
    fn start_pairing(&self) -> Result<Option<Arc<dyn CloseResource>>> {
        Ok(None)
    }
    fn select_iphone(&self) -> Result<BondedDevice> {
        Ok(BondedDevice {
            address: "00:00:00:00:00:00".into(),
            name: "Replay iPhone".into(),
            connected: true,
            paired: true,
            phone_class: true,
            iap2: true,
        })
    }
    fn create_bluetooth(&self, _: &BondedDevice) -> Result<Arc<dyn BluetoothConnection>> {
        Ok(Arc::new(ReplayLink))
    }
    fn create_network(
        &self,
        info: WirelessHotspotInfo,
        _: Arc<dyn MfiAuthenticator>,
        callbacks: NetworkCallbacks,
    ) -> Result<Arc<dyn NetworkAttachment>> {
        let network = Arc::new(ReplayNetwork {
            plan: self.plan.clone(),
            address: info.host_address,
            callbacks,
            closed: AtomicBool::new(false),
            session: Mutex::new(None),
        });
        *self.network.lock().unwrap() = Some(network.clone());
        Ok(network)
    }
    /// The iAP2 bootstrap is skipped: once the network is up, StartSession goes out and the
    /// session connects at once.
    fn run_bootstrap(
        &self,
        _: Arc<dyn BluetoothConnection>,
        network: Arc<dyn PendingNetwork>,
        _: Arc<dyn MfiAuthenticator>,
        callbacks: BootstrapCallbacks,
    ) -> Result<()> {
        // Ends with an error when the attempt does.
        while network.wait(Duration::from_secs(1))?.is_none() {}
        (callbacks.on_start_session)();
        let network = self
            .network
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| std::io::Error::other("Replay network missing"))?;
        network.connect()
    }
}
struct NoMfi;
impl MfiAuthenticator for NoMfi {
    fn protocol_major(&self) -> Result<u8> {
        Err(std::io::Error::other(
            "The test input has no MFi coprocessor",
        ))
    }
    fn read_certificate(&self, _: usize) -> Result<Vec<u8>> {
        self.protocol_major().map(|_| Vec::new())
    }
    fn sign_challenge(&self, _: &[u8]) -> Result<Vec<u8>> {
        self.protocol_major().map(|_| Vec::new())
    }
    fn baa_certificates(&self) -> Result<BaaCertificatePair> {
        Err(std::io::Error::other(
            "The test input has no MFi coprocessor",
        ))
    }
}
struct ReplayLink;
impl DuplexStream for ReplayLink {
    fn send(&self, _: &[u8]) -> Result<()> {
        Ok(())
    }
    fn recv(&self, _: usize, timeout: Duration) -> Result<Receive> {
        std::thread::sleep(timeout.min(Duration::from_millis(100)));
        Ok(Receive::Timeout)
    }
    fn close(&self) -> Result<()> {
        Ok(())
    }
}
impl BluetoothConnection for ReplayLink {
    fn connect(&self) -> Result<()> {
        Ok(())
    }
}

struct ReplayNetwork {
    plan: Arc<Plan>,
    address: Option<std::net::Ipv4Addr>,
    callbacks: NetworkCallbacks,
    closed: AtomicBool,
    session: Mutex<Option<ReplaySession>>,
}
impl ReplayNetwork {
    fn connect(&self) -> Result<()> {
        let mut session = self.session.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(std::io::Error::other("Replay network closed"));
        }
        if session.is_none() {
            *session = Some(ReplaySession::start(&self.plan, &self.callbacks)?);
        }
        Ok(())
    }
}
impl NetworkAttachment for ReplayNetwork {
    fn start(&self) -> Result<()> {
        self.plan.events.log(&format!(
            "Replay input attached to {}",
            self.address.map_or("the network".into(), |a| a.to_string())
        ));
        Ok(())
    }
    fn endpoint(&self) -> Result<Iap2WirelessCarPlayEndpoint> {
        Err(std::io::Error::other(
            "The test input has no CarPlay endpoint",
        ))
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        let session = self.session.lock().unwrap().take();
        if let Some(session) = session {
            session.stop();
        }
        Ok(())
    }
}

/// Touch reports go to the "iPhone": logged per press and release, and counted.
struct ReplayControl {
    events: Arc<JsonEvents>,
    config: Arc<Config>,
    pressed: Mutex<u64>,
}
impl SessionControl for ReplayControl {
    fn send_touch(&self, contacts: &[AirPlayContact]) -> Result<bool> {
        let counters = &self.config.counters;
        counters.touch_events.fetch_add(1, Ordering::Relaxed);
        let mut pressed = self.pressed.lock().unwrap();
        for c in contacts {
            let bit = 1u64 << (c.id % 64);
            if c.down == (*pressed & bit != 0) {
                continue;
            }
            if c.down {
                *pressed |= bit;
                counters.presses.fetch_add(1, Ordering::Relaxed);
            } else {
                *pressed &= !bit;
            }
            self.events.log(&format!(
                "Replay touch {} contact={} x={:.3} y={:.3}",
                if c.down { "down" } else { "up" },
                c.id,
                c.x,
                c.y
            ));
        }
        Ok(true)
    }
}

#[derive(Default)]
struct Signals {
    stop: bool,
    requests: u32,
}
type Wake = Arc<(Mutex<Signals>, Condvar)>;
/// One simulated CarPlay session: the screen stream, its key-frame answers and the context script.
struct ReplaySession {
    signals: Wake,
    bridge: Arc<StreamBridgeSink>,
    input: Arc<DiPlayInput>,
    workers: Vec<std::thread::JoinHandle<()>>,
}
impl ReplaySession {
    fn start(plan: &Plan, callbacks: &NetworkCallbacks) -> Result<Self> {
        let config = plan.config.clone();
        let clip = config
            .library
            .select(plan.codec, plan.fps)
            .ok_or_else(|| std::io::Error::other("No replay clip"))?;
        let events = plan.events.clone();
        if clip.codec != plan.codec {
            events.log(&format!(
                "Replay has no {} clip; the session plays {} {}",
                codec_name(plan.codec),
                codec_name(clip.codec),
                clip.name
            ));
        }
        let rule = config.keyframes.rule(clip.codec);
        let control = Arc::new(ReplayControl {
            events: events.clone(),
            config: config.clone(),
            pressed: Mutex::new(0),
        });
        let bridge = Arc::new(
            StreamBridgeSink::new(
                plan.input.clone(),
                plan.sink.clone(),
                control,
                events.clone(),
            )
            .with_arrival_clock(Arc::new(now_ns)),
        );
        {
            let (events, config) = (events.clone(), config.clone());
            plan.input.state.lock().unwrap().keys = Some(Arc::new(move |code, down| {
                config.counters.keys.fetch_add(1, Ordering::Relaxed);
                events.log(&format!(
                    "Replay key code={code} {}",
                    if down { "down" } else { "up" }
                ));
                true
            }));
        }
        let signals: Wake = Arc::default();
        let wake = signals.clone();
        // The screen stream opens like the media engine's: recovery handler, then active.
        bridge.set_video_recovery_handler(
            SCREEN,
            Arc::new(move || {
                wake.0.lock().unwrap().requests += 1;
                wake.1.notify_all();
            }),
        );
        bridge.on_screen_stream_active(SCREEN, true);
        plan.input.session_context(true);
        events.emit(
            "airplay-session-active",
            &serde_json::json!({"testInput":"replay"}),
        );
        events.emit(
            "replay-session",
            &serde_json::json!({"clip":clip.name,"codec":codec_name(clip.codec),
                "requestedCodec":codec_name(plan.codec),"fps":plan.fps,"keyframes":rule.to_string(),
                "clipPeakFps":(clip.peak_fps * 10.0).round() / 10.0}),
        );
        (callbacks.on_air_play)();
        (callbacks.on_tunnel_ready)();
        let player = Player::new(clip, rule, plan.fps, now_ns());
        let mut session = Self {
            signals,
            bridge,
            input: plan.input.clone(),
            workers: Vec::new(),
        };
        if let Err(e) = session.spawn(plan, player, callbacks) {
            session.stop();
            return Err(e);
        }
        Ok(session)
    }
    fn spawn(&mut self, plan: &Plan, player: Player, callbacks: &NetworkCallbacks) -> Result<()> {
        let (bridge, signals, events, config, failure) = (
            self.bridge.clone(),
            self.signals.clone(),
            plan.events.clone(),
            plan.config.clone(),
            callbacks.on_failure.clone(),
        );
        self.workers.push(crate::platform::spawn_worker(
            "replay-video",
            Box::new(move || play(player, &bridge, &signals, &events, &config, &failure)),
        )?);
        match &plan.config.script {
            Some(script) if plan.input.plugins_enabled() => {
                let (script, input, events, signals) = (
                    script.clone(),
                    plan.input.clone(),
                    plan.events.clone(),
                    self.signals.clone(),
                );
                self.workers.push(crate::platform::spawn_worker(
                    "replay-context",
                    Box::new(move || {
                        script.run(&input, &events, &|delay| {
                            let (lock, wake) = &*signals;
                            let signals = lock.lock().unwrap();
                            wake.wait_timeout_while(signals, delay, |s| !s.stop)
                                .unwrap()
                                .0
                                .stop
                        })
                    }),
                )?);
            }
            Some(script) => plan.events.log(&format!(
                "Replay context script {} ignored: the plugins setting is off",
                script.name
            )),
            None => {}
        }
        Ok(())
    }
    /// Ends the session like the media engine closing the screen stream.
    fn stop(self) {
        self.signals.0.lock().unwrap().stop = true;
        self.signals.1.notify_all();
        for worker in self.workers {
            let _ = worker.join();
        }
        self.bridge.on_screen_stream_active(SCREEN, false);
        self.input.session_context(false);
    }
}
/// The screen stream: delivers the player's output in real time and its key-frame answers.
fn play(
    mut player: Player,
    bridge: &StreamBridgeSink,
    signals: &Wake,
    events: &JsonEvents,
    config: &Config,
    on_failure: &Arc<dyn Fn(std::io::Error) + Send + Sync>,
) {
    let codec = player.clip().codec;
    let stats = |player: &Player, last: bool| {
        let mut v = serde_json::to_value(player.stats()).unwrap_or_default();
        v["clip"] = player.clip().name.clone().into();
        v["codec"] = codec_name(codec).into();
        v["final"] = last.into();
        if let (Some(o), Ok(serde_json::Value::Object(c))) = (
            v.as_object_mut(),
            serde_json::to_value(config.counters.snapshot()),
        ) {
            o.extend(c);
        }
        events.emit("replay-stats", &v);
    };
    let result = (|| -> Result<()> {
        bridge.on_video_codec(SCREEN, codec)?;
        let mut next_stats = now_ns() + STATS_INTERVAL_NS;
        loop {
            let requests = {
                let (lock, wake) = &**signals;
                let mut s = lock.lock().unwrap();
                loop {
                    if s.stop {
                        return Ok(());
                    }
                    if s.requests > 0 {
                        break;
                    }
                    let (now, due) = (now_ns(), player.next_wake().min(next_stats));
                    if due <= now {
                        break;
                    }
                    s = wake
                        .wait_timeout(s, Duration::from_nanos(due - now))
                        .unwrap()
                        .0;
                }
                std::mem::take(&mut s.requests)
            };
            let now = now_ns();
            for _ in 0..requests {
                match player.request_keyframe(now) {
                    Request::Ignored => events.log(&format!(
                        "Replay ignored a key-frame request ({})",
                        player.rule()
                    )),
                    Request::Scheduled(at) => events.log(&format!(
                        "Replay answers a key-frame request in {} ms{}",
                        (at - now) / 1_000_000,
                        if player.rule().stream_restart {
                            " with a stream restart"
                        } else {
                            ""
                        }
                    )),
                    Request::Pending(_) => {}
                }
            }
            for output in player.poll(now)? {
                let delivered = match output {
                    Output::Config(parameter_sets) => {
                        let record = clip::decoder_config(codec, &parameter_sets)?;
                        bridge.on_video_config(SCREEN, &record)
                    }
                    Output::Frame { data, due_ns, .. } => {
                        bridge.on_video_frame(SCREEN, &data, due_ns, now_ns())
                    }
                };
                if let Err(e) = delivered {
                    events.log(&format!("Replay delivery failed: {e}"));
                }
            }
            if now >= next_stats {
                stats(&player, false);
                next_stats = now + STATS_INTERVAL_NS;
            }
        }
    })();
    stats(&player, true);
    if let Err(e) = result {
        events.log(&format!("Replay failed: {e}"));
        on_failure(e);
    }
}
