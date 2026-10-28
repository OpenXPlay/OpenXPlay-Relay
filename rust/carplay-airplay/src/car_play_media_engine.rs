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

/// `AudioStreamId` protocol data.
#[derive(Clone, Debug)]
pub struct AudioStreamId {
    pub kind: u16,

    pub audio_type: String,
}
/// Platform interface for `MediaSink`. Implementations must support bounded shutdown.
pub trait MediaSink: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn on_video_codec(&self, kind: u16, codec: VideoCodec) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_video_config(&self, kind: u16, codec_data: &[u8]) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_video_frame(
        &self,
        kind: u16,
        nalu_bytes: &[u8],
        sender_nanos: u64,
        arrival_nanos: u64,
    ) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn set_video_recovery_handler(&self, kind: u16, handler: Arc<dyn Fn() + Send + Sync>);
    fn set_video_diagnostic_handler(
        &self,
        _kind: u16,
        _handler: Arc<dyn Fn(String) + Send + Sync>,
    ) {
    }
    /// Errors propagate to the lifecycle owner.
    fn on_screen_stream_active(&self, kind: u16, active: bool);
    /// Errors propagate to the lifecycle owner.
    fn on_audio_started(
        &self,
        id: &AudioStreamId,
        format: &AudioFormat,
        first_sample: u32,
    ) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_audio_rtp(
        &self,
        id: &AudioStreamId,
        format: &AudioFormat,
        rtp: &[u8],
        sample: u32,
    ) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_audio_stopped(&self, id: &AudioStreamId) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_microphone_started(&self, id: &AudioStreamId, config: &MicrophoneConfig) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_microphone_stopped(&self, id: &AudioStreamId) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_iap_message(&self, bytes: &[u8]) -> Result<()>;
}
/// Platform interface for `InputEvents`. Implementations must support bounded shutdown.
pub trait InputEvents: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn touch(&self, screen: u16, contacts: &[AirPlayContact]) -> Result<bool>;
    /// Errors propagate to the lifecycle owner.
    fn key(&self, key: u16, down: bool) -> Result<bool>;
    /// Errors propagate to the lifecycle owner.
    fn request_keyframe(&self, screen: u16) -> Result<bool>;
    /// Errors propagate to the lifecycle owner.
    fn microphone_packet(&self, stream: u16, encoded: &[u8]) -> Result<bool>;
}
use crate::buffered_audio_stream::DeliveryGate;
use crate::screen_stream::invalid;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, Weak,
    },
};
#[derive(Clone)]
pub struct CarPlayMediaEngine {
    inner: Arc<Engine>,
}
struct Engine {
    sink: Arc<dyn MediaSink>,
    transport: Arc<dyn StreamTransport>,
    microphone_enabled: bool,
    screen_policy: ScreenSessionPolicy,
    screen_delivery: DeliveryGate,
    state: Mutex<EngineState>,
}
#[derive(Default)]
struct EngineState {
    streams: HashMap<StreamKey, Arc<Slot>>,
    screen_owners: HashMap<u16, (StreamKey, Arc<Slot>)>,
    retired_screens: HashSet<(usize, u16)>,
    sessions: HashMap<usize, SessionBinding>,
    handler: Option<Arc<dyn IapTunnelListener>>,
    pending: HashMap<usize, Arc<AirPlayIapTunnelStream>>,
    buffered_owner: Option<usize>,
    buffered_transition: Option<BufferedTransition>,
    retired: HashSet<usize>,
    closed: bool,
}
struct BufferedTransition {
    session: usize,
    cancelled: bool,
}
struct SessionBinding {
    session: Weak<AirPlaySession>,
    peer: std::net::IpAddr,
}
#[derive(Clone, Hash, PartialEq, Eq)]
struct StreamKey {
    session: usize,
    kind: u16,
    audio_type: String,
}
struct Slot {
    #[cfg(test)]
    screen_listener: Mutex<Option<Arc<dyn ScreenListener>>>,
    active: AtomicBool,
    delivery: DeliveryGate,
    resource: Mutex<Option<Resource>>,
    id: AudioStreamId,
    audio: Option<AudioFormat>,
    connection: u64,
    latency_ms: u64,
    origin: Mutex<Option<(u32, Instant)>>,
    position: Option<FeedbackPositionMonitor>,
    microphone: Mutex<Option<MicrophoneState>>,
}
struct MicrophoneState {
    config: MicrophoneConfig,
    start: MicrophoneStart,
    packetizer: MicrophonePacketizer,
    transport: Option<Arc<dyn DatagramTransport>>,
}
enum Resource {
    Screen(ScreenStream),
    Audio(AudioStream),
    Buffered(BufferedAudioStream),
    Tunnel(Arc<IapTunnel>),
    Bridge(Arc<AirPlayIapTunnelStream>),
}
impl Resource {
    fn close(&self) -> Result<()> {
        match self {
            Self::Screen(s) => s.close(),
            Self::Audio(s) => s.close(),
            Self::Buffered(s) => s.close(),
            Self::Tunnel(s) => s.close(),
            Self::Bridge(s) => s.close(),
        }
    }
}
impl Slot {
    fn new(
        kind: u16,
        audio_type: String,
        audio: Option<AudioFormat>,
        connection: u64,
        latency_ms: u64,
    ) -> Self {
        let position = audio
            .as_ref()
            .map(|f| FeedbackPositionMonitor::new(f.rtp_clock_rate));
        Self {
            #[cfg(test)]
            screen_listener: Mutex::new(None),
            active: AtomicBool::new(true),
            delivery: DeliveryGate::default(),
            resource: Mutex::new(None),
            id: AudioStreamId { kind, audio_type },
            audio,
            connection,
            latency_ms,
            origin: Mutex::new(None),
            position,
            microphone: Mutex::new(None),
        }
    }
    fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}
