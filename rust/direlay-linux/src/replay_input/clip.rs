// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Replay clips: Annex B with its `oxp-annexb-index/1` timing index (as `--record` writes it),
//! or raw Annex B split into access units at a nominal rate.
use crate::Result;
use carplay_airplay::VideoCodec;
use std::{
    io::Write,
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const INDEX_FORMAT: &str = "oxp-annexb-index/1";
/// Frame interval of raw Annex B without an index (30 fps).
pub const RAW_FRAME_NS: u64 = 1_000_000_000 / 30;
/// Longer recorded pauses are shortened to this.
pub const MAX_GAP_NS: u64 = 20_000_000_000;
/// Raw Annex B is split in memory; recordings this large come with an index.
const MAX_RAW_BYTES: u64 = 64 << 20;
const VIDEO_EXTENSIONS: [&str; 6] = ["h264", "264", "avc", "h265", "265", "hevc"];

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}
pub fn codec_name(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::H264 => "H264",
        VideoCodec::H265 => "H265",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    /// Annex B parameter sets, delivered as the session's codec config.
    Config,
    /// One access unit in Annex B.
    Frame,
}
#[derive(Clone, Debug)]
pub struct Unit {
    pub kind: UnitKind,
    /// Byte ranges in the clip data; a raw access unit can surround removed parameter sets.
    pub ranges: Vec<(u64, u32)>,
    /// When the unit is due, from the clip start (recorded arrival or nominal rate).
    pub at_ns: u64,
    /// Encoder time from the clip start (recorded sender clock or nominal rate); frame-rate caps
    /// compare it, so network bursts in a recording do not count as a high frame rate.
    pub media_ns: u64,
    pub key: bool,
}
pub enum ClipData {
    File(std::fs::File),
    Memory(Arc<[u8]>),
}
impl ClipData {
    fn len(&self) -> Result<u64> {
        match self {
            Self::File(f) => Ok(f.metadata()?.len()),
            Self::Memory(b) => Ok(b.len() as u64),
        }
    }
    fn read(&self, offset: u64, len: u32, out: &mut Vec<u8>) -> Result<()> {
        let start = out.len();
        out.resize(start + len as usize, 0);
        match self {
            Self::File(f) => f.read_exact_at(&mut out[start..], offset),
            Self::Memory(b) => {
                let end = offset
                    .checked_add(len as u64)
                    .filter(|end| *end <= b.len() as u64)
                    .ok_or_else(|| invalid("Clip range outside its data"))?;
                out[start..].copy_from_slice(&b[offset as usize..end as usize]);
                Ok(())
            }
        }
    }
}

pub struct Clip {
    pub name: String,
    pub codec: VideoCodec,
    pub units: Vec<Unit>,
    /// The first key frame with a codec config before it; playback and loops start here.
    pub first_key: usize,
    pub frames: usize,
    pub duration_ns: u64,
    /// Peak encoder rate (from the 10th percentile of frame intervals) compared with a cap.
    pub peak_fps: f64,
    /// Typical gap between frames, also used between the last frame and a loop.
    pub frame_interval_ns: u64,
    /// Load notes (truncated index or data, skipped frames) for the session log.
    pub notes: Vec<String>,
    data: ClipData,
}
impl std::fmt::Debug for Clip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clip")
            .field("name", &self.name)
            .field("codec", &self.codec)
            .field("frames", &self.frames)
            .finish()
    }
}

