// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use crate::*;
use carplay_airplay::*;
use oxp_wire::{Header, Kind, Message};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
struct BridgeState {
    session: u64,
    codec: Option<VideoCodec>,
    format: Option<oxp_core::model::VideoFormat>,
    sequence: u32,
    audio: BTreeMap<u16, (AudioStreamId, Arc<AudioPolicyRun>)>,
    tails: BTreeMap<u16, Arc<AudioPolicyRun>>,
}
pub struct StreamBridgeSink {
    clock: Option<Arc<dyn Fn() -> u64 + Send + Sync>>,
    input: Arc<DiPlayInput>,
    delegate: Arc<dyn MediaSink>,
    events: Arc<JsonEvents>,
    state: Mutex<BridgeState>,
}
impl StreamBridgeSink {
    pub fn new(
        input: Arc<DiPlayInput>,
        delegate: Arc<dyn MediaSink>,
        control: Arc<dyn SessionControl>,
        events: Arc<JsonEvents>,
    ) -> Self {
        let session = input.begin(control).expect("begin session");
        {
            let mut f = events.facts.lock().unwrap();
            f.picture_on_car = false;
            f.session = Some(session);
        }
        Self {
            clock: None,
            input,
            delegate,
            events,
            state: Mutex::new(BridgeState {
                session,
                codec: None,
                format: None,
                sequence: 0,
                audio: BTreeMap::new(),
                tails: BTreeMap::new(),
            }),
        }
    }
    pub fn with_arrival_clock(mut self, clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        self.clock = Some(clock);
        self
    }
    pub fn end(&self) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        for a in s.tails.values() {
            let _ = a.cancel();
        }
        s.audio.clear();
        s.tails.clear();
        s.codec = None;
        s.format = None;
        self.input.end(s.session)
    }
    fn resume(&self) {
        let mut s = self.state.lock().unwrap();
        let next = self.input.resume(s.session);
        if next != s.session {
            s.session = next;
            s.codec = None;
            s.format = None;
            s.audio.clear();
            for a in s.tails.values() {
                let _ = a.cancel();
            }
            s.tails.clear();
        }
    }
    fn dropped(&self, event: &str, e: impl std::fmt::Display) {
        self.events
            .emit(event, &serde_json::json!({"reason":e.to_string()}));
    }
}
impl MediaSink for StreamBridgeSink {
    fn on_video_codec(&self, kind: u16, codec: VideoCodec) -> Result<()> {
        if kind == 110 {
            let mut s = self.state.lock().unwrap();
            if s.codec != Some(codec) {
                s.codec = Some(codec);
                s.format = None;
                self.input.publish(
                    s.session,
                    "1-video-0",
                    Message::Json(
                        serde_json::json!({"type":"video-format","screen":0,"format":null}),
                    ),
                );
            }
        }
        self.delegate.on_video_codec(kind, codec)
    }
    fn on_video_config(&self, kind: u16, data: &[u8]) -> Result<()> {
        if kind == 110 {
            let mut s = self.state.lock().unwrap();
            let result = (|| -> Result<_> {
                let codec = match s
                    .codec
                    .ok_or_else(|| std::io::Error::other("Video config arrived before codec"))?
                {
                    VideoCodec::H264 => oxp_core::model::VideoCodec::H264,
                    VideoCodec::H265 => oxp_core::model::VideoCodec::H265,
                };
                oxp_core::model::VideoFormat::create(0, codec, Arc::from(data), None)
                    .map_err(std::io::Error::other)
            })();
            match result {
                Ok(format) => {
                    let codec = if format.codec == oxp_core::model::VideoCodec::H264 {
                        "H264"
                    } else {
                        "H265"
                    };
                    self.input.publish(s.session,"1-video-0",Message::Json(serde_json::json!({"type":"video-format","screen":0,"format":{"codec":codec,"codecString":format.codec_string,"config":base64(data),"width":format.width,"height":format.height,"fpsHint":self.input.applied_frame_rate()}})));
                    s.format = Some(format);
                    let mut input = self.input.state.lock().unwrap();
                    if input.current == Some(s.session) && !input.active {
                        input.active = true;
                        DiPlayInput::publish_locked(
                            &mut input,
                            "0-session",
                            Message::Json(
                                serde_json::json!({"type":"session","session":{"id":s.session.to_string(),"phase":"ACTIVE"}}),
                            ),
                        );
                        drop(input);
                        self.events
                            .milestone(crate::boot_timing::Milestone::SessionActive);
                    }
                }
                Err(e) => {
                    s.format = None;
                    self.input.publish(
                        s.session,
                        "1-video-0",
                        Message::Json(
                            serde_json::json!({"type":"video-format","screen":0,"format":null}),
                        ),
                    );
                    self.dropped("video-dropped", e);
                }
            }
        }
        self.delegate.on_video_config(kind, data)
    }
    fn on_video_frame(&self, kind: u16, data: &[u8], sender: u64, arrival: u64) -> Result<()> {
        let frame_arrival = self.clock.as_ref().map(|now| now()).unwrap_or(arrival);
        if kind == 110 {
            let mut s = self.state.lock().unwrap();
            if let Some(format) = &s.format {
                match annex_b_to_avcc(data) {
                    Ok(payload) => {
                        let h264 = format.codec == oxp_core::model::VideoCodec::H264;
                        let mut p = 0;
                        let mut key = false;
                        while p + 4 < payload.len() {
                            let n =
                                u32::from_be_bytes(payload[p..p + 4].try_into().unwrap()) as usize;
                            p += 4;
                            if n == 0 || p + n > payload.len() {
                                break;
                            }
                            let t = if h264 {
                                payload[p] & 31
                            } else {
                                payload[p] >> 1 & 63
                            };
                            key |= if h264 { t == 5 } else { (16..=21).contains(&t) };
                            p += n;
                        }
                        let m = Message::Binary {
                            header: Header {
                                kind: Kind::Video,
                                stream: 0,
                                flags: if key { oxp_wire::KEYFRAME } else { 0 },
                                sequence: s.sequence,
                                pts_us: (frame_arrival / 1000) as i64,
                                sender_clock_us: (frame_arrival / 1000) as i64,
                            },
                            payload,
                        };
                        s.sequence = s.sequence.wrapping_add(1);
                        {
                            let mut input = self.input.state.lock().unwrap();
                            if input.current == Some(s.session) {
                                input.playback =
                                    input.control.as_ref().and_then(|c| c.playback_token());
                            }
                        }
                        self.input.publish(s.session, "", m);
                    }
                    Err(e) => self.dropped("video-dropped", e),
                }
            }
        }
        self.delegate.on_video_frame(kind, data, sender, arrival)
    }
    fn set_video_recovery_handler(&self, kind: u16, handler: Arc<dyn Fn() + Send + Sync>) {
        if kind == 110 {
            self.resume();
            let id = self.state.lock().unwrap().session;
            let mut s = self.input.state.lock().unwrap();
            if s.latest == Some(id) {
                s.recovery.insert(0, handler.clone());
            }
        }
        self.delegate.set_video_recovery_handler(kind, handler)
    }
    fn on_screen_stream_active(&self, kind: u16, active: bool) {
        if kind == 110 {
            if active {
                self.resume()
            } else {
                let _ = self.end();
            }
        }
        self.delegate.on_screen_stream_active(kind, active)
    }
    fn on_audio_started(&self, id: &AudioStreamId, format: &AudioFormat, first: u32) -> Result<()> {
        if let Some(stream) = AudioPolicy::stream(id) {
            let mut s = self.state.lock().unwrap();
            if let Some((_, old)) = s.audio.remove(&stream) {
                old.finish()?;
            }
            match AudioPolicy::format(id, format) {
                Err(e) => self.dropped("audio-dropped", e),
                Ok(f) => {
                    let codec = match f.codec {
                        oxp_core::model::AudioCodec::AacLc => oxp_wire::AudioCodec::AacLc,
                        oxp_core::model::AudioCodec::Opus => oxp_wire::AudioCodec::Opus,
                        oxp_core::model::AudioCodec::PcmS16le => oxp_wire::AudioCodec::PcmS16Le,
                    };
                    let role = match f.role {
                        oxp_core::model::AudioRole::Media => oxp_wire::AudioRole::Media,
                        oxp_core::model::AudioRole::Alert => oxp_wire::AudioRole::Alert,
                        oxp_core::model::AudioRole::Voice => oxp_wire::AudioRole::Voice,
                        oxp_core::model::AudioRole::Telephony => oxp_wire::AudioRole::Telephony,
                        _ => oxp_wire::AudioRole::Other,
                    };
                    let target = oxp_wire::AudioFormat {
                        codec,
                        role,
                        sample_rate: f.sample_rate,
                        channels: f.channels,
                        config: f.config.map(|c| c.to_vec()),
                    };
                    let input = self.input.clone();
                    let end_input = input.clone();
                    let session = s.session;
                    let predecessor = s.tails.get(&stream).cloned();
                    let announced = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let started = announced.clone();
                    let run = Arc::new(AudioPolicyRun::new(
                        format.clone(),
                        predecessor,
                        Arc::new(move |payload, pts| {
                            if !started.swap(true, std::sync::atomic::Ordering::AcqRel) {
                                if let Ok(m) = Message::audio_format(stream as u8, Some(&target)) {
                                    input.publish(session, &format!("2-audio-{stream}"), m);
                                }
                            }
                            input.publish(
                                session,
                                "",
                                Message::Binary {
                                    header: Header {
                                        kind: Kind::Audio,
                                        stream: stream as u8,
                                        flags: 0,
                                        sequence: 0,
                                        pts_us: pts,
                                        sender_clock_us: pts,
                                    },
                                    payload,
                                },
                            );
                        }),
                        Arc::new(move || {
                            if let Ok(m) = Message::audio_format(stream as u8, None) {
                                end_input.publish(session, &format!("2-audio-{stream}"), m);
                            }
                        }),
                    ));
                    s.tails.insert(stream, run.clone());
                    s.audio.insert(stream, (id.clone(), run.clone()));
                    run.start()?;
                }
            }
        }
        self.delegate.on_audio_started(id, format, first)
    }
    fn on_audio_rtp(
        &self,
        id: &AudioStreamId,
        f: &AudioFormat,
        rtp: &[u8],
        sample: u32,
    ) -> Result<()> {
        if let Some(stream) = AudioPolicy::stream(id) {
            let s = self.state.lock().unwrap();
            if let Some((owner, run)) = s.audio.get(&stream) {
                if owner.kind == id.kind && owner.audio_type == id.audio_type {
                    if let Err(e) = run.offer(rtp, oxp_core::monotonic_nanos() as u64 / 1000) {
                        self.dropped("audio-dropped", e);
                    }
                }
            }
        }
        self.delegate.on_audio_rtp(id, f, rtp, sample)
    }
    fn on_audio_stopped(&self, id: &AudioStreamId) -> Result<()> {
        if let Some(stream) = AudioPolicy::stream(id) {
            let mut s = self.state.lock().unwrap();
            if s.audio
                .get(&stream)
                .is_some_and(|(a, _)| a.audio_type == id.audio_type)
            {
                if let Some((_, run)) = s.audio.remove(&stream) {
                    run.finish()?;
                }
            }
        }
        self.delegate.on_audio_stopped(id)
    }
    fn on_microphone_started(&self, id: &AudioStreamId, config: &MicrophoneConfig) -> Result<()> {
        if let Some(stream) = AudioPolicy::stream(id) {
            let session = self.state.lock().unwrap().session;
            let uplink = Arc::new(LinuxMicrophoneUplink::new(config.clone())?);
            let old = {
                let mut s = self.input.state.lock().unwrap();
                if s.current != Some(session) {
                    return Ok(());
                }
                s.microphones.insert(stream, uplink.clone())
            };
            if let Some(old) = old {
                old.close()?;
            }
            let codec = match config.codec {
                AudioCodecKind::Lpcm => oxp_wire::AudioCodec::PcmS16Le,
                AudioCodecKind::Opus => oxp_wire::AudioCodec::Opus,
                _ => return Err(std::io::Error::other("Unsupported microphone codec")),
            };
            uplink.start()?;
            self.input.publish(
                session,
                &format!("3-mic-{stream}"),
                Message::microphone(
                    stream as u8,
                    Some(&oxp_wire::MicRequest {
                        codec,
                        sample_rate: config.sample_rate,
                        channels: if config.codec == AudioCodecKind::Lpcm {
                            1
                        } else {
                            config.channels.into()
                        },
                        frame_ms: if config.codec == AudioCodecKind::Lpcm {
                            20
                        } else {
                            config.frame_millis
                        },
                    }),
                )
                .map_err(std::io::Error::other)?,
            );
        }
        self.delegate.on_microphone_started(id, config)
    }
    fn on_microphone_stopped(&self, id: &AudioStreamId) -> Result<()> {
        if let Some(stream) = AudioPolicy::stream(id) {
            let session = self.state.lock().unwrap().session;
            let old = {
                let mut s = self.input.state.lock().unwrap();
                if s.current == Some(session) {
                    s.microphones.remove(&stream)
                } else {
                    None
                }
            };
            if let Some(old) = old {
                old.close()?;
            }
            self.input.publish(
                session,
                &format!("3-mic-{stream}"),
                Message::microphone(stream as u8, None).map_err(std::io::Error::other)?,
            );
        }
        self.delegate.on_microphone_stopped(id)
    }
    fn on_iap_message(&self, b: &[u8]) -> Result<()> {
        self.delegate.on_iap_message(b)
    }
}
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for p in bytes.chunks(3) {
        let n = ((p[0] as u32) << 16)
            | ((p.get(1).copied().unwrap_or(0) as u32) << 8)
            | p.get(2).copied().unwrap_or(0) as u32;
        s.push(T[(n >> 18 & 63) as usize] as char);
        s.push(T[(n >> 12 & 63) as usize] as char);
        s.push(if p.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        s.push(if p.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    s
}
pub fn unbase64(s: &str) -> Result<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if !s.len().is_multiple_of(4) {
        return Err(std::io::Error::other("Invalid base64"));
    }
    let mut out = Vec::new();
    for p in s.as_bytes().as_chunks::<4>().0 {
        let mut n = 0u32;
        for b in p {
            n = (n << 6)
                | if *b == b'=' {
                    0
                } else {
                    T.iter()
                        .position(|t| t == b)
                        .ok_or_else(|| std::io::Error::other("Invalid base64"))?
                        as u32
                };
        }
        out.push((n >> 16) as u8);
        if p[2] != b'=' {
            out.push((n >> 8) as u8)
        }
        if p[3] != b'=' {
            out.push(n as u8)
        }
    }
    if base64(&out) != s {
        return Err(std::io::Error::other("Invalid base64"));
    }
    Ok(out)
}
pub fn annex_b_to_avcc(b: &[u8]) -> Result<Vec<u8>> {
    let mut marks = Vec::new();
    let mut i = 0;
    while i + 3 <= b.len() {
        let n = if b[i..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if b[i..].starts_with(&[0, 0, 1]) {
            3
        } else {
            i += 1;
            continue;
        };
        marks.push((i, n));
        i += n;
    }
    if marks.is_empty() || marks[0].0 != 0 {
        return Err(std::io::Error::other("Video frame is not Annex-B"));
    }
    let mut out = Vec::new();
    for j in 0..marks.len() {
        let p = marks[j].0 + marks[j].1;
        let end = marks.get(j + 1).map(|m| m.0).unwrap_or(b.len());
        if end <= p {
            return Err(std::io::Error::other("Empty NAL unit"));
        }
        out.extend_from_slice(&((end - p) as u32).to_be_bytes());
        out.extend_from_slice(&b[p..end]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::net::UnixStream,
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        time::Duration,
    };
    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|s| u8::from_str_radix(s, 16).unwrap())
            .collect()
    }
    fn quiet() -> Arc<JsonEvents> {
        Arc::new(JsonEvents::new(Arc::new(|_| {})))
    }
    struct Control(AtomicUsize);
    impl SessionControl for Control {
        fn send_touch(&self, _: &[AirPlayContact]) -> Result<bool> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }
    fn setup() -> (
        Arc<DiPlayInput>,
        StreamBridgeSink,
        mpsc::Receiver<Message>,
        Arc<Control>,
    ) {
        let events = quiet();
        let input = Arc::new(DiPlayInput::new(events.clone()));
        let (tx, rx) = mpsc::sync_channel(128);
        input.state.lock().unwrap().output = Some(tx);
        let control = Arc::new(Control(AtomicUsize::new(0)));
        let bridge = StreamBridgeSink::new(
            input.clone(),
            Arc::new(RecordingMediaSink::discard(events.clone())),
            control.clone(),
            events,
        );
        (input, bridge, rx, control)
    }
    fn config() -> Vec<u8> {
        hex("01 42 c0 1e ff e1 00 09 67 42 c0 1e f4 05 01 ec 80 01 00 04 68 ce 3c 80")
    }
    #[test]
    fn media_translates_through_fake_unix_socket() {
        let (_input, b, rx, _) = setup();
        b.on_video_codec(110, VideoCodec::H264).unwrap();
        b.on_video_config(110, &config()).unwrap();
        b.on_video_frame(110, &hex("00 00 00 01 65 88 84"), 100, 8_000_000)
            .unwrap();
        let messages = rx.try_iter().collect::<Vec<_>>();
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        for m in &messages {
            oxp_wire::write(&mut writer, m).unwrap_or_else(|e| panic!("{e}: {m:?}"));
        }
        let mut received = Vec::new();
        for _ in &messages {
            received.push(oxp_wire::read(&mut reader).unwrap().unwrap());
        }
        assert_eq!(received, messages);
        let format = received
            .iter()
            .find_map(|m| match m {
                Message::Json(v) if v["type"] == "video-format" && !v["format"].is_null() => {
                    Some(v)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(format["screen"], 0);
        assert_eq!(format["format"]["width"], 640);
        assert_eq!(format["format"]["height"], 480);
        let (header, payload) = received
            .iter()
            .find_map(|m| match m {
                Message::Binary { header, payload } => Some((header, payload)),
                _ => None,
            })
            .unwrap();
        assert_eq!(header.pts_us, 8000);
        assert_eq!(header.flags, oxp_wire::KEYFRAME);
        assert_eq!(*payload, hex("00 00 00 03 65 88 84"));
    }
    #[test]
    fn touch_and_pause_resume_recovery() {
        let (input, b, rx, control) = setup();
        let contact = AirPlayContact {
            id: 1,
            x: 0.25,
            y: 0.75,
            down: true,
        };
        assert!(input.touch(0, &[contact]).unwrap());
        assert!(!input.touch(1, &[]).unwrap());
        let start = std::time::Instant::now();
        while control.0.load(Ordering::SeqCst) == 0 {
            assert!(start.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(1));
        }
        let (tx, ready) = mpsc::channel();
        b.set_video_recovery_handler(
            110,
            Arc::new(move || {
                tx.send(()).unwrap();
            }),
        );
        b.on_screen_stream_active(110, false);
        assert!(!input.request_keyframe(0).unwrap());
        assert!(!input.touch(0, &[]).unwrap());
        b.on_screen_stream_active(110, true);
        assert!(input.request_keyframe(0).unwrap());
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        let sessions = rx
            .try_iter()
            .filter_map(|m| match m {
                Message::Json(v) if v["type"] == "session" => Some(v),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[1]["session"]["phase"], "ENDED");
        assert!(
            sessions[2]["session"]["id"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                > sessions[0]["session"]["id"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
        );
    }
    #[test]
    fn stale_bridge_callbacks_are_fenced() {
        let (input, old, rx, _) = setup();
        let new = StreamBridgeSink::new(
            input.clone(),
            Arc::new(RecordingMediaSink::discard(quiet())),
            Arc::new(Control(AtomicUsize::new(0))),
            quiet(),
        );
        rx.try_iter().for_each(drop);
        old.on_video_codec(110, VideoCodec::H264).unwrap();
        old.on_video_config(110, &config()).unwrap();
        old.on_video_frame(110, &hex("00 00 01 65 88 84"), 0, 1)
            .unwrap();
        old.on_screen_stream_active(110, true);
        old.end().unwrap();
        assert!(rx.try_recv().is_err());
        new.on_video_codec(110, VideoCodec::H264).unwrap();
        new.on_video_config(110, &config()).unwrap();
        assert!(rx
            .try_iter()
            .any(|m| matches!(m,Message::Json(v)if v["session"]["phase"]=="ACTIVE")));
    }
    #[test]
    fn announce_waits_for_stream_and_end_removes_formats() {
        let (input, b, rx, _) = setup();
        assert!(!input.state.lock().unwrap().active);
        b.on_video_codec(110, VideoCodec::H264).unwrap();
        b.on_video_config(110, &config()).unwrap();
        b.end().unwrap();
        let m = rx.try_iter().collect::<Vec<_>>();
        assert!(matches!(m.last(),Some(Message::Json(v))if v["session"]["phase"]=="ENDED"));
        let ended = m
            .iter()
            .rposition(
                |m| matches!(m,Message::Json(v)if v["type"]=="video-format"&&v["format"].is_null()),
            )
            .unwrap();
        assert!(ended < m.len() - 1);
    }
    #[test]
    fn audio_replacement_drains_packets_before_end() {
        let (_input, b, rx, _) = setup();
        let id = AudioStreamId {
            kind: 100,
            audio_type: "alert".into(),
        };
        let f = AudioFormat {
            codec: AudioCodecKind::Lpcm,
            sample_rate: 16000,
            channels: 1,
            payload_type: 96,
            audio_type: "alert".into(),
            rtp_clock_rate: 16000,
        };
        b.on_audio_started(&id, &f, 0).unwrap();
        let mut rtp = vec![0; 12];
        rtp.extend_from_slice(&[0x12, 0x34]);
        b.on_audio_rtp(&id, &f, &rtp, 0).unwrap();
        b.on_audio_started(&id, &f, 1).unwrap();
        rtp[12..].copy_from_slice(&[0x56, 0x78]);
        b.on_audio_rtp(&id, &f, &rtp, 1).unwrap();
        b.on_audio_stopped(&id).unwrap();
        let mut packets = Vec::new();
        let mut order = Vec::new();
        let mut ends = 0;
        while ends < 2 {
            let m = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            match m {
                Message::Json(v) if v["type"] == "audio-format" => {
                    if v["format"].is_null() {
                        ends += 1;
                        order.push("end")
                    } else {
                        order.push("format")
                    }
                }
                Message::Binary { header, payload } if header.kind == Kind::Audio => {
                    packets.push(payload);
                    order.push("packet")
                }
                _ => {}
            }
        }
        assert_eq!(packets, vec![vec![0x34, 0x12], vec![0x78, 0x56]]);
        assert_eq!(
            order,
            ["format", "packet", "end", "format", "packet", "end"]
        );
    }
    #[test]
    fn bounded_queue_disconnects_slow_wire_consumer() {
        let (input, _b, _rx, _) = setup();
        let id = input.state.lock().unwrap().current.unwrap();
        for _ in 0..200 {
            input.publish(
                id,
                "",
                Message::Json(serde_json::json!({"type":"keyframe","screen":0})),
            );
        }
        assert!(input.state.lock().unwrap().output.is_none());
    }
    #[test]
    fn malformed_config_drops_frame_and_reports() {
        let (_input, b, rx, _) = setup();
        b.on_video_codec(110, VideoCodec::H264).unwrap();
        b.on_video_config(110, &[1, 2]).unwrap();
        b.on_video_frame(110, &hex("00 00 01 65 88"), 0, 0).unwrap();
        assert!(!rx.try_iter().any(|m| matches!(m, Message::Binary { .. })));
    }
    #[test]
    fn base64_compatibility() {
        for b in [
            vec![],
            vec![1],
            vec![1, 2],
            vec![1, 2, 3],
            (0..=255).collect(),
        ] {
            assert_eq!(unbase64(&base64(&b)).unwrap(), b);
        }
        assert_eq!(base64(b"hello"), "aGVsbG8=");
        assert!(unbase64("a==a").is_err());
    }
}
