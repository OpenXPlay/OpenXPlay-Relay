// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

mod wired_common;
use carplay_iap2::transport::{
    ncm_usb_bridge::{retry_usb_setup, NcmFrameBuffer, NcmUsbBridge},
    ntb16_codec::Ntb16Codec,
    usb_mux_frame_buffer::{UsbMuxFrame, UsbMuxFrameBuffer},
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use wired_common::*;
fn ntb(frame: &[u8]) -> Vec<u8> {
    Ntb16Codec::build(frame, 1, 512).unwrap()
}
fn aligned() -> Vec<u8> {
    (0..484).map(|i| (i * 31) as u8).collect()
}
const SMALL: &[u8] = &[0x33, 0x33, 0, 0, 0, 1, 0x86, 0xdd];
fn unpadded(frame: &[u8]) -> Vec<u8> {
    let mut b = ntb(frame);
    assert_eq!(b.len() % 512, 1);
    b.pop();
    b
}
fn ncm(chunks: Vec<Vec<u8>>, packet: u16) -> (Vec<Vec<u8>>, Vec<String>) {
    let log = Arc::new(Mutex::new(vec![]));
    let capture = log.clone();
    let mut b = NcmFrameBuffer::new(packet, Arc::new(move |s| capture.lock().unwrap().push(s)));
    let mut frames = vec![];
    for chunk in chunks {
        b.append(&chunk).unwrap();
        while let Some(f) = b.take_frame().unwrap() {
            frames.push(f);
        }
    }
    let messages = log.lock().unwrap().clone();
    (frames, messages)
}
#[test]
fn ntb_range_maximum_and_overflow_vectors() {
    let b = ntb(SMALL);
    let buffer = [vec![0; 11], b.clone(), vec![0; 3]].concat();
    assert_eq!(
        Ntb16Codec::parse_range(&buffer, 11, b.len()).unwrap(),
        [SMALL]
    );
    let b = Ntb16Codec::build(&vec![0; 65507], 0x1234, 512).unwrap();
    assert_eq!(b.len(), 65535);
    assert_eq!(&b[8..10], &[255, 255]);
    assert!(Ntb16Codec::build(&vec![0; 65508], 0, 512).is_err());
    assert!(Ntb16Codec::build(&[], 0, 512).is_err());
    assert_eq!(Ntb16Codec::build(&[0; 36], 1, 64).unwrap().len(), 65);
    assert_eq!(
        Ntb16Codec::parse_range(&buffer, buffer.len(), 1)
            .unwrap_err()
            .to_string(),
        "NTB16 range is outside the source buffer"
    );
}
#[test]
fn ntb_malformed_entries_and_pointer_cycles_are_bounded() {
    for b in [vec![], vec![0; 12], vec![255; 100]] {
        assert!(Ntb16Codec::parse(&b).is_empty());
    }
    let mut b = ntb(SMALL);
    b[18..20].copy_from_slice(&12u16.to_le_bytes());
    assert!(!Ntb16Codec::parse(&b).is_empty());
    b[20..22].copy_from_slice(&65530u16.to_le_bytes());
    assert!(Ntb16Codec::parse(&b).is_empty());
}
#[test]
fn complete_aligned_and_adjacent_blocks_do_not_wait_for_pad() {
    let a = aligned();
    assert_eq!(
        ncm(vec![unpadded(&a)], 512).0.as_slice(),
        std::slice::from_ref(&a)
    );
    let second = (0..996).map(|i| (i * 17) as u8).collect::<Vec<_>>();
    assert_eq!(
        ncm(
            vec![[unpadded(&a), unpadded(&second), ntb(SMALL)].concat()],
            512
        )
        .0,
        [a, second, SMALL.to_vec()]
    );
}
#[test]
fn optional_zero_pad_coalesced_and_split_headers() {
    let a = aligned();
    assert_eq!(
        ncm(vec![[ntb(&a), ntb(SMALL)].concat()], 512).0,
        [a.clone(), SMALL.to_vec()]
    );
    for padded in [false, true] {
        let b = ntb(SMALL);
        let mut chunks = vec![unpadded(&a)];
        if padded {
            chunks.push(vec![0]);
        }
        chunks.extend([
            b[..1].to_vec(),
            b[1..3].to_vec(),
            b[3..12].to_vec(),
            b[12..].to_vec(),
        ]);
        assert_eq!(ncm(chunks, 512).0, [a.clone(), SMALL.to_vec()]);
    }
}
#[test]
fn invalid_pad_recovery_has_exact_diagnostic() {
    let a = aligned();
    let (frames, log) = ncm(vec![[unpadded(&a), vec![0x55]].concat(), ntb(SMALL)], 512);
    assert_eq!(frames, [a, SMALL.to_vec()]);
    assert_eq!(
        log,
        ["NCM resynchronized after Invalid NTB16 short-packet pad; skipped=1 resyncs=1"]
    );
}
#[test]
fn invalid_next_signature_extra_pads_and_unaligned_pad_recover() {
    let a = aligned();
    let mut malformed = ntb(SMALL);
    malformed[1] = 0x55;
    assert_eq!(
        ncm(vec![[unpadded(&a), malformed].concat(), ntb(&a)], 512).0,
        [a.clone(), a.clone()]
    );
    assert_eq!(
        ncm(vec![[ntb(&a), vec![0], ntb(SMALL)].concat()], 512).0,
        [a, SMALL.to_vec()]
    );
    assert_eq!(
        ncm(vec![[ntb(SMALL), vec![0], ntb(SMALL)].concat()], 512).0,
        [SMALL, SMALL]
    );
}
#[test]
fn full_speed_pad_is_expected_and_uses_out_packet_size() {
    let block = Ntb16Codec::build(&[7; 36], 1, 64).unwrap();
    assert_eq!(block.len(), 65);
    let (frames, log) = ncm(vec![[block, ntb(SMALL)].concat()], 64);
    assert_eq!(frames, [vec![7; 36], SMALL.to_vec()]);
    assert!(log.is_empty());
}
#[test]
fn cut_block_loses_only_the_damaged_region() {
    let b = ntb(&(0..996).map(|i| (i * 17) as u8).collect::<Vec<_>>());
    let following = (3..43).flat_map(|_| ntb(SMALL)).collect::<Vec<_>>();
    let (frames, _) = ncm(
        vec![
            [ntb(SMALL), b[..300].to_vec()].concat(),
            [b[700..].to_vec(), following].concat(),
        ],
        512,
    );
    assert_eq!(frames[0], SMALL);
    assert!(frames.len() >= 21);
    assert!(frames[frames.len() - 20..].iter().all(|f| f == SMALL));
}
#[test]
fn repeated_damage_and_queue_overflow_remain_terminal() {
    let mut b = NcmFrameBuffer::new(512, Arc::new(|_| {}));
    let mut error = None;
    for _ in 0..20 {
        b.append(&[ntb(SMALL), vec![0x55]].concat()).unwrap();
        loop {
            match b.take_frame() {
                Ok(Some(_)) => (),
                Ok(None) => break,
                Err(e) => {
                    error = Some(e.to_string());
                    break;
                }
            }
        }
        if error.is_some() {
            break;
        }
    }
    let error = error.unwrap();
    assert!(error.ends_with("(repeated)"));
    assert_eq!(b.take_frame().unwrap_err().to_string(), error);
    let mut b = NcmFrameBuffer::new(512, Arc::new(|_| {}));
    b.append(&(0..257).flat_map(|_| ntb(SMALL)).collect::<Vec<_>>())
        .unwrap();
    assert_eq!(
        b.take_frame().unwrap_err().to_string(),
        "NCM frame queue exceeded its bounds"
    );
}
#[test]
fn ncm_not_ready_timeout_then_callers_timeout_vectors() {
    for (results, expected) in [
        (vec![-1, -1, 36, 36], vec![20, 20, 20, 2000]),
        (vec![36, -1, 36], vec![20, 2000, 2000]),
        (vec![-1], vec![5]),
    ] {
        let usb = Arc::new(FakeUsb::default());
        usb.results.lock().unwrap().extend(results);
        let bridge = NcmUsbBridge::open(
            usb.clone(),
            function(512),
            [2, 0, 0, 0, 0, 2],
            Arc::new(|_| {}),
        )
        .unwrap();
        for _ in &expected {
            bridge
                .send(
                    SMALL,
                    Duration::from_millis(if expected.len() == 1 { 5 } else { 2000 }),
                )
                .unwrap();
        }
        assert_eq!(
            usb.writes
                .lock()
                .unwrap()
                .iter()
                .map(|(_, t)| t.as_millis())
                .collect::<Vec<_>>(),
            expected
        );
        bridge.close().unwrap();
        assert!(!usb.closed.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(*usb.released.lock().unwrap(), [3, 2]);
    }
}
#[test]
fn ncm_write_diagnostic_and_short_write() {
    let usb = Arc::new(FakeUsb::default());
    usb.results.lock().unwrap().extend([-1, -1, 36]);
    let log = Arc::new(Mutex::new(vec![]));
    let l = log.clone();
    let b = NcmUsbBridge::open(
        usb.clone(),
        function(512),
        [0; 6],
        Arc::new(move |s| l.lock().unwrap().push(s)),
    )
    .unwrap();
    for _ in 0..3 {
        b.send(SMALL, Duration::from_secs(2)).unwrap();
    }
    assert_eq!(
        *log.lock().unwrap(),
        ["NCM bulk-out ready after 2 dropped frames"]
    );
    usb.results.lock().unwrap().push_back(3);
    assert_eq!(
        b.send(SMALL, Duration::from_secs(1))
            .unwrap_err()
            .to_string(),
        "NCM write transferred 3 of 36 bytes"
    );
}
#[test]
fn ncm_setup_retry_vectors() {
    let mut attempts = 0;
    let mut pauses = vec![];
    assert_eq!(
        retry_usb_setup(
            || {
                attempts += 1;
                if attempts == 3 {
                    Ok(())
                } else {
                    Err(carplay_iap2::transport::iphone_usb_host::UsbError::InterfaceBusy(3))
                }
            },
            |t| pauses.push(t)
        )
        .unwrap(),
        3
    );
    assert_eq!(pauses, [Duration::from_millis(100); 2]);
    let mut pauses = vec![];
    assert!(retry_usb_setup(
        || Err(carplay_iap2::transport::iphone_usb_host::UsbError::InterfaceBusy(3)),
        |t| pauses.push(t)
    )
    .is_err());
    assert_eq!(pauses.len(), 4);
    assert_eq!(retry_usb_setup(|| Ok(()), |_| panic!()).unwrap(), 1);
}
fn mux(proto: u32, payload: &[u8], magic: u32) -> Vec<u8> {
    let mut b = vec![0; 16];
    b[..4].copy_from_slice(&proto.to_be_bytes());
    b[4..8].copy_from_slice(&((16 + payload.len()) as u32).to_be_bytes());
    b[8..12].copy_from_slice(&magic.to_be_bytes());
    b.extend_from_slice(payload);
    b
}
fn tcp(flags: u8, data: &[u8], h: usize, magic: u32) -> Vec<u8> {
    let mut b = vec![0; h];
    b[0] = 0xf2;
    b[1] = 0x7e;
    b[3] = 1;
    b[12] = (h / 4 * 16) as u8;
    b[13] = flags;
    b.extend_from_slice(data);
    mux(6, &b, magic)
}
const MAGIC: u32 = 0xfaceface;
const DATA: &[u8] = b"private-sample payload mCon";
fn version() -> Vec<u8> {
    mux(0, &[0x49, 0x28, 0x73, 0], 2)
}
fn syn() -> Vec<u8> {
    tcp(0x12, &[], 20, MAGIC)
}
fn padding() -> Vec<u8> {
    hex("6d436f6e")
}
fn trailer() -> Vec<u8> {
    hex("00000168")
}
fn after(previous: &[u8]) -> UsbMuxFrameBuffer {
    let mut b = UsbMuxFrameBuffer::default();
    b.append(previous);
    assert_eq!(b.take_frame().unwrap().unwrap().length, previous.len());
    b
}
fn reject(previous: &[u8], suffix: &[u8]) {
    let mut b = after(previous);
    b.append(suffix);
    assert!(b.take_frame().is_err());
    assert_eq!(b.buffered_bytes(), suffix.len());
}
fn drain(b: &mut UsbMuxFrameBuffer) -> Vec<UsbMuxFrame> {
    let mut v = vec![];
    while let Some(f) = b.take_frame().unwrap() {
        v.push(f);
    }
    v
}
fn every_split(frames: &[Vec<u8>], pads: &[Vec<u8>]) {
    let wire = frames
        .iter()
        .enumerate()
        .flat_map(|(i, f)| [f.clone(), pads.get(i).cloned().unwrap_or_default()].concat())
        .collect::<Vec<_>>();
    for split in 0..=wire.len() {
        let mut b = UsbMuxFrameBuffer::default();
        b.append(&wire[..split]);
        let mut parsed = drain(&mut b);
        b.append(&wire[split..]);
        parsed.extend(drain(&mut b));
        assert_eq!(parsed.len(), frames.len(), "split {split}");
        for (actual, expected) in parsed.iter().zip(frames) {
            assert_eq!(actual.payload, expected[16..], "split {split}");
        }
        assert_eq!(b.buffered_bytes(), 0);
    }
}
#[test]
fn all_eligible_reply_shapes_preserve_every_usb_split() {
    for (first, pad, next) in [
        (version(), vec![0; 4], tcp(0x10, DATA, 20, MAGIC)),
        (syn(), padding(), tcp(0x10, DATA, 20, MAGIC)),
        (
            tcp(0x10, DATA, 20, MAGIC),
            trailer(),
            tcp(0x10, DATA, 20, MAGIC),
        ),
        (
            tcp(0x10, &[], 20, MAGIC),
            trailer(),
            tcp(0x10, DATA, 20, MAGIC),
        ),
        (
            tcp(0x12, DATA, 20, MAGIC),
            trailer(),
            tcp(0x10, DATA, 20, MAGIC),
        ),
        (
            tcp(0x10, DATA, 24, MAGIC),
            trailer(),
            tcp(0x10, DATA, 24, MAGIC),
        ),
        (version(), vec![0; 4], version()),
        (syn(), padding(), version()),
    ] {
        every_split(&[first, next], &[pad]);
    }
}
#[test]
fn unpadded_unknown_protocol_and_header_like_payloads_are_preserved() {
    let unknown = mux(73, &[1, 2, 3], 9);
    every_split(
        &[version(), syn(), unknown, tcp(0x10, DATA, 20, MAGIC)],
        &[],
    );
    let payload = [DATA.to_vec(), padding(), syn(), DATA.to_vec()].concat();
    every_split(&[version(), tcp(0x10, &payload, 20, MAGIC)], &[]);
    every_split(
        &[
            version(),
            syn(),
            tcp(0x10, DATA, 20, MAGIC),
            tcp(0x10, DATA, 20, MAGIC),
        ],
        &[vec![0; 4], padding()],
    );
}
#[test]
fn partial_mux_and_tcp_headers_and_trailer_timeouts_retain_bytes() {
    let next = tcp(0x10, DATA, 20, MAGIC);
    let mut b = after(&syn());
    for (chunk, expected) in [
        (padding()[..2].to_vec(), 2),
        ([padding()[2..].to_vec(), next[..12].to_vec()].concat(), 16),
        (next[12..16].to_vec(), 20),
        (next[16..25].to_vec(), 29),
    ] {
        b.append(&chunk);
        assert!(b.take_frame().unwrap().is_none());
        assert_eq!(b.buffered_bytes(), expected);
    }
    b.append(&next[25..]);
    assert_eq!(b.take_frame().unwrap().unwrap().payload, next[16..]);
    for previous in [version(), tcp(0x10, DATA, 20, MAGIC)] {
        let mut b = after(&previous);
        b.append(&trailer());
        for _ in 0..10 {
            assert!(b.take_frame().unwrap().is_none());
        }
        assert_eq!(b.buffered_bytes(), 4);
        for byte in &next[..next.len() - 1] {
            b.append(&[*byte]);
            assert!(b.take_frame().unwrap().is_none());
        }
        b.append(&next[next.len() - 1..]);
        assert_eq!(b.take_frame().unwrap().unwrap().payload, next[16..]);
    }
}
#[test]
fn invalid_recovery_targets_and_previous_frames_are_rejected() {
    let good = tcp(0x10, DATA, 20, MAGIC);
    let mut port = good.clone();
    port[18] = 0;
    port[19] = 0;
    let mut short = good.clone();
    short[28] = 0x30;
    let mut long = syn();
    long[28] = 0xf0;
    let mut large = syn();
    large[4..8].copy_from_slice(&65537u32.to_be_bytes());
    for target in [
        tcp(0x10, DATA, 20, 0xfeedface),
        mux(7, &[0; 20], MAGIC),
        port,
        short,
        long,
        large,
    ] {
        reject(&version(), &[padding(), target].concat());
    }
    let mut invalid = syn()[..16].to_vec();
    invalid[4..8].copy_from_slice(&6u32.to_be_bytes());
    reject(&version(), &invalid);
    for previous in [
        mux(73, DATA, MAGIC),
        mux(6, &[0; 19], MAGIC),
        tcp(0x12, &[], 20, 7),
        {
            let mut b = version();
            b[8..12].copy_from_slice(&1u32.to_be_bytes());
            b
        },
    ] {
        reject(&previous, &[trailer(), good.clone()].concat());
    }
    for offset in [0x30, 0xf0] {
        let mut p = good.clone();
        p[28] = offset;
        reject(&p, &[trailer(), good.clone()].concat());
    }
    for port in [16, 18] {
        let mut p = good.clone();
        p[port] = 0;
        p[port + 1] = 0;
        reject(&p, &[trailer(), good.clone()].concat());
    }
    reject(&syn(), &[vec![0; 4], padding(), good.clone()].concat());
    let mut b = UsbMuxFrameBuffer::default();
    b.append(&[padding(), good].concat());
    assert!(b.take_frame().is_err());
    let mut b = UsbMuxFrameBuffer::default();
    b.append(&[0; 16]);
    assert!(b.take_frame().is_err());
}
#[test]
fn plausible_length_is_never_scanned_and_maximum_frame_is_intact() {
    let mut b = after(&tcp(0x10, DATA, 20, MAGIC));
    let suffix = [vec![0; 4], trailer(), tcp(0x10, DATA, 20, MAGIC)].concat();
    b.append(&suffix);
    assert!(b.take_frame().unwrap().is_none());
    assert_eq!(b.buffered_bytes(), suffix.len());
    let payload = (0..65500).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let next = tcp(0x10, &payload, 20, MAGIC);
    let mut b = after(&version());
    b.append(&[vec![0; 4], next[..25].to_vec()].concat());
    assert!(b.take_frame().unwrap().is_none());
    b.append(&next[25..]);
    let f = b.take_frame().unwrap().unwrap();
    assert_eq!(f.length, 65536);
    assert_eq!(&f.payload[20..], payload);
}
#[test]
fn reported_lockdown_lengths_preserve_all_split_offsets() {
    let mut last = (0..918).map(|i| (i % 239) as u8).collect::<Vec<_>>();
    let embedded = [padding(), syn()].concat();
    last[100..100 + embedded.len()].copy_from_slice(&embedded);
    every_split(
        &[
            version(),
            syn(),
            tcp(0x10, &[0, 1, 2, 3], 20, MAGIC),
            tcp(
                0x10,
                &(0..360).map(|i| (i % 251) as u8).collect::<Vec<_>>(),
                20,
                MAGIC,
            ),
            tcp(0x10, &last, 20, MAGIC),
        ],
        &[trailer(), trailer(), trailer(), trailer()],
    );
}
fn captured() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let tcp=hex("00000006000000b7faceface01840197e05600020005015d000005165010020000000000170303008ee0f6cf38b2e09782cb37c255c64a1cc0941687c920f807acfbe4741aa5a10776fe70c26d5059c78ba2d486c7b3145a1bd4c258f7096a002e00dc19aafc87cbe8b72ff25d962b129b462704d35fcc29a703f6004983fc07d315c4c7baec956b97b4dabfbdfee1da166e19cc07c922617e8a67bd70917b519fa0cf713b0d9ee97036efade9e4b83e40a2904a9f1ba438343134");
    let diagnostic = [
        hex("0000000100000045faceface01850197"),
        vec![4],
        b"detected duplicate packet. Expected 408 received 410".to_vec(),
    ]
    .concat();
    let mut second = diagnostic.clone();
    second[13] = 0x87;
    *second.last_mut().unwrap() = b'1';
    (tcp, diagnostic, second)
}
#[test]
fn captured_187_and_73_byte_protocol_one_boundaries_all_splits() {
    let (tcp, diag, second) = captured();
    assert_eq!(tcp.len(), 187);
    assert_eq!(diag.len() + 4, 73);
    every_split(&[tcp[..183].to_vec(), diag.clone()], &[tcp[183..].to_vec()]);
    every_split(
        &[
            tcp[..183].to_vec(),
            diag.clone(),
            second,
            tcp_frame_with_embedded(&diag),
        ],
        &[tcp[183..].to_vec(), hex("a5a10776"), hex("e724c574")],
    );
    let mut b = UsbMuxFrameBuffer::default();
    b.append(&[tcp.clone(), diag.clone(), hex("a5a10776")].concat());
    assert_eq!(b.take_frame().unwrap().unwrap().protocol, 6);
    let f = b.take_frame().unwrap().unwrap();
    assert_eq!((f.protocol, f.length, f.sequence), (1, 69, 0x185));
    assert_eq!(b.buffered_bytes(), 4);
    let mut b = after(&tcp[..183]);
    b.append(&tcp[183..]);
    for (i, byte) in diag.iter().enumerate() {
        b.append(&[*byte]);
        if i + 1 < diag.len() {
            for _ in 0..3 {
                assert!(b.take_frame().unwrap().is_none());
            }
            assert_eq!(b.buffered_bytes(), i + 5);
        }
    }
    assert_eq!(b.take_frame().unwrap().unwrap().payload, diag[16..]);
}
fn tcp_frame_with_embedded(d: &[u8]) -> Vec<u8> {
    tcp(
        0x10,
        &[DATA.to_vec(), d.to_vec(), hex("a5a10776"), DATA.to_vec()].concat(),
        20,
        MAGIC,
    )
}
#[test]
fn malformed_diagnostic_targets_and_predecessors_do_not_recover() {
    let (_, diag, _) = captured();
    let mut wrong_magic = diag.clone();
    wrong_magic[8..12].copy_from_slice(&0xfeedfaceu32.to_be_bytes());
    let mut wrong_subtype = diag.clone();
    wrong_subtype[16] = 3;
    let mut control = diag.clone();
    control[25] = 0;
    let mut nonascii = diag.clone();
    nonascii[25] = 0x80;
    let mut large = diag.clone();
    large[4..8].copy_from_slice(&1041u32.to_be_bytes());
    for bad in [
        wrong_magic,
        wrong_subtype.clone(),
        control.clone(),
        nonascii,
        large,
        mux(1, &[], MAGIC),
        mux(1, &[4], MAGIC),
    ] {
        reject(&syn(), &[trailer(), bad].concat());
    }
    let mut bad = diag.clone();
    bad[8..12].copy_from_slice(&7u32.to_be_bytes());
    for previous in [
        bad,
        wrong_subtype,
        control,
        mux(1, &[4], MAGIC),
        mux(1, &[vec![4], vec![0x41; 1024]].concat(), MAGIC),
    ] {
        reject(&previous, &[trailer(), syn()].concat());
    }
    reject(&mux(73, DATA, MAGIC), &[trailer(), diag.clone()].concat());
    reject(&syn(), &[vec![0; 4], padding(), diag.clone()].concat());
    reject(&diag, &[trailer(), mux(7, &[0; 20], MAGIC)].concat());
    let mut b = UsbMuxFrameBuffer::default();
    b.append(&[trailer(), diag.clone()].concat());
    assert!(b.take_frame().is_err());
    let payload = [vec![0, 0x80], diag, syn()].concat();
    every_split(&[syn(), mux(1, &payload, 9), syn()], &[]);
    let maximum = [vec![4], vec![0x41; 1023]].concat();
    every_split(
        &[syn(), mux(1, &maximum, MAGIC), syn()],
        &[trailer(), trailer()],
    );
}
#[test]
fn version_recovery_requires_exact_header_and_preserves_leftover_trailer() {
    let mut bad = version();
    bad[8..12].copy_from_slice(&1u32.to_be_bytes());
    for target in [
        bad,
        mux(0, &[0; 8], 2),
        mux(0, &[0x49, 0x28, 0x73, 0], MAGIC),
    ] {
        reject(&version(), &[vec![0; 4], target].concat());
    }
    let mut b = after(&version());
    b.append(&[0; 4]);
    assert!(b.take_frame().unwrap().is_none());
    b.append(&[version(), vec![0; 4]].concat());
    assert_eq!(b.buffered_bytes(), 28);
    assert_eq!(b.take_frame().unwrap().unwrap().length, 20);
    assert_eq!(b.buffered_bytes(), 4);
    b.append(&syn());
    assert_eq!(b.take_frame().unwrap().unwrap().length, 36);
    assert_eq!(b.buffered_bytes(), 0);
}
#[test]
fn capped_exact_metadata_diagnostics_and_panicking_callback() {
    let reports = Arc::new(Mutex::new(vec![]));
    let r = reports.clone();
    let mut b = UsbMuxFrameBuffer::new(Arc::new(move |s| r.lock().unwrap().push(s)));
    for _ in 0..10 {
        b.append(&[syn(), padding(), tcp(0x10, DATA, 20, MAGIC)].concat());
        drain(&mut b);
    }
    let r = reports.lock().unwrap();
    assert_eq!(r.len(), 4);
    assert_eq!(r[0],"USBMUX optional reply padding skipped bytes=4 previousProtocol=6 previousLength=36 nextProtocol=6 nextLength=63 lastUsbReadBytes=103 bufferedBytes=63");
    assert!(r
        .iter()
        .all(|s| !s.contains("private-sample") && !s.contains("62078")));
    let mut b = UsbMuxFrameBuffer::new(Arc::new(|_| panic!("logger unavailable")));
    b.append(&[version(), vec![0; 4], syn()].concat());
    assert_eq!(drain(&mut b).len(), 2);
    let (tcp, diag, _) = captured();
    let reports = Arc::new(Mutex::new(vec![]));
    let r = reports.clone();
    let mut b = UsbMuxFrameBuffer::new(Arc::new(move |s| r.lock().unwrap().push(s)));
    b.append(&[tcp, diag, hex("a5a10776"), syn()].concat());
    assert_eq!(drain(&mut b).len(), 3);
    let r = reports.lock().unwrap();
    assert_eq!(r.len(), 2);
    assert!(r[0].contains("nextProtocol=1 nextLength=69"));
    assert!(!r
        .iter()
        .any(|s| s.contains("detected") || s.contains("Expected") || s.contains("received")));
}