#[derive(serde::Deserialize)]
struct IndexHeader {
    format: String,
    codec: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexEntry {
    kind: String,
    offset: u64,
    len: u64,
    arrival_ns: Option<u64>,
    sender_ns: Option<u64>,
    nal: Option<Vec<u8>>,
}
/// A parsed index: its codec, entries and whether a cut-off last line was dropped.
struct Index {
    codec: VideoCodec,
    entries: Vec<IndexEntry>,
    truncated: bool,
}
fn parse_index(name: &str, text: &str) -> Result<Index> {
    let mut lines = text
        .split('\n')
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let (_, header) = lines
        .next()
        .ok_or_else(|| invalid(format!("{name}: empty index")))?;
    let header: IndexHeader = serde_json::from_str(header)
        .map_err(|e| invalid(format!("{name}: invalid index header: {e}")))?;
    if header.format != INDEX_FORMAT {
        return Err(invalid(format!(
            "{name}: unsupported index format {}",
            header.format
        )));
    }
    let codec = match header.codec.as_str() {
        "H264" => VideoCodec::H264,
        "H265" => VideoCodec::H265,
        other => return Err(invalid(format!("{name}: unsupported codec {other}"))),
    };
    let rest = lines.collect::<Vec<_>>();
    let mut entries = Vec::with_capacity(rest.len());
    let mut truncated = false;
    for (position, (number, line)) in rest.iter().enumerate() {
        match serde_json::from_str::<IndexEntry>(line) {
            Ok(entry) => entries.push(entry),
            // A recorder stopped mid-write leaves a partial last line.
            Err(_) if position + 1 == rest.len() => truncated = true,
            Err(e) => return Err(invalid(format!("{name}: index line {}: {e}", number + 1))),
        }
    }
    Ok(Index {
        codec,
        entries,
        truncated,
    })
}

/// NAL units of an Annex B buffer: start-code offset, payload offset and payload end (trailing
/// zero bytes excluded).
pub fn nal_units(b: &[u8]) -> Vec<(usize, usize, usize)> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= b.len() {
        if b[i] == 0 && b[i + 1] == 0 && b[i + 2] == 1 {
            let code = if i > 0 && b[i - 1] == 0 { i - 1 } else { i };
            starts.push((code, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (n, &(code, payload)) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(b.len(), |next| next.0);
        while end > payload && b[end - 1] == 0 {
            end -= 1;
        }
        if end > payload {
            out.push((code, payload, end));
        }
    }
    out
}
pub fn nal_type(codec: VideoCodec, header: u8) -> u8 {
    match codec {
        VideoCodec::H264 => header & 0x1f,
        VideoCodec::H265 => (header >> 1) & 0x3f,
    }
}
pub fn is_vcl(codec: VideoCodec, t: u8) -> bool {
    match codec {
        VideoCodec::H264 => (1..=5).contains(&t),
        VideoCodec::H265 => t < 32,
    }
}
pub fn is_key_type(codec: VideoCodec, t: u8) -> bool {
    match codec {
        VideoCodec::H264 => t == 5,
        VideoCodec::H265 => (16..=21).contains(&t),
    }
}
fn is_parameter_set(codec: VideoCodec, t: u8) -> bool {
    match codec {
        VideoCodec::H264 => matches!(t, 7 | 8 | 13),
        VideoCodec::H265 => matches!(t, 32..=34),
    }
}
fn is_delimiter(codec: VideoCodec, t: u8) -> bool {
    match codec {
        VideoCodec::H264 => t == 9,
        VideoCodec::H265 => t == 35,
    }
}
/// Non-VCL units that begin the next access unit once the current one has a picture.
fn starts_access_unit(codec: VideoCodec, t: u8) -> bool {
    match codec {
        VideoCodec::H264 => matches!(t, 6 | 14..=18),
        VideoCodec::H265 => matches!(t, 39 | 41..=44 | 48..=55),
    }
}
/// The first slice of a picture: H.264 first_mb_in_slice is 0, H.265
/// first_slice_segment_in_pic_flag is set.
fn first_slice(codec: VideoCodec, payload: &[u8]) -> bool {
    match codec {
        VideoCodec::H264 => payload.get(1).is_some_and(|b| b & 0x80 != 0),
        VideoCodec::H265 => payload.get(2).is_some_and(|b| b & 0x80 != 0),
    }
}
pub fn is_key_frame(codec: VideoCodec, annex_b: &[u8]) -> bool {
    nal_units(annex_b)
        .iter()
        .any(|&(_, p, _)| is_key_type(codec, nal_type(codec, annex_b[p])))
}
/// Every picture slice is a non-reference one, so a decoder never needs this access unit later.
pub fn droppable(codec: VideoCodec, annex_b: &[u8]) -> bool {
    let mut slices = 0;
    for (_, p, _) in nal_units(annex_b) {
        let header = annex_b[p];
        let t = nal_type(codec, header);
        if !is_vcl(codec, t) {
            continue;
        }
        slices += 1;
        let reference = match codec {
            VideoCodec::H264 => header & 0x60 != 0,
            // Sub-layer non-reference pictures have even types below 16.
            VideoCodec::H265 => t >= 16 || t % 2 == 1,
        };
        if reference {
            return false;
        }
    }
    slices > 0
}

/// Removes emulation-prevention bytes from the first `limit` RBSP bytes.
fn rbsp_prefix(nal: &[u8], limit: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(limit);
    let mut zeros = 0;
    for &b in nal {
        if out.len() == limit {
            break;
        }
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}
fn push_nal(out: &mut Vec<u8>, nal: &[u8]) -> Result<()> {
    let n = u16::try_from(nal.len()).map_err(|_| invalid("Parameter set too large"))?;
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(nal);
    Ok(())
}
/// Converts Annex B parameter sets into the decoder configuration record a CarPlay session
/// delivers (avcC for H.264, hvcC for H.265 Main: 4:2:0, 8 bit), with four-byte NAL lengths.
pub fn decoder_config(codec: VideoCodec, annex_b: &[u8]) -> Result<Vec<u8>> {
    let mut sets: [Vec<&[u8]>; 3] = Default::default();
    for (_, p, end) in nal_units(annex_b) {
        let nal = &annex_b[p..end];
        let slot = match (codec, nal_type(codec, nal[0])) {
            (VideoCodec::H264, 7) | (VideoCodec::H265, 32) => 0,
            (VideoCodec::H264, 8) | (VideoCodec::H265, 33) => 1,
            (VideoCodec::H265, 34) => 2,
            _ => continue,
        };
        sets[slot].push(nal);
    }
    match codec {
        VideoCodec::H264 => {
            let [sps, pps, _] = &sets;
            let first = sps
                .first()
                .filter(|s| s.len() >= 4)
                .ok_or_else(|| invalid("Codec config has no SPS"))?;
            if pps.is_empty() || sps.len() > 31 || pps.len() > 255 {
                return Err(invalid("Codec config needs one to 31 SPS and a PPS"));
            }
            let mut out = vec![
                1,
                first[1],
                first[2],
                first[3],
                0xff,
                0xe0 | sps.len() as u8,
            ];
            for s in sps {
                push_nal(&mut out, s)?;
            }
            out.push(pps.len() as u8);
            for p in pps {
                push_nal(&mut out, p)?;
            }
            Ok(out)
        }
        VideoCodec::H265 => {
            let [vps, sps, pps] = &sets;
            if vps.is_empty() || sps.is_empty() || pps.is_empty() {
                return Err(invalid("Codec config needs VPS, SPS and PPS"));
            }
            // NAL header (2), then vps id, sub-layer count and nesting flag (1), then the
            // general profile, tier and level (12), all byte-aligned.
            let rbsp = rbsp_prefix(sps[0], 15);
            if rbsp.len() < 15 {
                return Err(invalid("Truncated HEVC SPS"));
            }
            let layers = ((rbsp[2] >> 1) & 7) + 1;
            let nested = rbsp[2] & 1;
            let mut out = vec![1];
            out.extend_from_slice(&rbsp[3..15]);
            out.extend_from_slice(&[0xf0, 0x00, 0xfc, 0xfd, 0xf8, 0xf8, 0x00, 0x00]);
            out.push((layers << 3) | (nested << 2) | 3);
            out.push(3);
            for (t, list) in [(32u8, vps), (33, sps), (34, pps)] {
                out.push(0x80 | t);
                out.extend_from_slice(&(list.len() as u16).to_be_bytes());
                for nal in list {
                    push_nal(&mut out, nal)?;
                }
            }
            Ok(out)
        }
    }
}

impl Clip {
    /// A data file (its `<file>.jsonl` index is used when present) or an index file.
    pub fn open(path: &Path) -> Result<Self> {
        let text = path.to_string_lossy();
        let (data_path, index_path) = match text.strip_suffix(".jsonl") {
            Some(data) => (PathBuf::from(data), Some(path.to_path_buf())),
            None => {
                let index = PathBuf::from(format!("{text}.jsonl"));
                (path.to_path_buf(), index.is_file().then_some(index))
            }
        };
        let name = data_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| text.into_owned());
        match index_path {
            Some(index) => {
                let file = std::fs::File::open(&data_path)
                    .map_err(|e| invalid(format!("{}: {e}", data_path.display())))?;
                let index = std::fs::read_to_string(&index)
                    .map_err(|e| invalid(format!("{}: {e}", index.display())))?;
                Self::from_index(&name, &index, ClipData::File(file))
            }
            None => Self::open_raw(&data_path),
        }
    }
    /// Raw Annex B, ignoring any index; the codec comes from the file extension.
    pub fn open_raw(path: &Path) -> Result<Self> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let codec = codec_from_name(path).ok_or_else(|| {
            invalid(format!(
                "{name}: unknown codec; use a .h264 or .h265 extension"
            ))
        })?;
        let file =
            std::fs::File::open(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        if file.metadata()?.len() > MAX_RAW_BYTES {
            return Err(invalid(format!(
                "{name}: raw Annex B over 64 MiB needs a timing index"
            )));
        }
        let bytes = std::fs::read(path)?;
        Self::from_raw(&name, codec, &bytes, ClipData::File(file))
    }
    /// A recording with its index; frames are due at their recorded arrival times.
    pub fn from_index(name: &str, index: &str, data: ClipData) -> Result<Self> {
        let Index {
            codec,
            entries,
            truncated,
        } = parse_index(name, index)?;
        let mut notes = Vec::new();
        if truncated {
            notes.push(format!("{name}: ignored a truncated last index line"));
        }
        let size = data.len()?;
        let mut units = Vec::with_capacity(entries.len());
        let mut previous: Option<(u64, Option<u64>)> = None;
        let (mut at, mut media) = (0u64, 0u64);
        for (n, e) in entries.iter().enumerate() {
            if e.len == 0 || e.len > u32::MAX as u64 {
                return Err(invalid(format!(
                    "{name}: entry {} has length {}",
                    n + 1,
                    e.len
                )));
            }
            if e.offset.saturating_add(e.len) > size {
                notes.push(format!(
                    "{name}: data ends inside entry {}; {} entries ignored",
                    n + 1,
                    entries.len() - n
                ));
                break;
            }
            let kind = match e.kind.as_str() {
                "config" => UnitKind::Config,
                "frame" => UnitKind::Frame,
                _ => continue,
            };
            let ranges = vec![(e.offset, e.len as u32)];
            if kind == UnitKind::Frame {
                // Entries without times fall back to the nominal rate.
                let arrival = e
                    .arrival_ns
                    .or(previous.map(|(a, _)| a + RAW_FRAME_NS))
                    .unwrap_or(0);
                if let Some((a, s)) = previous {
                    let delta = arrival.saturating_sub(a).min(MAX_GAP_NS);
                    at += delta;
                    media += match (e.sender_ns, s) {
                        (Some(now), Some(before)) => now.saturating_sub(before).min(MAX_GAP_NS),
                        _ => delta,
                    };
                }
                previous = Some((arrival, e.sender_ns));
            }
            let key = match (&e.nal, kind) {
                (Some(types), UnitKind::Frame) => types.iter().any(|t| is_key_type(codec, *t)),
                (None, UnitKind::Frame) => {
                    let mut b = Vec::new();
                    data.read(e.offset, e.len as u32, &mut b)?;
                    is_key_frame(codec, &b)
                }
                _ => false,
            };
            units.push(Unit {
                kind,
                ranges,
                at_ns: at,
                media_ns: media,
                key,
            });
        }
        Self::finish(name, codec, units, data, notes)
    }
    /// Raw Annex B: parameter sets become config units, the rest is split into access units at
    /// a nominal 30 fps.
    pub fn from_raw(name: &str, codec: VideoCodec, bytes: &[u8], data: ClipData) -> Result<Self> {
        struct Pending {
            ranges: Vec<(u64, u32)>,
            pictures: bool,
            key: bool,
        }
        let mut units = Vec::new();
        let mut config: Vec<(u64, u32)> = Vec::new();
        let mut frame: Option<Pending> = None;
        let mut frames = 0u64;
        let mut finish = |frame: &mut Option<Pending>, config: &mut Vec<(u64, u32)>| {
            let Some(f) = frame.take() else {
                return;
            };
            if !f.pictures {
                return;
            }
            let at = frames * RAW_FRAME_NS;
            if !config.is_empty() {
                units.push(Unit {
                    kind: UnitKind::Config,
                    ranges: std::mem::take(config),
                    at_ns: at,
                    media_ns: at,
                    key: false,
                });
            }
            units.push(Unit {
                kind: UnitKind::Frame,
                ranges: f.ranges,
                at_ns: at,
                media_ns: at,
                key: f.key,
            });
            frames += 1;
        };
        let has_picture = |f: &Option<Pending>| f.as_ref().is_some_and(|f| f.pictures);
        for (code, payload, end) in nal_units(bytes) {
            let range = (code as u64, (end - code) as u32);
            let t = nal_type(codec, bytes[payload]);
            if is_parameter_set(codec, t) {
                if has_picture(&frame) {
                    finish(&mut frame, &mut config);
                }
                config.push(range);
            } else if is_delimiter(codec, t) {
                if has_picture(&frame) {
                    finish(&mut frame, &mut config);
                }
            } else if is_vcl(codec, t) {
                if has_picture(&frame) && first_slice(codec, &bytes[payload..end]) {
                    finish(&mut frame, &mut config);
                }
                let f = frame.get_or_insert(Pending {
                    ranges: Vec::new(),
                    pictures: false,
                    key: false,
                });
                f.ranges.push(range);
                f.pictures = true;
                f.key |= is_key_type(codec, t);
            } else {
                if starts_access_unit(codec, t) && has_picture(&frame) {
                    finish(&mut frame, &mut config);
                }
                frame
                    .get_or_insert(Pending {
                        ranges: Vec::new(),
                        pictures: false,
                        key: false,
                    })
                    .ranges
                    .push(range);
            }
        }
        finish(&mut frame, &mut config);
        // Merge adjacent byte ranges so a contiguous unit reads in one call.
        for unit in &mut units {
            let mut merged: Vec<(u64, u32)> = Vec::with_capacity(unit.ranges.len());
            for &(offset, len) in &unit.ranges {
                match merged.last_mut() {
                    Some(last) if last.0 + last.1 as u64 == offset => last.1 += len,
                    _ => merged.push((offset, len)),
                }
            }
            unit.ranges = merged;
        }
        Self::finish(name, codec, units, data, Vec::new())
    }
    fn finish(
        name: &str,
        codec: VideoCodec,
        mut units: Vec<Unit>,
        data: ClipData,
        mut notes: Vec<String>,
    ) -> Result<Self> {
        // A config goes out right before the frame it precedes.
        let mut next: Option<(u64, u64)> = None;
        for unit in units.iter_mut().rev() {
            match unit.kind {
                UnitKind::Frame => next = Some((unit.at_ns, unit.media_ns)),
                UnitKind::Config => {
                    if let Some((at, media)) = next {
                        (unit.at_ns, unit.media_ns) = (at, media);
                    }
                }
            }
        }
        let mut seen_config = false;
        let first_key = units
            .iter()
            .position(|u| {
                seen_config |= u.kind == UnitKind::Config;
                u.kind == UnitKind::Frame && u.key && seen_config
            })
            .ok_or_else(|| {
                invalid(format!(
                    "{name}: no key frame with a codec config before it"
                ))
            })?;
        let skipped = units[..first_key]
            .iter()
            .filter(|u| u.kind == UnitKind::Frame)
            .count();
        if skipped > 0 {
            notes.push(format!(
                "{name}: {skipped} frames before the first key frame are skipped"
            ));
        }
        let frames = units
            .iter()
            .filter(|u| u.kind == UnitKind::Frame)
            .collect::<Vec<_>>();
        let intervals = |f: &dyn Fn(&Unit) -> u64| {
            let mut v = frames
                .windows(2)
                .map(|w| f(w[1]).saturating_sub(f(w[0])))
                .filter(|d| *d > 0)
                .collect::<Vec<_>>();
            v.sort_unstable();
            v
        };
        let media = intervals(&|u| u.media_ns);
        let arrival = intervals(&|u| u.at_ns);
        let peak_fps = media
            .get(media.len() / 10)
            .map_or(30.0, |d| 1e9 / *d as f64);
        let frame_interval_ns = arrival
            .get(arrival.len() / 2)
            .copied()
            .unwrap_or(RAW_FRAME_NS)
            .clamp(1_000_000, 1_000_000_000);
        let duration_ns = frames.last().map_or(0, |l| l.at_ns) - units[first_key].at_ns;
        Ok(Self {
            name: name.into(),
            codec,
            frames: frames.len(),
            units,
            first_key,
            duration_ns,
            peak_fps,
            frame_interval_ns,
            notes,
            data,
        })
    }
    pub fn read(&self, unit: &Unit) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(unit.ranges.iter().map(|r| r.1 as usize).sum());
        for &(offset, len) in &unit.ranges {
            self.data.read(offset, len, &mut out)?;
        }
        Ok(out)
    }
    /// The config unit in effect at `index`.
    pub fn config_before(&self, index: usize) -> Option<usize> {
        self.units[..index.min(self.units.len())]
            .iter()
            .rposition(|u| u.kind == UnitKind::Config)
    }
    /// The first key frame at or after `index`.
    pub fn next_key(&self, index: usize) -> Option<usize> {
        (index..self.units.len())
            .find(|&i| self.units[i].kind == UnitKind::Frame && self.units[i].key)
    }
    /// Writes an `oxp-annexb-index/1` index for this clip's data, with its unit timing as arrival
    /// and sender times. Every unit must be one contiguous byte range.
    pub fn write_index(&self, out: &mut impl Write) -> Result<()> {
        writeln!(
            out,
            "{}",
            serde_json::json!({"format":INDEX_FORMAT,"codec":codec_name(self.codec),"type":110})
        )?;
        for unit in &self.units {
            let [(offset, len)] = unit.ranges[..] else {
                return Err(invalid(format!(
                    "{}: an access unit is not contiguous; remove access unit delimiters",
                    self.name
                )));
            };
            match unit.kind {
                UnitKind::Config => writeln!(
                    out,
                    "{}",
                    serde_json::json!({"kind":"config","offset":offset,"len":len})
                )?,
                UnitKind::Frame => {
                    let mut b = Vec::new();
                    self.data.read(offset, len, &mut b)?;
                    let nal = nal_units(&b)
                        .iter()
                        .map(|&(_, p, _)| nal_type(self.codec, b[p]))
                        .collect::<Vec<_>>();
                    writeln!(
                        out,
                        "{}",
                        serde_json::json!({"kind":"frame","offset":offset,"len":len,
                            "senderNs":unit.media_ns,"arrivalNs":unit.at_ns,"nal":nal})
                    )?
                }
            }
        }
        Ok(())
    }
}
fn codec_from_name(path: &Path) -> Option<VideoCodec> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "h264" | "264" | "avc" => Some(VideoCodec::H264),
        "h265" | "265" | "hevc" => Some(VideoCodec::H265),
        _ => None,
    }
}

