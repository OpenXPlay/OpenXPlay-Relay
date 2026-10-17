// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// LPCM microphone tests; Opus downlink remains encoded.
use carplay_airplay::{AirPlayCrypto, AudioCodecKind, MicrophoneConfig};
use openxplay_relay::MicrophoneFrames;
fn config(channels: u8, ms: u32) -> MicrophoneConfig {
    MicrophoneConfig {
        audio_type: "speechrecognition".into(),
        sample_rate: 16000,
        channels,
        payload_type: 96,
        frame_millis: ms,
        host: "127.0.0.1".parse().unwrap(),
        port: 1234,
        key: (0..32).collect(),
        codec: AudioCodecKind::Lpcm,
        bitrate: None,
        opus_clock_rate: 24000,
    }
}
fn open(c: &MicrophoneConfig, p: &[u8]) -> Vec<u8> {
    let end = p.len() - 8;
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&p[end..]);
    AirPlayCrypto::chacha_open(
        c.key.as_slice().try_into().unwrap(),
        &nonce,
        &p[12..end],
        &p[4..12],
    )
    .unwrap()
}
fn timestamp(p: &[u8]) -> u32 {
    u32::from_be_bytes(p[4..8].try_into().unwrap())
}
#[test]
fn tick_twenty_ms_and_fills_missing_frames_with_silence() {
    let c = config(1, 20);
    let f = MicrophoneFrames::new(c.clone());
    assert!(!f.offer(&[0; 12]).unwrap());
    let mut pcm = vec![0; 640];
    pcm[..2].copy_from_slice(&[0x34, 0x12]);
    assert!(f.offer(&pcm).unwrap());
    pcm[0] = 0;
    let packets = (0..3).flat_map(|_| f.tick().unwrap()).collect::<Vec<_>>();
    assert_eq!(packets.len(), 3);
    assert_eq!(&open(&c, &packets[0])[..2], [0x12, 0x34]);
    assert_eq!(open(&c, &packets[1]), [0; 640]);
    assert_eq!(open(&c, &packets[2]), [0; 640]);
    assert_eq!(
        packets.iter().map(|p| timestamp(p)).collect::<Vec<_>>(),
        [0, 320, 640]
    );
}
#[test]
fn mono_capture_expands_to_channels_and_packet_duration() {
    let c = config(2, 10);
    let f = MicrophoneFrames::new(c.clone());
    let mut pcm = vec![0; 640];
    pcm[..2].copy_from_slice(&[0x34, 0x12]);
    assert!(f.offer(&pcm).unwrap());
    let packets = f.tick().unwrap();
    assert_eq!(packets.len(), 2);
    assert_eq!(&open(&c, &packets[0])[..4], [0x12, 0x34, 0x12, 0x34]);
    assert_eq!(
        packets.iter().map(|p| timestamp(p)).collect::<Vec<_>>(),
        [0, 160]
    );
}
#[test]
fn queue_drops_oldest_and_remains_bounded() {
    let c = config(1, 20);
    let f = MicrophoneFrames::new(c.clone());
    for i in 0..8 {
        let mut pcm = vec![0; 640];
        pcm[0] = i;
        f.offer(&pcm).unwrap();
    }
    let packets = (0..6).flat_map(|_| f.tick().unwrap()).collect::<Vec<_>>();
    assert_eq!(
        packets.iter().map(|p| open(&c, p)[1]).collect::<Vec<_>>(),
        [3, 4, 5, 6, 7, 0]
    );
}
