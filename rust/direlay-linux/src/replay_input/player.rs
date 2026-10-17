// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Replay timing as a clock-free state machine: the session worker supplies monotonic
//! nanoseconds, tests supply their own.
use super::clip::{droppable, Clip, UnitKind};
use crate::Result;
use carplay_airplay::VideoCodec;
use std::sync::Arc;

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}

/// How the simulated iPhone answers key-frame requests for one codec.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyframeRule {
    /// Answer after this delay.
    pub delay_ms: u64,
    /// Requests ignored in each gap before one is answered; `u32::MAX` ignores every request.
    pub ignore: u32,
    /// Answer with a stream restart: the codec config again, then the key frame.
    pub stream_restart: bool,
}
impl std::fmt::Display for KeyframeRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ignore == u32::MAX {
            return write!(f, "ignore-all");
        }
        let mut parts = vec![if self.delay_ms == 0 {
            "immediate".to_string()
        } else {
            format!("delay:{}", self.delay_ms)
        }];
        if self.ignore > 0 {
            parts.push(format!("ignore:{}", self.ignore));
        }
        if self.stream_restart {
            parts.push("stream-restart".into());
        }
        write!(f, "{}", parts.join(","))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyframeModel {
    pub h264: KeyframeRule,
    pub h265: KeyframeRule,
}
impl KeyframeModel {
    /// Comma-separated rules: `immediate`, `delay:MS`, `ignore-first`, `ignore:N`, `ignore-all`
    /// and `stream-restart`; an `h264:` or `h265:` prefix limits a rule to that codec.
    pub fn parse(spec: &str) -> Result<Self> {
        let mut model = Self::default();
        for token in spec.split(',').map(str::trim) {
            let (codecs, rule): (&[VideoCodec], &str) = match token.split_once(':') {
                Some(("h264", rest)) => (&[VideoCodec::H264], rest),
                Some(("h265", rest)) => (&[VideoCodec::H265], rest),
                _ => (&[VideoCodec::H264, VideoCodec::H265], token),
            };
            for codec in codecs {
                let r = match codec {
                    VideoCodec::H264 => &mut model.h264,
                    VideoCodec::H265 => &mut model.h265,
                };
                match rule.split_once(':') {
                    None if rule == "immediate" => r.delay_ms = 0,
                    None if rule == "ignore-first" => r.ignore = 1,
                    None if rule == "ignore-all" => r.ignore = u32::MAX,
                    None if rule == "stream-restart" => r.stream_restart = true,
                    Some(("delay", ms)) => {
                        r.delay_ms =
                            ms.parse().ok().filter(|ms| *ms <= 60_000).ok_or_else(|| {
                                invalid(format!("Invalid key-frame delay: {token}"))
                            })?
                    }
                    Some(("ignore", n)) => {
                        r.ignore = n
                            .parse()
                            .map_err(|_| invalid(format!("Invalid ignore count: {token}")))?
                    }
                    _ => return Err(invalid(format!("Unknown key-frame rule: {token}"))),
                }
            }
        }
        Ok(model)
    }
    pub fn rule(&self, codec: VideoCodec) -> KeyframeRule {
        match codec {
            VideoCodec::H264 => self.h264,
            VideoCodec::H265 => self.h265,
        }
    }
}
impl std::fmt::Display for KeyframeModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.h264 == self.h265 {
            write!(f, "{}", self.h264)
        } else {
            write!(f, "h264 {}; h265 {}", self.h264, self.h265)
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Output {
    /// Annex B parameter sets: the session's codec config (again).
    Config(Vec<u8>),
    Frame {
        data: Vec<u8>,
        key: bool,
        /// When the frame was due on the replay clock; non-decreasing.
        due_ns: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Ignored,
    /// Answered at this time.
    Scheduled(u64),
    /// An earlier request in this gap is already being answered.
    Pending(u64),
}
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub frames: u64,
    pub key_frames: u64,
    pub configs: u64,
    /// Non-reference frames skipped to honour the frame-rate cap.
    pub dropped: u64,
    /// Frames held back to honour the frame-rate cap.
    pub delayed: u64,
    pub loops: u64,
    pub keyframe_requests: u64,
    pub ignored: u64,
    pub answered: u64,
}
struct Held {
    data: Vec<u8>,
    key: bool,
    media_ns: u64,
    until: u64,
}
pub struct Player {
    clip: Arc<Clip>,
    rule: KeyframeRule,
    min_interval: u64,
    /// The next unit to deliver; always a valid index.
    cursor: usize,
    /// The unit with clip time `anchor.1` is due at replay time `anchor.0`.
    anchor: (u64, u64),
    last_due: Option<u64>,
    last_media: Option<u64>,
    current_config: Option<usize>,
    /// Delivered right before the next frame (a key frame after a jump or loop).
    pending_config: Option<usize>,
    gap_requests: u32,
    answer_at: Option<u64>,
    held: Option<Held>,
    stats: Stats,
}
impl Player {
    /// Starts at the clip's first key frame with its config, due at `now`.
    pub fn new(clip: Arc<Clip>, rule: KeyframeRule, fps_cap: u32, now: u64) -> Self {
        let key = clip.first_key;
        Self {
            anchor: (now, clip.units[key].at_ns),
            pending_config: clip.config_before(key),
            cursor: key,
            min_interval: 1_000_000_000 / fps_cap.max(1) as u64,
            clip,
            rule,
            last_due: None,
            last_media: None,
            current_config: None,
            gap_requests: 0,
            answer_at: None,
            held: None,
            stats: Stats::default(),
        }
    }
    pub fn clip(&self) -> &Arc<Clip> {
        &self.clip
    }
    pub fn rule(&self) -> KeyframeRule {
        self.rule
    }
    pub fn stats(&self) -> &Stats {
        &self.stats
    }
    fn due(&self, index: usize) -> u64 {
        self.anchor.0 + self.clip.units[index].at_ns.saturating_sub(self.anchor.1)
    }
    /// When `poll` has something to do next.
    pub fn next_wake(&self) -> u64 {
        let media = match &self.held {
            Some(h) => h.until,
            None => self.due(self.cursor),
        };
        self.answer_at.map_or(media, |a| a.min(media))
    }
    pub fn request_keyframe(&mut self, now: u64) -> Request {
        self.stats.keyframe_requests += 1;
        self.gap_requests = self.gap_requests.saturating_add(1);
        if self.gap_requests <= self.rule.ignore {
            self.stats.ignored += 1;
            return Request::Ignored;
        }
        match self.answer_at {
            Some(at) => Request::Pending(at),
            None => {
                let at = now + self.rule.delay_ms * 1_000_000;
                self.answer_at = Some(at);
                Request::Scheduled(at)
            }
        }
    }
    /// Delivers everything due at `now`.
    pub fn poll(&mut self, now: u64) -> Result<Vec<Output>> {
        let mut out = Vec::new();
        loop {
            if let Some(at) = self.answer_at.filter(|at| *at <= now) {
                // A late poll still answers at the scheduled time, never before the last frame.
                self.answer_at = None;
                self.answer(at.max(self.last_due.unwrap_or(0)));
                continue;
            }
            if let Some(held) = &self.held {
                if held.until > now {
                    break;
                }
                let h = self.held.take().unwrap();
                self.emit_frame(&mut out, h.data, h.key, h.media_ns, h.until)?;
                continue;
            }
            let index = self.cursor;
            let due = self.due(index);
            if due > now {
                break;
            }
            let unit = &self.clip.units[index];
            let (kind, key, media) = (unit.kind, unit.key, unit.media_ns);
            let data = self.clip.read(unit)?;
            // Encoder time running backwards (a loop) starts a new timeline.
            let over_cap = self.last_media.is_some_and(|m| {
                media >= m && media - m + self.min_interval / 20 < self.min_interval
            });
            self.advance();
            match kind {
                UnitKind::Config => {
                    out.push(Output::Config(data));
                    self.current_config = Some(index);
                    self.stats.configs += 1;
                }
                UnitKind::Frame => {
                    if over_cap && !key && droppable(self.clip.codec, &data) {
                        self.stats.dropped += 1;
                        continue;
                    }
                    // Behind schedule (held frames before a loop), frames keep the cap spacing.
                    let until = match self.last_due {
                        Some(last) if over_cap || due < last => due.max(last + self.min_interval),
                        _ => due,
                    };
                    if until > now {
                        self.stats.delayed += 1;
                        self.held = Some(Held {
                            data,
                            key,
                            media_ns: media,
                            until,
                        });
                        break;
                    }
                    self.emit_frame(&mut out, data, key, media, until)?;
                }
            }
        }
        Ok(out)
    }
    /// Moves to the next unit, looping to the first key frame (with its config) one typical
    /// frame interval after the last unit.
    fn advance(&mut self) {
        self.cursor += 1;
        if self.cursor < self.clip.units.len() {
            return;
        }
        let end = self.due(self.clip.units.len() - 1);
        let key = self.clip.first_key;
        self.cursor = key;
        self.anchor = (
            end + self.clip.frame_interval_ns,
            self.clip.units[key].at_ns,
        );
        self.pending_config = self.clip.config_before(key);
        self.stats.loops += 1;
    }
    /// Jumps to the next key frame in the clip, or back to the clip's first one with its config.
    fn answer(&mut self, now: u64) {
        self.stats.answered += 1;
        if let Some(h) = &mut self.held {
            if h.key {
                h.until = now;
                return;
            }
        }
        self.held = None;
        let (key, wrapped) = match self.clip.next_key(self.cursor) {
            Some(key) => (key, false),
            None => (self.clip.first_key, true),
        };
        let config = self.clip.config_before(key);
        if wrapped || self.rule.stream_restart || config != self.current_config {
            self.pending_config = config;
        }
        self.cursor = key;
        self.anchor = (now, self.clip.units[key].at_ns);
        self.last_media = None;
    }
    fn emit_frame(
        &mut self,
        out: &mut Vec<Output>,
        data: Vec<u8>,
        key: bool,
        media: u64,
        due: u64,
    ) -> Result<()> {
        // A pending config belongs to the key frame a jump or loop moved to; a frame held from
        // before the loop goes out first.
        if let Some(config) = self.pending_config.filter(|_| key) {
            self.pending_config = None;
            out.push(Output::Config(self.clip.read(&self.clip.units[config])?));
            self.current_config = Some(config);
            self.stats.configs += 1;
        }
        out.push(Output::Frame {
            data,
            key,
            due_ns: due,
        });
        self.last_due = Some(due);
        self.last_media = Some(media);
        self.stats.frames += 1;
        if key {
            self.stats.key_frames += 1;
            self.gap_requests = 0;
            self.answer_at = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::clip::{
        tests::{annex, recording, recording_with, PPS, SPS},
        Clip, ClipData,
    };
    use super::*;
    const MS: u64 = 1_000_000;
    const T0: u64 = 10_000 * MS;
    fn clip(deltas_ms: &[u64]) -> Arc<Clip> {
        let (data, index) = recording(deltas_ms, false);
        Arc::new(Clip::from_index("rec.h264", &index, ClipData::Memory(data.into())).unwrap())
    }
    fn player(deltas_ms: &[u64], rule: &str, fps: u32) -> Player {
        let model = KeyframeModel::parse(rule).unwrap();
        Player::new(clip(deltas_ms), model.h264, fps, T0)
    }
    /// One letter per output: C config, K key frame, P other frame; plus frame due times in ms.
    fn shape(out: &[Output]) -> (String, Vec<u64>) {
        let mut s = String::new();
        let mut due = Vec::new();
        for o in out {
            match o {
                Output::Config(_) => s.push('C'),
                Output::Frame { key, due_ns, .. } => {
                    s.push(if *key { 'K' } else { 'P' });
                    due.push((due_ns - T0) / MS);
                }
            }
        }
        (s, due)
    }
    /// Polls at every wake time up to `until_ms` after the start.
    fn run(p: &mut Player, until_ms: u64) -> (String, Vec<u64>) {
        let mut out = Vec::new();
        loop {
            let wake = p.next_wake();
            if wake > T0 + until_ms * MS {
                break;
            }
            out.extend(p.poll(wake).unwrap());
        }
        shape(&out)
    }
    #[test]
    fn frames_follow_recorded_arrival_deltas_and_loop_with_config_again() {
        let mut p = player(&[40, 20, 30], "immediate", 60);
        let (s, due) = run(&mut p, 240);
        // The clip's typical interval (30 ms) separates the last frame from the loop.
        assert_eq!(s, "CKPPPCKPPPCK");
        assert_eq!(due, [0, 40, 60, 90, 120, 160, 180, 210, 240]);
        assert_eq!(p.stats().loops, 2);
        assert_eq!(p.stats().configs, 3);
    }
    #[test]
    fn immediate_answer_restarts_the_single_key_frame_clip_with_its_config() {
        let mut p = player(&[40, 40, 40], "immediate", 60);
        assert_eq!(shape(&p.poll(T0 + 45 * MS).unwrap()).0, "CKP");
        assert_eq!(
            p.request_keyframe(T0 + 50 * MS),
            Request::Scheduled(T0 + 50 * MS)
        );
        assert_eq!(
            p.request_keyframe(T0 + 55 * MS),
            Request::Pending(T0 + 50 * MS)
        );
        let (s, due) = shape(&p.poll(T0 + 60 * MS).unwrap());
        assert_eq!((s.as_str(), due), ("CK", vec![50]));
        // Playback continues from the key frame at the recorded pace.
        assert_eq!(
            shape(&p.poll(T0 + 90 * MS).unwrap()),
            ("P".into(), vec![90])
        );
        assert_eq!(p.stats().answered, 1);
    }
    fn two_key_frames() -> Arc<Clip> {
        let (idr, p): (&[u8], &[u8]) = (&[0x65, 0x88, 0x01], &[0x41, 0x9a, 0x01]);
        let bytes = annex(&[SPS, PPS, idr, p, p, idr, p]);
        let clip = Clip::from_raw(
            "raw.h264",
            VideoCodec::H264,
            &bytes,
            ClipData::Memory(bytes.clone().into()),
        )
        .unwrap();
        Arc::new(clip)
    }
    #[test]
    fn a_later_key_frame_is_reached_by_skipping_and_stream_restart_resends_config() {
        let mut p = Player::new(two_key_frames(), KeyframeRule::default(), 60, T0);
        assert_eq!(shape(&p.poll(T0).unwrap()).0, "CK");
        p.request_keyframe(T0 + 5 * MS);
        // Both P frames are skipped; the config in effect there is the one in use.
        assert_eq!(shape(&p.poll(T0 + 5 * MS).unwrap()), ("K".into(), vec![5]));
        assert_eq!(
            shape(&p.poll(T0 + 39 * MS).unwrap()),
            ("P".into(), vec![38])
        );
        let restart = KeyframeModel::parse("stream-restart").unwrap().h264;
        let mut p = Player::new(two_key_frames(), restart, 60, T0);
        p.poll(T0).unwrap();
        p.request_keyframe(T0 + 5 * MS);
        assert_eq!(shape(&p.poll(T0 + 5 * MS).unwrap()), ("CK".into(), vec![5]));
    }
    #[test]
    fn delayed_and_ignored_requests_follow_the_model() {
        let mut p = player(&[40; 20], "delay:120", 60);
        p.poll(T0).unwrap();
        assert_eq!(
            p.request_keyframe(T0 + 10 * MS),
            Request::Scheduled(T0 + 130 * MS)
        );
        let (s, _) = shape(&p.poll(T0 + 129 * MS).unwrap());
        assert_eq!(s, "PPP");
        let (s, due) = shape(&p.poll(T0 + 130 * MS).unwrap());
        assert_eq!((s.as_str(), due), ("CK", vec![130]));

        let mut p = player(&[40; 40], "ignore-first", 60);
        p.poll(T0).unwrap();
        assert_eq!(p.request_keyframe(T0 + 10 * MS), Request::Ignored);
        assert_eq!(shape(&p.poll(T0 + 1000 * MS).unwrap()).0.find('K'), None);
        assert_eq!(
            p.request_keyframe(T0 + 1010 * MS),
            Request::Scheduled(T0 + 1010 * MS)
        );
        assert_eq!(shape(&p.poll(T0 + 1010 * MS).unwrap()).0, "CK");
        // A key frame starts a new gap: its first request is ignored again.
        assert_eq!(p.request_keyframe(T0 + 1020 * MS), Request::Ignored);
        assert_eq!(p.stats().ignored, 2);

        let mut p = player(&[40; 400], "ignore-all", 60);
        p.poll(T0).unwrap();
        for n in 0..10 {
            assert_eq!(p.request_keyframe(T0 + n * 1000 * MS), Request::Ignored);
        }
        assert!(!shape(&p.poll(T0 + 10_000 * MS).unwrap()).0.contains('K'));
    }
    #[test]
    fn codec_scoped_rules_and_spec_errors() {
        let model =
            KeyframeModel::parse("h265:ignore-first,delay:150,h264:stream-restart").unwrap();
        assert_eq!(
            model.h264,
            KeyframeRule {
                delay_ms: 150,
                ignore: 0,
                stream_restart: true
            }
        );
        assert_eq!(model.h265.ignore, 1);
        assert_eq!(model.h265.delay_ms, 150);
        assert_eq!(
            model.to_string(),
            "h264 delay:150,stream-restart; h265 delay:150,ignore:1"
        );
        assert_eq!(
            KeyframeModel::parse("ignore-all").unwrap().to_string(),
            "ignore-all"
        );
        for bad in [
            "",
            "delay:x",
            "delay:70000",
            "sometimes",
            "ignore:-1",
            "h266:immediate",
        ] {
            assert!(KeyframeModel::parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn frame_rate_cap_delays_reference_frames_by_encoder_time() {
        // 60 fps by sender time with bunched arrivals, capped at 30 fps.
        let (data, index) = recording_with(&[16, 1, 16, 1, 16], Some(&[16; 5]));
        let clip =
            Arc::new(Clip::from_index("x.h264", &index, ClipData::Memory(data.into())).unwrap());
        let mut p = Player::new(clip, KeyframeRule::default(), 30, T0);
        let (s, due) = run(&mut p, 170);
        // Each frame waits for the previous one plus 33.3 ms.
        assert_eq!(s, "CKPPPPP");
        assert_eq!(due, [0, 33, 66, 99, 133, 166]);
        // Five P frames, then the looped key frame keeps the spacing.
        assert_eq!((p.stats().delayed, p.stats().dropped), (6, 0));
        assert_eq!(
            shape(&p.poll(T0 + 200 * MS).unwrap()),
            ("CK".into(), vec![199])
        );
        // Without the cap the bunched arrivals pass unchanged.
        let (data, index) = recording_with(&[16, 1, 16, 1, 16], Some(&[16; 5]));
        let clip =
            Arc::new(Clip::from_index("x.h264", &index, ClipData::Memory(data.into())).unwrap());
        let mut p = Player::new(clip, KeyframeRule::default(), 60, T0);
        assert_eq!(run(&mut p, 50).1, [0, 16, 17, 33, 34, 50]);
    }
    #[test]
    fn frame_rate_cap_drops_non_reference_frames() {
        let (mut data, index) = recording(&[16, 16, 16], false);
        // Turn the P frames into non-reference ones.
        for i in 0..data.len() - 3 {
            if data[i..i + 4] == [0, 0, 1, 0x41] {
                data[i + 3] = 0x01;
            }
        }
        let clip =
            Arc::new(Clip::from_index("x.h264", &index, ClipData::Memory(data.into())).unwrap());
        let mut p = Player::new(clip, KeyframeRule::default(), 30, T0);
        let (s, due) = shape(&p.poll(T0 + 48 * MS).unwrap());
        // K at 0, P at 16 dropped, P at 32 sent (within 5 % of 33.3 ms), P at 48 dropped.
        assert_eq!((s.as_str(), due), ("CKP", vec![0, 32]));
        assert_eq!(p.stats().dropped, 2);
    }
}
