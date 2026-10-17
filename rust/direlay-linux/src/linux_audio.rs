// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use carplay_airplay::{AudioCodecKind, AudioFormat, AudioStreamId};
use oxp_core::model::{AudioCodec, AudioFormat as Target, AudioRole};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
pub struct AudioPolicy;
impl AudioPolicy {
    pub fn stream(id: &AudioStreamId) -> Option<u16> {
        match id.kind {
            102 => Some(0),
            100 => Some(1),
            101 => Some(2),
            _ => None,
        }
    }
    pub fn format(id: &AudioStreamId, source: &AudioFormat) -> Result<Target> {
        let stream =
            Self::stream(id).ok_or_else(|| std::io::Error::other("Unsupported audio stream"))?;
        let role = match id.audio_type.as_str() {
            "media" => AudioRole::Media,
            "alert" => AudioRole::Alert,
            "speechrecognition" => AudioRole::Voice,
            "telephony" => AudioRole::Telephony,
            _ => AudioRole::Other,
        };
        let codec = match source.codec {
            AudioCodecKind::AacLc => AudioCodec::AacLc,
            AudioCodecKind::Opus => AudioCodec::Opus,
            AudioCodecKind::Lpcm => AudioCodec::PcmS16le,
            AudioCodecKind::Alac { .. } => {
                return Err(std::io::Error::other(
                    "ALAC passthrough unavailable: Stream wire has no ALAC codec",
                ))
            }
        };
        if !(1..=2).contains(&source.channels)
            || !(8000..=48000).contains(&source.sample_rate)
            || (role == AudioRole::Media
                && (codec != AudioCodec::AacLc
                    || source.sample_rate != 48000
                    || source.channels != 2))
            || (role != AudioRole::Media && codec == AudioCodec::AacLc)
        {
            return Err(std::io::Error::other(format!(
                "Unsupported {} {:?} {} Hz/{}; no AAC transcoder",
                id.audio_type, source.codec, source.sample_rate, source.channels
            )));
        }
        Ok(Target {
            stream: stream.into(),
            role,
            codec,
            sample_rate: if source.codec == AudioCodecKind::Opus {
                48000
            } else {
                source.sample_rate
            },
            channels: source.channels.into(),
            config: (codec == AudioCodec::AacLc).then(|| Arc::from([0x11, 0x90])),
        })
    }
    pub fn swap(pcm: &[u8]) -> Result<Vec<u8>> {
        if !pcm.len().is_multiple_of(2) {
            return Err(std::io::Error::other("Odd PCM byte length"));
        }
        Ok(pcm.chunks_exact(2).flat_map(|p| [p[1], p[0]]).collect())
    }
}
pub struct AudioNormalizer {
    source: AudioFormat,
}
impl AudioNormalizer {
    pub fn new(source: AudioFormat) -> Self {
        Self { source }
    }
    pub fn convert(&mut self, payload: &[u8]) -> Result<(Vec<u8>, u32)> {
        let samples = match self.source.codec {
            AudioCodecKind::AacLc => 1024,
            AudioCodecKind::Lpcm => {
                let frame = self.source.channels as usize * 2;
                if frame == 0 || !payload.len().is_multiple_of(frame) {
                    return Err(std::io::Error::other("Invalid PCM frame length"));
                }
                return Ok((AudioPolicy::swap(payload)?, (payload.len() / frame) as u32));
            }
            AudioCodecKind::Opus => opus_samples(payload)?,
            AudioCodecKind::Alac { .. } => {
                return Err(std::io::Error::other("ALAC unsupported by Stream"))
            }
        };
        Ok((payload.to_vec(), samples))
    }
}
pub fn opus_samples(p: &[u8]) -> Result<u32> {
    let toc = *p
        .first()
        .ok_or_else(|| std::io::Error::other("Empty Opus packet"))?;
    let frame = if toc & 0x80 != 0 {
        120 << (toc >> 3 & 3)
    } else if toc & 0x60 == 0x60 {
        if toc & 8 != 0 {
            960
        } else {
            480
        }
    } else {
        match toc >> 3 & 3 {
            0 => 480,
            1 => 960,
            2 => 1920,
            _ => 2880,
        }
    };
    let count = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => p.get(1).copied().unwrap_or(0) & 63,
    };
    let samples = frame * count as u32;
    if samples == 0 || samples > 5760 {
        return Err(std::io::Error::other("Invalid Opus duration"));
    }
    Ok(samples)
}
pub const MAX_QUEUED_US: u64 = 1_500_000;
type PacketCallback = Arc<dyn Fn(Vec<u8>, i64) + Send + Sync>;
struct RunState {
    queue: std::collections::VecDeque<(Vec<u8>, u64, u64)>,
    queued_us: u64,
    bytes: usize,
    accepting: bool,
    done: bool,
}
pub struct AudioPolicyRun {
    source: AudioFormat,
    state: Arc<(Mutex<RunState>, std::sync::Condvar)>,
    cancelled: Arc<AtomicBool>,
    started: AtomicBool,
    packet: PacketCallback,
    end: Arc<dyn Fn() + Send + Sync>,
    predecessor: Option<Arc<AudioPolicyRun>>,
}
impl AudioPolicyRun {
    pub fn new(
        source: AudioFormat,
        predecessor: Option<Arc<Self>>,
        packet: PacketCallback,
        end: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            source,
            state: Arc::new((
                Mutex::new(RunState {
                    queue: Default::default(),
                    queued_us: 0,
                    bytes: 0,
                    accepting: true,
                    done: false,
                }),
                std::sync::Condvar::new(),
            )),
            cancelled: Arc::new(AtomicBool::new(false)),
            started: AtomicBool::new(false),
            packet,
            end,
            predecessor,
        }
    }
    pub fn start(&self) -> Result<()> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(std::io::Error::other("Audio run already started"));
        }
        let (state, cancelled, source, packet, end, predecessor) = (
            self.state.clone(),
            self.cancelled.clone(),
            self.source.clone(),
            self.packet.clone(),
            self.end.clone(),
            self.predecessor.clone(),
        );
        crate::platform::spawn_worker(
            "linux-audio",
            Box::new(move || {
                if let Some(p) = predecessor {
                    let (m, c) = &*p.state;
                    let mut s = m.lock().unwrap();
                    while !s.done && !cancelled.load(Ordering::Acquire) {
                        s = c
                            .wait_timeout(s, std::time::Duration::from_millis(50))
                            .unwrap()
                            .0;
                    }
                }
                let mut normal = AudioNormalizer::new(source.clone());
                let mut clock =
                    oxp_core::audio::AudioClock::new(if source.codec == AudioCodecKind::Opus {
                        48000
                    } else {
                        source.sample_rate
                    })
                    .unwrap();
                loop {
                    let (m, c) = &*state;
                    let mut s = m.lock().unwrap();
                    while s.queue.is_empty() && s.accepting && !cancelled.load(Ordering::Acquire) {
                        s = c.wait(s).unwrap();
                    }
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let Some((b, a, d)) = s.queue.pop_front() else {
                        break;
                    };
                    s.queued_us -= d;
                    s.bytes -= b.len();
                    drop(s);
                    if let Ok((data, n)) = normal.convert(&b) {
                        if let Ok(pts) = clock.timestamp(n, a as i64) {
                            packet(data, pts);
                        }
                    }
                }
                if !cancelled.load(Ordering::Acquire) {
                    end()
                }
                let (m, c) = &*state;
                m.lock().unwrap().done = true;
                c.notify_all();
            }),
        )?;
        Ok(())
    }
    pub fn offer(&self, rtp: &[u8], arrival_us: u64) -> Result<()> {
        if rtp.len() <= 12 || rtp.len() > 65536 {
            return Ok(());
        }
        let payload = &rtp[12..];
        let n = AudioNormalizer::new(self.source.clone())
            .convert(payload)?
            .1;
        let duration = n as u64 * 1_000_000
            / if self.source.codec == AudioCodecKind::Opus {
                48000
            } else {
                self.source.sample_rate as u64
            };
        if duration > MAX_QUEUED_US {
            return Ok(());
        }
        let (m, c) = &*self.state;
        let mut s = m.lock().unwrap();
        if !s.accepting {
            return Ok(());
        }
        while s.queue.len() >= 96
            || s.queued_us + duration > MAX_QUEUED_US
            || s.bytes + payload.len() > 1024 * 1024
        {
            if let Some((b, _, d)) = s.queue.pop_front() {
                s.queued_us -= d;
                s.bytes -= b.len();
            } else {
                break;
            }
        }
        s.bytes += payload.len();
        s.queued_us += duration;
        s.queue.push_back((payload.to_vec(), arrival_us, duration));
        c.notify_one();
        Ok(())
    }
    pub fn finish(&self) -> Result<()> {
        let (m, c) = &*self.state;
        m.lock().unwrap().accepting = false;
        c.notify_all();
        Ok(())
    }
    pub fn cancel(&self) -> Result<()> {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.finish();
        self.state.0.lock().unwrap().queue.clear();
        Ok(())
    }
}
