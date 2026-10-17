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

/// `MicrophoneConfig` protocol data.
#[derive(Clone, Debug)]
pub struct MicrophoneConfig {
    pub audio_type: String,

    pub sample_rate: u32,

    pub channels: u8,

    pub payload_type: u8,

    pub frame_millis: u32,

    pub host: std::net::IpAddr,

    pub port: u16,

    pub key: Vec<u8>,

    pub codec: AudioCodecKind,
    /// Optional encoder target; encoding is a platform concern.
    pub bitrate: Option<u32>,
    /// Negotiated RTP clock, independent of Opus capture rate.
    pub opus_clock_rate: u32,
}
/// State owned by `MicrophonePacketizer`; construct through its factory.
pub struct MicrophonePacketizer {
    config: MicrophoneConfig,
    counters: MicrophoneCounters,
}
impl MicrophonePacketizer {
    pub fn new(config: MicrophoneConfig) -> Result<Self> {
        if config.key.len() != 32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "microphone key must be 32 bytes",
            ));
        }
        Ok(Self {
            config,
            counters: MicrophoneCounters {
                sequence: 0,
                timestamp: 0,
                nonce: 0,
            },
        })
    }

    pub fn packetize(&mut self, encoded: &[u8]) -> Result<Vec<u8>> {
        let body = if self.config.codec == AudioCodecKind::Lpcm {
            Self::to_wire_pcm(encoded)?
        } else {
            encoded.to_vec()
        };
        Self::seal_packet(
            self.config.key.as_slice().try_into().unwrap(),
            self.config.payload_type,
            &mut self.counters,
            &body,
            self.config.rtp_samples_per_packet(),
        )
    }
}

/// Protocol message, field, or framing constant.
pub const OPUS_SAMPLES_PER_PACKET: u64 = 960;
/// Protocol message, field, or framing constant.
pub const OPUS_CAPTURE_RATE: u64 = 48_000;
/// Protocol message, field, or framing constant.
pub const OPUS_FRAME_MILLIS: u64 = 20;
/// Protocol message, field, or framing constant.
pub const RTP_HEADER_LEN: u64 = 12;
/// Protocol message, field, or framing constant.
pub const TAG_LEN: u64 = 16;
/// Protocol message, field, or framing constant.
pub const NONCE_LEN: u64 = 8;

/// Mutable RTP sequence, sample timestamp and AEAD nonce counters.
#[derive(Clone, Debug)]
pub struct MicrophoneCounters {
    /// RTP sequence, wrapping at 16 bits.
    pub sequence: u16,
    /// Negotiated RTP sample clock, wrapping at 32 bits.
    pub timestamp: u32,
    /// Independent little-endian nonce counter.
    pub nonce: u64,
}

impl MicrophonePacketizer {
    pub fn seal_packet(
        key: &[u8; 32],
        payload_type: u8,
        counters: &mut MicrophoneCounters,
        body: &[u8],
        samples: u32,
    ) -> Result<Vec<u8>> {
        use chacha20poly1305::{
            aead::{Aead, Payload},
            ChaCha20Poly1305, KeyInit,
        };
        let mut header = [0u8; 12];
        header[0] = 0x80;
        header[1] = payload_type & 0x7f;
        header[2..4].copy_from_slice(&counters.sequence.to_be_bytes());
        header[4..8].copy_from_slice(&counters.timestamp.to_be_bytes());
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&counters.nonce.to_le_bytes());
        let sealed = ChaCha20Poly1305::new(key.into())
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: body,
                    aad: &header[4..],
                },
            )
            .map_err(|_| std::io::Error::other("microphone encryption failed"))?;
        let mut packet = header.to_vec();
        packet.extend_from_slice(&sealed);
        packet.extend_from_slice(&nonce[4..]);
        counters.sequence = counters.sequence.wrapping_add(1);
        counters.timestamp = counters.timestamp.wrapping_add(samples);
        counters.nonce = counters.nonce.wrapping_add(1);
        Ok(packet)
    }

    pub fn to_wire_pcm(pcm_little_endian: &[u8]) -> Result<Vec<u8>> {
        let mut output = pcm_little_endian.to_vec();
        for pair in output.chunks_exact_mut(2) {
            pair.swap(0, 1);
        }
        Ok(output)
    }
}

impl MicrophoneConfig {
    pub fn samples_per_packet(&self) -> u32 {
        if self.codec == AudioCodecKind::Opus {
            960
        } else {
            ((self.sample_rate as u64 * self.frame_millis as u64 / 1000).max(1)) as u32
        }
    }

    pub fn rtp_samples_per_packet(&self) -> u32 {
        if self.codec == AudioCodecKind::Opus {
            self.opus_clock_rate * 20 / 1000
        } else {
            self.samples_per_packet()
        }
    }

    pub fn frame_bytes(&self) -> usize {
        self.samples_per_packet() as usize * self.channels as usize * 2
    }
}

impl MicrophoneConfig {
    pub fn opus_clock_rate(bits: u64) -> u32 {
        if bits & 0x40000000 != 0 {
            48000
        } else if bits & 0x20000000 != 0 {
            24000
        } else if bits & 0x10000000 != 0 {
            16000
        } else {
            48000
        }
    }
}
