// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
use carplay_airplay::*;
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
fn config(codec: AudioCodecKind, clock: u32) -> MicrophoneConfig {
    MicrophoneConfig {
        audio_type: "speechrecognition".into(),
        sample_rate: 48000,
        channels: 1,
        payload_type: 100,
        frame_millis: 20,
        host: "127.0.0.1".parse().unwrap(),
        port: 1,
        key: (1..=32).collect(),
        codec,
        bitrate: None,
        opus_clock_rate: clock,
    }
}
fn packet_sequence(bits: u64, step: u32) {
    let c = config(
        AudioCodecKind::Opus,
        MicrophoneConfig::opus_clock_rate(bits),
    );
    assert_eq!(c.samples_per_packet(), 960);
    assert_eq!(c.frame_bytes(), 1920);
    let mut counters = MicrophoneCounters {
        sequence: 0,
        timestamp: 0,
        nonce: 0,
    };
    for i in 0..3 {
        let payload = [0xf8, i as u8, 0x31, 0x42];
        let wire = MicrophonePacketizer::seal_packet(
            &c.key.clone().try_into().unwrap(),
            100,
            &mut counters,
            &payload,
            c.rtp_samples_per_packet(),
        )
        .unwrap();
        assert_eq!(u16::from_be_bytes(wire[2..4].try_into().unwrap()), i as u16);
        assert_eq!(u32::from_be_bytes(wire[4..8].try_into().unwrap()), i * step);
        assert_eq!(
            u64::from_le_bytes(wire[wire.len() - 8..].try_into().unwrap()),
            i as u64
        );
        assert_eq!(
            &AudioStreamCodec::open(&c.key.clone().try_into().unwrap(), &wire).unwrap()[12..],
            payload
        );
    }
    assert_eq!(counters.timestamp, 3 * step);
    assert_eq!(counters.sequence, 3);
    assert_eq!(counters.nonce, 3);
}
#[test]
fn packet_uses_rtp_header_and_input_key_aad() {
    let key: [u8; 32] = std::array::from_fn(|i| i as u8 + 1);
    let mut c = MicrophoneCounters {
        sequence: 0,
        timestamp: 0,
        nonce: 0,
    };
    let wire = MicrophonePacketizer::seal_packet(&key, 100, &mut c, &[0x28, 1, 2, 3], 480).unwrap();
    assert_eq!(&wire[..12], &[0x80, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(
        &AudioStreamCodec::open(&key, &wire).unwrap()[12..],
        &[0x28, 1, 2, 3]
    );
    assert_eq!((c.sequence, c.timestamp, c.nonce), (1, 480, 1));
}
#[test]
fn pcm_is_converted_to_big_endian() {
    assert_eq!(
        MicrophonePacketizer::to_wire_pcm(&[0x34, 0x12, 0x78, 0x56]).unwrap(),
        [0x12, 0x34, 0x56, 0x78]
    );
}
#[test]
fn pcm_odd_trailing_byte_is_retained() {
    assert_eq!(
        MicrophonePacketizer::to_wire_pcm(&[1, 2, 3]).unwrap(),
        [2, 1, 3]
    );
}
#[test]
fn opus_timestamps_use_negotiated_clock() {
    for (bits, clock) in [
        (0x10000000, 16000),
        (0x20000000, 24000),
        (0x40000000, 48000),
        (0, 48000),
    ] {
        assert_eq!(MicrophoneConfig::opus_clock_rate(bits), clock);
    }
    let mut pcm = config(AudioCodecKind::Lpcm, 48000);
    pcm.sample_rate = 16000;
    assert_eq!(pcm.rtp_samples_per_packet(), 320);
}
#[test]
fn observed_siri_packets_use_480_ticks() {
    packet_sequence(0x20000000, 480);
}
#[test]
fn call_and_default_packets_use_960_ticks() {
    for bits in [0x40000000, 0, 0x60000000] {
        packet_sequence(bits, 960);
    }
}
#[test]
fn opus_16khz_packets_use_320_ticks() {
    packet_sequence(0x10000000, 320);
}
#[test]
fn counters_wrap_independently() {
    let mut c = MicrophoneCounters {
        sequence: u16::MAX,
        timestamp: u32::MAX - 319,
        nonce: u64::MAX,
    };
    MicrophonePacketizer::seal_packet(&[0; 32], 255, &mut c, b"opus", 320).unwrap();
    assert_eq!((c.sequence, c.timestamp, c.nonce), (0, 0, 0));
}
fn gate() -> (MicrophoneStart, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let count = starts.clone();
    (
        MicrophoneStart::after_setup_response(Arc::new(move || {
            count.fetch_add(1, Ordering::SeqCst);
        })),
        starts,
    )
}
fn eventually(f: impl Fn() -> bool) {
    let end = std::time::Instant::now() + Duration::from_secs(2);
    while !f() && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(f());
}
#[test]
fn voice_message_without_downlink_starts_once() {
    let (g, c) = gate();
    g.on_setup_response_sent();
    g.schedule_silent_start(Duration::ZERO, Arc::new(|| true))
        .unwrap();
    eventually(|| c.load(Ordering::SeqCst) == 1);
    g.on_downlink();
    assert_eq!(c.load(Ordering::SeqCst), 1);
    g.close();
}
#[test]
fn first_downlink_cancels_silent_start() {
    let (g, c) = gate();
    g.on_setup_response_sent();
    g.schedule_silent_start(Duration::from_secs(60), Arc::new(|| panic!("cancelled")))
        .unwrap();
    g.on_downlink();
    g.on_downlink();
    assert_eq!(c.load(Ordering::SeqCst), 1);
    g.close();
}
#[test]
fn torn_down_stream_never_starts_late() {
    let (g, c) = gate();
    g.on_setup_response_sent();
    let checked = Arc::new(AtomicUsize::new(0));
    let check = checked.clone();
    g.schedule_silent_start(
        Duration::ZERO,
        Arc::new(move || {
            check.fetch_add(1, Ordering::SeqCst);
            false
        }),
    )
    .unwrap();
    eventually(|| checked.load(Ordering::SeqCst) == 1);
    assert_eq!(c.load(Ordering::SeqCst), 0);
    g.close();
}
#[test]
fn calls_wait_for_downlink() {
    assert!(!MicrophoneStart::starts_without_downlink("telephony"));
    assert!(MicrophoneStart::starts_without_downlink(
        "speechrecognition"
    ));
    assert!(MicrophoneStart::starts_without_downlink("default"));
}
#[test]
fn downlink_cannot_start_before_setup_response() {
    let (g, c) = gate();
    g.on_downlink();
    assert_eq!(c.load(Ordering::SeqCst), 0);
    g.on_setup_response_sent();
    g.on_setup_response_sent();
    assert_eq!(c.load(Ordering::SeqCst), 1);
}
#[test]
fn close_cancels_delayed_microphone() {
    let (g, c) = gate();
    g.on_setup_response_sent();
    g.close();
    g.on_downlink();
    g.schedule_silent_start(Duration::ZERO, Arc::new(|| true))
        .unwrap();
    assert_eq!(c.load(Ordering::SeqCst), 0);
}
#[test]
fn measures_rtp_clock_once_after_window() {
    let mut p = RtpClockProbe::with_window(1_000_000_000);
    assert_eq!(p.observe(1000, 0), None);
    assert_eq!(p.observe(9000, 500_000_000), None);
    assert_eq!(p.observe(17000, 1_000_000_000), Some("16000".into()));
    assert_eq!(p.observe(33000, 2_000_000_000), None);
}
#[test]
fn clock_counts_across_unsigned_wrap() {
    let mut p = RtpClockProbe::with_window(1_000_000_000);
    p.observe((-24000i32) as u32, 0);
    assert_eq!(p.observe(24000, 1_000_000_000), Some("48000".into()));
}
#[test]
fn feedback_first_answer_then_only_overruns() {
    let m = FeedbackPositionMonitor::with_repeat(48000, 10000);
    assert_eq!(m.on_feedback(102, 7, 1000, 0), None);
    m.observe_rtp(48000);
    assert_eq!(
        m.on_feedback(102, 7, 24000, 0).unwrap(),
        "Audio: feedback type=102 id=7 reported=24000 received=48000 aheadMs=-500"
    );
    assert_eq!(m.on_feedback(102, 7, 48960, 1), None);
    assert_eq!(
        m.on_feedback(102, 7, 72000, 2).unwrap(),
        "Audio: feedback type=102 id=7 reported=72000 received=48000 aheadMs=500 overrun=true"
    );
    assert_eq!(m.on_feedback(102, 7, 96000, 9000), None);
    assert!(m
        .on_feedback(102, 7, 120000, 10002)
        .unwrap()
        .ends_with("aheadMs=1500 overrun=true"));
}
#[test]
fn feedback_newest_sample_survives_wrap_and_reordering() {
    let m = FeedbackPositionMonitor::new(48000);
    m.observe_rtp((-960i32) as u32);
    m.observe_rtp((-1920i32) as u32);
    m.observe_rtp(960);
    assert_eq!(
        m.on_feedback(100, 0, 0, 0).unwrap(),
        "Audio: feedback type=100 id=0 reported=0 received=960 aheadMs=-20"
    );
    assert_eq!(
        FeedbackPositionMonitor::samples_ahead(24000, u32::MAX - 23999),
        48000
    );
}
#[test]
fn opus_capture_rate_keeps_negotiated_rtp_clock() {
    for (bits, clock) in [
        (0x10000000, 16000),
        (0x20000000, 24000),
        (0x40000000, 48000),
    ] {
        let f = AudioStreamCodec::from_format_bits(bits, 100, "telephony").unwrap();
        assert_eq!(f.codec, AudioCodecKind::Opus);
        assert_eq!(f.sample_rate, 48000);
        assert_eq!(f.rtp_clock_rate, clock);
    }
}
#[test]
fn other_formats_count_at_sample_rate() {
    for (bits, clock) in [(0x800000, 48000), (0x400000, 44100), (0x10, 16000)] {
        assert_eq!(
            AudioStreamCodec::from_format_bits(bits, 102, "media")
                .unwrap()
                .rtp_clock_rate,
            clock
        );
    }
}
#[test]
fn alac_formats_are_compressed_and_preserve_sample_depth() {
    for (bits, rate, depth) in [
        (0x40000, 44100, 16),
        (0x80000, 44100, 24),
        (0x100000, 48000, 16),
        (0x200000, 48000, 24),
    ] {
        let f = AudioStreamCodec::from_format_bits(bits, 102, "media").unwrap();
        assert_eq!(
            f.codec,
            AudioCodecKind::Alac {
                bits_per_sample: depth
            }
        );
        assert_eq!(f.sample_rate, rate);
    }
}
#[test]
fn valid_nal_lengths_become_annex_b_in_place() {
    let mut p = vec![0, 0, 0, 2, 0x40, 1, 0, 0, 0, 3, 0x42, 1, 2];
    let pointer = p.as_ptr();
    ScreenCodec::length_prefixed_to_annex_b(&mut p).unwrap();
    assert_eq!(pointer, p.as_ptr());
    assert_eq!(p, [0, 0, 0, 1, 0x40, 1, 0, 0, 0, 1, 0x42, 1, 2]);
}
#[test]
fn malformed_nal_lengths_leave_all_bytes_untouched() {
    for mut p in [
        vec![0, 0, 0, 5, 0x40, 1],
        vec![0, 0, 0, 2, 0x40, 1, 0, 0, 0, 9, 0x41],
        vec![0, 0, 0, 0],
        vec![0, 0, 0, 2, 0x40, 1, 0],
    ] {
        let old = p.clone();
        ScreenCodec::length_prefixed_to_annex_b(&mut p).unwrap();
        assert_eq!(p, old);
    }
}
#[test]
fn reads_sender_frame_time_from_header() {
    let mut h = [0; 128];
    let raw = (3u64 << 32) | 0x40000000;
    h[8..16].copy_from_slice(&raw.to_le_bytes());
    assert_eq!(ScreenCodec::sender_nanos(&h), 3_250_000_000);
    h[8..16].copy_from_slice(&(raw + 0x04444444).to_le_bytes());
    assert_eq!(ScreenCodec::sender_nanos(&h) - 3_250_000_000, 16_666_666);
    assert_eq!(ScreenCodec::sender_nanos(&[0; 12]), 0);
}
#[test]
fn detects_embedded_and_raw_video_configs() {
    for (tag, codec) in [(b"hvcC", VideoCodec::H265), (b"avcC", VideoCodec::H264)] {
        let mut p = vec![0; 4];
        p.extend_from_slice(tag);
        p.extend_from_slice(&[1, 2, 3]);
        let (c, d) = ScreenCodec::detect_config(&p).unwrap();
        assert_eq!(c, codec);
        assert_eq!(d, [1, 2, 3]);
        // A sized box stops at its end, so a following box is not part of the config.
        let mut p = vec![0, 0, 0, 11];
        p.extend_from_slice(tag);
        p.extend_from_slice(&[1, 2, 3, 0, 0, 0, 8, b'c', b'o', b'l', b'r']);
        assert_eq!(
            ScreenCodec::detect_config(&p).unwrap(),
            (codec, vec![1, 2, 3])
        );
    }
    assert_eq!(
        ScreenCodec::detect_config(&[1, 0, 0, 0, 0, 1, 0, 1, 0x67])
            .unwrap()
            .0,
        VideoCodec::H264
    );
    assert_eq!(
        ScreenCodec::detect_config(&[1, 0]).unwrap().0,
        VideoCodec::H265
    );
}
#[test]
fn screen_decryption_authenticates_entire_header() {
    use chacha20poly1305::{
        aead::{Aead, Payload},
        ChaCha20Poly1305, KeyInit,
    };
    let key = [7; 32];
    let h = [0; 128];
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&42u64.to_le_bytes());
    let sealed = ChaCha20Poly1305::new((&key).into())
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: b"encoded frame",
                aad: &h,
            },
        )
        .unwrap();
    assert_eq!(
        ScreenCodec::decrypt_frame(&key, 42, &h, &sealed).unwrap(),
        b"encoded frame"
    );
    let mut bad = h;
    bad[127] = 1;
    assert!(ScreenCodec::decrypt_frame(&key, 42, &bad, &sealed).is_err());
    assert!(ScreenCodec::decrypt_frame(&key, 43, &h, &sealed).is_err());
    assert_eq!(
        ScreenCodec::decrypt_frame(&key, 42, &h, b"short").unwrap(),
        b"short"
    );
}
fn stats() -> (StreamReceiveStats, Arc<AtomicU64>, Arc<Mutex<Vec<String>>>) {
    let time = Arc::new(AtomicU64::new(0));
    let t = time.clone();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let l = lines.clone();
    (
        StreamReceiveStats::with_clock(
            "audio",
            Arc::new(move || t.load(Ordering::SeqCst)),
            Arc::new(move |s| l.lock().unwrap().push(s)),
        ),
        time,
        lines,
    )
}
fn packet(
    stats: &StreamReceiveStats,
    time: &AtomicU64,
    sequence: u16,
    read_ms: u64,
    process_ns: u64,
) {
    stats.reading();
    time.fetch_add(read_ms * 1_000_000, Ordering::SeqCst);
    stats.received_rtp(100, Some(sequence), None, time.load(Ordering::SeqCst));
    time.fetch_add(process_ns, Ordering::SeqCst);
    stats.processed();
}
#[test]
fn gaps_link_to_same_long_read_and_only_window_counters_reset() {
    let (s, t, l) = stats();
    packet(&s, &t, 1, 10, 0);
    packet(&s, &t, 4, 300, 0);
    packet(&s, &t, 7, 10, 0);
    packet(&s, &t, 8, 400, 0);
    s.flush(true);
    assert!(l.lock().unwrap()[0].contains(
        "windowMs=720 readsOver250Ms=2 seqGapAfterReadOver250Ms=1 seqMissingAfterReadOver250Ms=2"
    ));
    packet(&s, &t, 9, 10, 0);
    s.flush(true);
    let lines = l.lock().unwrap();
    assert!(lines[1].contains(
        "windowMs=10 readsOver250Ms=0 seqGapAfterReadOver250Ms=0 seqMissingAfterReadOver250Ms=0"
    ));
    assert!(lines[1].contains("seqForwardGaps=0"));
}
#[test]
fn socket_wait_is_separate_from_processing_and_sequence_wrap() {
    let (s, t, l) = stats();
    for seq in [65535, 0, 3, 2, 4] {
        packet(&s, &t, seq, 400, 200000);
    }
    s.flush(true);
    let lines = l.lock().unwrap();
    assert!(lines[0].contains("readMaxMs=400 processMaxUs=200 seqForwardGaps=2 lateOrDuplicate=1"));
    assert!(lines[0].contains("packets=5 bytes=500"));
}
#[test]
fn reports_retain_sequence_continuity() {
    let (s, t, l) = stats();
    s.reading();
    t.store(5_000_000_000, Ordering::SeqCst);
    s.received_rtp(10, Some(1), None, 5_000_000_000);
    s.processed();
    s.reading();
    s.received_rtp(20, Some(3), None, 5_000_000_000);
    s.processed();
    s.flush(true);
    let lines = l.lock().unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("packets=1 bytes=20 readMaxMs=0"));
    assert!(lines[1].contains("seqForwardGaps=1"));
}
#[test]
fn decrypt_timing_is_reported_only_when_recorded_and_reset() {
    let (s, t, l) = stats();
    s.received(1000, 0);
    s.decrypted(2_000_000, 500000);
    s.decrypted(500000, 250000);
    s.processed();
    t.store(5_000_000_000, Ordering::SeqCst);
    s.flush(false);
    assert!(l.lock().unwrap()[0].ends_with(" decryptAvgUs=1250 decryptMaxUs=2000 decryptMBps=300"));
    t.store(10_000_000_000, Ordering::SeqCst);
    s.flush(false);
    assert!(!l.lock().unwrap()[1].contains("decrypt"));
}
fn map(entries: Vec<(&str, BplistValue)>) -> BplistValue {
    BplistValue::Dictionary(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
#[test]
fn calls_and_siri_always_request_microphone() {
    for kind in ["telephony", "speechrecognition"] {
        assert!(CarPlayMediaEngine::wants_microphone(kind, &map(vec![])));
    }
}
#[test]
fn app_audio_records_only_when_iphone_requests_input() {
    for kind in ["default", "compatibility"] {
        assert!(CarPlayMediaEngine::wants_microphone(
            kind,
            &map(vec![("dataPort", BplistValue::Integer(50000.into()))])
        ));
        assert!(!CarPlayMediaEngine::wants_microphone(kind, &map(vec![])));
        for input in [BplistValue::Boolean(false), BplistValue::Integer(0.into())] {
            assert!(!CarPlayMediaEngine::wants_microphone(
                kind,
                &map(vec![
                    ("input", input),
                    ("dataPort", BplistValue::Integer(50000.into()))
                ])
            ));
        }
        assert!(CarPlayMediaEngine::wants_microphone(
            kind,
            &map(vec![("input", BplistValue::Boolean(true))])
        ));
    }
}
#[test]
fn media_and_alerts_never_request_microphone() {
    for kind in ["media", "alert"] {
        assert!(!CarPlayMediaEngine::wants_microphone(
            kind,
            &map(vec![
                ("input", BplistValue::Boolean(true)),
                ("dataPort", BplistValue::Integer(50000.into()))
            ])
        ));
    }
}
#[test]
fn buffered_anchor_uses_unix_epoch_and_64_bit_fraction() {
    let ntp = ((2_208_988_800 + 131959) << 32) | 0x80000000u64;
    let BplistValue::Dictionary(a) = BufferedAudioStream::anchor_plist(0x10, ntp, 1) else {
        panic!()
    };
    let value = |k: &str| a.iter().find(|(key, _)| key == k).unwrap().1.clone();
    for (k, n) in [
        ("rtpTime", 16u64),
        ("networkTimeSecs", 131959),
        ("networkTimeFrac", 1 << 63),
        ("rate", 1),
    ] {
        let BplistValue::Integer(v) = value(k) else {
            panic!()
        };
        assert_eq!(v, num_bigint::BigInt::from(n));
    }
}
#[test]
fn buffered_timestamps_compare_across_wrap() {
    assert!(BufferedAudioStream::before(0xffffff00, 0x100));
    assert!(!BufferedAudioStream::before(0x100, 0xffffff00));
    assert!(BufferedAudioStream::before(1000, 2048));
    assert!(!BufferedAudioStream::before(2048, 2048));
}
#[test]
fn buffered_frame_opens_and_keeps_rtp_header() {
    let key = [8; 32];
    let mut c = MicrophoneCounters {
        sequence: 41,
        timestamp: 0xffffffe0,
        nonce: 0x0807060504030201,
    };
    let wire = MicrophonePacketizer::seal_packet(&key, 96, &mut c, &[0x21, 1, 2], 1024).unwrap();
    let f = BufferedAudioStream::open(&key, &wire).unwrap();
    assert_eq!(f.sequence, 41);
    assert_eq!(f.timestamp, 0xffffffe0);
    assert_eq!(&f.rtp[..12], &wire[..12]);
    assert_eq!(&f.rtp[12..], &[0x21, 1, 2]);
    let mut bad = wire;
    bad[0] = 0x90;
    assert!(BufferedAudioStream::open(&key, &bad).is_none());
}
#[test]
fn ntp_conversion_keeps_unsigned_epoch_and_fraction() {
    assert_eq!(
        NtpClock::to_ntp(500_000_000),
        (2_208_988_800u64 << 32) | 0x80000000
    );
}