impl CarPlayMediaEngine {
    pub fn new(
        sink: Arc<dyn MediaSink>,
        transport: Arc<dyn StreamTransport>,
        microphone_enabled: bool,
    ) -> Self {
        Self::with_screen_session_policy(
            sink,
            transport,
            microphone_enabled,
            ScreenSessionPolicy::default(),
        )
    }
    pub fn with_screen_session_policy(
        sink: Arc<dyn MediaSink>,
        transport: Arc<dyn StreamTransport>,
        microphone_enabled: bool,
        policy: ScreenSessionPolicy,
    ) -> Self {
        Self {
            inner: Arc::new(Engine {
                sink,
                transport,
                microphone_enabled,
                screen_policy: policy,
                screen_delivery: DeliveryGate::default(),
                state: Mutex::new(EngineState::default()),
            }),
        }
    }
    /// Supply the owning session and peer without capturing a borrowed RTSP callback.
    /// The shell calls this after session construction and before starting its worker.
    pub fn bind_session(&self, session: &Arc<AirPlaySession>, peer: std::net::IpAddr) {
        let mut s = self.inner.state.lock().unwrap();
        s.retired.remove(&session_id(session));
        if !s
            .sessions
            .get(&session_id(session))
            .and_then(|b| b.session.upgrade())
            .is_some_and(|bound| Arc::ptr_eq(&bound, session))
        {
            s.retired_screens
                .retain(|(sid, _)| *sid != session_id(session));
        }
        s.sessions.insert(
            session_id(session),
            SessionBinding {
                session: Arc::downgrade(session),
                peer,
            },
        );
    }
    pub fn set_iap_tunnel_handler(
        &self,
        handler: Option<Arc<dyn IapTunnelListener>>,
    ) -> Result<()> {
        self.inner.state.lock().unwrap().handler = handler;
        Ok(())
    }
    fn diagnostic(&self, session: &AirPlaySession) -> Arc<dyn Fn(String) + Send + Sync> {
        let session = self.binding(session).map(|(s, _)| Arc::downgrade(&s));
        Arc::new(move |line| {
            if let Some(session) = session.as_ref().and_then(Weak::upgrade) {
                session.log_debug(&line);
            } else {
                eprintln!("{line}");
            }
        })
    }
    fn transport(&self, session: &AirPlaySession) -> Arc<dyn StreamTransport> {
        match session
            .remote_address()
            .or_else(|| self.binding(session).map(|(_, peer)| peer))
        {
            Some(peer) => Arc::new(PeerTransport {
                inner: self.inner.transport.clone(),
                peer,
            }),
            None => self.inner.transport.clone(),
        }
    }
    fn binding(&self, session: &AirPlaySession) -> Option<(Arc<AirPlaySession>, std::net::IpAddr)> {
        self.inner
            .state
            .lock()
            .unwrap()
            .sessions
            .get(&session_id(session))
            .and_then(|b| b.session.upgrade().map(|session| (session, b.peer)))
    }
    fn registered_screen(&self, state: &EngineState, key: &StreamKey, slot: &Arc<Slot>) -> bool {
        state.streams.get(key).is_some_and(|s| Arc::ptr_eq(s, slot))
            && (!self.inner.screen_policy.isolate_sessions
                || state
                    .screen_owners
                    .get(&key.kind)
                    .is_some_and(|(_, s)| Arc::ptr_eq(s, slot)))
    }
    fn owns_screen(&self, key: &StreamKey, slot: &Arc<Slot>) -> bool {
        let state = self.inner.state.lock().unwrap();
        slot.active()
            && self.registered_screen(&state, key, slot)
            && !state.closed
            && !state
                .sessions
                .get(&key.session)
                .and_then(|b| b.session.upgrade())
                .is_some_and(|s| s.is_closed())
    }
    // Screen mutations hold screen_delivery, but release state before entering the sink.
    fn forget_screen(&self, key: &StreamKey, slot: &Arc<Slot>) {
        let mut state = self.inner.state.lock().unwrap();
        slot.active.store(false, Ordering::Release);
        if state.streams.get(key).is_some_and(|s| Arc::ptr_eq(s, slot)) {
            state.streams.remove(key);
        }
        if state
            .screen_owners
            .get(&key.kind)
            .is_some_and(|(_, s)| Arc::ptr_eq(s, slot))
        {
            state.screen_owners.remove(&key.kind);
        }
    }
    fn remove_screen(&self, key: &StreamKey, slot: &Arc<Slot>) -> bool {
        if !self.registered_screen(&self.inner.state.lock().unwrap(), key, slot) {
            return false;
        }
        self.forget_screen(key, slot);
        self.inner.sink.on_screen_stream_active(key.kind, false);
        true
    }
    fn install(&self, key: StreamKey, slot: Arc<Slot>) -> Result<bool> {
        let previous = {
            let mut state = self.inner.state.lock().unwrap();
            if state.closed {
                drop(state);
                self.retire(slot)?;
                return Ok(false);
            }
            state.streams.insert(key, slot)
        };
        if let Some(previous) = previous {
            self.retire(previous)?;
        }
        Ok(true)
    }
    fn retire(&self, slot: Arc<Slot>) -> Result<()> {
        let mut failure = None;
        let resource = {
            let _delivery = slot.delivery.enter();
            slot.active.store(false, Ordering::Release);
            let microphone = slot.microphone.lock().unwrap().take();
            if let Some(mic) = microphone {
                mic.start.close();
                if let Some(udp) = mic.transport {
                    if let Err(error) = udp.close() {
                        failure.get_or_insert(error);
                    }
                }
                if let Err(error) = self.inner.sink.on_microphone_stopped(&slot.id) {
                    failure.get_or_insert(error);
                }
            }
            let resource = slot.resource.lock().unwrap().take();
            if slot.audio.is_some() && !matches!(&resource, Some(Resource::Buffered(_))) {
                if let Err(error) = self.inner.sink.on_audio_stopped(&slot.id) {
                    failure.get_or_insert(error);
                }
            }
            resource
        };
        if let Some(resource) = resource {
            if let Err(error) = resource.close() {
                failure.get_or_insert(error);
            }
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
        Ok(())
    }
    pub fn close(&self) -> Result<()> {
        let (slots, pending) = {
            let _delivery = self.inner.screen_delivery.enter();
            let mut s = self.inner.state.lock().unwrap();
            s.closed = true;
            let screens = s
                .streams
                .iter()
                .filter(|(k, _)| k.kind == 110)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>();
            drop(s);
            for (key, slot) in &screens {
                self.remove_screen(key, slot);
            }
            let mut s = self.inner.state.lock().unwrap();
            s.sessions.clear();
            (
                s.streams
                    .drain()
                    .map(|(_, v)| v)
                    .chain(screens.into_iter().map(|(_, v)| v))
                    .collect::<Vec<_>>(),
                s.pending.drain().map(|(_, v)| v).collect::<Vec<_>>(),
            )
        };
        let mut failure = None;
        for bridge in pending {
            if let Err(error) = bridge.close() {
                failure.get_or_insert(error);
            }
        }
        for slot in slots {
            if let Err(error) = self.retire(slot) {
                failure.get_or_insert(error);
            }
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
        Ok(())
    }
    fn key(session: &AirPlaySession, stream: &BplistValue, label: &str) -> Option<[u8; 32]> {
        let shared = session.shared_secret()?;
        let id = unsigned(get(stream, "streamConnectionID")?)?;
        Some(derive_key(&shared, &id.to_string(), label))
    }
    pub fn wants_microphone(audio_type: &str, stream: &BplistValue) -> bool {
        match audio_type {
            "telephony" | "speechrecognition" => true,
            "default" | "compatibility" => match get(stream, "input") {
                Some(BplistValue::Boolean(v)) => *v,
                Some(BplistValue::Integer(n)) => n != &num_bigint::BigInt::from(0),
                _ => number(stream, "dataPort").is_some_and(|v| (1..=65535).contains(&v)),
            },
            _ => false,
        }
    }
    fn microphone_config(
        &self,
        session: &AirPlaySession,
        kind: u16,
        stream: &BplistValue,
        format: &AudioFormat,
    ) -> Option<MicrophoneConfig> {
        if !matches!(format.codec, AudioCodecKind::Lpcm | AudioCodecKind::Opus)
            || !self.inner.microphone_enabled
            || kind != 100
            || !Self::wants_microphone(&format.audio_type, stream)
        {
            return None;
        }
        let port = u16::try_from(number(stream, "dataPort")?)
            .ok()
            .filter(|p| *p > 0)?;
        let host = session
            .remote_address()
            .or_else(|| self.binding(session).map(|(_, host)| host))?;
        let key = Self::key(session, stream, DATASTREAM_INPUT_KEY)?;
        let bits = number(stream, "audioFormat").unwrap_or(0);
        let frames = number(stream, "framesPerPacket").unwrap_or(0);
        let frame_millis = if format.codec == AudioCodecKind::Opus || frames == 0 {
            20
        } else {
            ((frames as f64 * 1000.0 / format.sample_rate as f64).round() as u32).clamp(5, 60)
        };
        Some(MicrophoneConfig {
            audio_type: format.audio_type.clone(),
            sample_rate: format.sample_rate,
            channels: format.channels,
            payload_type: kind as u8,
            frame_millis,
            host,
            port,
            key: key.to_vec(),
            codec: format.codec,
            bitrate: (format.codec == AudioCodecKind::Opus).then_some(if bits & OPUS_48K != 0 {
                96000
            } else if bits & OPUS_24K != 0 {
                64000
            } else {
                48000
            }),
            opus_clock_rate: MicrophoneConfig::opus_clock_rate(bits),
        })
    }
    fn start_microphone(&self, slot: &Arc<Slot>, config: MicrophoneConfig) -> Result<()> {
        let weak = Arc::downgrade(slot);
        let sink = self.inner.sink.clone();
        let config2 = config.clone();
        let action = Arc::new(move || {
            if let Some(slot) = weak.upgrade() {
                let _delivery = slot.delivery.enter();
                if slot.active() {
                    let _ = sink.on_microphone_started(&slot.id, &config2);
                }
            }
        });
        let start = MicrophoneStart::after_setup_response(action);
        *slot.microphone.lock().unwrap() = Some(MicrophoneState {
            packetizer: MicrophonePacketizer::new(config.clone())?,
            config,
            start,
            transport: None,
        });
        Ok(())
    }
}
fn session_id(session: &AirPlaySession) -> usize {
    session as *const AirPlaySession as usize
}
pub(crate) fn get<'a>(value: &'a BplistValue, key: &str) -> Option<&'a BplistValue> {
    if let BplistValue::Dictionary(fields) = value {
        fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    } else {
        None
    }
}
fn unsigned(value: &BplistValue) -> Option<u64> {
    if let BplistValue::Integer(n) = value {
        n.to_string()
            .parse::<u64>()
            .ok()
            .or_else(|| n.to_string().parse::<i64>().ok().map(|v| v as u64))
    } else {
        None
    }
}
fn number(value: &BplistValue, key: &str) -> Option<u64> {
    unsigned(get(value, key)?)
}
fn string(value: &BplistValue, key: &str) -> Option<String> {
    if let BplistValue::String(s) = get(value, key)? {
        Some(s.clone())
    } else {
        None
    }
}
fn integer(v: impl Into<num_bigint::BigInt>) -> BplistValue {
    BplistValue::Integer(v.into())
}
fn dict(fields: impl IntoIterator<Item = (&'static str, BplistValue)>) -> BplistValue {
    BplistValue::Dictionary(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn derive_key(shared: &[u8; 32], seed: &str, label: &str) -> [u8; 32] {
    let mut key = [0; 32];
    hkdf::Hkdf::<sha2::Sha512>::new(Some(format!("DataStream-Salt{seed}").as_bytes()), shared)
        .expand(label.as_bytes(), &mut key)
        .expect("32-byte HKDF output");
    key
}
/// NAL unit types of an Annex B access unit, in order.
fn nal_types(codec: VideoCodec, annex_b: &[u8]) -> Vec<u8> {
    let header_len = if codec == VideoCodec::H264 { 1 } else { 2 };
    let mut types = Vec::new();
    let mut i = 0;
    while i + 3 < annex_b.len() {
        let prefix = match annex_b[i..i + 4] {
            [0, 0, 1, _] => 3,
            [0, 0, 0, 1] => 4,
            _ => 0,
        };
        if prefix == 0 {
            i += 1;
            continue;
        }
        if i + prefix + header_len <= annex_b.len() {
            let header = annex_b[i + prefix];
            types.push(if codec == VideoCodec::H264 {
                header & 31
            } else {
                header >> 1 & 63
            });
        }
        i += prefix;
    }
    types
}
/// IDR (H.264) or IRAP (H.265) starts an independently decodable reference chain.
fn is_random_access(codec: VideoCodec, nal_type: u8) -> bool {
    match codec {
        VideoCodec::H264 => nal_type == 5,
        VideoCodec::H265 => (16..=21).contains(&nal_type),
    }
}
/// Times the iPhone's answer to forceKeyFrame for the session log. While no request is
/// outstanding a frame costs one uncontended lock.
#[derive(Default)]
struct KeyframeProbe(Mutex<KeyframeProbeState>);
#[derive(Default)]
struct KeyframeProbeState {
    codec: Option<VideoCodec>,
    pending: Option<PendingKeyframe>,
    /// Requests since the first of the current gap, sent or not; a key frame ends the gap.
    gap: Option<RequestGap>,
}
struct PendingKeyframe {
    since: Instant,
    requests: u32,
    frames: u32,
}
struct RequestGap {
    since: Instant,
    logged: Instant,
    requests: u32,
    unsent: u32,
}
/// The relay repeats its request about once a second while no key frame arrives.
const REQUEST_LOG_INTERVAL: Duration = Duration::from_secs(5);
impl KeyframeProbe {
    /// Records a forceKeyFrame request; a sent one starts or extends the wait, keeping the first
    /// request's time. Returns the log line for the first request of a gap, then at most one
    /// every 5 s with the gap's counts.
    fn requested(&self, sent: bool, now: Instant) -> Option<String> {
        let mut s = self.0.lock().unwrap();
        if sent {
            match &mut s.pending {
                Some(p) => p.requests = p.requests.saturating_add(1),
                None => {
                    s.pending = Some(PendingKeyframe {
                        since: now,
                        requests: 1,
                        frames: 0,
                    })
                }
            }
        }
        let unsent = u32::from(!sent);
        let Some(gap) = &mut s.gap else {
            s.gap = Some(RequestGap {
                since: now,
                logged: now,
                requests: 1,
                unsent,
            });
            return Some(format!("Video recovery: requested keyframe sent={sent}"));
        };
        gap.requests = gap.requests.saturating_add(1);
        gap.unsent = gap.unsent.saturating_add(unsent);
        if now.saturating_duration_since(gap.logged) < REQUEST_LOG_INTERVAL {
            return None;
        }
        gap.logged = now;
        Some(format!(
            "Video recovery: requested keyframe sent={sent} ({} requests in {} ms, {} not sent)",
            gap.requests,
            now.saturating_duration_since(gap.since).as_millis(),
            gap.unsent
        ))
    }
    fn codec(&self, codec: VideoCodec) {
        self.0.lock().unwrap().codec = Some(codec);
    }
    fn config(&self, bytes: usize, now: Instant) -> Option<String> {
        let s = self.0.lock().unwrap();
        let p = s.pending.as_ref()?;
        Some(format!(
            "Video recovery: config ({bytes} bytes) {} ms after keyframe request",
            now.saturating_duration_since(p.since).as_millis()
        ))
    }
    /// One line for the first frame after a request when it is not a key frame, one for the key frame.
    fn frame(&self, annex_b: &[u8], now: Instant) -> Option<String> {
        let mut s = self.0.lock().unwrap();
        let codec = s.codec?;
        if s.pending.is_none() && s.gap.is_none() {
            return None;
        }
        let types = nal_types(codec, annex_b);
        let key = types.iter().copied().find(|&t| is_random_access(codec, t));
        if key.is_some() {
            s.gap = None;
        }
        let p = s.pending.as_mut()?;
        let elapsed = now.saturating_duration_since(p.since).as_millis();
        if let Some(t) = key {
            let line = format!(
                "Video recovery: key frame nal={t} bytes={} {elapsed} ms after keyframe request ({} frames, {} requests before it)",
                annex_b.len(),
                p.frames,
                p.requests
            );
            s.pending = None;
            return Some(line);
        }
        p.frames = p.frames.saturating_add(1);
        (p.frames == 1).then(|| {
            format!(
                "Video recovery: first frame {elapsed} ms after keyframe request is not a key frame nal={types:?} bytes={}",
                annex_b.len()
            )
        })
    }
}
struct CurrentScreenListener {
    owner: Option<(Weak<Engine>, StreamKey)>,
    slot: Weak<Slot>,
    sink: Arc<dyn MediaSink>,
    session: Option<Weak<AirPlaySession>>,
    keyframes: Arc<KeyframeProbe>,
    received: AtomicBool,
    first_frame: Box<dyn Fn() + Send + Sync>,
}
impl CurrentScreenListener {
    fn engine(&self) -> Option<CarPlayMediaEngine> {
        self.owner
            .as_ref()?
            .0
            .upgrade()
            .map(|inner| CarPlayMediaEngine { inner })
    }
    fn current(&self, slot: &Arc<Slot>) -> bool {
        slot.active()
            && !self
                .session
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|s| s.is_closed())
            && self
                .owner
                .as_ref()
                .is_none_or(|(_, key)| self.engine().is_some_and(|e| e.owns_screen(key, slot)))
    }
    fn deliver<T>(&self, default: T, action: impl FnOnce(&Arc<Slot>) -> T) -> T {
        let Some(slot) = self.slot.upgrade() else {
            return default;
        };
        let engine = self.engine();
        let gate = engine
            .as_ref()
            .map_or(&slot.delivery, |e| &e.inner.screen_delivery);
        let serialize = engine
            .as_ref()
            .is_none_or(|e| e.inner.screen_policy.serialize_delivery);
        let _delivery = serialize.then(|| gate.enter());
        if self.current(&slot) {
            action(&slot)
        } else {
            default
        }
    }
    fn log(&self, line: Option<String>) {
        if let (Some(line), Some(session)) = (line, self.session.as_ref().and_then(Weak::upgrade)) {
            session.log_debug(&line);
        }
    }
}
impl ScreenListener for CurrentScreenListener {
    fn on_codec(&self, codec: VideoCodec) {
        self.deliver((), |s| {
            self.keyframes.codec(codec);
            let _ = self.sink.on_video_codec(s.id.kind, codec);
        });
    }
    fn on_config(&self, bytes: &[u8]) -> Result<()> {
        self.deliver(Ok(()), |s| {
            self.log(self.keyframes.config(bytes.len(), Instant::now()));
            self.sink.on_video_config(s.id.kind, bytes)
        })
    }
    fn on_frame(&self, bytes: &[u8], sender: u64, arrival: u64) -> Result<()> {
        self.deliver(Ok(()), |s| {
            if !self.received.swap(true, Ordering::AcqRel) {
                (self.first_frame)();
            }
            if !self.current(s) {
                return Ok(());
            }
            let result = self.sink.on_video_frame(s.id.kind, bytes, sender, arrival);
            self.log(self.keyframes.frame(bytes, Instant::now()));
            result
        })
    }
    fn on_closed(&self, _cause: Option<std::io::Error>) {
        let Some(slot) = self.slot.upgrade() else {
            if self
                .engine()
                .is_some_and(|e| !e.inner.screen_policy.isolate_sessions)
            {
                if let Some(session) = self.session.as_ref().and_then(Weak::upgrade) {
                    let _ = session.close();
                }
            }
            return;
        };
        let close = if let (Some(engine), Some((_, key))) = (self.engine(), &self.owner) {
            let _delivery = engine.inner.screen_delivery.enter();
            slot.active.store(false, Ordering::Release);
            let registered =
                engine.registered_screen(&engine.inner.state.lock().unwrap(), key, &slot);
            if registered {
                engine
                    .inner
                    .state
                    .lock()
                    .unwrap()
                    .retired_screens
                    .insert((key.session, key.kind));
                engine.remove_screen(key, &slot);
            }
            registered || !engine.inner.screen_policy.isolate_sessions
        } else {
            let _delivery = slot.delivery.enter();
            if slot.active.swap(false, Ordering::AcqRel) {
                self.sink.on_screen_stream_active(slot.id.kind, false);
                true
            } else {
                false
            }
        };
        if close {
            if let Some(session) = self.session.as_ref().and_then(Weak::upgrade) {
                let _ = session.close();
            }
        }
    }
}
struct CurrentAudioListener {
    slot: Weak<Slot>,
    sink: Arc<dyn MediaSink>,
    probe: Mutex<RtpClockProbe>,
}
impl AudioListener for CurrentAudioListener {
    fn on_started(&self, format: &AudioFormat, sample: u32) -> Result<()> {
        if let Some(s) = self.slot.upgrade() {
            let _d = s.delivery.enter();
            if !s.active() {
                return Ok(());
            }
            *s.origin.lock().unwrap() = Some((sample, Instant::now()));
            self.sink.on_audio_started(&s.id, format, sample)?;
            let start = s
                .microphone
                .lock()
                .unwrap()
                .as_ref()
                .map(|m| m.start.clone());
            if let Some(start) = start {
                start.on_downlink();
            }
        }
        Ok(())
    }
    fn on_rtp(&self, format: &AudioFormat, bytes: &[u8], sample: u32) -> Result<()> {
        if let Some(s) = self.slot.upgrade() {
            let _d = s.delivery.enter();
            if !s.active() {
                return Ok(());
            }
            s.position.as_ref().unwrap().observe_rtp(sample);
            if let Some(hz) = self
                .probe
                .lock()
                .unwrap()
                .observe(sample, crate::stream_receive_stats::monotonic_nanos())
            {
                eprintln!(
                    "Audio: rtp clock type={} audioType={} measuredHz={hz}",
                    s.id.kind, s.id.audio_type
                );
            }
            self.sink.on_audio_rtp(&s.id, format, bytes, sample)?;
        }
        Ok(())
    }
    fn on_stopped(&self) -> Result<()> {
        if let Some(s) = self.slot.upgrade() {
            let _d = s.delivery.enter();
            self.sink.on_audio_stopped(&s.id)?;
        }
        Ok(())
    }
}
impl AirPlayMediaHandler for CarPlayMediaEngine {
    fn on_screen(
        &self,
        session: &AirPlaySession,
        kind: u16,
        stream: &BplistValue,
    ) -> Result<Option<u16>> {
        if session.is_closed() {
            return Ok(None);
        }
        if kind != 110 {
            return Ok(None);
        }
        let Some(key) = Self::key(session, stream, DATASTREAM_OUTPUT_KEY) else {
            return Ok(None);
        };
        let slot = Arc::new(Slot::new(
            kind,
            String::new(),
            None,
            number(stream, "streamConnectionID").unwrap_or(0),
            0,
        ));
        let sk = StreamKey {
            session: session_id(session),
            kind,
            audio_type: String::new(),
        };
        let transport = self.transport(session);
        let diagnostic = self.diagnostic(session);
        let bound_session = self.binding(session).map(|(s, _)| Arc::downgrade(&s));
        let first_frame_session = bound_session.clone();
        let keyframes = Arc::new(KeyframeProbe::default());
        let listener = Arc::new(CurrentScreenListener {
            owner: Some((Arc::downgrade(&self.inner), sk.clone())),
            slot: Arc::downgrade(&slot),
            sink: self.inner.sink.clone(),
            session: bound_session.clone(),
            keyframes: keyframes.clone(),
            received: AtomicBool::new(false),
            first_frame: Box::new(move || {
                if let Some(session) = first_frame_session.as_ref().and_then(Weak::upgrade) {
                    session.startup_milestone("first_video_received");
                }
            }),
        });
        let screen = ScreenStream::with_key(transport, key).with_diagnostic(diagnostic);
        #[cfg(test)]
        {
            *slot.screen_listener.lock().unwrap() = Some(listener.clone());
        }
        let port = match screen.listen(listener) {
            Ok(port) => port,
            Err(error) => {
                screen.close()?;
                return Err(error);
            }
        };
        let mut previous = None;
        let mut displaced = None;
        let installed = {
            let _delivery = self.inner.screen_delivery.enter();
            // Retain the resource even if EOF or a reentrant callback prevents publication.
            *slot.resource.lock().unwrap() = Some(Resource::Screen(screen));
            let policy = self.inner.screen_policy;
            let allowed = || {
                let state = self.inner.state.lock().unwrap();
                !state.closed
                    && !session.is_closed()
                    && slot.active()
                    && !(policy.isolate_sessions
                        && state.retired_screens.contains(&(sk.session, kind)))
            };
            if !allowed() {
                false
            } else {
                previous = self
                    .inner
                    .state
                    .lock()
                    .unwrap()
                    .screen_owners
                    .get(&kind)
                    .cloned();
                if let Some((old_key, old)) = &previous {
                    if old_key.session != sk.session {
                        if policy.isolate_sessions {
                            self.inner
                                .state
                                .lock()
                                .unwrap()
                                .retired_screens
                                .insert((old_key.session, kind));
                        }
                        if policy.reset_on_reconnect {
                            self.remove_screen(old_key, old);
                        } else if policy.isolate_sessions {
                            self.forget_screen(old_key, old);
                        }
                    } else {
                        // Reconfiguration preserves the session's audio and control.
                        self.forget_screen(old_key, old);
                    }
                }
                let changed_owner = self
                    .inner
                    .state
                    .lock()
                    .unwrap()
                    .screen_owners
                    .get(&kind)
                    .is_some_and(|(_, s)| {
                        !previous
                            .as_ref()
                            .is_some_and(|(_, old)| Arc::ptr_eq(s, old))
                    });
                if !allowed() || changed_owner {
                    false
                } else {
                    displaced = self
                        .inner
                        .state
                        .lock()
                        .unwrap()
                        .streams
                        .get(&sk)
                        .cloned()
                        .filter(|s| {
                            !previous
                                .as_ref()
                                .is_some_and(|(_, old)| Arc::ptr_eq(s, old))
                        });
                    if let Some(old) = &displaced {
                        self.forget_screen(&sk, old);
                    }
                    {
                        let mut state = self.inner.state.lock().unwrap();
                        state.screen_owners.insert(kind, (sk.clone(), slot.clone()));
                        state.streams.insert(sk.clone(), slot.clone());
                    }
                    let weak = Arc::downgrade(&self.inner);
                    let output = Arc::downgrade(&slot);
                    let output_key = sk.clone();
                    let diagnostic_session = bound_session.clone();
                    self.inner.sink.set_video_diagnostic_handler(
                        kind,
                        Arc::new(move |message| {
                            let current = weak.upgrade().zip(output.upgrade()).is_some_and(
                                |(inner, slot)| {
                                    let _delivery = inner.screen_delivery.enter();
                                    CarPlayMediaEngine {
                                        inner: inner.clone(),
                                    }
                                    .owns_screen(&output_key, &slot)
                                },
                            );
                            // Controller callbacks must run outside the delivery gate.
                            if current {
                                if let Some(session) =
                                    diagnostic_session.as_ref().and_then(Weak::upgrade)
                                {
                                    if message == "first frame rendered" {
                                        if let Some(token) = session.playback_token() {
                                            session.report_video_frame_rendered(&token);
                                        }
                                    }
                                    session.log_debug(&format!("Video: {message}"));
                                }
                            }
                        }),
                    );
                    if self.owns_screen(&sk, &slot) {
                        let weak = Arc::downgrade(&self.inner);
                        let output = Arc::downgrade(&slot);
                        let output_key = sk.clone();
                        self.inner.sink.set_video_recovery_handler(
                            kind,
                            Arc::new(move || {
                                let current = weak.upgrade().zip(output.upgrade()).is_some_and(
                                    |(inner, slot)| {
                                        let _delivery = inner.screen_delivery.enter();
                                        CarPlayMediaEngine {
                                            inner: inner.clone(),
                                        }
                                        .owns_screen(&output_key, &slot)
                                    },
                                );
                                if current {
                                    if let Some(session) =
                                        bound_session.as_ref().and_then(Weak::upgrade)
                                    {
                                        let sent = session
                                            .send_command(&dict([(
                                                "type",
                                                BplistValue::String("forceKeyFrame".into()),
                                            )]))
                                            .unwrap_or(false);
                                        if let Some(line) =
                                            keyframes.requested(sent, Instant::now())
                                        {
                                            session.log_debug(&line);
                                        }
                                    }
                                }
                            }),
                        );
                    }
                    if self.owns_screen(&sk, &slot) {
                        self.inner.sink.on_screen_stream_active(kind, true);
                    }
                    self.owns_screen(&sk, &slot)
                }
            }
        };
        // Closing sockets can wake workers; publish ownership before closing old resources.
        if let Some((old_key, old)) = previous {
            if self.inner.screen_policy.isolate_sessions
                || self.inner.screen_policy.reset_on_reconnect
                || old_key == sk
            {
                self.retire(old)?;
            }
        }
        if let Some(old) = displaced {
            self.retire(old)?;
        }
        if !installed {
            self.retire(slot)?;
            return Ok(None);
        }
        Ok(Some(port))
    }
    fn on_audio(
        &self,
        session: &AirPlaySession,
        kind: u16,
        stream: &BplistValue,
    ) -> Result<Option<BplistValue>> {
        if session.is_closed() {
            return Ok(None);
        }
        let audio_type = string(stream, "audioType")
            .unwrap_or("default".into())
            .to_lowercase();
        let Some(key) = Self::key(session, stream, DATASTREAM_OUTPUT_KEY) else {
            return Ok(None);
        };
        let format = AudioStreamCodec::from_format_bits(
            number(stream, "audioFormat").unwrap_or(0),
            kind as u8,
            &audio_type,
        )?;
        let slot = Arc::new(Slot::new(
            kind,
            audio_type.clone(),
            Some(format.clone()),
            number(stream, "streamConnectionID").unwrap_or(0),
            number(stream, "audioLatencyMs").unwrap_or(0),
        ));
        if let Some(config) = self.microphone_config(session, kind, stream, &format) {
            self.start_microphone(&slot, config)?;
        }
        if !self.install(
            StreamKey {
                session: session_id(session),
                kind,
                audio_type,
            },
            slot.clone(),
        )? {
            return Ok(None);
        }
        let audio = AudioStream::new(self.transport(session), format.clone(), key.to_vec())?
            .with_diagnostic(self.diagnostic(session));
        let (data, control) = audio.listen(Arc::new(CurrentAudioListener {
            slot: Arc::downgrade(&slot),
            sink: self.inner.sink.clone(),
            probe: Mutex::new(RtpClockProbe::new(format.rtp_clock_rate)),
        }))?;
        let _d = slot.delivery.enter();
        if !slot.active() {
            audio.close()?;
            return Ok(None);
        }
        *slot.resource.lock().unwrap() = Some(Resource::Audio(audio));
        Ok(Some(dict([
            ("type", integer(kind)),
            ("dataPort", integer(data)),
            ("controlPort", integer(control)),
            ("streamConnectionID", integer(slot.connection)),
        ])))
    }
    fn on_buffered_audio(
        &self,
        session: &AirPlaySession,
        stream: &BplistValue,
    ) -> Result<Option<BplistValue>> {
        if session.is_closed() {
            return Ok(None);
        }
        let sid = session_id(session);
        let Some(BplistValue::Data(key)) = get(stream, "shk") else {
            return Ok(None);
        };
        if key.len() != 32 {
            return Ok(None);
        }
        let format = AudioStreamCodec::from_format_bits(
            number(stream, "audioFormat").unwrap_or(0),
            103,
            "media",
        )?;
        if format.codec != AudioCodecKind::AacLc
            || get(stream, "ct").is_some_and(|v| unsigned(v) != Some(4))
            || get(stream, "spf").is_some_and(|v| unsigned(v) != Some(1024))
        {
            return Ok(None);
        }
        let slot = Arc::new(Slot::new(
            103,
            "media".into(),
            Some(format.clone()),
            number(stream, "streamConnectionID").unwrap_or(0),
            0,
        ));
        let buffered = BufferedAudioStream::new(
            self.transport(session),
            format.clone(),
            key.clone(),
            Arc::new(CurrentAudioListener {
                slot: Arc::downgrade(&slot),
                sink: self.inner.sink.clone(),
                probe: Mutex::new(RtpClockProbe::new(format.rtp_clock_rate)),
            }),
        )?
        .with_diagnostic(self.diagnostic(session));
        let previous = {
            let mut s = self.inner.state.lock().unwrap();
            if s.closed || s.buffered_transition.is_some() || s.retired.contains(&sid) {
                drop(s);
                buffered.close()?;
                return Ok(None);
            }
            s.buffered_transition = Some(BufferedTransition {
                session: sid,
                cancelled: false,
            });
            let owner = s.buffered_owner.take();
            if let Some(owner) = owner {
                if owner != sid {
                    s.retired.insert(owner);
                }
                s.streams.remove(&StreamKey {
                    session: owner,
                    kind: 103,
                    audio_type: "media".into(),
                })
            } else {
                None
            }
        };
        if let Some(previous) = previous {
            if let Err(error) = self.retire(previous) {
                self.inner.state.lock().unwrap().buffered_transition = None;
                buffered.close()?;
                return Err(error);
            }
        }
        let port = buffered.port();
        let installed = {
            let mut s = self.inner.state.lock().unwrap();
            let usable = !s.closed
                && !session.is_closed()
                && !s.retired.contains(&sid)
                && s.buffered_transition
                    .as_ref()
                    .is_some_and(|t| t.session == sid && !t.cancelled);
            s.buffered_transition = None;
            if usable {
                *slot.resource.lock().unwrap() = Some(Resource::Buffered(buffered.clone()));
                s.streams.insert(
                    StreamKey {
                        session: sid,
                        kind: 103,
                        audio_type: "media".into(),
                    },
                    slot,
                );
                s.buffered_owner = Some(sid);
            }
            usable
        };
        if !installed {
            buffered.close()?;
            return Ok(None);
        }
        buffered.start()?;
        Ok(Some(dict([
            ("type", integer(103)),
            ("dataPort", integer(port)),
            ("audioBufferSize", integer(8 * 1024 * 1024)),
        ])))
    }
    fn on_buffered_audio_control(
        &self,
        session: &AirPlaySession,
        method: &str,
        body: &BplistValue,
    ) -> Result<Option<BplistValue>> {
        let slot = self
            .inner
            .state
            .lock()
            .unwrap()
            .streams
            .get(&StreamKey {
                session: session_id(session),
                kind: 103,
                audio_type: "media".into(),
            })
            .cloned();
        let buffered = slot.and_then(|s| match &*s.resource.lock().unwrap() {
            Some(Resource::Buffered(b)) => Some(b.clone()),
            _ => None,
        });
        let Some(b) = buffered else {
            return Ok(None);
        };
        match method {
            "SETRATE" | "SETRATEANCHORTIME" => {
                let rate = match get(body, "rate") {
                    Some(BplistValue::Integer(n)) if n == &0.into() => Some(0),
                    Some(BplistValue::Integer(n)) if n == &1.into() => Some(1),
                    Some(BplistValue::Real(v)) if *v == 0.0 => Some(0),
                    Some(BplistValue::Real(v)) if *v == 1.0 => Some(1),
                    _ => None,
                };
                if let Some(rate) = rate {
                    b.set_rate(
                        number(body, "rtpTime").map(|v| v as u32),
                        rate,
                        session.synced_ntp(),
                    )
                } else {
                    Ok(None)
                }
            }
            "GETANCHOR" => b.anchor(),
            "FLUSHBUFFERED" => {
                if let Some(ts) = number(body, "flushUntilTS") {
                    b.flush(Some(ts as u32))?;
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
    fn on_data_stream(
        &self,
        session: &AirPlaySession,
        stream: &BplistValue,
    ) -> Result<Option<BplistValue>> {
        if session.is_closed() {
            return Ok(None);
        }
        if string(stream, "clientTypeUUID").is_none_or(|s| s.to_uppercase() != IAP_DATASTREAM_UUID)
        {
            return Ok(None);
        }
        let Some(shared) = session.shared_secret() else {
            return Ok(None);
        };
        let Some(seed) = number(stream, "seed") else {
            return Ok(None);
        };
        let tunnel = Arc::new(IapTunnel::with_key(
            self.transport(session),
            derive_key(&shared, &seed.to_string(), DATASTREAM_OUTPUT_KEY),
        ));
        let handler = self.inner.state.lock().unwrap().handler.clone();
        let slot = Arc::new(Slot::new(
            130,
            String::new(),
            None,
            number(stream, "streamConnectionID").unwrap_or(0),
            0,
        ));
        let port = if handler.is_some() {
            let Some((session, _)) = self.binding(session) else {
                return Err(invalid(
                    "AirPlay media engine requires bind_session before iAP SETUP",
                ));
            };
            let sid = session_id(&session);
            let bridge = Arc::new(AirPlayIapTunnelStream::new(tunnel, session));
            let port = bridge.listen()?;
            let old = self
                .inner
                .state
                .lock()
                .unwrap()
                .pending
                .insert(sid, bridge.clone());
            if let Some(old) = old {
                old.close()?;
            }
            *slot.resource.lock().unwrap() = Some(Resource::Bridge(bridge));
            port
        } else {
            let port = tunnel.listen(Arc::new(SinkIap {
                sink: self.inner.sink.clone(),
                slot: Arc::downgrade(&slot),
                session: self.binding(session).map(|(s, _)| Arc::downgrade(&s)),
            }))?;
            *slot.resource.lock().unwrap() = Some(Resource::Tunnel(tunnel));
            port
        };
        if !self.install(
            StreamKey {
                session: session_id(session),
                kind: 130,
                audio_type: String::new(),
            },
            slot,
        )? {
            return Ok(None);
        }
        Ok(Some(dict([
            ("type", integer(130)),
            ("streamID", integer(1)),
            ("dataPort", integer(port)),
            (
                "streamConnectionID",
                integer(number(stream, "streamConnectionID").unwrap_or(0)),
            ),
        ])))
    }
    fn on_feedback(&self, session: &AirPlaySession) -> Result<Option<BplistValue>> {
        let slots = self
            .inner
            .state
            .lock()
            .unwrap()
            .streams
            .iter()
            .filter(|(k, v)| k.session == session_id(session) && v.audio.is_some())
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>();
        if slots.is_empty() {
            return Ok(None);
        }
        let mut entries = Vec::new();
        for s in slots {
            let buffered = {
                let r = s.resource.lock().unwrap();
                match &*r {
                    Some(Resource::Buffered(b)) => Some(b.clone()),
                    _ => None,
                }
            };
            if let Some(b) = buffered {
                entries.push(b.feedback(session.synced_ntp(), s.connection)?);
                continue;
            }
            let format = s.audio.as_ref().unwrap();
            let mut entry = vec![
                ("type".into(), integer(s.id.kind)),
                ("sampleRate".into(), integer(format.rtp_clock_rate)),
            ];
            if let Some((sample, origin)) = *s.origin.lock().unwrap() {
                let elapsed =
                    (origin.elapsed().as_secs_f64() - s.latency_ms as f64 / 1000.0).max(0.0);
                let position =
                    sample.wrapping_add((elapsed * format.rtp_clock_rate as f64).round() as u32);
                let now = crate::stream_receive_stats::monotonic_nanos();
                entry.extend([
                    ("streamConnectionID".into(), integer(s.connection)),
                    ("timestamp".into(), integer(session.synced_ntp())),
                    ("timestampRawNs".into(), integer(now)),
                    ("sampleTime".into(), integer(position)),
                ]);
                if let Some(line) = s.position.as_ref().unwrap().on_feedback(
                    s.id.kind,
                    s.connection,
                    position as u64,
                    now,
                ) {
                    eprintln!("{line}");
                }
            }
            entries.push(BplistValue::Dictionary(entry));
        }
        Ok(Some(dict([("streams", BplistValue::Array(entries))])))
    }
    fn on_setup_response_sent(&self, session: &AirPlaySession) -> Result<()> {
        let sid = session_id(session);
        let (slots, pending, handler) = {
            let mut s = self.inner.state.lock().unwrap();
            (
                s.streams
                    .iter()
                    .filter(|(k, _)| k.session == sid)
                    .map(|(_, v)| v.clone())
                    .collect::<Vec<_>>(),
                s.pending.remove(&sid),
                s.handler.clone(),
            )
        };
        for slot in slots {
            let start = slot
                .microphone
                .lock()
                .unwrap()
                .as_ref()
                .map(|m| (m.start.clone(), m.config.audio_type.clone()));
            if let Some((start, audio_type)) = start {
                start.on_setup_response_sent();
                if MicrophoneStart::starts_without_downlink(&audio_type) {
                    let weak = Arc::downgrade(&slot);
                    start.schedule_silent_start(
                        Duration::from_millis(500),
                        Arc::new(move || weak.upgrade().is_some_and(|s| s.active())),
                    )?;
                }
            }
        }
        if let (Some(bridge), Some(handler)) = (pending, handler) {
            if !handler.on_connected(bridge.clone())? {
                bridge.close()?;
                session.close()?;
            }
        }
        Ok(())
    }
    fn on_teardown(&self, session: &AirPlaySession, kind: u16) -> Result<()> {
        let sid = session_id(session);
        if kind == 110 {
            let key = StreamKey {
                session: sid,
                kind,
                audio_type: String::new(),
            };
            let slot = {
                let _delivery = self.inner.screen_delivery.enter();
                let slot = self.inner.state.lock().unwrap().streams.get(&key).cloned();
                if let Some(slot) = &slot {
                    self.remove_screen(&key, slot);
                } else if !self.inner.screen_policy.isolate_sessions {
                    self.inner.sink.on_screen_stream_active(kind, false);
                }
                slot
            };
            if let Some(slot) = slot {
                self.retire(slot)?;
            }
            return Ok(());
        }
        let (slots, owns_barrier) = {
            let mut s = self.inner.state.lock().unwrap();
            let mut owns_barrier = false;
            if kind == 103 {
                if let Some(transition) = s.buffered_transition.as_mut() {
                    if transition.session == sid {
                        transition.cancelled = true;
                    }
                } else {
                    s.buffered_transition = Some(BufferedTransition {
                        session: sid,
                        cancelled: true,
                    });
                    owns_barrier = true;
                }
                if s.buffered_owner == Some(sid) {
                    s.buffered_owner = None;
                }
            }
            if kind == 130 {
                s.pending.remove(&sid);
            }
            let keys = s
                .streams
                .keys()
                .filter(|k| k.session == sid && k.kind == kind)
                .cloned()
                .collect::<Vec<_>>();
            (
                keys.into_iter()
                    .filter_map(|k| s.streams.remove(&k))
                    .collect::<Vec<_>>(),
                owns_barrier,
            )
        };
        let mut failure = None;
        for slot in slots {
            if let Err(error) = self.retire(slot) {
                failure.get_or_insert(error);
            }
        }
        if owns_barrier {
            self.inner.state.lock().unwrap().buffered_transition = None;
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
        Ok(())
    }
    fn on_session_closed(&self, session: &AirPlaySession) -> Result<()> {
        let sid = session_id(session);
        let screens = {
            let _delivery = self.inner.screen_delivery.enter();
            let mut state = self.inner.state.lock().unwrap();
            state.retired.insert(sid);
            state.retired_screens.insert((sid, 110));
            let screens = state
                .streams
                .iter()
                .filter(|(k, _)| k.session == sid && k.kind == 110)
                .map(|(k, s)| (k.clone(), s.clone()))
                .collect::<Vec<_>>();
            drop(state);
            for (key, slot) in &screens {
                self.remove_screen(key, slot);
            }
            screens
        };
        for (_, slot) in screens {
            self.retire(slot)?;
        }
        let kinds = {
            let s = self.inner.state.lock().unwrap();
            s.streams
                .keys()
                .filter(|k| k.session == sid)
                .map(|k| k.kind)
                .collect::<HashSet<_>>()
        };
        for kind in kinds {
            self.on_teardown(session, kind)?;
        }
        let mut s = self.inner.state.lock().unwrap();
        s.sessions.remove(&sid);
        s.pending.remove(&sid);
        Ok(())
    }
}
struct PeerTransport {
    inner: Arc<dyn StreamTransport>,
    peer: std::net::IpAddr,
}
impl StreamTransport for PeerTransport {
    fn listen(&self, _peer: std::net::IpAddr) -> Result<Arc<dyn StreamListener>> {
        self.inner.listen(self.peer)
    }
    fn bind_datagram(&self, peer: std::net::SocketAddr) -> Result<Arc<dyn DatagramTransport>> {
        self.inner.bind_datagram(std::net::SocketAddr::new(
            if peer.ip().is_unspecified() {
                self.peer
            } else {
                peer.ip()
            },
            peer.port(),
        ))
    }
}
struct SinkIap {
    sink: Arc<dyn MediaSink>,
    slot: Weak<Slot>,
    session: Option<Weak<AirPlaySession>>,
}
impl IapTunnelListener for SinkIap {
    fn on_connected(&self, _stream: Arc<dyn DuplexStream>) -> Result<bool> {
        Ok(true)
    }
    fn on_message(&self, bytes: &[u8]) -> Result<()> {
        if let Some(slot) = self.slot.upgrade() {
            let _delivery = slot.delivery.enter();
            if slot.active() {
                return self.sink.on_iap_message(bytes);
            }
        }
        Ok(())
    }
    fn on_closed(&self, _cause: Option<std::io::Error>) {
        if !self.slot.upgrade().is_some_and(|s| s.active()) {
            return;
        }
        if let Some(session) = self.session.as_ref().and_then(Weak::upgrade) {
            let _ = session.close();
        }
    }
}
impl InputEvents for CarPlayMediaEngine {
    fn touch(&self, screen: u16, contacts: &[AirPlayContact]) -> Result<bool> {
        if screen != 110 && screen != 0 {
            return Ok(false);
        }
        let session = self.current_session();
        match session {
            Some(s) => s.send_touch(contacts),
            None => Ok(false),
        }
    }
    fn key(&self, _key: u16, _down: bool) -> Result<bool> {
        Ok(false)
    }
    fn request_keyframe(&self, screen: u16) -> Result<bool> {
        if screen != 110 && screen != 0 {
            return Ok(false);
        }
        match self.current_session() {
            Some(s) => s.send_command(&dict([(
                "type",
                BplistValue::String("forceKeyFrame".into()),
            )])),
            None => Ok(false),
        }
    }
    fn microphone_packet(&self, stream: u16, encoded: &[u8]) -> Result<bool> {
        let slots = {
            let s = self.inner.state.lock().unwrap();
            s.streams
                .values()
                .filter(|v| v.id.kind == stream)
                .cloned()
                .collect::<Vec<_>>()
        };
        for slot in slots {
            let _d = slot.delivery.enter();
            if !slot.active() {
                continue;
            }
            let mut microphone = slot.microphone.lock().unwrap();
            if let Some(m) = microphone.as_mut() {
                if m.transport.is_none() {
                    m.transport = Some(
                        self.inner
                            .transport
                            .bind_datagram(std::net::SocketAddr::new(
                                m.config.host,
                                m.config.port,
                            ))?,
                    );
                }
                let packet = m.packetizer.packetize(encoded)?;
                m.transport.as_ref().unwrap().send(&packet)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
impl CarPlayMediaEngine {
    fn current_session(&self) -> Option<Arc<AirPlaySession>> {
        let s = self.inner.state.lock().unwrap();
        s.streams
            .iter()
            .find(|(k, v)| k.kind == 110 && v.active())
            .and_then(|(k, _)| s.sessions.get(&k.session))
            .and_then(|b| b.session.upgrade())
    }
}
/// Protocol message, field, or framing constant.
pub const TAG: &str = "xcertplay-usb";
/// Protocol message, field, or framing constant.
pub const MICROPHONE_SCHEDULER_IDLE_SECONDS: u64 = 5;
/// Protocol message, field, or framing constant.
pub const STREAM_TYPE_MAIN_SCREEN: u64 = 110;
/// Protocol message, field, or framing constant.
pub const STREAM_TYPE_ALT_SCREEN: u64 = 111;
/// Protocol message, field, or framing constant.
pub const STREAM_TYPE_MAIN_AUDIO: u64 = 100;
/// Protocol message, field, or framing constant.
pub const STREAM_TYPE_DATA: u64 = 130;
/// Protocol message, field, or framing constant.
pub const DATASTREAM_OUTPUT_KEY: &str = "DataStream-Output-Encryption-Key";
/// Protocol message, field, or framing constant.
pub const DATASTREAM_INPUT_KEY: &str = "DataStream-Input-Encryption-Key";
/// Protocol message, field, or framing constant.
pub const IAP_DATASTREAM_UUID: &str = "E9459FD0-BCAD-4C45-820F-1E72447EF2F2";
/// Protocol message, field, or framing constant.
pub const VIDEO_SETTINGS_STREAM_ID: u64 = 2;
/// Protocol message, field, or framing constant.
pub const FIRST_REMOTE_CONTROL_STREAM_ID: u64 = 3;
/// Protocol message, field, or framing constant.
pub const OPUS_24K: u64 = 0x20000000;
/// Protocol message, field, or framing constant.
pub const OPUS_48K: u64 = 0x40000000;

#[cfg(test)]
mod tests {
    use super::*;
    type DiagnosticHandler = Arc<dyn Fn(String) + Send + Sync>;
    #[derive(Default)]
    struct Sink {
        events: Mutex<Vec<String>>,
        invalidate: Mutex<Option<Weak<Slot>>>,
        states: Mutex<Vec<bool>>,
        recovery: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
        diagnostic: Mutex<Option<DiagnosticHandler>>,
        installing: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
        frame_action: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    }
    impl MediaSink for Sink {
        fn on_video_codec(&self, kind: u16, codec: VideoCodec) -> Result<()> {
            self.events
                .lock()
                .unwrap()
                .push(format!("codec {kind} {codec:?}"));
            if let Some(slot) = self
                .invalidate
                .lock()
                .unwrap()
                .as_ref()
                .and_then(Weak::upgrade)
            {
                slot.active.store(false, Ordering::Release);
            }
            Ok(())
        }
        fn on_video_config(&self, kind: u16, bytes: &[u8]) -> Result<()> {
            self.events
                .lock()
                .unwrap()
                .push(format!("config {kind} {bytes:?}"));
            Ok(())
        }
        fn on_video_frame(&self, kind: u16, bytes: &[u8], sender: u64, arrival: u64) -> Result<()> {
            let action = self.frame_action.lock().unwrap().clone();
            if let Some(action) = action {
                action();
            }
            self.events
                .lock()
                .unwrap()
                .push(format!("frame {kind} {bytes:?} {sender} {arrival}"));
            Ok(())
        }
        fn set_video_recovery_handler(&self, _: u16, handler: Arc<dyn Fn() + Send + Sync>) {
            *self.recovery.lock().unwrap() = Some(handler);
        }
        fn set_video_diagnostic_handler(&self, _: u16, handler: Arc<dyn Fn(String) + Send + Sync>) {
            *self.diagnostic.lock().unwrap() = Some(handler);
            let action = self.installing.lock().unwrap().clone();
            if let Some(action) = action {
                action();
            }
        }
        fn on_screen_stream_active(&self, _: u16, active: bool) {
            self.states.lock().unwrap().push(active);
        }
        fn on_audio_started(&self, id: &AudioStreamId, _: &AudioFormat, sample: u32) -> Result<()> {
            self.events
                .lock()
                .unwrap()
                .push(format!("start {} {sample}", id.kind));
            Ok(())
        }
        fn on_audio_rtp(
            &self,
            id: &AudioStreamId,
            _: &AudioFormat,
            _: &[u8],
            sample: u32,
        ) -> Result<()> {
            self.events
                .lock()
                .unwrap()
                .push(format!("rtp {} {sample}", id.kind));
            Ok(())
        }
        fn on_audio_stopped(&self, _: &AudioStreamId) -> Result<()> {
            Ok(())
        }
        fn on_microphone_started(&self, _: &AudioStreamId, _: &MicrophoneConfig) -> Result<()> {
            Ok(())
        }
        fn on_microphone_stopped(&self, _: &AudioStreamId) -> Result<()> {
            Ok(())
        }
        fn on_iap_message(&self, _: &[u8]) -> Result<()> {
            Ok(())
        }
    }
    fn listener(sink: Arc<Sink>) -> (Arc<Slot>, CurrentScreenListener) {
        let slot = Arc::new(Slot::new(110, String::new(), None, 0, 0));
        let listener = CurrentScreenListener {
            owner: None,
            slot: Arc::downgrade(&slot),
            sink,
            session: None,
            keyframes: Arc::default(),
            received: AtomicBool::new(false),
            first_frame: Box::new(|| {}),
        };
        (slot, listener)
    }
    /// One access unit as the iPhone sends it: NAL units with 4-byte big-endian lengths.
    fn length_prefixed(nals: &[&[u8]]) -> Vec<u8> {
        nals.iter()
            .flat_map(|n| {
                (n.len() as u32)
                    .to_be_bytes()
                    .into_iter()
                    .chain(n.iter().copied())
            })
            .collect()
    }
    fn annex_b(nals: &[&[u8]]) -> Vec<u8> {
        let mut payload = length_prefixed(nals);
        ScreenCodec::length_prefixed_to_annex_b(&mut payload).unwrap();
        payload
    }
    const VPS: &[u8] = &[0x40, 0x01, 0x0c];
    const SPS: &[u8] = &[0x42, 0x01, 0x01];
    const PPS: &[u8] = &[0x44, 0x01, 0xc1];
    const IDR_W_RADL: &[u8] = &[0x26, 0x01, 0xaf];
    const TRAIL_R: &[u8] = &[0x02, 0x01, 0xd0];
    #[test]
    fn hevc_irap_types_are_key_frames_after_length_prefix_conversion() {
        for (header, nal_type) in [(0x20, 16), (0x24, 18), (0x26, 19), (0x28, 20), (0x2a, 21)] {
            let types = nal_types(VideoCodec::H265, &annex_b(&[&[header, 0x01, 0x80]]));
            assert_eq!(types, [nal_type]);
            assert!(is_random_access(VideoCodec::H265, nal_type));
        }
        // Parameter sets in the same access unit as the IDR, and an IDR larger than 64 KiB.
        let mut large = IDR_W_RADL.to_vec();
        large.resize(70_000, 0x55);
        let types = nal_types(VideoCodec::H265, &annex_b(&[VPS, SPS, PPS, &large]));
        assert_eq!(types, [32, 33, 34, 19]);
        // A 300-byte first NAL has the length bytes 00 00 01 2c, which is not a start code.
        let mut idr = IDR_W_RADL.to_vec();
        idr.resize(300, 0x55);
        assert_eq!(
            nal_types(VideoCodec::H265, &annex_b(&[&idr, TRAIL_R])),
            [19, 1]
        );
    }
    #[test]
    fn parameter_sets_alone_and_trailing_pictures_are_not_key_frames() {
        let types = nal_types(VideoCodec::H265, &annex_b(&[VPS, SPS, PPS]));
        assert!(!types.iter().any(|&t| is_random_access(VideoCodec::H265, t)));
        let types = nal_types(VideoCodec::H265, &annex_b(&[TRAIL_R]));
        assert!(!types.iter().any(|&t| is_random_access(VideoCodec::H265, t)));
        assert_eq!(
            nal_types(VideoCodec::H264, &annex_b(&[&[0x67, 0x64], &[0x65, 0x88]])),
            [7, 5]
        );
        assert!(!is_random_access(VideoCodec::H264, 1));
        // Annex B input with 3-byte start codes is scanned too.
        assert_eq!(
            nal_types(
                VideoCodec::H265,
                &[0, 0, 1, 0x26, 0x01, 0, 0, 1, 0x02, 0x01]
            ),
            [19, 1]
        );
    }
    #[test]
    fn keyframe_probe_reports_wait_frames_and_requests() {
        let probe = KeyframeProbe::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let key = annex_b(&[VPS, SPS, PPS, IDR_W_RADL]);
        let trail = annex_b(&[TRAIL_R]);
        probe.codec(VideoCodec::H265);
        assert_eq!(probe.frame(&key, at(0)), None, "no request outstanding");
        assert_eq!(
            probe.requested(true, at(100)).unwrap(),
            "Video recovery: requested keyframe sent=true"
        );
        assert_eq!(
            probe.frame(&trail, at(150)).unwrap(),
            "Video recovery: first frame 50 ms after keyframe request is not a key frame nal=[1] bytes=7"
        );
        assert_eq!(probe.frame(&trail, at(200)), None);
        assert_eq!(probe.requested(true, at(4_100)), None);
        assert_eq!(
            probe.config(23, at(4_200)).unwrap(),
            "Video recovery: config (23 bytes) 4100 ms after keyframe request"
        );
        assert_eq!(
            probe.frame(&key, at(4_300)).unwrap(),
            "Video recovery: key frame nal=19 bytes=28 4200 ms after keyframe request (2 frames, 2 requests before it)"
        );
        assert_eq!(probe.frame(&trail, at(4_400)), None);
        assert_eq!(probe.config(23, at(4_500)), None);
    }
    #[test]
    fn keyframe_requests_log_the_first_of_a_gap_then_every_five_seconds() {
        let probe = KeyframeProbe::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        probe.codec(VideoCodec::H265);
        // One request a second through an 11 s gap; the fourth could not be sent.
        let lines: Vec<_> = (0..12)
            .filter_map(|i| probe.requested(i != 3, at(i * 1000)))
            .collect();
        assert_eq!(
            lines,
            [
                "Video recovery: requested keyframe sent=true",
                "Video recovery: requested keyframe sent=true (6 requests in 5000 ms, 1 not sent)",
                "Video recovery: requested keyframe sent=true (11 requests in 10000 ms, 1 not sent)",
            ]
        );
        // The key frame ends the gap, and the next request starts a new one.
        assert!(probe
            .frame(&annex_b(&[IDR_W_RADL]), at(11_500))
            .unwrap()
            .ends_with("(0 frames, 11 requests before it)"));
        assert_eq!(
            probe.requested(false, at(12_000)).unwrap(),
            "Video recovery: requested keyframe sent=false"
        );
        // A gap of unsent requests ends with a key frame as well.
        assert_eq!(probe.frame(&annex_b(&[IDR_W_RADL]), at(12_500)), None);
        assert_eq!(
            probe.requested(true, at(13_000)).unwrap(),
            "Video recovery: requested keyframe sent=true"
        );
    }
    #[test]
    fn screen_listener_feeds_codec_and_frames_to_keyframe_probe() {
        let sink = Arc::new(Sink::default());
        let (_slot, listener) = listener(sink.clone());
        listener.on_codec(VideoCodec::H265);
        listener.keyframes.requested(true, Instant::now());
        listener.on_frame(&annex_b(&[TRAIL_R]), 1, 2).unwrap();
        assert!(listener.keyframes.0.lock().unwrap().pending.is_some());
        listener.on_frame(&annex_b(&[IDR_W_RADL]), 3, 4).unwrap();
        assert!(listener.keyframes.0.lock().unwrap().pending.is_none());
        assert_eq!(sink.events.lock().unwrap().len(), 3);
    }
    fn deliver(listener: &CurrentScreenListener) {
        listener.on_codec(VideoCodec::H265);
        listener.on_config(&[1, 2]).unwrap();
        listener.on_frame(&[3, 4], 77, 88).unwrap();
    }
    #[test]
    fn current_stream_delivers_every_callback_with_type_and_times() {
        let sink = Arc::new(Sink::default());
        let (_slot, listener) = listener(sink.clone());
        deliver(&listener);
        assert_eq!(
            *sink.events.lock().unwrap(),
            [
                "codec 110 H265",
                "config 110 [1, 2]",
                "frame 110 [3, 4] 77 88"
            ]
        );
    }
    #[test]
    fn first_frame_milestone_precedes_delivery_once_per_current_stream() {
        let sink = Arc::new(Sink::default());
        let (slot, mut listener) = listener(sink.clone());
        let events = sink.clone();
        listener.first_frame = Box::new(move || {
            events.events.lock().unwrap().push("received".into());
        });
        listener.on_codec(VideoCodec::H265);
        listener.on_config(&[1, 2]).unwrap();
        assert!(!listener.received.load(Ordering::Acquire));
        listener.on_frame(&[3], 1, 2).unwrap();
        listener.on_frame(&[4], 3, 4).unwrap();
        assert_eq!(
            *sink.events.lock().unwrap(),
            [
                "codec 110 H265",
                "config 110 [1, 2]",
                "received",
                "frame 110 [3] 1 2",
                "frame 110 [4] 3 4",
            ]
        );
        slot.active.store(false, Ordering::Release);
        let (_successor, mut successor) = self::listener(sink.clone());
        let events = sink.clone();
        successor.first_frame = Box::new(move || {
            events
                .events
                .lock()
                .unwrap()
                .push("successor received".into());
        });
        listener.on_frame(&[5], 5, 6).unwrap();
        successor.on_frame(&[6], 7, 8).unwrap();
        assert_eq!(
            &sink.events.lock().unwrap()[5..],
            ["successor received", "frame 110 [6] 7 8",]
        );
    }
    #[test]
    fn replaced_stream_delivers_nothing() {
        let sink = Arc::new(Sink::default());
        let (slot, listener) = listener(sink.clone());
        slot.active.store(false, Ordering::Release);
        deliver(&listener);
        assert!(sink.events.lock().unwrap().is_empty());
    }
    #[test]
    fn currency_is_checked_at_each_delivery() {
        let sink = Arc::new(Sink::default());
        let (slot, listener) = listener(sink.clone());
        *sink.invalidate.lock().unwrap() = Some(Arc::downgrade(&slot));
        deliver(&listener);
        assert_eq!(*sink.events.lock().unwrap(), ["codec 110 H265"]);
    }
    #[test]
    fn late_audio_after_retirement_cannot_restart_sink() {
        let sink = Arc::new(Sink::default());
        let format = AudioStreamCodec::from_format_bits(0x20000000, 100, "telephony").unwrap();
        let slot = Arc::new(Slot::new(
            100,
            "telephony".into(),
            Some(format.clone()),
            0,
            0,
        ));
        let listener = CurrentAudioListener {
            slot: Arc::downgrade(&slot),
            sink: sink.clone(),
            probe: Mutex::new(RtpClockProbe::new(24000)),
        };
        slot.active.store(false, Ordering::Release);
        listener.on_started(&format, 42).unwrap();
        listener.on_rtp(&format, b"encoded opus", 522).unwrap();
        assert!(sink.events.lock().unwrap().is_empty());
        assert!(slot.origin.lock().unwrap().is_none());
    }
    #[derive(Default)]
    struct Observer {
        events: Mutex<Vec<String>>,
        close_at_first: AtomicBool,
        callback: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    }
    impl AirPlaySessionListener for Observer {
        fn on_startup_milestone(&self, session: &AirPlaySession, name: &str) {
            self.events.lock().unwrap().push(name.into());
            if self.close_at_first.load(Ordering::Acquire) {
                session.close().unwrap();
            }
        }
        fn on_debug_log(&self, line: &str) {
            self.events.lock().unwrap().push(line.into());
            let callback = self.callback.lock().unwrap().clone();
            if let Some(callback) = callback {
                callback();
            }
        }
        fn on_video_frame_rendered(&self, _: &AirPlaySession) {
            self.events.lock().unwrap().push("rendered".into());
        }
    }
    struct Peer(Arc<AirPlaySession>);
    impl std::ops::Deref for Peer {
        type Target = AirPlaySession;
        fn deref(&self) -> &AirPlaySession {
            &self.0
        }
    }
    impl Drop for Peer {
        fn drop(&mut self) {
            self.0.close().unwrap();
        }
    }
    #[derive(Default)]
    struct Server {
        closed: Mutex<bool>,
        changed: std::sync::Condvar,
    }
    impl StreamListener for Server {
        fn port(&self) -> Result<u16> {
            Ok(1234)
        }
        fn accept(&self, timeout: Duration) -> Result<Option<Arc<dyn DuplexStream>>> {
            let closed = self.closed.lock().unwrap();
            if !*closed {
                drop(self.changed.wait_timeout(closed, timeout).unwrap());
            }
            Ok(None)
        }
        fn close(&self) -> Result<()> {
            *self.closed.lock().unwrap() = true;
            self.changed.notify_all();
            Ok(())
        }
    }
    struct Transport;
    impl StreamTransport for Transport {
        fn listen(&self, _: std::net::IpAddr) -> Result<Arc<dyn StreamListener>> {
            Ok(Arc::new(Server::default()))
        }
        fn bind_datagram(&self, _: std::net::SocketAddr) -> Result<Arc<dyn DatagramTransport>> {
            Err(invalid("unused datagram"))
        }
    }
    struct Harness {
        engine: CarPlayMediaEngine,
        sink: Arc<Sink>,
    }
    impl Harness {
        fn new(policy: ScreenSessionPolicy) -> Self {
            let sink = Arc::new(Sink::default());
            let engine = CarPlayMediaEngine::with_screen_session_policy(
                sink.clone(),
                Arc::new(Transport),
                false,
                policy,
            );
            Self { engine, sink }
        }
        fn peer(&self) -> Peer {
            self.observed_peer(Arc::new(Observer::default()))
        }
        fn observed_peer(&self, observer: Arc<Observer>) -> Peer {
            let session = crate::air_play_session::event_tests::media_session(
                Arc::new(self.engine.clone()),
                observer,
            );
            self.engine
                .bind_session(&session, "127.0.0.1".parse().unwrap());
            Peer(session)
        }
        fn screen(&self, peer: &AirPlaySession, id: u64) -> Arc<dyn ScreenListener> {
            assert!(self
                .engine
                .on_screen(peer, 110, &screen_setup(id))
                .unwrap()
                .is_some());
            let key = StreamKey {
                session: session_id(peer),
                kind: 110,
                audio_type: String::new(),
            };
            let slot = self.engine.inner.state.lock().unwrap().streams[&key].clone();
            let listener = slot
                .screen_listener
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .clone();
            listener
        }
        fn states(&self) -> Vec<bool> {
            self.sink.states.lock().unwrap().clone()
        }
        fn active(&self) -> bool {
            self.states().last() == Some(&true)
        }
        fn events(&self) -> Vec<String> {
            self.sink.events.lock().unwrap().clone()
        }
    }
    impl Default for Harness {
        fn default() -> Self {
            Self::new(ScreenSessionPolicy::default())
        }
    }
    impl Drop for Harness {
        fn drop(&mut self) {
            self.engine.close().unwrap();
        }
    }
    fn screen_setup(id: u64) -> BplistValue {
        dict([("streamConnectionID", integer(id))])
    }
    fn deliver_screen(listener: &dyn ScreenListener) {
        listener.on_codec(VideoCodec::H265);
        listener.on_config(&[1, 2]).unwrap();
        listener.on_frame(&[3], 0, 0).unwrap();
        listener.on_frame(&[4], 77, 88).unwrap();
    }
    fn policy(bits: u8) -> ScreenSessionPolicy {
        ScreenSessionPolicy {
            isolate_sessions: bits & 1 != 0,
            reset_on_reconnect: bits & 2 != 0,
            serialize_delivery: bits & 4 != 0,
        }
    }
    #[test]
    fn closing_old_session_must_not_deactivate_the_new_main_screen() {
        let h = Harness::default();
        let old = h.peer();
        let fresh = h.peer();
        h.screen(&old, 1);
        let current = h.screen(&fresh, 2);
        current.on_frame(&[2], 0, 0).unwrap();
        let states = h.states();
        old.close().unwrap();
        assert!(!fresh.is_closed());
        assert!(h.active());
        assert_eq!(h.states(), states);
        assert_eq!(h.events(), ["frame 110 [2] 0 0"]);
    }
    #[test]
    fn old_session_frames_must_not_overwrite_the_new_main_screen() {
        let h = Harness::default();
        let old = h.peer();
        let fresh = h.peer();
        let previous = h.screen(&old, 1);
        let current = h.screen(&fresh, 2);
        current.on_frame(&[2], 0, 0).unwrap();
        previous.on_frame(&[1], 0, 0).unwrap();
        assert_eq!(h.events(), ["frame 110 [2] 0 0"]);
    }
    #[test]
    fn closing_the_only_screen_must_still_deactivate_it() {
        let h = Harness::default();
        let peer = h.peer();
        h.screen(&peer, 1);
        assert!(h.active());
        peer.close().unwrap();
        assert!(!h.active());
        assert_eq!(h.states(), [true, false]);
    }
    #[test]
    fn closing_an_old_session_without_a_screen_must_leave_the_new_screen_active() {
        let h = Harness::default();
        let old = h.peer();
        let fresh = h.peer();
        h.screen(&fresh, 2);
        old.close().unwrap();
        assert_eq!(h.states(), [true]);
    }
    #[test]
    fn old_teardown_and_delayed_end_cannot_stop_the_replacement() {
        let h = Harness::default();
        let old = h.peer();
        let fresh = h.peer();
        let previous = h.screen(&old, 1);
        let current = h.screen(&fresh, 2);
        let states = h.states();
        h.engine.on_teardown(&old, 110).unwrap();
        previous.on_closed(None);
        assert!(h.active());
        assert!(!fresh.is_closed());
        assert!(!old.is_closed());
        assert_eq!(h.states(), states);
        current.on_frame(&[2], 0, 0).unwrap();
        assert_eq!(h.events(), ["frame 110 [2] 0 0"]);
    }
    #[test]
    fn replaced_stream_end_cannot_close_the_same_control_session() {
        let h = Harness::default();
        let peer = h.peer();
        let previous = h.screen(&peer, 1);
        let current = h.screen(&peer, 2);
        previous.on_closed(None);
        assert!(!peer.is_closed());
        assert!(h.active());
        current.on_frame(&[2], 0, 0).unwrap();
        assert_eq!(h.events(), ["frame 110 [2] 0 0"]);
        assert_eq!(h.states(), [true, true]);
    }
    #[test]
    fn every_old_delivery_and_saved_callback_is_fenced() {
        let h = Harness::default();
        let observer = Arc::new(Observer::default());
        let old = h.observed_peer(observer.clone());
        let fresh = h.peer();
        let previous = h.screen(&old, 1);
        let recovery = h.sink.recovery.lock().unwrap().as_ref().unwrap().clone();
        let diagnostic = h.sink.diagnostic.lock().unwrap().as_ref().unwrap().clone();
        h.screen(&fresh, 2);
        observer.events.lock().unwrap().clear();
        deliver_screen(previous.as_ref());
        recovery();
        diagnostic("first frame rendered".into());
        assert!(h.events().is_empty());
        assert!(observer.events.lock().unwrap().is_empty());
    }
    #[test]
    fn a_retired_session_cannot_reclaim_the_new_main_screen() {
        let h = Harness::default();
        let old = h.peer();
        let fresh = h.peer();
        h.screen(&old, 1);
        let current = h.screen(&fresh, 2);
        assert!(h
            .engine
            .on_screen(&old, 110, &screen_setup(3))
            .unwrap()
            .is_none());
        current.on_frame(&[2], 0, 0).unwrap();
        assert!(h.active());
        assert_eq!(h.events(), ["frame 110 [2] 0 0"]);
    }
    #[test]
    fn current_teardown_can_reopen_on_the_same_control() {
        let h = Harness::default();
        let peer = h.peer();
        let previous = h.screen(&peer, 1);
        h.engine.on_teardown(&peer, 110).unwrap();
        assert!(!h.active());
        h.screen(&peer, 2);
        previous.on_closed(None);
        assert!(h.active());
        assert!(!peer.is_closed());
        assert_eq!(h.states(), [true, false, true]);
    }
    #[test]
    fn closed_session_cannot_start_or_install_callbacks() {
        let h = Harness::default();
        let peer = h.peer();
        peer.close().unwrap();
        assert!(h
            .engine
            .on_screen(&peer, 110, &screen_setup(1))
            .unwrap()
            .is_none());
        assert!(h.states().is_empty());
        assert!(h.sink.recovery.lock().unwrap().is_none());
        assert!(h.sink.diagnostic.lock().unwrap().is_none());
    }
    #[test]
    fn reentrant_close_during_callback_installation_cannot_publish_a_live_screen() {
        let h = Harness::default();
        let peer = h.peer();
        let session = Arc::downgrade(&peer.0);
        *h.sink.installing.lock().unwrap() = Some(Arc::new(move || {
            session.upgrade().unwrap().close().unwrap();
        }));
        assert!(h
            .engine
            .on_screen(&peer, 110, &screen_setup(1))
            .unwrap()
            .is_none());
        assert!(!h.states().contains(&true));
        assert!(peer.is_closed());
        assert!(h.sink.recovery.lock().unwrap().is_none());
    }
    #[test]
    fn reentrant_close_at_first_frame_cannot_deliver_after_the_stop() {
        for bits in 0..8 {
            let h = Harness::new(policy(bits));
            let observer = Arc::new(Observer::default());
            observer.close_at_first.store(true, Ordering::Release);
            let peer = h.observed_peer(observer);
            let current = h.screen(&peer, 1);
            current.on_frame(&[1], 0, 0).unwrap();
            assert!(!h.active());
            assert!(h.events().is_empty());
            assert!(peer.is_closed());
        }
    }
    #[test]
    fn all_policy_combinations_have_real_enabled_and_disabled_behavior() {
        for bits in 0..8 {
            let policy = policy(bits);
            let h = Harness::new(policy);
            let old = h.peer();
            let fresh = h.peer();
            let previous = h.screen(&old, 1);
            let current = h.screen(&fresh, 2);
            previous.on_frame(&[1], 0, 0).unwrap();
            current.on_frame(&[2], 0, 0).unwrap();
            assert_eq!(
                h.events(),
                if policy.isolate_sessions || policy.reset_on_reconnect {
                    vec!["frame 110 [2] 0 0"]
                } else {
                    vec!["frame 110 [1] 0 0", "frame 110 [2] 0 0"]
                },
                "{policy:?}"
            );
            assert_eq!(
                h.states(),
                if policy.reset_on_reconnect {
                    vec![true, false, true]
                } else {
                    vec![true, true]
                },
                "{policy:?}"
            );
            old.close().unwrap();
            assert_eq!(
                h.active(),
                policy.isolate_sessions || policy.reset_on_reconnect,
                "{policy:?}"
            );
            assert!(!fresh.is_closed());
        }
    }
    #[test]
    fn isolation_option_controls_retired_setup_and_unknown_teardown() {
        for isolate_sessions in [false, true] {
            let h = Harness::new(ScreenSessionPolicy {
                isolate_sessions,
                ..ScreenSessionPolicy::default()
            });
            let old = h.peer();
            let fresh = h.peer();
            h.screen(&old, 1);
            h.screen(&fresh, 2);
            h.engine.on_teardown(&old, 110).unwrap();
            assert_eq!(h.active(), isolate_sessions);
            assert_eq!(
                h.engine
                    .on_screen(&old, 110, &screen_setup(3))
                    .unwrap()
                    .is_some(),
                !isolate_sessions
            );
        }
    }
    #[test]
    fn same_control_reconfiguration_preserves_the_session_under_every_policy() {
        for bits in 0..8 {
            let h = Harness::new(policy(bits));
            let peer = h.peer();
            h.screen(&peer, 1);
            let format = AudioStreamCodec::from_format_bits(0x20000000, 100, "telephony").unwrap();
            let audio = Arc::new(Slot::new(
                100,
                "telephony".into(),
                Some(format.clone()),
                9,
                0,
            ));
            let key = StreamKey {
                session: session_id(&peer),
                kind: 100,
                audio_type: "telephony".into(),
            };
            h.engine.install(key.clone(), audio.clone()).unwrap();
            let listener = CurrentAudioListener {
                slot: Arc::downgrade(&audio),
                sink: h.sink.clone(),
                probe: Mutex::new(RtpClockProbe::new(24000)),
            };
            listener.on_started(&format, 42).unwrap();
            h.screen(&peer, 2);
            assert!(audio.active());
            assert!(Arc::ptr_eq(
                &h.engine.inner.state.lock().unwrap().streams[&key],
                &audio
            ));
            listener.on_rtp(&format, b"opus", 522).unwrap();
            assert_eq!(h.events(), ["start 100 42", "rtp 100 522"]);
            assert!(Arc::ptr_eq(&h.engine.current_session().unwrap(), &peer.0));
            assert_eq!(h.states(), [true, true], "{:?}", policy(bits));
            assert!(!peer.is_closed());
        }
    }
    #[test]
    fn on_screen_stops_the_replaced_streams_listener_for_the_same_key() {
        let h = Harness::default();
        let peer = h.peer();
        let first = h.screen(&peer, 1);
        deliver_screen(first.as_ref());
        let events = h.events();
        let second = h.screen(&peer, 2);
        assert!(!Arc::ptr_eq(&first, &second));
        deliver_screen(first.as_ref());
        assert_eq!(h.events(), events);
        deliver_screen(second.as_ref());
        assert_eq!(h.events(), [events.clone(), events].concat());
    }
    #[test]
    fn unknown_screen_teardown_does_not_stop_the_sink() {
        let h = Harness::default();
        let peer = h.peer();
        h.engine.on_teardown(&peer, 110).unwrap();
        h.engine.on_teardown(&peer, 100).unwrap();
        peer.close().unwrap();
        assert!(h.states().is_empty());
    }
    #[test]
    fn session_close_reports_all_supported_screen_streams_inactive() {
        let h = Harness::default();
        let peer = h.peer();
        h.screen(&peer, 1);
        let key = StreamKey {
            session: session_id(&peer),
            kind: 100,
            audio_type: String::new(),
        };
        h.engine
            .install(key, Arc::new(Slot::new(100, String::new(), None, 0, 0)))
            .unwrap();
        h.sink.states.lock().unwrap().clear();
        h.engine.on_session_closed(&peer).unwrap();
        peer.close().unwrap();
        assert_eq!(h.states(), [false]);
        assert!(h.engine.inner.state.lock().unwrap().streams.is_empty());
    }
    struct ReleaseFrame(Arc<(Mutex<bool>, std::sync::Condvar)>);
    impl Drop for ReleaseFrame {
        fn drop(&mut self) {
            *self.0 .0.lock().unwrap() = true;
            self.0 .1.notify_all();
        }
    }
    fn handoff_during_frame(serialize_delivery: bool, actual_engine: bool) -> Vec<String> {
        use std::sync::mpsc;
        let h = Harness::new(ScreenSessionPolicy {
            serialize_delivery,
            ..ScreenSessionPolicy::default()
        });
        let old = h.peer();
        let fresh = h.peer();
        let previous = h.screen(&old, 1);
        let (entered_tx, entered_rx) = mpsc::channel();
        let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let release_guard = ReleaseFrame(release.clone());
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = events.clone();
        *h.sink.frame_action.lock().unwrap() = Some(Arc::new(move || {
            log.lock().unwrap().push("old-frame-start".into());
            entered_tx.send(()).unwrap();
            let released = release.0.lock().unwrap();
            let (released, _) = release
                .1
                .wait_timeout_while(released, Duration::from_secs(5), |v| !*v)
                .unwrap();
            assert!(*released, "frame was not released");
            log.lock().unwrap().push("old-frame-end".into());
        }));
        let listener = previous.clone();
        let delivery = std::thread::spawn(move || listener.on_frame(&[1], 0, 0).unwrap());
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (replacing_tx, replacing_rx) = mpsc::channel();
        let (replaced_tx, replaced_rx) = mpsc::channel();
        let engine = h.engine.clone();
        let session = fresh.0.clone();
        let log = events.clone();
        let replacement = std::thread::spawn(move || {
            replacing_tx.send(()).unwrap();
            if actual_engine {
                assert!(engine
                    .on_screen(&session, 110, &screen_setup(2))
                    .unwrap()
                    .is_some());
            } else {
                let _gate = engine.inner.screen_delivery.enter();
                let (key, slot) = engine.inner.state.lock().unwrap().screen_owners[&110].clone();
                engine.forget_screen(&key, &slot);
                drop(_gate);
                engine.retire(slot).unwrap();
            }
            log.lock().unwrap().push("replacement".into());
            replaced_tx.send(()).unwrap();
        });
        replacing_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if serialize_delivery {
            assert_eq!(
                replaced_rx.recv_timeout(Duration::from_millis(100)),
                Err(mpsc::RecvTimeoutError::Timeout)
            );
        } else {
            replaced_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        drop(release_guard);
        delivery.join().unwrap();
        replacement.join().unwrap();
        *h.sink.frame_action.lock().unwrap() = None;
        let before = h.events();
        previous.on_frame(&[2], 0, 0).unwrap();
        assert_eq!(h.events(), before);
        let result = events.lock().unwrap().clone();
        result
    }
    #[test]
    fn replacement_waits_until_an_in_flight_delivery_finishes() {
        assert_eq!(
            handoff_during_frame(true, false),
            ["old-frame-start", "old-frame-end", "replacement"]
        );
    }
    #[test]
    fn disabled_serialization_allows_handoff_during_an_in_flight_frame() {
        assert_eq!(
            handoff_during_frame(false, false),
            ["old-frame-start", "replacement", "old-frame-end"]
        );
    }
    #[test]
    fn serialization_policy_controls_actual_engine_handoff_during_a_frame() {
        assert_eq!(
            handoff_during_frame(true, true),
            ["old-frame-start", "old-frame-end", "replacement"]
        );
        assert_eq!(
            handoff_during_frame(false, true),
            ["old-frame-start", "replacement", "old-frame-end"]
        );
    }
    #[test]
    fn current_screen_eof_stops_once_closes_and_retires_its_session() {
        let h = Harness::default();
        let peer = h.peer();
        let current = h.screen(&peer, 1);
        current.on_closed(None);
        current.on_closed(None);
        assert_eq!(h.states(), [true, false]);
        assert!(peer.is_closed());
        assert!(h
            .engine
            .on_screen(&peer, 110, &screen_setup(2))
            .unwrap()
            .is_none());
        assert!(h
            .engine
            .inner
            .state
            .lock()
            .unwrap()
            .retired_screens
            .contains(&(session_id(&peer), 110)));
    }
    #[test]
    fn default_policy_is_captured_per_engine_from_the_provider() {
        struct Provider(ScreenSessionPolicy);
        impl ScreenSessionPolicyProvider for Provider {
            fn screen_session_policy(&self) -> ScreenSessionPolicy {
                self.0
            }
        }
        assert_eq!(ScreenSessionPolicy::default(), policy(7));
        let mut provider = Provider(policy(0));
        let h = Harness::new(provider.screen_session_policy());
        provider.0 = policy(7);
        assert_eq!(h.engine.inner.screen_policy, policy(0));
        let default = CarPlayMediaEngine::new(h.sink.clone(), Arc::new(Transport), false);
        assert_eq!(
            default.inner.screen_policy,
            provider.screen_session_policy()
        );
        default.close().unwrap();
    }
    #[test]
    fn current_saved_handlers_run_controller_callbacks_outside_the_delivery_gate() {
        let h = Harness::default();
        let observer = Arc::new(Observer::default());
        let peer = h.observed_peer(observer.clone());
        h.screen(&peer, 1);
        let engine = h.engine.clone();
        *observer.callback.lock().unwrap() = Some(Arc::new(move || {
            let inner = engine.inner.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || {
                let _gate = inner.screen_delivery.enter();
                tx.send(()).unwrap();
            });
            rx.recv_timeout(Duration::from_secs(5))
                .expect("controller callback held the delivery gate");
            thread.join().unwrap();
        }));
        let recovery = h.sink.recovery.lock().unwrap().as_ref().unwrap().clone();
        let diagnostic = h.sink.diagnostic.lock().unwrap().as_ref().unwrap().clone();
        recovery();
        diagnostic("first frame rendered".into());
        let observed = observer.events.lock().unwrap();
        assert!(observed
            .iter()
            .any(|v| v.starts_with("Video recovery: requested keyframe")));
        assert!(observed.iter().any(|v| v == "rendered"));
        assert!(observed.iter().any(|v| v == "Video: first frame rendered"));
    }
    #[test]
    fn isolation_option_controls_delayed_old_eof_session_close() {
        for isolate_sessions in [false, true] {
            let h = Harness::new(ScreenSessionPolicy {
                isolate_sessions,
                ..ScreenSessionPolicy::default()
            });
            let old = h.peer();
            let fresh = h.peer();
            let previous = h.screen(&old, 1);
            h.screen(&fresh, 2);
            let states = h.states();
            previous.on_closed(None);
            assert_eq!(old.is_closed(), !isolate_sessions);
            assert!(!fresh.is_closed());
            assert_eq!(h.states(), states);
        }
    }
    #[test]
    fn rebinding_a_retired_session_cannot_restore_screen_ownership() {
        let h = Harness::default();
        let old = h.peer();
        let fresh = h.peer();
        h.screen(&old, 1);
        h.screen(&fresh, 2);
        h.engine.bind_session(&old.0, "127.0.0.1".parse().unwrap());
        assert!(h
            .engine
            .on_screen(&old, 110, &screen_setup(3))
            .unwrap()
            .is_none());
        assert!(h.active());
    }
    #[test]
    fn engine_close_stops_the_current_screen_once() {
        let h = Harness::default();
        let peer = h.peer();
        let previous = h.screen(&peer, 1);
        h.engine.close().unwrap();
        h.engine.close().unwrap();
        previous.on_closed(None);
        deliver_screen(previous.as_ref());
        assert_eq!(h.states(), [true, false]);
        assert!(h.events().is_empty());
        assert!(h
            .engine
            .on_screen(&peer, 110, &screen_setup(2))
            .unwrap()
            .is_none());
    }
    #[test]
    fn nonisolated_session_can_replace_its_retained_screen_key() {
        let h = Harness::new(policy(0));
        let old = h.peer();
        let fresh = h.peer();
        let previous = h.screen(&old, 1);
        let other = h.screen(&fresh, 2);
        let current = h.screen(&old, 3);
        previous.on_frame(&[1], 0, 0).unwrap();
        other.on_frame(&[2], 0, 0).unwrap();
        current.on_frame(&[3], 0, 0).unwrap();
        assert_eq!(h.events(), ["frame 110 [2] 0 0", "frame 110 [3] 0 0"]);
        assert_eq!(h.states(), [true, true, true]);
        assert!(!old.is_closed());
        assert!(!fresh.is_closed());
    }
}
