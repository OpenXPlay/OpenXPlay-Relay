// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use crate::*;
use carplay_airplay::*;
use oxp_core::{model::VideoCodec, video_codec_policy::VideoCodecDecision};
use oxp_wire::Message;
use std::{
    collections::BTreeMap,
    sync::{
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
/// Re-send interval while a screen-0 key frame is outstanding; DiPlay also asks once a second,
/// and the iPhone often ignores a single request in H.265.
const KEYFRAME_REPEAT: Duration = Duration::from_secs(1);
/// An H.265 session that answers none of the requests within this time falls back to H.264.
const HEVC_KEYFRAME_TIMEOUT: Duration = Duration::from_secs(8);
/// How often the recovery worker checks for a due repeat.
const KEYFRAME_POLL: Duration = Duration::from_millis(100);
/// A receiver's frame-rate restart waits at least this long after the previous one.
const RATE_RESTART_INTERVAL: Duration = Duration::from_secs(10);
type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;
/// One gap without a screen-0 key frame, from the first request until a key frame is published.
struct KeyframeWatch {
    session: u64,
    screen: u16,
    /// The H.265 fallback deadline counts from the first request.
    requested: Instant,
    last_request: Instant,
    /// Requests handed to the CarPlay session in this gap.
    forwarded: u32,
    /// The request outcome is logged once per gap; repeats are summarized when it ends.
    logged: bool,
}
impl KeyframeWatch {
    fn summary(&self, now: Instant, outcome: &str) -> Option<String> {
        (self.forwarded > 1).then(|| {
            format!(
                "Keyframe {outcome} for screen {} after {} requests in {} ms",
                self.screen,
                self.forwarded,
                now.saturating_duration_since(self.requested).as_millis()
            )
        })
    }
}
/// A codec write prepared under the input lock and performed after releasing it.
struct CodecWrite {
    path: std::path::PathBuf,
    codec: VideoCodec,
    sequence: u64,
}
pub(crate) struct InputState {
    pub generation: u64,
    pub current: Option<u64>,
    pub latest: Option<u64>,
    pub control: Option<Arc<dyn SessionControl>>,
    pub recovery: BTreeMap<u16, Arc<dyn Fn() + Send + Sync>>,
    /// Receives key events for the current session when its source handles keys.
    pub keys: Option<Arc<dyn Fn(u16, bool) -> bool + Send + Sync>>,
    pub microphones: BTreeMap<u16, Arc<dyn MicrophoneUplink>>,
    pub declarations: BTreeMap<String, Message>,
    pub output: Option<SyncSender<Message>>,
    pub active: bool,
    pub desired_fps: u32,
    pub negotiated_fps: Option<f64>,
    pub frame_rate_setting: oxp_core::model::FrameRatePreference,
    pub playback: Option<PlaybackToken>,
    pub carlink_frames: u64,
    pub applied_fps: u32,
    /// A receiver asked to apply `desired_fps`; it stays pending through the rate limit.
    pub restart_requested: bool,
    pub frame_rate_changed: bool,
    pub last_restart: Option<Instant>,
    pub desired_video: VideoCodecDecision,
    pub applied_video: VideoCodecDecision,
    pub codec_restarts: CodecRestarts,
    pub codec_restart_pending: bool,
    keyframe_watch: Option<KeyframeWatch>,
    hevc_failed: bool,
    hevc_early_drops: u32,
    first_hevc_format: Option<Instant>,
    hevc_failure: Option<String>,
    requested_end: bool,
    video_frames: u64,
    video_bytes: u64,
    count_video_bytes: bool,
    codec_state_path: Option<std::path::PathBuf>,
    codec_writes: u64,
    receiver_decided: bool,
    auto_video: VideoCodecDecision,
    codec_setting: String,
    /// A controller has applied `applied_video`.
    video_configured: bool,
}
#[derive(Default)]
pub(crate) struct CodecRestarts {
    last_restart: Option<Instant>,
    last_change: Option<Instant>,
    count: u8,
}
impl CodecRestarts {
    /// Re-arms after two minutes without a codec change or restart, also when nothing changes.
    fn decay(&mut self, now: Instant) {
        if self
            .last_change
            .max(self.last_restart)
            .is_some_and(|t| now.saturating_duration_since(t).as_secs() >= 120)
        {
            self.count = 0;
        }
    }
    fn changed(&mut self, now: Instant) {
        self.decay(now);
        self.last_change = Some(now);
    }
    fn allow(&mut self, now: Instant) -> bool {
        self.decay(now);
        if self.count >= 2
            || self
                .last_restart
                .is_some_and(|t| now.saturating_duration_since(t).as_secs() < 30)
        {
            return false;
        }
        self.record(now);
        true
    }
    fn record(&mut self, now: Instant) {
        self.count = self.count.saturating_add(1);
        self.last_restart = Some(now);
    }
}
enum InputCommand {
    Recovery(u64, u16),
    Touch(u64, Vec<AirPlayContact>),
}
pub struct DiPlayInput {
    pub(crate) display: Arc<Mutex<crate::display::DisplayRuntime>>,
    pub(crate) context: Mutex<Option<crate::context::Producer>>,
    pub(crate) state: Arc<Mutex<InputState>>,
    recovery: SyncSender<InputCommand>,
    events: Arc<JsonEvents>,
    clock: Clock,
    /// Last written sequence and codec; serializes writers and drops stale ones.
    codec_written: Mutex<(u64, Option<VideoCodec>)>,
}
impl DiPlayInput {
    pub fn new(events: Arc<JsonEvents>) -> Self {
        Self::with_clock(events, Arc::new(Instant::now))
    }
    pub fn with_plugins(events: Arc<JsonEvents>, plugins: bool) -> Self {
        Self::build(events, Arc::new(Instant::now), plugins)
    }
    pub fn with_clock(events: Arc<JsonEvents>, clock: Clock) -> Self {
        Self::build(events, clock, false)
    }
    fn build(events: Arc<JsonEvents>, clock: Clock, plugins: bool) -> Self {
        let state = Arc::new(Mutex::new(InputState {
            generation: 0,
            current: None,
            latest: None,
            control: None,
            recovery: BTreeMap::new(),
            keys: None,
            microphones: BTreeMap::new(),
            declarations: BTreeMap::from([
                (
                    "display-settings".into(),
                    Message::Json(oxp_wire::display_settings(
                        &crate::display::DisplayState::default().settings(),
                    )),
                ),
                (
                    "display-view-area".into(),
                    Message::Json(serde_json::json!({"type":"view-area","screen":0,"area":null})),
                ),
            ]),
            output: None,
            active: false,
            desired_fps: 60,
            negotiated_fps: None,
            frame_rate_setting: Default::default(),
            playback: None,
            carlink_frames: 0,
            applied_fps: 60,
            restart_requested: false,
            frame_rate_changed: false,
            last_restart: None,
            desired_video: Default::default(),
            applied_video: Default::default(),
            codec_restarts: Default::default(),
            codec_restart_pending: false,
            keyframe_watch: None,
            hevc_failed: false,
            hevc_early_drops: 0,
            first_hevc_format: None,
            hevc_failure: None,
            requested_end: false,
            video_frames: 0,
            video_bytes: 0,
            count_video_bytes: false,
            codec_state_path: None,
            codec_writes: 0,
            receiver_decided: false,
            auto_video: Default::default(),
            codec_setting: "auto".into(),
            video_configured: false,
        }));
        let startup_state = Arc::downgrade(&state);
        events.startup_listener(Arc::new(move |reason| {
            if let Some(state)=startup_state.upgrade() {
                let mut s=state.lock().unwrap();
                if s.active && !matches!(reason,"failed"|"disconnected") { return; }
                let id=s.current.unwrap_or(s.generation+1);
                Self::publish_locked(&mut s,"0-session",Message::Json(serde_json::json!({"type":"session","session":{"id":id.to_string(),"phase":"CONNECTING","reason":format!("startup/{reason}")}})));
            }
        }));
        let display = Arc::new(Mutex::new(crate::display::DisplayRuntime::default()));
        let worker_display = display.clone();
        let (tx, rx) = sync_channel::<InputCommand>(16);
        let (s, e, c) = (state.clone(), events.clone(), clock.clone());
        let _ = crate::platform::spawn_worker(
            "direlay-recovery",
            Box::new(move || Self::run_recovery(&s, &worker_display, &e, &c, rx)),
        );
        Self {
            display,
            state,
            recovery: tx,
            events,
            clock,
            codec_written: Mutex::new((0, None)),
            context: Mutex::new(plugins.then(crate::context::Producer::default)),
        }
    }
    /// Delivers recovery requests and touches, and repeats an outstanding key-frame request.
    fn run_recovery(
        state: &Mutex<InputState>,
        display: &Mutex<crate::display::DisplayRuntime>,
        events: &JsonEvents,
        clock: &Clock,
        rx: Receiver<InputCommand>,
    ) {
        loop {
            match rx.recv_timeout(KEYFRAME_POLL) {
                Ok(InputCommand::Recovery(id, screen)) => {
                    let (handler, log) = {
                        let mut s = state.lock().unwrap();
                        let handler = if s.current == Some(id) {
                            s.recovery.get(&screen).cloned()
                        } else {
                            None
                        };
                        let log = match &mut s.keyframe_watch {
                            Some(w) if w.session == id && w.screen == screen => {
                                w.forwarded += handler.is_some() as u32;
                                !std::mem::replace(&mut w.logged, true)
                            }
                            _ => true,
                        };
                        (handler, log)
                    };
                    let forwarded = handler.is_some();
                    if let Some(h) = handler {
                        h();
                    }
                    if log {
                        events.log(&format!(
                            "Keyframe requested for screen {screen} forwarded={forwarded}"
                        ));
                    }
                }
                Ok(InputCommand::Touch(id, contacts)) => {
                    let control = {
                        let s = state.lock().unwrap();
                        if s.current == Some(id) {
                            s.control.clone()
                        } else {
                            None
                        }
                    };
                    if let Some(c) = control {
                        let _ = c.send_touch(&contacts);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(h) = Self::due_keyframe_repeat(state, clock()) {
                        h();
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
            Self::poll_view_area_state(display, state, clock());
        }
    }
    /// The handler to call again when a screen-0 key frame is still outstanding after
    /// `KEYFRAME_REPEAT`, for H.264 and H.265 alike.
    fn due_keyframe_repeat(
        state: &Mutex<InputState>,
        now: Instant,
    ) -> Option<Arc<dyn Fn() + Send + Sync>> {
        let mut s = state.lock().unwrap();
        let s = &mut *s;
        let watch = s.keyframe_watch.as_mut()?;
        if now.saturating_duration_since(watch.last_request) < KEYFRAME_REPEAT {
            return None;
        }
        watch.last_request = now;
        let handler = if s.current == Some(watch.session) {
            s.recovery.get(&watch.screen).cloned()
        } else {
            None
        };
        watch.forwarded += handler.is_some() as u32;
        handler
    }
    /// Load once per process, before the first controller snapshots its codec.
    pub fn initialize_video_state(&self, state_dir: &std::path::Path) {
        self.initialize_display(state_dir);
        let setting = self.display_settings().video_codec;
        let mut s = self.state.lock().unwrap();
        if s.codec_state_path.is_some() {
            return;
        }
        let path = state_dir.join("video-codec.json");
        if !s.receiver_decided && !s.hevc_failed {
            let saved = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
            let codec = saved.as_ref().and_then(|v| match v["codec"].as_str() {
                Some("H264") => Some(VideoCodec::H264),
                Some("H265") => Some(VideoCodec::H265),
                _ => None,
            });
            if let Some(codec) = codec {
                s.desired_video = VideoCodecDecision {
                    codec,
                    reason: "remembered-codec",
                };
            }
        }
        s.auto_video = s.desired_video;
        s.codec_setting = setting;
        s.desired_video = Self::setting_decision(&s.codec_setting, s.auto_video);
        s.codec_state_path = Some(path);
    }
    fn stage_codec_write(s: &mut InputState, codec: VideoCodec) -> Option<CodecWrite> {
        if s.codec_setting != "auto" {
            return None;
        }
        let path = s.codec_state_path.clone()?;
        s.codec_writes += 1;
        Some(CodecWrite {
            path,
            codec,
            sequence: s.codec_writes,
        })
    }
    /// Runs without the input lock: the fsync must not stall touch and video delivery.
    fn persist_codec(&self, write: Option<CodecWrite>) {
        let Some(write) = write else {
            return;
        };
        let mut written = self.codec_written.lock().unwrap();
        if write.sequence <= written.0 {
            return;
        }
        written.0 = write.sequence;
        if written.1 == Some(write.codec) {
            return;
        }
        let result = (|| -> Result<()> {
            std::fs::create_dir_all(write.path.parent().unwrap())?;
            let temporary = write.path.with_extension("json.tmp");
            let mut file = std::fs::File::create(&temporary)?;
            use std::io::Write;
            file.write_all(
                serde_json::json!({"codec":format!("{:?}", write.codec)})
                    .to_string()
                    .as_bytes(),
            )?;
            file.sync_all()?;
            std::fs::rename(temporary, &write.path)?;
            Ok(())
        })();
        match result {
            Ok(()) => written.1 = Some(write.codec),
            Err(e) => self.events.log(&format!("Cannot persist video codec: {e}")),
        }
    }
    /// Falls back to H.264 when an H.265 key frame is still missing after the timeout.
    fn poll_keyframe_watchdog(&self, s: &mut InputState) -> Option<CodecWrite> {
        let watch = s.keyframe_watch.as_ref()?;
        if s.codec_setting != "auto"
            || s.applied_video.codec != VideoCodec::H265
            || s.hevc_failed
            || s.requested_end
        {
            return None;
        }
        let now = (self.clock)();
        if now.saturating_duration_since(watch.requested) < HEVC_KEYFRAME_TIMEOUT {
            return None;
        }
        let forwarded = watch.forwarded;
        let wait_ms = now.saturating_duration_since(watch.requested).as_millis();
        s.keyframe_watch = None;
        s.hevc_failed = true;
        s.hevc_failure = Some(format!("keyframe-timeout waitMs={wait_ms}"));
        s.desired_video = VideoCodecDecision {
            codec: VideoCodec::H264,
            reason: "hevc-keyframe-timeout",
        };
        s.codec_restarts.changed(now);
        self.events.log(&format!(
            "video codec H264 reason=hevc-keyframe-timeout after {forwarded} keyframe requests; HEVC disabled until process restart"
        ));
        Self::stage_codec_write(s, VideoCodec::H264)
    }
    pub fn start(&self, _sink: Arc<dyn MediaSink>) -> Result<()> {
        Ok(())
    }
    pub fn stop(&self) -> Result<()> {
        self.expect_stop();
        self.session_context(false);
        let id = self.state.lock().unwrap().current;
        if let Some(id) = id {
            self.end(id)?;
        }
        let mut s = self.state.lock().unwrap();
        s.control = None;
        s.keys = None;
        s.latest = None;
        s.recovery.clear();
        Ok(())
    }
    pub fn begin(&self, control: Arc<dyn SessionControl>) -> Result<u64> {
        self.expect_stop();
        let current = self.state.lock().unwrap().current;
        if let Some(id) = current {
            self.end(id)?;
        }
        let mut s = self.state.lock().unwrap();
        s.generation += 1;
        let id = s.generation;
        s.current = Some(id);
        s.latest = Some(id);
        s.control = Some(control);
        let media = s.control.clone().unwrap();
        s.keys = Some(Arc::new(
            move |key, down| match crate::carplay_policy::media_button(key) {
                Some(index) => !down || media.send_media_button(index).unwrap_or(false),
                None => false,
            },
        ));
        s.active = false;
        Self::reset_video_session(&mut s);
        s.playback = None;
        s.recovery.clear();
        s.declarations.retain(|key, _| key.starts_with("display-"));
        Self::publish_locked(
            &mut s,
            "0-session",
            Message::Json(
                serde_json::json!({"type":"session","session":{"id":id.to_string(),"phase":"CONNECTING","reason":"startup/waiting-media"}}),
            ),
        );
        drop(s);
        self.events
            .milestone(crate::boot_timing::Milestone::SessionConnecting);
        Ok(id)
    }
    pub(crate) fn resume(&self, id: u64) -> u64 {
        let mut s = self.state.lock().unwrap();
        if s.current.is_none() && s.latest == Some(id) {
            s.generation += 1;
            let next = s.generation;
            s.latest = Some(next);
            s.current = Some(next);
            s.active = false;
            Self::reset_video_session(&mut s);
            s.declarations.retain(|key, _| key.starts_with("display-"));
            Self::publish_locked(
                &mut s,
                "0-session",
                Message::Json(
                    serde_json::json!({"type":"session","session":{"id":next.to_string(),"phase":"CONNECTING","reason":"startup/waiting-media"}}),
                ),
            );
            drop(s);
            self.events
                .milestone(crate::boot_timing::Milestone::SessionConnecting);
            return next;
        }
        id
    }
    pub fn end(&self, id: u64) -> Result<()> {
        let write;
        let (microphones, summary) = {
            let mut s = self.state.lock().unwrap();
            if s.current != Some(id) {
                return Ok(());
            }
            write = self.record_video_end(&mut s);
            let keys: Vec<_> = s
                .declarations
                .keys()
                .filter(|k| k.as_str() != "0-session" && !k.starts_with("display-"))
                .cloned()
                .collect();
            for key in keys {
                let old = s.declarations.remove(&key).unwrap();
                if let Message::Json(mut v) = old {
                    if let Some(o) = v.as_object_mut() {
                        if o.contains_key("format") {
                            o.insert("format".into(), serde_json::Value::Null);
                        }
                        if o.contains_key("request") {
                            o.insert("request".into(), serde_json::Value::Null);
                        }
                    }
                    Self::publish_locked(&mut s, "", Message::Json(v));
                }
            }
            Self::publish_locked(
                &mut s,
                "0-session",
                Message::Json(
                    serde_json::json!({"type":"session","session":{"id":id.to_string(),"phase":"ENDED"}}),
                ),
            );
            s.current = None;
            s.active = false;
            s.playback = None;
            let summary = s
                .keyframe_watch
                .take()
                .and_then(|w| w.summary((self.clock)(), "still missing at session end"));
            (std::mem::take(&mut s.microphones), summary)
        };
        self.persist_codec(write);
        {
            let mut f = self.events.facts.lock().unwrap();
            f.picture_on_car = false;
            f.session = None;
            f.iphone.session_since_millis = None;
        }
        if let Some(line) = summary {
            self.events.log(&line);
        }
        for m in microphones.values() {
            let _ = m.close();
        }
        Ok(())
    }
    pub(crate) fn publish_locked(s: &mut InputState, key: &str, m: Message) {
        if !key.is_empty() {
            s.declarations.insert(key.into(), m.clone());
        }
        if let Some(tx) = &s.output {
            if tx.try_send(m).is_err() {
                s.output = None;
            }
        }
    }
    pub(crate) fn publish(&self, id: u64, key: &str, m: Message) {
        let mut summary = None;
        {
            let mut s = self.state.lock().unwrap();
            if s.current != Some(id) {
                return;
            }
            if matches!(&m, Message::Json(v) if v["type"] == "video-format" && v["screen"] == 0 && v["format"]["codec"] == "H265")
                && s.first_hevc_format.is_none()
            {
                s.first_hevc_format = Some((self.clock)());
            }
            if let Message::Binary { header, payload } = &m {
                if s.active && header.kind == oxp_wire::Kind::Video && header.stream == 0 {
                    s.video_frames = s.video_frames.saturating_add(1);
                    if s.count_video_bytes {
                        s.video_bytes = s.video_bytes.saturating_add(payload.len() as u64);
                    }
                }
            }
            if matches!(&m, Message::Binary { header, .. }
                if header.kind == oxp_wire::Kind::Video && header.stream == 0
                    && header.flags & oxp_wire::KEYFRAME != 0)
            {
                if let Some(w) = s.keyframe_watch.take() {
                    summary = w.summary((self.clock)(), "arrived");
                }
            }
            Self::publish_locked(&mut s, key, m);
        }
        if let Some(line) = summary {
            self.events.log(&line);
        }
    }
    fn log_video_codec(&self, decision: VideoCodecDecision) {
        self.events.log(&format!(
            "video codec {:?} reason={}",
            decision.codec, decision.reason
        ));
    }
    /// Consume pending changes once, before the controller worker can race tick.
    pub fn configure_session(&self) -> (u32, bool) {
        self.freeze_display();
        let (fps, video, write) = {
            let mut s = self.state.lock().unwrap();
            s.restart_requested = false;
            s.frame_rate_changed = false;
            s.applied_fps = s.desired_fps;
            s.applied_video = s.desired_video;
            s.video_configured = true;
            s.codec_restart_pending = false;
            s.keyframe_watch = None;
            let codec = s.applied_video.codec;
            let write = Self::stage_codec_write(&mut s, codec);
            (s.applied_fps, s.applied_video, write)
        };
        self.persist_codec(write);
        self.log_video_codec(video);
        (fps, video.codec == VideoCodec::H265)
    }
    /// Snapshot the negotiated rate when constructing the next CarPlay controller.
    pub fn configure_frame_rate(&self) -> u32 {
        let mut s = self.state.lock().unwrap();
        s.applied_fps = s.desired_fps;
        s.applied_fps
    }
    pub fn configure_video_codec(&self) -> bool {
        let (video, write) = {
            let mut s = self.state.lock().unwrap();
            s.applied_video = s.desired_video;
            s.video_configured = true;
            s.codec_restart_pending = false;
            s.keyframe_watch = None;
            let codec = s.applied_video.codec;
            let write = Self::stage_codec_write(&mut s, codec);
            (s.applied_video, write)
        };
        self.persist_codec(write);
        self.log_video_codec(video);
        video.codec == VideoCodec::H265
    }
    /// Whether a CarPlay session is running (between `begin` and `end`).
    pub fn session_active(&self) -> bool {
        self.state.lock().unwrap().current.is_some()
    }
    /// The codec in use and why, plus the iPhone stream's codec string and size once known.
    pub fn video_status(&self) -> serde_json::Value {
        let s = self.state.lock().unwrap();
        let applied = format!("{:?}", s.applied_video.codec);
        let format = s.declarations.get("1-video-0").and_then(|m| match m {
            Message::Json(v) if v["format"].is_object() => Some(&v["format"]),
            _ => None,
        });
        let codec = format.and_then(|f| f["codec"].as_str()).unwrap_or(&applied);
        let desired = format!("{:?}", s.desired_video.codec);
        let reason = if codec == desired {
            s.desired_video.reason.to_string()
        } else {
            format!("{}; codec-change-pending", s.desired_video.reason)
        };
        let field = |key: &str| format.map_or(serde_json::Value::Null, |f| f[key].clone());
        serde_json::json!({"codec":codec,"reason":reason,"codecString":field("codecString"),
            "width":field("width"),"height":field("height"),
            "session":s.current.map(|id| id.to_string()),"active":s.current.is_some() && s.active,
            "frames":s.video_frames,"bytes":s.video_bytes,"bytesMeasured":s.count_video_bytes,
            "appliedFps":s.applied_fps,"requestedFps":s.desired_fps,"hevcFailure":s.hevc_failure})
    }
    pub fn clear_receiver_capabilities(&self) {
        self.events.facts.lock().unwrap().picture_on_car = false;
        self.display.lock().unwrap().disconnected();
        self.context_disconnected();
        self.set_video_decision(oxp_core::video_codec_policy::resolve(&[], false));
    }
    fn set_video_decision(&self, mut decision: VideoCodecDecision) {
        let mut s = self.state.lock().unwrap();
        if decision.reason != "no-main-screen-output" {
            s.receiver_decided = true;
        }
        s.auto_video = if decision.reason == "no-main-screen-output" {
            VideoCodecDecision {
                codec: s.auto_video.codec,
                ..decision
            }
        } else {
            decision
        };
        if s.codec_setting != "auto" {
            decision = Self::setting_decision(&s.codec_setting, decision);
        } else if s.hevc_failed {
            return;
        }
        // With no main-screen output nobody needs a different codec: keep the one in use (or,
        // before the first controller, the remembered one), so a browser that drops off Wi-Fi
        // for a moment neither causes a CarPlay restart nor fires a pending change.
        if decision.reason == "no-main-screen-output" {
            decision.codec = if s.video_configured {
                s.applied_video.codec
            } else {
                s.desired_video.codec
            };
        }
        if s.desired_video != decision {
            if s.desired_video.codec != decision.codec {
                s.codec_restarts.changed((self.clock)());
            }
            self.log_video_codec(decision);
            s.desired_video = decision;
        }
    }
    pub fn applied_frame_rate(&self) -> u32 {
        self.state.lock().unwrap().applied_fps
    }
    /// The relay shell owns the controller; the producer never restarts Stream.
    pub fn apply_requested_restart(&self, stop_controller: impl FnOnce()) -> bool {
        self.poll_view_area();
        let (display_change, waiting_settings) = {
            let mut d = self.display.lock().unwrap();
            let due = d.due((self.clock)());
            (due, d.waiting_settings())
        };
        let (requested, write) = {
            let mut s = self.state.lock().unwrap();
            let write = self.poll_keyframe_watchdog(&mut s);
            let now = (self.clock)();
            let codec_change = !waiting_settings
                && !s.codec_restart_pending
                && s.desired_video.codec != s.applied_video.codec
                && if display_change || s.hevc_failed {
                    // A stuck H.265 session without key frames is worse than another restart.
                    s.codec_restarts.record(now);
                    true
                } else {
                    s.codec_restarts.allow(now)
                };
            if codec_change {
                s.codec_restart_pending = true;
            }
            if s.desired_fps == s.applied_fps {
                s.restart_requested = false;
            }
            let rate_change = !waiting_settings
                && s.restart_requested
                && (s.frame_rate_changed
                    || s.last_restart.is_none_or(|at| {
                        now.saturating_duration_since(at) >= RATE_RESTART_INTERVAL
                    }));
            if rate_change {
                s.restart_requested = false;
                s.last_restart = Some(now);
                s.frame_rate_changed = false;
            }
            (display_change || rate_change || codec_change, write)
        };
        self.persist_codec(write);
        if requested {
            self.expect_stop();
            stop_controller();
        }
        requested
    }
    pub fn control(&self, m: &Message) -> Result<bool> {
        match m {
            Message::Json(v) if v["type"] == "hello" && self.plugins_enabled() => {
                self.context_peer(v);
                Ok(true)
            }
            Message::Json(v) if v["type"] == "stats" => Ok(self.receiver_playback(v)),
            Message::Json(v) if v["type"] == "display-settings" => {
                let mut v = v.clone();
                if v.get("frameRate").is_none() {
                    v["frameRate"] = self.display_settings().frame_rate.wire_name().into();
                }
                let settings =
                    oxp_wire::decode_display_settings(&v).map_err(std::io::Error::other)?;
                self.update_display_settings(settings)?;
                Ok(true)
            }
            Message::Json(v) if v["type"] == "capabilities" => {
                oxp_wire::validate_json(v).map_err(std::io::Error::other)?;
                self.observe_display(v)?;
                let display = &v["displays"]["0"];
                let preference = display["frameRate"]
                    .as_str()
                    .and_then(oxp_core::model::FrameRatePreference::from_wire)
                    .unwrap_or_default();
                let fps = oxp_core::model::FrameRatePolicy::resolve(
                    preference,
                    display["refreshHz"].as_f64(),
                    display["sustainedFps"].as_f64(),
                ) as u32;
                {
                    let mut s = self.state.lock().unwrap();
                    s.negotiated_fps = Some(fps as f64);
                    s.desired_fps = crate::carplay_policy::requested_frame_rate(
                        s.frame_rate_setting,
                        s.negotiated_fps,
                    );
                }
                let hardware = v["decoders"].as_array().is_some_and(|ds| {
                    ds.iter().any(|d| {
                        d["hardware"] == true
                            && d["codecString"]
                                .as_str()
                                .is_some_and(oxp_core::video_codec_policy::is_hevc)
                    })
                });
                let outputs = if display.is_object() {
                    vec![hardware]
                } else {
                    vec![]
                };
                self.set_video_decision(oxp_core::video_codec_policy::resolve_with_preferences(
                    &outputs,
                    false,
                    v["prefersH264"].as_bool().unwrap_or(false),
                ));
                Ok(true)
            }
            Message::Json(v) if v["type"] == "restart" => {
                oxp_wire::validate_json(v).map_err(std::io::Error::other)?;
                let mut s = self.state.lock().unwrap();
                // `apply_requested_restart` applies it once the rate limit allows.
                if s.desired_fps != s.applied_fps {
                    s.restart_requested = true;
                }
                Ok(true)
            }
            Message::Json(v) if v["type"] == "keyframe" => {
                self.request_keyframe(v["screen"].as_u64().unwrap_or(255) as u16)
            }
            Message::Binary { header, payload } => match header.kind {
                oxp_wire::Kind::Touch => {
                    let c = oxp_wire::decode_touch(payload).map_err(std::io::Error::other)?;
                    self.touch(
                        header.stream.into(),
                        &c.into_iter()
                            .map(|c| AirPlayContact {
                                id: c.id,
                                x: c.x,
                                y: c.y,
                                down: c.pressed,
                            })
                            .collect::<Vec<_>>(),
                    )
                }
                oxp_wire::Kind::Microphone => self.microphone_packet(header.stream.into(), payload),
                oxp_wire::Kind::Key => {
                    let (code, down) =
                        oxp_wire::decode_key(payload).map_err(std::io::Error::other)?;
                    self.key(code, down)
                }
                _ => Ok(false),
            },
            _ => Ok(false),
        }
    }
    pub fn close(&self) -> Result<()> {
        self.stop()
    }
}
impl DiPlayInput {
    fn record_video_end(&self, s: &mut InputState) -> Option<CodecWrite> {
        let first = s.first_hevc_format.take()?;
        let now = (self.clock)();
        let elapsed = now.checked_duration_since(first)?;
        if s.codec_setting != "auto"
            || s.applied_video.codec != VideoCodec::H265
            || s.requested_end
            || elapsed >= Duration::from_secs(30)
        {
            return None;
        }
        s.hevc_early_drops = s.hevc_early_drops.saturating_add(1);
        if s.hevc_early_drops < 2 || s.hevc_failed {
            return None;
        }
        s.hevc_failed = true;
        let failure = format!(
            "early-drop waitMs={} count={}",
            elapsed.as_millis(),
            s.hevc_early_drops
        );
        self.events.log(&format!(
            "video codec H264 reason=hevc-failed {failure}; HEVC disabled until process restart"
        ));
        s.hevc_failure = Some(failure);
        s.desired_video = Self::hevc_fallback(s);
        s.codec_restarts.changed(now);
        Self::stage_codec_write(s, VideoCodec::H264)
    }
    /// The shell must call this before closing a controller, so its EOF is not a codec failure.
    pub fn expect_stop(&self) {
        self.state.lock().unwrap().requested_end = true;
    }
    fn reset_video_session(s: &mut InputState) {
        s.first_hevc_format = None;
        s.requested_end = false;
        s.video_frames = 0;
        s.video_bytes = 0;
    }
    fn hevc_fallback(s: &InputState) -> VideoCodecDecision {
        VideoCodecDecision {
            codec: VideoCodec::H264,
            reason: if s
                .hevc_failure
                .as_ref()
                .is_some_and(|failure| failure.starts_with("early-drop "))
            {
                "hevc-failed"
            } else {
                "hevc-keyframe-timeout"
            },
        }
    }
    pub fn host_ui_requested(&self) {
        let mut s = self.state.lock().unwrap();
        if s.current.is_some() {
            Self::publish_locked(
                &mut s,
                "",
                Message::Json(serde_json::json!({"type":"host-ui"})),
            );
        }
    }
    pub fn configure_runtime_settings(&self, settings: &crate::settings::Settings) -> Result<()> {
        self.state.lock().unwrap().count_video_bytes = settings.video_stats != "off";
        let mut d = self.display.lock().unwrap();
        if d.saved.select_car(settings.car().key()) {
            d.pictures.automatic_canvas = None;
            d.disconnected();
            d.persist()?;
        }
        d.configure(settings);
        d.fallback = settings.car().built_in();
        if d.picture_default != settings.carlink_picture_default
            && (d.pictures.trial.is_some() || self.session_active())
        {
            d.pending_picture_default = true;
        }
        d.picture_default = settings.carlink_picture_default;
        d.carlink_mode = settings.carlink_picture_size == "mode";
        d.carlink_primary = settings.output == "carlink"
            || (settings.output == "both" && {
                let facts = self.events.facts.lock().unwrap();
                facts.carlink.phase == "PLAYING" || facts.carlink.screen.is_some()
            });
        d.disable_view_areas = !settings.car_play_view_areas;
        Ok(())
    }
    /// Connection-local selection. The shell applies the restart through its normal tick.
    pub fn try_carlink_picture(&self, size: Option<crate::display::Size>) -> Result<bool> {
        let mut d = self.display.lock().unwrap();
        let default = d.picture_default;
        if size.is_some() && d.pictures.automatic_canvas.is_none() {
            d.pictures.automatic_canvas = Some(d.saved.planned_canvas());
        }
        let changed = d.pictures.choose(size, default)?;
        if changed {
            d.pictures.applied = size;
            d.settings_since = Some((self.clock)());
        }
        Ok(changed)
    }
    pub fn accept_picture_default(&self) {
        let mut d = self.display.lock().unwrap();
        if std::mem::take(&mut d.pending_picture_default) {
            d.pictures.trial = None;
        }
    }
    pub fn clear_picture_trial(&self) {
        let mut d = self.display.lock().unwrap();
        if d.saved.head_unit.is_none() {
            if let Some(canvas) = d.pictures.automatic_canvas {
                d.saved.canvas = canvas;
            }
        }
        d.pictures.stopped();
    }
    pub fn screen_known(&self) -> bool {
        let d = self.display.lock().unwrap();
        d.head().is_some()
    }
    pub fn screen_reported(&self) -> bool {
        self.display.lock().unwrap().connected
    }
    /// A receiver report only proves a frame from the current stream, captured before delivery.
    pub fn receiver_playback(&self, event: &serde_json::Value) -> bool {
        let (id, control, token) = {
            let mut s = self.state.lock().unwrap();
            let Some(id) = s.current else {
                return false;
            };
            let proof = if event["type"] == "stats" {
                let session = event["session"]
                    .as_u64()
                    .or_else(|| event["session"].as_str().and_then(|s| s.parse().ok()));
                event["screen"].as_u64().unwrap_or(0) == 0 && session == Some(id)
            } else if event["type"] == "media-stats" {
                let n = event["liveFrames"].as_u64().unwrap_or(0);
                let grew = n > s.carlink_frames;
                s.carlink_frames = n;
                grew
            } else {
                false
            };
            if !proof {
                return false;
            }
            (id, s.control.clone(), s.playback.clone())
        };
        let accepted = control
            .zip(token)
            .is_some_and(|(c, t)| c.report_video_frame_rendered(&t));
        if accepted && self.state.lock().unwrap().current == Some(id) {
            let mut f = self.events.facts.lock().unwrap();
            f.picture_on_car = true;
        }
        accepted
    }
    pub fn runtime_path_facts(&self) -> crate::path_facts::RuntimePathFacts {
        let mut facts = self.events.runtime_path_facts();
        let d = self.display.lock().unwrap();
        let s = self.state.lock().unwrap();
        facts.session = s.current;
        facts.saved_screen = d.saved.head_unit.as_ref().map(|h| crate::display::Size {
            width: h.screen.width,
            height: h.screen.height,
        });
        facts.screen = d
            .live_head
            .as_ref()
            .or(d.saved.head_unit.as_ref())
            .filter(|_| d.connected)
            .map(|h| crate::display::Size {
                width: h.screen.width,
                height: h.screen.height,
            });
        facts.built_in_screen = d.fallback.as_ref().map(|(h, _)| crate::display::Size {
            width: h.screen.width,
            height: h.screen.height,
        });
        facts.picture_on_car &= s.current.is_some();
        if let Some(Message::Json(v)) = s.declarations.get("1-video-0") {
            let f = &v["format"];
            if let Some((w, h)) = f["width"].as_u64().zip(f["height"].as_u64()) {
                facts.chips = vec![
                    format!("{w}×{h}"),
                    format!("{} fps", s.applied_fps),
                    if f["codec"] == "H265" {
                        "H.265"
                    } else {
                        "H.264"
                    }
                    .into(),
                ];
            }
        }
        facts
    }
    fn setting_decision(setting: &str, automatic: VideoCodecDecision) -> VideoCodecDecision {
        match setting {
            "h264" => VideoCodecDecision {
                codec: VideoCodec::H264,
                reason: "setting-h264",
            },
            "h265" => VideoCodecDecision {
                codec: VideoCodec::H265,
                reason: "setting-h265",
            },
            _ => automatic,
        }
    }
    fn initialize_display(&self, state_dir: &std::path::Path) {
        let mut d = self.display.lock().unwrap();
        if d.path.is_none() {
            let path = state_dir.join("display.json");
            d.saved = crate::display::DisplayState::load(&path);
            d.path = Some(path);
            if let Ok(settings) = crate::settings::Settings::load(state_dir) {
                d.saved.frame_rate =
                    crate::carplay_policy::frame_rate_setting(&settings.frame_rate)
                        .unwrap_or_default();
                let mut s = self.state.lock().unwrap();
                s.frame_rate_setting = d.saved.frame_rate;
                s.desired_fps = crate::carplay_policy::requested_frame_rate(
                    s.frame_rate_setting,
                    s.negotiated_fps,
                );
            }
        }
        let message = Message::Json(oxp_wire::display_settings(&d.saved.settings()));
        Self::publish_locked(&mut self.state.lock().unwrap(), "display-settings", message);
        Self::publish_locked(
            &mut self.state.lock().unwrap(),
            "display-view-area",
            d.area_message(),
        );
    }
    pub fn display_settings(&self) -> oxp_core::model::DisplaySettings {
        self.display.lock().unwrap().saved.settings()
    }
    pub fn update_display_settings(
        &self,
        settings: oxp_core::model::DisplaySettings,
    ) -> Result<()> {
        settings.validate().map_err(std::io::Error::other)?;
        let state_dir = self
            .display
            .lock()
            .unwrap()
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_owned());
        if let Some(dir) = state_dir {
            let path = dir.join("settings.json");
            let mut value: serde_json::Value = match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice(&bytes)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
                Err(e) => return Err(e),
            };
            value
                .as_object_mut()
                .ok_or_else(|| std::io::Error::other("Invalid settings file"))?
                .insert(
                    "frameRate".into(),
                    crate::carplay_policy::frame_rate_value(settings.frame_rate).into(),
                );
            crate::platform::private_state_file(&path, &serde_json::to_vec_pretty(&value)?)?;
        }
        let mut d = self.display.lock().unwrap();
        if d.saved.settings() != settings {
            let old = d.saved.clone();
            d.saved.set_settings(&settings);
            d.saved.canvas = d.saved.planned_canvas();
            if let Err(e) = d.persist() {
                d.saved = old;
                return Err(e);
            }
            if old.resolution != settings.resolution
                || old.screen_width != settings.screen_width
                || old.video_codec != settings.video_codec
            {
                d.settings_since = Some((self.clock)());
            }
            let mut s = self.state.lock().unwrap();
            if s.codec_setting != settings.video_codec {
                s.keyframe_watch = None;
            }
            if s.frame_rate_setting != settings.frame_rate {
                s.frame_rate_setting = settings.frame_rate;
                s.desired_fps = crate::carplay_policy::requested_frame_rate(
                    settings.frame_rate,
                    s.negotiated_fps,
                );
                s.restart_requested = s.desired_fps != s.applied_fps;
                s.frame_rate_changed = true;
            }
            s.codec_setting = settings.video_codec.clone();
            let automatic = if s.hevc_failed {
                Self::hevc_fallback(&s)
            } else {
                s.auto_video
            };
            s.desired_video = Self::setting_decision(&s.codec_setting, automatic);
        }
        Self::publish_locked(
            &mut self.state.lock().unwrap(),
            "display-settings",
            Message::Json(oxp_wire::display_settings(&settings)),
        );
        Ok(())
    }
    fn freeze_display(&self) {
        let mut d = self.display.lock().unwrap();
        d.freeze();
        Self::publish_locked(
            &mut self.state.lock().unwrap(),
            "display-view-area",
            d.area_message(),
        );
    }
    pub(crate) fn configure_display(&self, config: &mut AirPlayDisplayConfig) {
        self.display.lock().unwrap().config(config);
    }
    fn observe_display(&self, v: &serde_json::Value) -> Result<()> {
        use crate::display::{HeadUnit, Screen, Size};
        let report = &v["displays"]["0"];
        let number = |v: &serde_json::Value| {
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| (1..=32768).contains(n))
        };
        let Some((width, height, density_dpi)) = number(&report["widthPx"])
            .zip(number(&report["heightPx"]))
            .zip(number(&report["densityDpi"]))
            .map(|((w, h), d)| (w, h, d))
        else {
            self.display.lock().unwrap().disconnected();
            return Ok(());
        };
        let viewport = number(&report["viewport"]["widthPx"])
            .zip(number(&report["viewport"]["heightPx"]))
            .map(|(width, height)| Size { width, height });
        let head = HeadUnit {
            screen: Screen {
                width,
                height,
                density_dpi,
            },
            viewport,
            fullscreen: report["fullscreen"].as_bool(),
            fullscreen_preferred: report["fullscreenPreferred"].as_bool(),
        };
        let limit = v["decoders"].as_array().and_then(|ds| {
            ds.iter()
                .filter_map(|d| {
                    d["codecString"]
                        .as_str()
                        .and_then(crate::display::decoder_limit)
                })
                .max()
        });
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let mut d = self.display.lock().unwrap();
        if !d.connected {
            d.saved.head_unit = None;
        }
        d.connected = true;
        if let Some(size) = viewport {
            d.resize.window_size(size);
        }
        if d.observe(head, limit, now) {
            d.persist()?;
        }
        d.switch_due((self.clock)());
        Ok(())
    }
    pub(crate) fn show_view_area(&self, index: usize) -> bool {
        let mut d = self.display.lock().unwrap();
        let (id, control) = {
            let s = self.state.lock().unwrap();
            (s.current, s.control.clone().filter(|_| s.active))
        };
        let queued = index < d.areas.len()
            && d.view_areas_enabled()
            && control.is_some_and(|c| c.show_view_area(index));
        let mut s = self.state.lock().unwrap();
        let accepted = queued && s.current == id && s.active;
        if accepted {
            d.active = index;
            Self::publish_locked(&mut s, "display-view-area", d.area_message());
        }
        d.record_switch(index, accepted);
        accepted
    }
    fn poll_view_area(&self) {
        Self::poll_view_area_state(&self.display, &self.state, (self.clock)());
    }
    fn poll_view_area_state(
        display: &Mutex<crate::display::DisplayRuntime>,
        state: &Mutex<InputState>,
        now: Instant,
    ) {
        let mut d = display.lock().unwrap();
        let Some(index) = d.switch_due(now) else {
            return;
        };
        let (id, control) = {
            let s = state.lock().unwrap();
            (s.current, s.control.clone().filter(|_| s.active))
        };
        if control.is_some_and(|c| c.show_view_area(index)) {
            let mut s = state.lock().unwrap();
            if s.current == id && s.active {
                d.active = index;
                d.record_switch(index, true);
                Self::publish_locked(&mut s, "display-view-area", d.area_message());
            }
        }
    }
}
impl InputEvents for DiPlayInput {
    fn touch(&self, screen: u16, contacts: &[AirPlayContact]) -> Result<bool> {
        let s = self.state.lock().unwrap();
        if screen != 0 || s.current.is_none() {
            return Ok(false);
        }
        if s.control.is_some() {
            Ok(self
                .recovery
                .try_send(InputCommand::Touch(s.current.unwrap(), contacts.to_vec()))
                .is_ok())
        } else {
            Ok(false)
        }
    }

    fn key(&self, key: u16, down: bool) -> Result<bool> {
        let handler = {
            let s = self.state.lock().unwrap();
            s.current.and(s.keys.clone())
        };
        Ok(handler.is_some_and(|h| h(key, down)))
    }
    fn request_keyframe(&self, screen: u16) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        if let Some(id) = s.current {
            if s.recovery.contains_key(&screen) {
                let now = (self.clock)();
                // Watch before queueing, so the worker counts this request in the gap.
                let armed = screen == 0 && s.keyframe_watch.is_none();
                if armed {
                    s.keyframe_watch = Some(KeyframeWatch {
                        session: id,
                        screen,
                        requested: now,
                        last_request: now,
                        forwarded: 0,
                        logged: false,
                    });
                }
                let accepted = self
                    .recovery
                    .try_send(InputCommand::Recovery(id, screen))
                    .is_ok();
                match &mut s.keyframe_watch {
                    // Repeated joins must not push the original deadline back.
                    Some(w) if accepted && w.session == id && w.screen == screen => {
                        w.last_request = now
                    }
                    _ if armed && !accepted => s.keyframe_watch = None,
                    _ => {}
                }
                return Ok(accepted);
            }
        }
        Ok(false)
    }
    fn microphone_packet(&self, stream: u16, b: &[u8]) -> Result<bool> {
        let s = self.state.lock().unwrap();
        match s.microphones.get(&stream) {
            Some(m) if s.current.is_some() => m.offer(b),
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod runtime_parity_tests {
    use super::*;
    struct Control(Mutex<Vec<u8>>);
    impl SessionControl for Control {
        fn send_touch(&self, _: &[AirPlayContact]) -> Result<bool> {
            Ok(false)
        }
        fn send_media_button(&self, index: u8) -> Result<bool> {
            self.0.lock().unwrap().push(index);
            Ok(true)
        }
    }
    fn input() -> DiPlayInput {
        DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))))
    }
    #[test]
    fn startup_reasons_arrive_before_media_and_host_ui_is_never_replayed() {
        let input = input();
        let (tx, rx) = sync_channel(16);
        input.state.lock().unwrap().output = Some(tx);
        input
            .events
            .emit("stage", &serde_json::json!({"stage":"ConnectingBluetooth"}));
        let Message::Json(v) = rx.recv().unwrap() else {
            panic!()
        };
        assert_eq!(v["session"]["reason"], "startup/authenticating");
        input.begin(Arc::new(Control(Mutex::default()))).unwrap();
        rx.recv().unwrap();
        input.host_ui_requested();
        assert_eq!(
            rx.recv().unwrap(),
            Message::Json(serde_json::json!({"type":"host-ui"}))
        );
        assert!(input
            .state
            .lock()
            .unwrap()
            .declarations
            .values()
            .all(|m| !matches!(m,Message::Json(v) if v["type"]=="host-ui")));
    }
    #[test]
    fn media_keys_delegate_once_per_press_and_stop_with_the_session() {
        let input = input();
        let c = Arc::new(Control(Mutex::default()));
        let id = input.begin(c.clone()).unwrap();
        for key in [7, 8, 9] {
            assert!(input.key(key, true).unwrap());
            assert!(input.key(key, false).unwrap());
        }
        assert_eq!(*c.0.lock().unwrap(), [3, 4, 5]);
        assert!(!input.key(0, true).unwrap());
        input.end(id).unwrap();
        assert!(!input.key(7, true).unwrap());
    }
    #[test]
    fn setting_changes_restart_only_if_resolved_rate_changes() {
        let input = input();
        input.configure_session();
        let mut setting = input.display_settings();
        setting.frame_rate = oxp_core::model::FrameRatePreference::Fps60;
        input.update_display_settings(setting.clone()).unwrap();
        assert!(!input.apply_requested_restart(|| panic!("same rate")));
        setting.frame_rate = oxp_core::model::FrameRatePreference::Fps45;
        input.update_display_settings(setting).unwrap();
        assert!(input.apply_requested_restart(|| {}));
        assert_eq!(input.configure_session().0, 45);
        assert!(!input.apply_requested_restart(|| panic!("duplicate")));
        input.state.lock().unwrap().negotiated_fps = Some(30.);
        input.configure_session();
        assert_eq!(input.applied_frame_rate(), 45);
    }
    #[test]
    fn stats_require_current_session_and_a_captured_playback_token() {
        let input = input();
        let id = input.begin(Arc::new(Control(Mutex::default()))).unwrap();
        for session in [id, id + 1] {
            assert!(!input.receiver_playback(
                &serde_json::json!({"type":"stats","session":session,"screen":0})
            ));
        }
        assert!(!input.runtime_path_facts().picture_on_car);
        assert!(!input.receiver_playback(&serde_json::json!({"type":"media-stats","liveFrames":0})));
    }
}

#[cfg(test)]
mod frame_rate_tests {
    use super::*;
    fn input() -> DiPlayInput {
        DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))))
    }
    fn caps(rate: &str, refresh: f64, sustained: Option<f64>) -> Message {
        let mut v = serde_json::json!({"type":"capabilities","latency":"INTERACTIVE","decoders":[],"displays":{"0":{"widthPx":1280,"heightPx":720,"densityDpi":160,"refreshHz":refresh,"safeArea":{"left":0,"top":0,"right":0,"bottom":0},"frameRate":rate}},"touch":0,"keys":[],"audio":[],"microphone":false});
        if let Some(fps) = sustained {
            v["displays"]["0"]["sustainedFps"] = serde_json::json!(fps);
        }
        Message::Json(v)
    }
    #[test]
    fn wire_capabilities_map_to_airplay_rates_and_fallback() {
        let input = input();
        assert_eq!(input.configure_frame_rate(), 60);
        for fps in (30..=60).step_by(5) {
            input
                .control(&caps(&format!("FPS_{fps}"), 24.0, Some(10.0)))
                .unwrap();
            assert_eq!(input.configure_frame_rate(), fps);
        }
        for (refresh, sustained, expected) in [
            (60.0, None, 60),
            (49.0, None, 30),
            (60.0, Some(49.0), 30),
            (50.0, Some(50.0), 60),
        ] {
            input.control(&caps("AUTO", refresh, sustained)).unwrap();
            assert_eq!(input.configure_frame_rate(), expected);
        }
    }
    #[test]
    fn restart_is_pending_only_controller_only_and_rate_limited() {
        let now = Arc::new(Mutex::new(std::time::Instant::now()));
        let clock = now.clone();
        let input = DiPlayInput::with_clock(
            Arc::new(JsonEvents::new(Arc::new(|_| {}))),
            Arc::new(move || *clock.lock().unwrap()),
        );
        let advance = |seconds| *now.lock().unwrap() += std::time::Duration::from_secs(seconds);
        let restart = Message::Json(serde_json::json!({"type":"restart"}));
        let controller_stops = std::cell::Cell::new(0);
        let stop = || controller_stops.set(controller_stops.get() + 1);
        input.control(&restart).unwrap();
        assert!(!input.apply_requested_restart(stop));
        input.control(&caps("FPS_35", 60.0, None)).unwrap();
        assert_eq!(input.applied_frame_rate(), 60);
        input.control(&restart).unwrap();
        assert!(input.apply_requested_restart(stop));
        assert_eq!(controller_stops.get(), 1);
        assert_eq!(input.configure_frame_rate(), 35);
        // A request inside the window stays pending and fires once the window expires.
        input.control(&caps("FPS_45", 60.0, None)).unwrap();
        input.control(&restart).unwrap();
        advance(9);
        assert!(!input.apply_requested_restart(stop));
        advance(1);
        assert!(input.apply_requested_restart(stop));
        assert_eq!(input.configure_frame_rate(), 45);
        assert_eq!(controller_stops.get(), 2);
        assert!(!input.apply_requested_restart(stop));
        // A pending request is dropped when the receiver returns to the applied rate.
        input.control(&caps("FPS_30", 60.0, None)).unwrap();
        input.control(&restart).unwrap();
        input.control(&caps("FPS_45", 60.0, None)).unwrap();
        advance(10);
        assert!(!input.apply_requested_restart(stop));
        input.control(&caps("FPS_30", 60.0, None)).unwrap();
        assert!(!input.apply_requested_restart(stop));
        assert_eq!(controller_stops.get(), 2);
    }
}