/// The clips of one test run; a session picks one by codec and frame-rate cap.
pub struct ClipLibrary {
    pub clips: Vec<Arc<Clip>>,
    /// Skipped files and load notes, logged once when the first session starts.
    pub notes: Vec<String>,
}
impl ClipLibrary {
    /// Files load as given; directories contribute every usable clip (`*.h264`, `*.h265`, ...).
    pub fn open(paths: &[PathBuf]) -> Result<Self> {
        let mut clips = Vec::new();
        let mut notes = Vec::new();
        for path in paths {
            if path.is_dir() {
                let mut files = std::fs::read_dir(path)
                    .map_err(|e| invalid(format!("{}: {e}", path.display())))?
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.is_file()
                            && p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                                VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())
                            })
                    })
                    .collect::<Vec<_>>();
                files.sort();
                for file in files {
                    match Clip::open(&file) {
                        Ok(clip) => clips.push(Arc::new(clip)),
                        Err(e) => notes.push(format!("skipped {e}")),
                    }
                }
            } else {
                clips.push(Arc::new(Clip::open(path)?));
            }
        }
        if clips.is_empty() {
            return Err(invalid(format!(
                "No usable replay clip in {}{}",
                paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                if notes.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", notes.join("; "))
                }
            )));
        }
        for clip in &clips {
            notes.extend(clip.notes.iter().cloned());
        }
        Ok(Self { clips, notes })
    }
    pub fn from_clips(clips: Vec<Clip>) -> Self {
        Self {
            clips: clips.into_iter().map(Arc::new).collect(),
            notes: Vec::new(),
        }
    }
    /// The longest clip of `codec` whose peak rate fits `fps` and that lasts at least half as
    /// long as its longest clip, else its longest clip (paced to the cap). Without a clip of that
    /// codec, the same choice among all clips.
    pub fn select(&self, codec: VideoCodec, fps: u32) -> Option<Arc<Clip>> {
        let matching = self.clips.iter().any(|c| c.codec == codec);
        let pool = self
            .clips
            .iter()
            .filter(|c| !matching || c.codec == codec)
            .collect::<Vec<_>>();
        let longest = |clips: &mut dyn Iterator<Item = &&Arc<Clip>>| {
            let mut best: Option<&Arc<Clip>> = None;
            for clip in clips {
                if best.is_none_or(|b| clip.duration_ns > b.duration_ns) {
                    best = Some(clip);
                }
            }
            best.cloned()
        };
        let all = longest(&mut pool.iter())?;
        let fitting =
            longest(&mut pool.iter().filter(|c| {
                c.peak_fps <= fps as f64 * 1.05 && c.duration_ns * 2 >= all.duration_ns
            }));
        fitting.or(Some(all))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    /// Parameter sets of a 1280x720 High L3.2 stream (as tests/data/replay-h264-high.h264).
    pub(crate) const SPS: &[u8] = &[
        0x67, 0x64, 0x00, 0x20, 0xac, 0xb2, 0x00, 0xa0, 0x0b, 0x76, 0x02, 0x20, 0x00, 0x00, 0x03,
        0x00, 0x20, 0x00, 0x00, 0x07, 0x91, 0xe3, 0x06, 0x49,
    ];
    pub(crate) const PPS: &[u8] = &[0x68, 0xeb, 0xcc, 0xb2, 0x2c];
    pub(crate) fn annex(nals: &[&[u8]]) -> Vec<u8> {
        nals.iter()
            .flat_map(|n| [&[0, 0, 0, 1][..], n].concat())
            .collect()
    }
    /// A recording: config, an IDR and P frames with `deltas_ms` between arrivals.
    pub(crate) fn recording(deltas_ms: &[u64], truncate_last_line: bool) -> (Vec<u8>, String) {
        let (data, mut index) = recording_with(deltas_ms, None);
        if truncate_last_line {
            index.push_str("{\"arrivalNs\":123,\"kind\":\"fra");
        }
        (data, index)
    }
    /// Sender deltas default to the arrival deltas.
    pub(crate) fn recording_with(
        arrival_ms: &[u64],
        sender_ms: Option<&[u64]>,
    ) -> (Vec<u8>, String) {
        let mut data = annex(&[SPS, PPS]);
        let mut index = format!(
            "{}\n{}\n",
            serde_json::json!({"format":INDEX_FORMAT,"codec":"H264","type":110}),
            serde_json::json!({"kind":"config","offset":0,"len":data.len()})
        );
        let (mut arrival, mut sender) = (1_000_000_000u64, 7_000_000_000u64);
        for n in 0..=arrival_ms.len() {
            if n > 0 {
                arrival += arrival_ms[n - 1] * 1_000_000;
                sender += sender_ms.unwrap_or(arrival_ms)[n - 1] * 1_000_000 + 666_666;
            }
            let frame = if n == 0 {
                annex(&[&[0x65, 0x88, 0x84, n as u8]])
            } else {
                annex(&[&[0x41, 0x9a, 0x02, n as u8]])
            };
            index.push_str(&format!(
                "{}\n",
                serde_json::json!({"kind":"frame","offset":data.len(),"len":frame.len(),
                    "senderNs":sender,"arrivalNs":arrival,"nal":[if n == 0 { 5 } else { 1 }]})
            ));
            data.extend_from_slice(&frame);
        }
        (data, index)
    }
    #[test]
    fn index_timing_survives_a_truncated_last_line_and_short_data() {
        let (data, index) = recording(&[40, 10, 25_000], true);
        let clip =
            Clip::from_index("rec.h264", &index, ClipData::Memory(data.clone().into())).unwrap();
        assert_eq!(clip.codec, VideoCodec::H264);
        assert_eq!(clip.frames, 4);
        assert_eq!(clip.first_key, 1);
        let at = clip
            .units
            .iter()
            .map(|u| u.at_ns / 1_000_000)
            .collect::<Vec<_>>();
        // The config is due with the IDR; the 25 s pause is shortened to 20 s.
        assert_eq!(at, [0, 0, 40, 50, 20_050]);
        assert!(clip.notes[0].contains("truncated last index line"));
        assert_eq!(
            clip.read(&clip.units[1]).unwrap(),
            annex(&[&[0x65, 0x88, 0x84, 0]])
        );
        // Data cut inside the third frame drops it and everything after it.
        let cut = data.len() - 6;
        let clip =
            Clip::from_index("cut.h264", &index, ClipData::Memory(data[..cut].into())).unwrap();
        assert_eq!(clip.frames, 3);
        assert!(clip
            .notes
            .iter()
            .any(|n| n.contains("data ends inside entry 5")));
        // A broken line elsewhere is an error.
        let broken = index.replacen("\"kind\":\"frame\"", "\"kind\":", 1);
        assert!(Clip::from_index("x", &broken, ClipData::Memory(data.into())).is_err());
        assert!(parse_index("x", "{\"format\":\"other\",\"codec\":\"H264\"}").is_err());
    }
    #[test]
    fn raw_annex_b_splits_access_units_and_moves_parameter_sets_to_config() {
        let sei: &[u8] = &[0x06, 0x05, 0x01, 0x80];
        let idr_a: &[u8] = &[0x65, 0x88, 0x01];
        let idr_b: &[u8] = &[0x65, 0x08, 0x02]; // second slice of the same picture
        let p1: &[u8] = &[0x41, 0x9a, 0x03];
        let aud: &[u8] = &[0x09, 0x10];
        let p2: &[u8] = &[0x01, 0x9a, 0x04]; // non-reference
        let bytes = annex(&[SPS, PPS, sei, idr_a, idr_b, p1, aud, p2, SPS, PPS, idr_a]);
        let clip = Clip::from_raw(
            "raw.h264",
            VideoCodec::H264,
            &bytes,
            ClipData::Memory(bytes.clone().into()),
        )
        .unwrap();
        let kinds = clip
            .units
            .iter()
            .map(|u| (u.kind, u.key, u.at_ns / RAW_FRAME_NS))
            .collect::<Vec<_>>();
        use UnitKind::*;
        assert_eq!(
            kinds,
            [
                (Config, false, 0),
                (Frame, true, 0),
                (Frame, false, 1),
                (Frame, false, 2),
                (Config, false, 3),
                (Frame, true, 3)
            ]
        );
        assert_eq!(clip.read(&clip.units[0]).unwrap(), annex(&[SPS, PPS]));
        assert_eq!(
            clip.read(&clip.units[1]).unwrap(),
            annex(&[sei, idr_a, idr_b])
        );
        assert_eq!(clip.units[1].ranges.len(), 1);
        // The delimiter is dropped.
        assert_eq!(clip.read(&clip.units[3]).unwrap(), annex(&[p2]));
        assert!(droppable(VideoCodec::H264, &annex(&[p2])));
        assert!(!droppable(VideoCodec::H264, &annex(&[p1])));
        assert!(!droppable(VideoCodec::H264, &annex(&[sei])));
        assert_eq!(clip.next_key(2), Some(5));
        assert_eq!(clip.config_before(5), Some(4));
        let mut index = Vec::new();
        clip.write_index(&mut index).unwrap();
        let copy = Clip::from_index(
            "raw.h264",
            std::str::from_utf8(&index).unwrap(),
            ClipData::Memory(bytes.into()),
        )
        .unwrap();
        assert_eq!(copy.units.len(), clip.units.len());
        assert!(copy
            .units
            .iter()
            .zip(&clip.units)
            .all(|(a, b)| a.at_ns == b.at_ns && a.key == b.key));
    }
    #[test]
    fn avcc_from_parameter_sets_parses_like_a_session_config() {
        let config = decoder_config(VideoCodec::H264, &annex(&[SPS, PPS])).unwrap();
        let format = oxp_core::model::VideoFormat::create(
            0,
            oxp_core::model::VideoCodec::H264,
            config.into(),
            None,
        )
        .unwrap();
        assert_eq!(format.codec_string, "avc1.640020");
        assert_eq!((format.width, format.height), (1280, 720));
        assert!(decoder_config(VideoCodec::H264, &annex(&[PPS])).is_err());
        assert!(decoder_config(VideoCodec::H265, &annex(&[SPS, PPS])).is_err());
    }
    #[test]
    fn selection_prefers_long_clips_that_fit_the_frame_rate_cap() {
        let make = |name: &str, frames: usize, ms: u64| {
            let (data, index) = recording_with(&vec![ms; frames], None);
            Clip::from_index(name, &index, ClipData::Memory(data.into())).unwrap()
        };
        // Sender intervals of 16.7 ms (60 fps) and 33.7 ms (30 fps).
        let library = ClipLibrary::from_clips(vec![
            make("long60", 600, 16),
            make("short30", 30, 33),
            make("mid30", 200, 33),
        ]);
        assert_eq!(library.select(VideoCodec::H264, 30).unwrap().name, "mid30");
        assert_eq!(library.select(VideoCodec::H264, 60).unwrap().name, "long60");
        // A much shorter clip that fits loses to pacing the long one.
        let library =
            ClipLibrary::from_clips(vec![make("long60", 600, 16), make("short30", 30, 33)]);
        assert_eq!(library.select(VideoCodec::H264, 30).unwrap().name, "long60");
        assert_eq!(library.select(VideoCodec::H265, 30).unwrap().name, "long60");
    }
    #[test]
    fn no_key_frame_with_config_is_rejected() {
        let bytes = annex(&[&[0x65, 0x88, 0x01], SPS, PPS, &[0x41, 0x9a, 0x01]]);
        let e = Clip::from_raw(
            "x.h264",
            VideoCodec::H264,
            &bytes,
            ClipData::Memory(bytes.clone().into()),
        )
        .unwrap_err();
        assert!(e.to_string().contains("no key frame with a codec config"));
    }
}
