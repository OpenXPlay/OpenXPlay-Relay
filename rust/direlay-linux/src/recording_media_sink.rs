// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use crate::*;
use carplay_airplay::*;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};
struct VideoRecord {
    codec: VideoCodec,
    file: File,
    /// One JSON line per parameter-set write and per frame, so a replay can restore timing.
    index: BufWriter<File>,
    frames: u64,
    bytes: u64,
}
impl VideoRecord {
    fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        self.index.flush()
    }
}
/// NAL unit types in an Annex-B buffer, in order.
fn nal_types(codec: VideoCodec, b: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut i = 0;
    while i + 3 <= b.len() {
        if b[i] == 0 && b[i + 1] == 0 && b[i + 2] == 1 {
            if let Some(h) = b.get(i + 3) {
                types.push(if codec == VideoCodec::H264 {
                    h & 0x1f
                } else {
                    (h >> 1) & 0x3f
                });
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    types
}
struct RecordState {
    closed: bool,
    video: BTreeMap<u16, VideoRecord>,
    audio_packets: BTreeMap<(u16, String), u64>,
    audio: BTreeMap<(u16, String), File>,
}
pub struct RecordingMediaSink {
    directory: Option<PathBuf>,
    prefix: String,
    events: Arc<JsonEvents>,
    state: Mutex<RecordState>,
}
impl RecordingMediaSink {
    pub fn new(directory: PathBuf, events: Arc<JsonEvents>) -> Result<Self> {
        std::fs::create_dir_all(&directory)?;
        Ok(Self {
            directory: Some(directory),
            prefix: crate::admin::auth::random_hex(16)?,
            events,
            state: Mutex::new(RecordState {
                closed: false,
                video: BTreeMap::new(),
                audio: BTreeMap::new(),
                audio_packets: BTreeMap::new(),
            }),
        })
    }
    pub fn discard(events: Arc<JsonEvents>) -> Self {
        Self {
            directory: None,
            prefix: String::new(),
            events,
            state: Mutex::new(RecordState {
                closed: false,
                video: BTreeMap::new(),
                audio: BTreeMap::new(),
                audio_packets: BTreeMap::new(),
            }),
        }
    }
    pub fn close(&self) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Ok(());
        }
        s.closed = true;
        for (kind, video) in &mut s.video {
            video.flush()?;
            self.report(*kind, video);
        }
        for ((kind, audio_type), count) in &s.audio_packets {
            self.events.emit(
                "audio-stop",
                &serde_json::json!({"type":kind,"audioType":audio_type,"packets":count}),
            );
        }
        s.audio_packets.clear();
        for f in s.audio.values_mut() {
            f.flush()?;
        }
        s.video.clear();
        s.audio.clear();
        Ok(())
    }
    fn report(&self, kind: u16, video: &VideoRecord) {
        self.events.emit("video-stats", &serde_json::json!({"type":kind,"codec":format!("{:?}", video.codec),"frames":video.frames,"bytes":video.bytes}));
    }
    pub fn parameter_sets(codec: VideoCodec, b: &[u8]) -> Result<Vec<u8>> {
        if b.starts_with(&[0, 0, 1]) || b.starts_with(&[0, 0, 0, 1]) {
            return Ok(b.to_vec());
        }
        let fail = || std::io::Error::other("Truncated codec config");
        if b.first() != Some(&1) {
            return Err(fail());
        }
        let mut p = if codec == VideoCodec::H264 { 5 } else { 22 };
        let mut out = Vec::new();
        fn read(b: &[u8], p: &mut usize, n: usize) -> Result<u16> {
            if *p + n > b.len() {
                return Err(std::io::Error::other("Truncated codec config"));
            }
            let v = b[*p..*p + n].iter().fold(0, |v, b| (v << 8) | *b as u16);
            *p += n;
            Ok(v)
        }
        fn nal(b: &[u8], p: &mut usize, out: &mut Vec<u8>) -> Result<()> {
            let n = read(b, p, 2)? as usize;
            if n == 0 || *p + n > b.len() {
                return Err(std::io::Error::other("Invalid parameter set length"));
            }
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(&b[*p..*p + n]);
            *p += n;
            Ok(())
        }
        if codec == VideoCodec::H264 {
            let n = read(b, &mut p, 1)? & 31;
            for _ in 0..n {
                nal(b, &mut p, &mut out)?
            }
            let n = read(b, &mut p, 1)?;
            for _ in 0..n {
                nal(b, &mut p, &mut out)?
            }
        } else {
            let n = read(b, &mut p, 1)?;
            for _ in 0..n {
                read(b, &mut p, 1)?;
                let n = read(b, &mut p, 2)?;
                for _ in 0..n {
                    nal(b, &mut p, &mut out)?
                }
            }
        }
        if out.is_empty() {
            return Err(std::io::Error::other(
                "Codec config contains no parameter sets",
            ));
        }
        Ok(out)
    }
}
impl MediaSink for RecordingMediaSink {
    fn on_video_codec(&self, k: u16, c: VideoCodec) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Ok(());
        }
        if let Some(d) = &self.directory {
            let ext = if c == VideoCodec::H264 {
                "h264"
            } else {
                "h265"
            };
            let file = d.join(format!(
                "session-{}-video-{k}-{}.{}",
                self.prefix,
                crate::admin::auth::random_hex(8)?,
                ext
            ));
            if let Some(mut previous) = s.video.remove(&k) {
                previous.flush()?;
                self.report(k, &previous);
            }
            let mut index =
                BufWriter::new(File::create(file.with_extension(format!("{ext}.jsonl")))?);
            writeln!(
                index,
                "{}",
                serde_json::json!({"format":"oxp-annexb-index/1","codec":format!("{c:?}"),"type":k})
            )?;
            s.video.insert(
                k,
                VideoRecord {
                    codec: c,
                    file: File::create(&file)?,
                    index,
                    frames: 0,
                    bytes: 0,
                },
            );
            self.events.emit(
                "video-codec",
                &serde_json::json!({"type":k,"codec":format!("{c:?}"),"file":file}),
            );
        }
        Ok(())
    }
    fn on_video_config(&self, k: u16, b: &[u8]) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Ok(());
        }
        if self.directory.is_some() {
            let v = s
                .video
                .get_mut(&k)
                .ok_or_else(|| std::io::Error::other("Video config arrived before codec"))?;
            let annex = Self::parameter_sets(v.codec, b)?;
            v.file.write_all(&annex)?;
            writeln!(
                v.index,
                "{}",
                serde_json::json!({"kind":"config","offset":v.bytes,"len":annex.len()})
            )?;
            v.bytes += annex.len() as u64;
            self.events.emit(
                "video-config",
                &serde_json::json!({"type":k,"configBytes":b.len(),"annexBBytes":annex.len()}),
            );
        }
        Ok(())
    }
    fn on_video_frame(&self, k: u16, b: &[u8], sender: u64, arrival: u64) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Ok(());
        }
        if self.directory.is_some() {
            let v = s
                .video
                .get_mut(&k)
                .ok_or_else(|| std::io::Error::other("Video frame arrived before codec"))?;
            if !b.starts_with(&[0, 0, 1]) && !b.starts_with(&[0, 0, 0, 1]) {
                return Err(std::io::Error::other("Video frame is not Annex-B"));
            }
            v.file.write_all(b)?;
            let nal = nal_types(v.codec, b);
            writeln!(
                v.index,
                "{}",
                serde_json::json!({"kind":"frame","offset":v.bytes,"len":b.len(),"senderNs":sender,"arrivalNs":arrival,"nal":nal})
            )?;
            v.frames += 1;
            v.bytes += b.len() as u64;
            if v.frames == 1 || v.frames % 300 == 0 {
                self.report(k, v);
            }
        }
        Ok(())
    }
    fn set_video_recovery_handler(&self, _: u16, _: Arc<dyn Fn() + Send + Sync>) {}
    fn on_screen_stream_active(&self, _: u16, _: bool) {}
    fn on_audio_started(&self, id: &AudioStreamId, f: &AudioFormat, first: u32) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Ok(());
        }
        if let Some(d) = &self.directory {
            let file = d.join(format!(
                "session-{}-audio-{}-{}.rtp",
                self.prefix,
                id.kind,
                crate::admin::auth::random_hex(8)?
            ));
            s.audio
                .insert((id.kind, id.audio_type.clone()), File::create(&file)?);
            s.audio_packets.insert((id.kind, id.audio_type.clone()), 0);
            self.events.emit("audio-start", &serde_json::json!({"type":id.kind,"audioType":id.audio_type,"format":format!("{f:?}"),"file":file}));
            std::fs::write(
                file.with_extension("rtp.json"),
                serde_json::to_vec(
                    &serde_json::json!({"audioType":id.audio_type,"codec":match f.codec { AudioCodecKind::Lpcm=>"LPCM",AudioCodecKind::AacLc=>"AAC_LC",AudioCodecKind::Alac { .. }=>"ALAC",AudioCodecKind::Opus=>"OPUS" },"sampleRate":f.sample_rate,"channels":f.channels,"payloadType":f.payload_type,"rtpClockRate":f.rtp_clock_rate,"firstSample":first,"packetLayout":"big-endian int32 byteLength, int32 sample, RTP bytes"}),
                )?,
            )?;
        }
        Ok(())
    }
    fn on_audio_rtp(
        &self,
        id: &AudioStreamId,
        f: &AudioFormat,
        b: &[u8],
        sample: u32,
    ) -> Result<()> {
        let key = (id.kind, id.audio_type.clone());
        if !self.state.lock().unwrap().audio.contains_key(&key) {
            self.on_audio_started(id, f, sample)?;
        }
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Ok(());
        }
        if let Some(f) = s.audio.get_mut(&key) {
            f.write_all(&(b.len() as u32).to_be_bytes())?;
            f.write_all(&sample.to_be_bytes())?;
            f.write_all(b)?;
            let count = s.audio_packets.entry(key).or_default();
            *count += 1;
            if *count == 1 || (*count).is_multiple_of(1000) {
                self.events.emit(
                    "audio-stats",
                    &serde_json::json!({"type":id.kind,"audioType":id.audio_type,"packets":count}),
                );
            }
        }
        Ok(())
    }
    fn on_audio_stopped(&self, id: &AudioStreamId) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        let key = (id.kind, id.audio_type.clone());
        if let Some(mut f) = s.audio.remove(&key) {
            f.flush()?;
        }
        let count = s.audio_packets.remove(&key);
        self.events.emit(
            "audio-stop",
            &serde_json::json!({"type":id.kind,"audioType":id.audio_type,"packets":count}),
        );
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