#[cfg(test)]
mod codec_tests {
    use super::*;
    #[test]
    fn codec_restart_limiter_caps_flapping_and_rearms_after_stability() {
        let now = std::time::Instant::now();
        let mut limiter = CodecRestarts::default();
        limiter.changed(now);
        assert!(limiter.allow(now));
        limiter.changed(now + std::time::Duration::from_secs(1));
        assert!(!limiter.allow(now + std::time::Duration::from_secs(29)));
        assert!(limiter.allow(now + std::time::Duration::from_secs(30)));
        limiter.changed(now + std::time::Duration::from_secs(31));
        assert!(!limiter.allow(now + std::time::Duration::from_secs(60)));
        limiter.changed(now + std::time::Duration::from_secs(151));
        assert!(limiter.allow(now + std::time::Duration::from_secs(151)));
    }
    #[test]
    fn codec_restart_limiter_decays_without_further_changes() {
        let now = std::time::Instant::now();
        let at = |seconds| now + std::time::Duration::from_secs(seconds);
        let mut limiter = CodecRestarts::default();
        limiter.changed(at(0));
        assert!(limiter.allow(at(0)));
        limiter.changed(at(30));
        assert!(limiter.allow(at(30)));
        limiter.changed(at(61));
        assert!(!limiter.allow(at(61)));
        assert!(!limiter.allow(at(180)));
        // No further change re-arms the limiter, yet the pending change applies after 120 s.
        assert!(limiter.allow(at(181)));
    }
    #[test]
    fn main_screen_codec_preference_reaches_policy_and_preserves_restart_cooldown() {
        let input = DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))));
        let mut caps: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../OpenXPlay-Stream/spec/vectors/wire/capabilities-h264.json"
        ))
        .unwrap();
        caps["decoders"] = serde_json::json!([{"codecString":"hvc1.1.6.L120.B0","hardware":true}]);
        caps.as_object_mut().unwrap().remove("prefersH264");
        input.control(&Message::Json(caps.clone())).unwrap();
        assert!(input.apply_requested_restart(|| {}));
        assert!(input.configure_video_codec());
        caps["prefersH264"] = serde_json::json!(true);
        input.control(&Message::Json(caps.clone())).unwrap();
        assert_eq!(
            input.video_status()["reason"],
            "main-screen-output-prefers-h264; codec-change-pending"
        );
        assert!(!input.apply_requested_restart(|| panic!("cooldown")));
        input.state.lock().unwrap().codec_restarts.last_restart =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(30));
        assert!(input.apply_requested_restart(|| {}));
        assert!(!input.configure_video_codec());
        caps["prefersH264"] = serde_json::json!(false);
        input.control(&Message::Json(caps)).unwrap();
        assert_eq!(
            input.video_status()["reason"],
            "all-main-screen-outputs-hardware-hevc; codec-change-pending"
        );
    }

    #[test]
    fn capabilities_drive_next_airplay_codec_and_controller_only_restart() {
        let input = DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))));
        assert!(!input.configure_video_codec());
        let mut caps: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../OpenXPlay-Stream/spec/vectors/wire/capabilities.json"
        ))
        .unwrap();
        caps["decoders"] = serde_json::json!([{"codecString":"hvc1.1.6.L120.90","hardware":true}]);
        input.control(&Message::Json(caps.clone())).unwrap();
        assert!(input.apply_requested_restart(|| {}));
        assert!(!input.apply_requested_restart(|| panic!("duplicate stop")));
        assert!(input.configure_video_codec());
        assert_eq!(input.video_status()["codec"], "H265");
        caps["decoders"][0]["hardware"] = serde_json::json!(false);
        input.control(&Message::Json(caps)).unwrap();
        assert!(!input.apply_requested_restart(|| panic!("cooldown")));
        input.state.lock().unwrap().codec_restarts.last_restart =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(30));
        assert!(input.apply_requested_restart(|| {}));
        assert!(!input.configure_video_codec());
        input.clear_receiver_capabilities();
        assert_eq!(input.video_status()["reason"], "no-main-screen-output");
    }
    #[test]
    fn video_status_reports_the_stream_format_only_while_declared() {
        let input = DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))));
        let status = input.video_status();
        assert_eq!(status["codec"], "H264");
        assert!(status["codecString"].is_null() && status["width"].is_null());
        input.state.lock().unwrap().declarations.insert(
            "1-video-0".into(),
            Message::Json(serde_json::json!({"type":"video-format","screen":0,"format":{"codec":"H265","codecString":"hvc1.1.6.L120.B0","width":1920,"height":720}})),
        );
        let status = input.video_status();
        assert_eq!(status["codec"], "H265");
        assert_eq!(status["codecString"], "hvc1.1.6.L120.B0");
        assert_eq!(
            (status["width"].as_u64(), status["height"].as_u64()),
            (Some(1920), Some(720))
        );
        assert!(status["reason"]
            .as_str()
            .unwrap()
            .ends_with("codec-change-pending"));
        input.state.lock().unwrap().declarations.insert(
            "1-video-0".into(),
            Message::Json(serde_json::json!({"type":"video-format","screen":0,"format":null})),
        );
        assert!(input.video_status()["codecString"].is_null());
    }
    #[test]
    fn losing_every_output_keeps_the_current_codec_without_restart() {
        let input = DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))));
        let mut caps: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../OpenXPlay-Stream/spec/vectors/wire/capabilities.json"
        ))
        .unwrap();
        caps["decoders"] = serde_json::json!([{"codecString":"hvc1.1.6.L120.90","hardware":true}]);
        input.control(&Message::Json(caps.clone())).unwrap();
        assert!(input.apply_requested_restart(|| {}));
        assert!(input.configure_video_codec());
        input.clear_receiver_capabilities();
        caps["displays"] = serde_json::json!({});
        input.control(&Message::Json(caps)).unwrap();
        input.state.lock().unwrap().codec_restarts.last_restart =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(300));
        assert!(!input.apply_requested_restart(|| panic!("restart without any output")));
        assert_eq!(input.video_status()["codec"], "H265");
        assert_eq!(input.video_status()["reason"], "no-main-screen-output");
    }
    #[test]
    fn losing_every_output_drops_a_pending_codec_change() {
        let input = DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))));
        assert!(!input.configure_video_codec());
        input.set_video_decision(oxp_core::video_codec_policy::resolve(&[true], false));
        // The change waits for the limiter while the only viewer leaves.
        input.state.lock().unwrap().codec_restarts.last_restart = Some(std::time::Instant::now());
        assert!(!input.apply_requested_restart(|| panic!("cooldown")));
        input.clear_receiver_capabilities();
        input.state.lock().unwrap().codec_restarts.last_restart =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(300));
        assert!(!input.apply_requested_restart(|| panic!("restart without any output")));
        assert_eq!(input.video_status()["codec"], "H264");
        assert_eq!(input.video_status()["reason"], "no-main-screen-output");
    }
}

