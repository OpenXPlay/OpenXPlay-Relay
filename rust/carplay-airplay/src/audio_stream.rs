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

/// `AudioCodecKind` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCodecKind {
    AacLc,
    /// Lossless compressed payload; the source negotiates 16- or 24-bit samples.
    Alac {
        bits_per_sample: u8,
    },
    Opus,
    Lpcm,
}
/// `AudioFormat` protocol data.
#[derive(Clone, Debug)]
pub struct AudioFormat {
    pub codec: AudioCodecKind,

    pub sample_rate: u32,

    pub channels: u8,

    pub payload_type: u8,

    pub audio_type: String,

    pub rtp_clock_rate: u32,
}
/// Platform interface for `AudioListener`. Implementations must support bounded shutdown.
pub trait AudioListener: Send + Sync {
    fn on_started(&self, _format: &AudioFormat, _sample: u32) -> Result<()> {
        Ok(())
    }
    fn on_stopped(&self) -> Result<()> {
        Ok(())
    }
    fn on_packet(
        &self,
        _wire: &[u8],
        _rtp: Option<&[u8]>,
        _sample: Option<u32>,
        _error: Option<&std::io::Error>,
    ) {
    }
    /// Errors propagate to the lifecycle owner.
    fn on_rtp(&self, format: &AudioFormat, rtp: &[u8], sample: u32) -> Result<()>;
}
/// State owned by `AudioStreamCodec`; construct through its factory.
pub struct AudioStreamCodec {
    _private: (),
}
impl AudioStreamCodec {
    pub fn from_format_bits(bits: u64, payload_type: u8, audio_type: &str) -> Result<AudioFormat> {
        let (codec, sample_rate, channels) = if bits & 0x70000000 != 0 {
            (AudioCodecKind::Opus, 48000, 1)
        } else if bits & (AAC_LC_44K_STEREO | AAC_LC_48K_STEREO) != 0 {
            (
                AudioCodecKind::AacLc,
                if bits & AAC_LC_48K_STEREO != 0 {
                    48000
                } else {
                    44100
                },
                2,
            )
        } else if bits & 0x3c0000 != 0 {
            // AirPlay format bits 18..21 identify ALAC rate and sample depth.
            (
                AudioCodecKind::Alac {
                    bits_per_sample: if bits & 0x280000 != 0 { 24 } else { 16 },
                },
                if bits & 0x300000 != 0 { 48000 } else { 44100 },
                2,
            )
        } else {
            let (rate, channels) = match bits {
                0x4 => (8000, 1),
                0x8 => (8000, 2),
                0x10 => (16000, 1),
                0x20 => (16000, 2),
                0x40 => (24000, 1),
                0x80 => (24000, 2),
                0x100 => (32000, 1),
                0x200 => (32000, 2),
                0x400 => (44100, 1),
                0x800 => (44100, 2),
                0x4000 => (48000, 1),
                0x8000 => (48000, 2),
                _ => (44100, 2),
            };
            (AudioCodecKind::Lpcm, rate, channels)
        };
        Ok(AudioFormat {
            codec,
            sample_rate,
            channels,
            payload_type,
            audio_type: audio_type.into(),
            rtp_clock_rate: if codec == AudioCodecKind::Opus {
                MicrophoneConfig::opus_clock_rate(bits)
            } else {
                sample_rate
            },
        })
    }
}
/// State owned by `AudioStream`; construct through its factory.
pub struct AudioStream {
    transport: Arc<dyn StreamTransport>,
    format: AudioFormat,
    key: [u8; 32],
    worker: Arc<crate::screen_stream::Worker>,
    listening: std::sync::atomic::AtomicBool,
    diagnostic: Arc<dyn Fn(String) + Send + Sync>,
}
impl AudioStream {
    pub fn new(
        transport: Arc<dyn StreamTransport>,
        format: AudioFormat,
        key: Vec<u8>,
    ) -> Result<Self> {
        let key = key
            .try_into()
            .map_err(|_| crate::screen_stream::invalid("audio key must be 32 bytes"))?;
        Ok(Self {
            transport,
            format,
            key,
            worker: Arc::new(crate::screen_stream::Worker::default()),
            listening: std::sync::atomic::AtomicBool::new(false),
            diagnostic: Arc::new(|line| eprintln!("{line}")),
        })
    }
    pub fn with_diagnostic(mut self, diagnostic: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        self.diagnostic = diagnostic;
        self
    }

    pub fn listen(&self, listener: Arc<dyn AudioListener>) -> Result<(u16, u16)> {
        use std::sync::atomic::Ordering;
        if self.listening.swap(true, Ordering::AcqRel) {
            return Err(crate::screen_stream::invalid(
                "AudioStream is already running",
            ));
        }
        let result = (|| {
            let data = self.transport.bind_datagram("[::]:0".parse().unwrap())?;
            self.worker.add_datagram(data.clone())?;
            let _ = data.set_receive_buffer_bytes(AUDIO_RECEIVE_BUFFER_BYTES as usize);
            let control = self.transport.bind_datagram("[::]:0".parse().unwrap())?;
            self.worker.add_datagram(control.clone())?;
            let ports = (data.local_port()?, control.local_port()?);
            let worker = self.worker.clone();
            self.worker.spawn("airplay-rtcp-rx", move || {
                while !worker.closed() {
                    match control.recv(4096, Duration::from_millis(100)) {
                        Ok(Receive::Eof) => break,
                        _ => continue,
                    }
                }
            })?;
            let worker = self.worker.clone();
            let key = self.key;
            let format = self.format.clone();
            let diagnostic = self.diagnostic.clone();
            self.worker.spawn("airplay-audio-rx", move || {
                let stats = StreamReceiveStats::with_clock(&format!("audio type={}", format.payload_type), Arc::new(crate::stream_receive_stats::monotonic_nanos), diagnostic.clone());
                let mut received = 0u64; let mut decrypted = 0u64; let mut authentication_failures = 0u64;
                let mut started = false;
                while !worker.closed() {
                    stats.reading();
                    let wire = loop {
                        if worker.closed() { break None; }
                        match data.recv(4096, Duration::from_millis(100)) {
                            Ok(Receive::Data(wire)) => break Some(wire),
                            Ok(Receive::Eof) => break None,
                            // Poll timeouts are an implementation detail, not completed socket reads.
                            _ => continue,
                        }
                    };
                    let Some(wire) = wire else { break; };
                    received += 1;
                    let sample = wire
                        .get(4..8)
                        .map(|s| u32::from_be_bytes(s.try_into().unwrap()));
                    let seq = wire
                        .get(2..4)
                        .map(|s| u16::from_be_bytes(s.try_into().unwrap()));
                    stats.received_rtp(
                        wire.len(),
                        seq,
                        sample,
                        crate::stream_receive_stats::monotonic_nanos(),
                    );
                    match AudioStreamCodec::open(&key, &wire) {
                        Ok(rtp) => {
                            decrypted += 1;
                            if decrypted.is_multiple_of(PACKET_LOG_INTERVAL) {
                                diagnostic(format!("audio stream type={} decrypted={decrypted} authFailures={authentication_failures}",format.payload_type));
                            }
                            let sample = sample.unwrap();
                            listener.on_packet(&wire, Some(&rtp), Some(sample), None);
                            if worker.closed() {
                                break;
                            }
                            if !started {
                                started = true;
                                if listener.on_started(&format, sample).is_err() {
                                    break;
                                }
                            }
                            if listener.on_rtp(&format, &rtp, sample).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            if wire.len() < 36 {
                                if received == 1 { diagnostic(format!("audio stream type={} short packet bytes={}",format.payload_type,wire.len())); }
                            } else {
                                authentication_failures += 1;
                                if authentication_failures == 1 { diagnostic(format!("audio stream type={} first decrypt failure: {e}",format.payload_type)); }
                            }
                            listener.on_packet(&wire, None, sample, Some(&e));
                        },
                    }
                    stats.processed();
                }
                stats.flush(true);
            })?;
            Ok(ports)
        })();
        if result.is_err() {
            let _ = self.worker.close();
        }
        result
    }

    pub fn close(&self) -> Result<()> {
        self.worker.close()
    }
}

/// Protocol message, field, or framing constant.
pub const TAG: &str = "xcertplay-usb";
/// Protocol message, field, or framing constant.
pub const DATAGRAM_BYTES: u64 = 4_096;
/// Protocol message, field, or framing constant.
pub const RTP_HEADER_LEN: u64 = 12;
/// Protocol message, field, or framing constant.
pub const TAG_LEN: u64 = 16;
/// Protocol message, field, or framing constant.
pub const NONCE_LEN: u64 = 8;
/// Protocol message, field, or framing constant.
pub const FIRST_PACKET_LOG_COUNT: u64 = 3;
/// Protocol message, field, or framing constant.
pub const PACKET_LOG_INTERVAL: u64 = 100;
/// Protocol message, field, or framing constant.
pub const AAC_LC_44K_STEREO: u64 = 0x400000;
/// Protocol message, field, or framing constant.
pub const AAC_LC_48K_STEREO: u64 = 0x800000;

/// Constant expression: `512 * 1024`.
pub const AUDIO_RECEIVE_BUFFER_BYTES: u64 = 524288;

/// Constant expression: `TAG_LEN + NONCE_LEN`.
pub const TAIL_LEN: u64 = 24;

impl AudioStreamCodec {
    /// Open an encoded RTP packet, retaining its clear header and transport sample clock.
    pub fn open(key: &[u8; 32], wire: &[u8]) -> Result<Vec<u8>> {
        if wire.len() < 36 {
            return Err(crate::screen_stream::invalid(
                "audio packet shorter than RTP header plus tail",
            ));
        }
        let end = wire.len() - 8;
        let counter = u64::from_le_bytes(wire[end..].try_into().unwrap());
        let payload =
            crate::screen_stream::open_chacha(key, counter, &wire[12..end], &wire[4..12])?;
        let mut rtp = wire[..12].to_vec();
        rtp.extend_from_slice(&payload);
        Ok(rtp)
    }
}