#[cfg(test)]
mod watchdog_tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct Control;
    impl SessionControl for Control {
        fn send_touch(&self, _: &[AirPlayContact]) -> Result<bool> {
            Ok(true)
        }
    }
    struct Fixture {
        input: DiPlayInput,
        now: Arc<Mutex<Instant>>,
        requests: std::sync::mpsc::Receiver<()>,
        logs: Arc<Mutex<Vec<String>>>,
        session: u64,
    }
    impl Fixture {
        fn new(hevc: bool) -> Self {
            let now = Arc::new(Mutex::new(Instant::now()));
            let clock = now.clone();
            let logs = Arc::new(Mutex::new(Vec::new()));
            let copy = logs.clone();
            let input = DiPlayInput::with_clock(
                Arc::new(JsonEvents::new(Arc::new(move |line| {
                    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
                    if let Some(m) = v["message"].as_str() {
                        copy.lock().unwrap().push(m.to_string());
                    }
                }))),
                Arc::new(move || *clock.lock().unwrap()),
            );
            input.set_video_decision(oxp_core::video_codec_policy::resolve(&[hevc], false));
            assert_eq!(input.configure_video_codec(), hevc);
            let session = input.begin(Arc::new(Control)).unwrap();
            let (tx, requests) = std::sync::mpsc::channel();
            input.state.lock().unwrap().recovery.insert(
                0,
                Arc::new(move || {
                    tx.send(()).unwrap();
                }),
            );
            Self {
                input,
                now,
                requests,
                logs,
                session,
            }
        }
        fn advance(&self, seconds: u64) {
            *self.now.lock().unwrap() += Duration::from_secs(seconds);
        }
        fn received_request(&self) {
            self.requests.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        /// The recovery worker polls every 100 ms; 300 ms covers several polls.
        fn no_request(&self) {
            assert!(self
                .requests
                .recv_timeout(Duration::from_millis(300))
                .is_err());
        }
        fn logged(&self, prefix: &str) -> Vec<String> {
            let logs = self.logs.lock().unwrap();
            logs.iter()
                .filter(|l| l.starts_with(prefix))
                .cloned()
                .collect()
        }
        fn frame(&self, session: u64, key: bool) {
            self.input.publish(
                session,
                "",
                Message::Binary {
                    header: oxp_wire::Header {
                        kind: oxp_wire::Kind::Video,
                        stream: 0,
                        flags: if key { oxp_wire::KEYFRAME } else { 0 },
                        sequence: 0,
                        pts_us: 0,
                        sender_clock_us: 0,
                    },
                    payload: vec![1],
                },
            );
        }
    }
    #[test]
    fn outstanding_keyframe_is_requested_every_second_until_it_arrives() {
        for hevc in [false, true] {
            let f = Fixture::new(hevc);
            assert!(f.input.request_keyframe(0).unwrap());
            f.received_request();
            f.no_request();
            for _ in 0..3 {
                f.advance(1);
                f.received_request();
                f.frame(f.session, false);
            }
            // A stale session's key frame does not end the gap.
            f.frame(f.session + 1, true);
            f.advance(1);
            f.received_request();
            f.frame(f.session, true);
            f.advance(5);
            f.no_request();
            assert!(!f
                .input
                .apply_requested_restart(|| panic!("keyframe arrived")));
            // One line per gap, plus one summary with the request count.
            assert_eq!(
                f.logged("Keyframe requested"),
                ["Keyframe requested for screen 0 forwarded=true"]
            );
            assert_eq!(
                f.logged("Keyframe arrived"),
                ["Keyframe arrived for screen 0 after 5 requests in 4000 ms"]
            );
            // The next gap starts its own cadence and log line.
            assert!(f.input.request_keyframe(0).unwrap());
            f.received_request();
            f.advance(1);
            f.received_request();
            assert_eq!(f.logged("Keyframe requested").len(), 2);
        }
    }
    fn hevc_format(f: &Fixture, session: u64) {
        f.input.publish(
            session,
            "1-video-0",
            Message::Json(
                serde_json::json!({"type":"video-format","screen":0,"format":{"codec":"H265"}}),
            ),
        );
    }
    #[test]
    fn second_unrequested_early_hevc_disconnect_latches_fallback_across_sessions() {
        let f = Fixture::new(true);
        f.input.end(f.session).unwrap(); // No format: not an early drop.
        let id = f.input.begin(Arc::new(Control)).unwrap();
        hevc_format(&f, id);
        f.advance(1);
        f.input.end(id).unwrap();
        f.input.end(id).unwrap(); // Duplicate/stale end: counted once.
        assert!(!f.input.state.lock().unwrap().hevc_failed);
        let id = f.input.begin(Arc::new(Control)).unwrap();
        hevc_format(&f, id - 1); // A retired bridge cannot arm the tracker.
        assert!(f.input.state.lock().unwrap().first_hevc_format.is_none());
        hevc_format(&f, id);
        f.advance(29);
        *f.now.lock().unwrap() += Duration::from_millis(999);
        f.input.end(id).unwrap();
        assert_eq!(
            f.input.video_status()["hevcFailure"],
            "early-drop waitMs=29999 count=2"
        );
        f.input
            .set_video_decision(oxp_core::video_codec_policy::resolve(&[true], false));
        assert!(f.input.apply_requested_restart(|| {}));
        assert!(!f
            .input
            .apply_requested_restart(|| panic!("duplicate restart")));
        assert!(!f.input.configure_video_codec());
        assert_eq!(f.input.video_status()["reason"], "hevc-failed");
        f.input.begin(Arc::new(Control)).unwrap();
        assert_eq!(f.input.state.lock().unwrap().hevc_early_drops, 2);
    }
    #[test]
    fn requested_replaced_forced_and_thirty_second_sessions_do_not_count_as_early_drops() {
        for mode in [
            "stop",
            "expected",
            "replace",
            "forced",
            "long",
            "reconfigured",
        ] {
            let f = Fixture::new(true);
            hevc_format(&f, f.session);
            match mode {
                "stop" => {
                    f.input.stop().unwrap();
                }
                "expected" => {
                    f.input.expect_stop();
                }
                "replace" => {
                    f.input.begin(Arc::new(Control)).unwrap();
                }
                "forced" => {
                    f.input.state.lock().unwrap().codec_setting = "h265".into();
                }
                "long" => f.advance(30),
                "reconfigured" => {
                    f.advance(29);
                    hevc_format(&f, f.session);
                    f.advance(1);
                }
                _ => unreachable!(),
            }
            f.input.end(f.session).unwrap();
            assert_eq!(f.input.state.lock().unwrap().hevc_early_drops, 0, "{mode}");
        }
    }
    #[test]
    fn expected_shutdown_does_not_trigger_the_hevc_watchdog() {
        let f = Fixture::new(true);
        hevc_format(&f, f.session);
        assert!(f.input.request_keyframe(0).unwrap());
        f.received_request();
        f.advance(8);
        f.input.expect_stop();
        assert!(!f
            .input
            .apply_requested_restart(|| panic!("requested shutdown")));
        f.input.end(f.session).unwrap();
        assert!(f.input.video_status()["hevcFailure"].is_null());
        assert_eq!(f.input.state.lock().unwrap().hevc_early_drops, 0);
    }
    #[test]
    fn input_video_counters_count_main_active_frames_and_reset_on_resume() {
        let f = Fixture::new(false);
        f.frame(f.session, true); // Connecting frames are not measured.
        {
            let mut s = f.input.state.lock().unwrap();
            s.active = true;
        }
        f.frame(f.session, true);
        assert_eq!(f.input.video_status()["frames"], 1);
        assert_eq!(f.input.video_status()["bytes"], 0);
        let settings = crate::settings::Settings {
            video_stats: "5".into(),
            ..Default::default()
        };
        f.input.configure_runtime_settings(&settings).unwrap();
        f.frame(f.session, false);
        f.frame(f.session + 1, false);
        hevc_format(&f, f.session); // Reconfiguration keeps counters in the same session.
        assert_eq!(f.input.video_status()["frames"], 2);
        assert_eq!(f.input.video_status()["bytes"], 1);
        f.input.end(f.session).unwrap();
        assert_eq!(f.input.video_status()["active"], false);
        let id = f.input.resume(f.session);
        assert_ne!(id, f.session);
        let status = f.input.video_status();
        assert_eq!(status["session"], id.to_string());
        assert_eq!(status["frames"], 0);
        assert_eq!(status["bytes"], 0);
        assert!(f.input.state.lock().unwrap().first_hevc_format.is_none());
    }
    #[test]
    fn receiver_requests_restart_the_cadence_but_not_the_deadline() {
        let f = Fixture::new(true);
        assert!(f.input.request_keyframe(0).unwrap());
        f.received_request();
        f.advance(1);
        f.received_request();
        assert!(f.input.request_keyframe(0).unwrap());
        f.received_request();
        assert_eq!(f.logged("Keyframe requested").len(), 1);
        let watch = |s: &InputState| {
            let w = s.keyframe_watch.as_ref().unwrap();
            (w.requested, w.last_request, w.forwarded)
        };
        let (requested, last, forwarded) = watch(&f.input.state.lock().unwrap());
        assert_eq!(last.duration_since(requested), Duration::from_secs(1));
        assert_eq!(forwarded, 3);
    }
    #[test]
    fn hevc_fallback_fires_at_eight_seconds_bypassing_the_limiter_and_persists() {
        let f = Fixture::new(true);
        let directory = temporary_directory();
        f.input.initialize_video_state(&directory);
        // Model a recent H264 -> H265 controller restart: the limiter is cooling down.
        assert!(f
            .input
            .state
            .lock()
            .unwrap()
            .codec_restarts
            .allow(*f.now.lock().unwrap()));
        assert!(f.input.request_keyframe(0).unwrap());
        f.received_request();
        for _ in 0..7 {
            f.advance(1);
            f.received_request();
            assert!(!f.input.apply_requested_restart(|| panic!("early")));
        }
        f.advance(1);
        assert!(f.input.apply_requested_restart(|| {}));
        assert!(!f.input.apply_requested_restart(|| panic!("duplicate stop")));
        // The fallback ends the gap: at most the worker's repeat at 8 s, none later.
        let _ = f.requests.recv_timeout(Duration::from_millis(300));
        f.advance(2);
        f.no_request();
        assert_eq!(
            f.input.video_status()["reason"],
            "hevc-keyframe-timeout; codec-change-pending"
        );
        assert_eq!(
            f.logged("video codec H264 reason=hevc-keyframe-timeout after ")
                .len(),
            1
        );
        assert_eq!(
            std::fs::read_to_string(directory.join("video-codec.json")).unwrap(),
            "{\"codec\":\"H264\"}"
        );
        assert!(!f.input.configure_video_codec());
        f.input.end(f.session).unwrap();
        f.input.begin(Arc::new(Control)).unwrap();
        f.input
            .set_video_decision(oxp_core::video_codec_policy::resolve(&[true], false));
        assert!(!f.input.configure_video_codec());
        assert_eq!(f.input.video_status()["reason"], "hevc-keyframe-timeout");
        let next_process = input();
        next_process.initialize_video_state(&directory);
        assert!(!next_process.configure_video_codec());
        next_process.set_video_decision(oxp_core::video_codec_policy::resolve(&[true], false));
        assert!(next_process.configure_video_codec());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn ending_session_stops_requests_and_avc_never_falls_back() {
        let f = Fixture::new(true);
        assert!(f.input.request_keyframe(0).unwrap());
        f.received_request();
        f.advance(1);
        f.received_request();
        f.input.end(f.session).unwrap();
        assert_eq!(
            f.logged("Keyframe still missing"),
            ["Keyframe still missing at session end for screen 0 after 2 requests in 1000 ms"]
        );
        f.advance(10);
        f.no_request();
        assert!(!f.input.apply_requested_restart(|| panic!("ended session")));
        let f = Fixture::new(false);
        assert!(f.input.request_keyframe(0).unwrap());
        f.received_request();
        for _ in 0..10 {
            f.advance(1);
            f.received_request();
        }
        assert!(!f.input.apply_requested_restart(|| panic!("AVC fallback")));
        assert_eq!(
            f.input.video_status()["reason"],
            "hardware-hevc-unavailable"
        );
    }
    #[test]
    fn missing_screen_stream_is_logged_as_not_forwarded() {
        let f = Fixture::new(false);
        let id = f.session;
        f.input.state.lock().unwrap().current = None;
        f.input
            .recovery
            .try_send(InputCommand::Recovery(id, 0))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while f.logged("Keyframe requested").is_empty() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            f.logged("Keyframe requested"),
            ["Keyframe requested for screen 0 forwarded=false"]
        );
        f.no_request();
    }
    #[test]
    fn stale_codec_writes_never_overwrite_newer_ones() {
        let directory = temporary_directory();
        let input = input();
        let path = directory.join("video-codec.json");
        let write = |codec, sequence| CodecWrite {
            path: path.clone(),
            codec,
            sequence,
        };
        input.persist_codec(Some(write(VideoCodec::H265, 2)));
        input.persist_codec(Some(write(VideoCodec::H264, 1)));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"codec\":\"H265\"}"
        );
        std::fs::remove_file(&path).unwrap();
        // An unchanged codec is not rewritten.
        input.persist_codec(Some(write(VideoCodec::H265, 3)));
        assert!(!path.exists());
        input.persist_codec(Some(write(VideoCodec::H264, 4)));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"codec\":\"H264\"}"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    fn temporary_directory() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "openxplay-codec-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }
    fn input() -> DiPlayInput {
        DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))))
    }
    #[test]
    fn codec_round_trip_restore_obeys_first_receiver_and_corruption_is_ignored() {
        let directory = temporary_directory();
        let first = input();
        first.initialize_video_state(&directory);
        first.set_video_decision(oxp_core::video_codec_policy::resolve(&[true], false));
        assert!(first.configure_video_codec());
        let restored = input();
        restored.initialize_video_state(&directory);
        restored.clear_receiver_capabilities();
        assert!(restored.configure_video_codec());
        restored.set_video_decision(oxp_core::video_codec_policy::resolve(&[true], false));
        assert!(!restored.apply_requested_restart(|| panic!("same receiver")));
        restored.set_video_decision(oxp_core::video_codec_policy::resolve(&[false], false));
        assert!(restored.apply_requested_restart(|| {}));
        assert!(!restored.configure_video_codec());
        let avc = input();
        avc.initialize_video_state(&directory);
        assert!(!avc.configure_video_codec());
        std::fs::write(directory.join("video-codec.json"), b"{broken").unwrap();
        let corrupted = input();
        corrupted.initialize_video_state(&directory);
        assert!(!corrupted.configure_video_codec());
        std::fs::write(directory.join("video-codec.json"), b"{\"codec\":\"H265\"}").unwrap();
        let early_receiver = input();
        early_receiver.set_video_decision(oxp_core::video_codec_policy::resolve(&[false], false));
        early_receiver.initialize_video_state(&directory);
        assert!(!early_receiver.configure_video_codec());
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
mod session_restart_tests {
    use super::*;
    #[test]
    fn controller_replacement_consumes_pending_rate_and_codec_restart_once() {
        let input = DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))));
        input.state.lock().unwrap().desired_fps = 30;
        input
            .control(&Message::Json(serde_json::json!({"type":"restart"})))
            .unwrap();
        input.set_video_decision(oxp_core::video_codec_policy::resolve(&[true], false));
        assert_eq!(input.configure_session(), (30, true));
        assert!(
            !input.apply_requested_restart(|| panic!("replacement already applies both changes"))
        );
        // A later receiver change still requests a normal session restart.
        input.set_video_decision(oxp_core::video_codec_policy::resolve(&[false], false));
        assert!(input.apply_requested_restart(|| {}));
        assert_eq!(input.configure_session(), (30, false));
        assert!(!input.apply_requested_restart(|| panic!("duplicate restart")));
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    fn fixture() -> (DiPlayInput, Arc<Mutex<Instant>>) {
        let now = Arc::new(Mutex::new(Instant::now()));
        let clock = now.clone();
        (
            DiPlayInput::with_clock(
                Arc::new(JsonEvents::new(Arc::new(|_| {}))),
                Arc::new(move || *clock.lock().unwrap()),
            ),
            now,
        )
    }
    fn settings(codec: &str, width: u32) -> oxp_core::model::DisplaySettings {
        oxp_core::model::DisplaySettings {
            resolution: "auto".into(),
            screen_width: width,
            video_codec: codec.into(),
            frame_rate: Default::default(),
        }
    }
    #[test]
    fn forced_codec_changes_coalesce_and_bypass_automatic_limiter() {
        let (input, now) = fixture();
        assert_eq!(input.configure_session(), (60, false));
        input
            .state
            .lock()
            .unwrap()
            .codec_restarts
            .record(*now.lock().unwrap());
        input
            .update_display_settings(settings("h264", 125))
            .unwrap();
        *now.lock().unwrap() += Duration::from_millis(600);
        input
            .update_display_settings(settings("h265", 250))
            .unwrap();
        assert!(!input.apply_requested_restart(|| panic!("debounce")));
        *now.lock().unwrap() += Duration::from_millis(999);
        assert!(!input.apply_requested_restart(|| panic!("debounce")));
        *now.lock().unwrap() += Duration::from_millis(1);
        assert!(input.apply_requested_restart(|| {}));
        assert!(!input.apply_requested_restart(|| panic!("duplicate")));
        assert_eq!(input.configure_session(), (60, true));
        assert_eq!(input.video_status()["reason"], "setting-h265");
        input.set_video_decision(oxp_core::video_codec_policy::resolve(&[false], false));
        assert_eq!(input.video_status()["reason"], "setting-h265");
        {
            let mut s = input.state.lock().unwrap();
            let at = *now.lock().unwrap();
            s.current = Some(1);
            s.keyframe_watch = Some(KeyframeWatch {
                session: 1,
                screen: 0,
                requested: at,
                last_request: at,
                forwarded: 1,
                logged: true,
            });
        }
        *now.lock().unwrap() += Duration::from_secs(9);
        assert!(!input.apply_requested_restart(|| panic!("forced watchdog")));
        input
            .update_display_settings(settings("auto", 250))
            .unwrap();
        *now.lock().unwrap() += Duration::from_secs(1);
        assert!(input.apply_requested_restart(|| {}));
        assert_eq!(input.configure_session(), (60, false));
        assert_eq!(input.video_status()["reason"], "hardware-hevc-unavailable");
    }
    #[test]
    fn display_settings_round_trip_over_in_memory_consumer_wire() {
        let (input, _) = fixture();
        let (tx, rx) = sync_channel(8);
        input.state.lock().unwrap().output = Some(tx);
        let request = Message::Json(
            serde_json::json!({"type":"display-settings","resolution":"native","screenWidth":300,"videoCodec":"h264"}),
        );
        let mut bytes = Vec::new();
        oxp_wire::write(&mut bytes, &request).unwrap();
        let decoded = oxp_wire::read(&mut std::io::Cursor::new(bytes))
            .unwrap()
            .unwrap();
        assert!(input.control(&decoded).unwrap());
        let reply = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let mut bytes = Vec::new();
        oxp_wire::write(&mut bytes, &reply).unwrap();
        let mut expected = match request {
            Message::Json(v) => v,
            _ => unreachable!(),
        };
        expected["frameRate"] = "AUTO".into();
        assert_eq!(
            oxp_wire::read(&mut std::io::Cursor::new(bytes)).unwrap(),
            Some(Message::Json(expected))
        );
        assert_eq!(input.display_settings().screen_width, 300);
        let control = Arc::new(AreaControl(Mutex::new(vec![])));
        let id = input.begin(control).unwrap();
        input.end(id).unwrap();
        assert!(input
            .state
            .lock()
            .unwrap()
            .declarations
            .contains_key("display-settings"));
        assert!(input
            .state
            .lock()
            .unwrap()
            .declarations
            .contains_key("display-view-area"));
    }
    struct AreaControl(Mutex<Vec<usize>>);
    impl SessionControl for AreaControl {
        fn send_touch(&self, _: &[AirPlayContact]) -> Result<bool> {
            Ok(true)
        }
        fn show_view_area(&self, index: usize) -> bool {
            self.0.lock().unwrap().push(index);
            true
        }
    }
    #[test]
    fn manual_view_area_switch_publishes_crop_and_full_canvas_with_bounded_results() {
        let (input, _) = fixture();
        let control = Arc::new(AreaControl(Mutex::new(vec![])));
        input.begin(control.clone()).unwrap();
        input.state.lock().unwrap().active = true;
        input.display.lock().unwrap().areas = vec![
            crate::display::Area {
                size: crate::display::Size {
                    width: 1920,
                    height: 960,
                },
                fullscreen: true,
                windowed: true,
            },
            crate::display::Area {
                size: crate::display::Size {
                    width: 1450,
                    height: 960,
                },
                fullscreen: false,
                windowed: true,
            },
        ];
        assert!(input.show_view_area(1));
        let Message::Json(crop) =
            input.state.lock().unwrap().declarations["display-view-area"].clone()
        else {
            panic!()
        };
        assert_eq!(crop["area"]["width"], 1450);
        assert!(input.show_view_area(0));
        let Message::Json(full) =
            input.state.lock().unwrap().declarations["display-view-area"].clone()
        else {
            panic!()
        };
        assert!(full["area"].is_null());
        assert_eq!(*control.0.lock().unwrap(), vec![1, 0]);
        for _ in 0..40 {
            assert!(!input.show_view_area(7));
        }
        let d = input.display.lock().unwrap();
        assert_eq!(d.area_switches.len(), 32);
        assert_eq!(d.area_switches.back().unwrap()["accepted"], false);
    }
    #[test]
    fn planned_display_reaches_airplay_and_switch_announces_after_debounce() {
        let (input, now) = fixture();
        let mut caps: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../OpenXPlay-Stream/spec/vectors/wire/capabilities.json"
        ))
        .unwrap();
        caps["displays"]["0"] = serde_json::json!({"widthPx":2401,"heightPx":1201,"densityDpi":280,"refreshHz":60,"safeArea":{"left":0,"right":0,"top":0,"bottom":0},"viewport":{"widthPx":1533,"heightPx":1014},"fullscreen":true,"fullscreenPreferred":true});
        caps["decoders"] = serde_json::json!([{"codecString":"avc1.640033","hardware":true}]);
        input.control(&Message::Json(caps.clone())).unwrap();
        input.configure_session();
        let mut config = AirPlayDisplayConfig::new(1280, 720);
        input.configure_display(&mut config);
        assert_eq!(
            (
                config.width_pixels,
                config.height_pixels,
                config.width_physical_mm
            ),
            (1920, 960, Some(200))
        );
        assert_eq!(config.view_areas.as_ref().unwrap().len(), 2);
        let control = Arc::new(AreaControl(Mutex::new(vec![])));
        input.begin(control.clone()).unwrap();
        input.state.lock().unwrap().active = true;
        assert!(!input.apply_requested_restart(|| panic!("window")));
        *now.lock().unwrap() += Duration::from_millis(299);
        input.poll_view_area();
        assert!(control.0.lock().unwrap().is_empty());
        input.control(&Message::Json(caps)).unwrap();
        *now.lock().unwrap() += Duration::from_millis(1);
        input.poll_view_area();
        assert_eq!(*control.0.lock().unwrap(), vec![1]);
        let s = input.state.lock().unwrap();
        let Message::Json(area) = &s.declarations["display-view-area"] else {
            panic!()
        };
        oxp_wire::validate_json(area).unwrap();
        assert_eq!(area["area"]["canvasWidth"], 1920);
        assert!(area["area"]["width"].as_u64().unwrap() < 1920);
    }
}
