// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
use carplay_airplay::*;
use std::collections::BTreeMap;
fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn decodes_dates() {
    let bytes = hex(concat!(
        "62706c6973743030d201020308546974656d5474797065d2040506075f1010436f6e74656e742d4c6f636174696f6e",
        "5a53746172742d446174655f101968747470733a2f2f6578616d706c652e636f6d2f762e6d70343341c83698a00000005f1013",
        "696e73657274506c617951756575654974656d080d12171c2f3a565f000000000000010100000000000000090000000000000000",
        "0000000000000075"
    ));
    let root = BplistCodec::decode(&bytes).unwrap();
    assert_eq!(
        root.get("type").and_then(BplistValue::as_str),
        Some("insertPlayQueueItem")
    );
    let item = root.get("item").unwrap();
    assert_eq!(
        item.get("Content-Location").and_then(BplistValue::as_str),
        Some("https://example.com/v.mp4")
    );
    assert_eq!(
        item.get("Start-Date"),
        Some(&BplistValue::DateMillis(1_790_769_600_000))
    );
}
#[test]
fn bplist_preserves_null_unicode_unsigned_and_order() {
    let root = BplistValue::dict([
        (
            "z",
            BplistValue::Array(vec![
                BplistValue::Null,
                BplistValue::Boolean(true),
                BplistValue::Real(3.25),
                BplistValue::DateMillis(0),
            ]),
        ),
        ("a", BplistValue::String("é🚘".into())),
        ("u", BplistValue::Integer(u64::MAX.into())),
        ("data", BplistValue::Data(vec![17; 300])),
    ]);
    assert_eq!(
        BplistCodec::decode(&BplistCodec::encode(&root).unwrap()).unwrap(),
        root
    );
}
#[test]
fn bplist_rejects_bad_bounds_and_cycles() {
    assert_eq!(
        BplistCodec::decode(b"bad").unwrap_err().to_string(),
        "bplist: too short"
    );
    let mut bytes = BplistCodec::encode(&BplistValue::Array(vec![BplistValue::Null])).unwrap();
    bytes[9] = 0;
    assert!(BplistCodec::decode(&bytes).is_err());
    let len = bytes.len();
    bytes[len - 25] = 0xff;
    assert!(BplistCodec::decode(&bytes).is_err());
    assert!(BplistCodec::encode(&BplistValue::Integer((-1).into())).is_err());
}
#[test]
fn tlv_fragments_join_only_after_255() {
    let source = vec![42; 513];
    let encoded = Tlv8Codec::encode(&[Tlv8Item {
        kind: 3,
        value: source.clone(),
    }])
    .unwrap();
    assert_eq!(encoded[1], 255);
    assert_eq!(Tlv8Codec::decode(&encoded).unwrap().get(&3), Some(&source));
    assert_eq!(
        Tlv8Codec::decode(&[3, 1, 7, 3, 1, 8]).unwrap().get(&3),
        Some(&vec![8])
    );
}
#[test]
fn tlv_repeated_empty_and_truncated_values() {
    let encoded = Tlv8Codec::encode(&[
        Tlv8Item {
            kind: 1,
            value: vec![4],
        },
        Tlv8Item {
            kind: 1,
            value: vec![],
        },
        Tlv8Item {
            kind: 1,
            value: vec![5],
        },
    ])
    .unwrap();
    assert_eq!(encoded, vec![1, 1, 4, 255, 0, 1, 0, 255, 0, 1, 1, 5]);
    assert_eq!(
        Tlv8Codec::decode(&[1, 1, 9, 2, 4, 7]).unwrap().get(&1),
        Some(&vec![9])
    );
    assert!(!Tlv8Codec::decode(&[1, 1, 9, 2, 4, 7])
        .unwrap()
        .contains_key(&2));
}
#[test]
fn rfc8439_vector_seals_the_same() {
    let key: [u8; 32] = hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
        .try_into()
        .unwrap();
    let nonce: [u8; 12] = hex("070000004041424344454647").try_into().unwrap();
    let aad = hex("50515253c0c1c2c3c4c5c6c7");
    let plain=b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
    let sealed = AirPlayCrypto::chacha_seal(&key, &nonce, plain, &aad).unwrap();
    assert_eq!(
        &sealed[sealed.len() - 16..],
        hex("1ae10b594f09e26a7e902ecbd0600691")
    );
    assert_eq!(
        AirPlayCrypto::chacha_open(&key, &nonce, &sealed, &aad).unwrap(),
        plain
    );
}
#[test]
fn changed_byte_or_aad_fails_to_open() {
    let key = [9; 32];
    let nonce = AirPlayCrypto::nonce64(1);
    let sealed = AirPlayCrypto::chacha_seal(&key, &nonce, &[3; 100], b"aad").unwrap();
    let mut tampered = sealed.clone();
    tampered[10] ^= 1;
    assert!(AirPlayCrypto::chacha_open(&key, &nonce, &tampered, b"aad").is_err());
    assert!(AirPlayCrypto::chacha_open(&key, &nonce, &sealed, b"bad").is_err());
    assert_eq!(
        AirPlayCrypto::chacha_open(&key, &nonce, &sealed, b"aad").unwrap(),
        vec![3; 100]
    );
}
#[test]
fn sealing_the_same_key_and_nonce_twice_still_works() {
    let nonce = AirPlayCrypto::nonce_label("PV-Msg02");
    assert_eq!(&nonce[4..], b"PV-Msg02");
    assert_eq!(
        AirPlayCrypto::chacha_seal(&[1; 32], &nonce, &[1, 2, 3], &[]).unwrap(),
        AirPlayCrypto::chacha_seal(&[1; 32], &nonce, &[1, 2, 3], &[]).unwrap()
    );
    assert_eq!(
        AirPlayCrypto::nonce64(0x0102030405060708),
        [0, 0, 0, 0, 8, 7, 6, 5, 4, 3, 2, 1]
    );
}
#[test]
fn chacha_large_frames_and_empty_frames() {
    for size in [0, 1, 63, 64, 65, 1400, 500_000, 17, 300_000, 2_000_001, 0] {
        let plain = vec![size as u8; size];
        let nonce = AirPlayCrypto::nonce64(size as u64);
        let sealed = AirPlayCrypto::chacha_seal(&[5; 32], &nonce, &plain, b"aad").unwrap();
        assert_eq!(
            AirPlayCrypto::chacha_open(&[5; 32], &nonce, &sealed, b"aad").unwrap(),
            plain
        );
    }
}
#[test]
fn control_cipher_retains_partial_frames_and_separate_nonces() {
    let mut a = ControlCipher::new([2; 32], [1; 32]);
    let mut b = ControlCipher::new([1; 32], [2; 32]);
    let plain = vec![77; 0x8001];
    let wire = a.encrypt(&plain).unwrap();
    assert_eq!(&wire[..2], &[0, 0x40]);
    for len in [0, 1, 2, 100, 0x4011] {
        let mut b = ControlCipher::new([1; 32], [2; 32]);
        let first = b.decrypt(&wire[..len]).unwrap();
        let next = [first.rest, wire[len..].to_vec()].concat();
        let second = b.decrypt(&next).unwrap();
        assert_eq!([first.data, second.data].concat(), plain);
        assert!(second.rest.is_empty());
    }
    let reply = b.encrypt(b"reply").unwrap();
    assert_eq!(a.decrypt(&reply).unwrap().data, b"reply");
}
#[test]
fn control_cipher_empty_frame_and_bad_tag_dont_consume_nonce() {
    let mut a = ControlCipher::new([2; 32], [1; 32]);
    let mut b = ControlCipher::new([1; 32], [2; 32]);
    let wire = a.encrypt(&[]).unwrap();
    assert_eq!(wire.len(), 18);
    let mut bad = wire.clone();
    bad[5] ^= 1;
    assert!(b.decrypt(&bad).is_err());
    assert!(b.decrypt(&wire).unwrap().data.is_empty());
}
#[test]
fn rtsp_fragmented_coalesced_and_incomplete_tail() {
    let wire=b"POST /pair-verify HTTP/1.1\r\nCSeq: 9\r\nContent-Length: 3\r\n\r\nabcOPTIONS * RTSP/1.0\r\n\r\nPOST /";
    let parsed = RtspMessage::parse_messages(wire).unwrap();
    assert_eq!(parsed.messages.len(), 2);
    assert_eq!(parsed.messages[0].body, b"abc");
    assert_eq!(parsed.messages[0].headers.get("cseq").unwrap(), "9");
    assert_eq!(parsed.rest, b"POST /");
    for len in 0..61 {
        let parsed = RtspMessage::parse_messages(&wire[..len]).unwrap();
        assert!(parsed.messages.is_empty());
        assert_eq!(parsed.rest, &wire[..len]);
    }
}
#[test]
fn rtsp_content_length_follows_kotlin_and_response_echoes_cseq() {
    for value in ["-1", "bad", "2147483648"] {
        let wire = format!("GET /info RTSP/1.0\r\nContent-Length: {value}\r\n\r\n");
        assert_eq!(
            RtspMessage::parse_messages(wire.as_bytes())
                .unwrap()
                .messages
                .len(),
            1
        );
    }
    let request = RtspRequest {
        method: "GET".into(),
        path: "/info".into(),
        protocol: "HTTP/1.1".into(),
        headers: BTreeMap::from([("cseq".into(), "7".into())]),
        body: vec![],
    };
    let response = RtspMessage::build_response(
        &request,
        &RtspResponse {
            body: b"ok".to_vec(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(String::from_utf8_lossy(&response).contains("CSeq: 7\r\n"));
    assert!(response.ends_with(b"\r\n\r\nok"));
}
#[test]
fn identifies_negotiation_without_private_values() {
    assert_eq!(
        AirPlayControlDiagnostics::request(
            "POST",
            "rtsp://192.168.49.1/pair-verify?token=secret",
            128
        ),
        "airplay control request method=POST route=pair-verify contentBytes=128"
    );
    assert_eq!(
        AirPlayControlDiagnostics::request("SETUP", "rtsp://192.168.49.1/private-session", 64),
        "airplay control request method=SETUP route=other contentBytes=64"
    );
    assert_eq!(
        AirPlayControlDiagnostics::request("Jane's iPhone\npassword=secret", "/private-secret", 0),
        "airplay control request method=OTHER route=other contentBytes=0"
    );
}
#[test]
fn product_and_versions_available_without_identity() {
    assert_eq!(
        AirPlayPeerDiagnostics::summary("iPhone11,2", "18.7.10", "950.7.1"),
        "Phone display peer model=iPhone11,2 ios=18.7.10 airplay=950.7.1"
    );
}
#[test]
fn unexpected_peer_fields_cannot_leak_personal_or_multiline_values() {
    assert_eq!(
        AirPlayPeerDiagnostics::summary("Jane's iPhone", "26.0\npassword=secret", ""),
        "Phone display peer model=not_reported ios=not_reported airplay=not_reported"
    );
}
#[derive(Debug)]
struct Field {
    page: u32,
    usages: Vec<u32>,
    size: u32,
    count: u32,
    flags: u32,
}
fn fields(bytes: &[u8]) -> Vec<Field> {
    let (mut out, mut usages) = (Vec::new(), Vec::new());
    let (mut page, mut size, mut count, mut offset) = (0, 0, 0, 0);
    while offset < bytes.len() {
        let prefix = bytes[offset];
        offset += 1;
        assert_ne!(prefix, 254);
        let len = if prefix & 3 == 3 { 4 } else { prefix & 3 } as usize;
        let value = bytes[offset..offset + len]
            .iter()
            .enumerate()
            .fold(0, |v, (i, b)| v | (*b as u32) << (8 * i));
        offset += len;
        match ((prefix >> 2) & 3, prefix >> 4) {
            (1, 0) => page = value,
            (1, 7) => size = value,
            (1, 9) => count = value,
            (2, 0) => usages.push(value),
            (0, tag) => {
                if tag == 8 {
                    out.push(Field {
                        page,
                        usages: usages.clone(),
                        size,
                        count,
                        flags: value,
                    });
                }
                usages.clear();
            }
            _ => {}
        }
    }
    out
}
#[test]
fn touchscreen_coordinates_remain_absolute_sixteen_bit_for_both_contacts() {
    let device = AirPlayHid::touch_hid_device(1920, 1080, "test-display");
    let fields: Vec<_> = fields(device.get("hidDescriptor").unwrap().as_data().unwrap())
        .into_iter()
        .filter(|f| f.page == 1 && (f.usages.contains(&0x30) || f.usages.contains(&0x31)))
        .collect();
    assert_eq!(fields.len(), 4);
    assert!(fields
        .iter()
        .all(|f| f.size == 16 && f.count == 1 && f.flags == 2));
    assert_eq!(
        fields.iter().map(|f| f.usages.clone()).collect::<Vec<_>>(),
        vec![vec![0x30], vec![0x31], vec![0x30], vec![0x31]]
    );
}
#[test]
fn legacy_knob_coordinates_absolute_and_wheel_relative() {
    let device = AirPlayHid::knob_hid_device("test-display");
    let fields = fields(device.get("hidDescriptor").unwrap().as_data().unwrap());
    let xy = fields
        .iter()
        .find(|f| f.page == 1 && f.usages == [0x30, 0x31])
        .unwrap();
    let wheel = fields
        .iter()
        .find(|f| f.page == 1 && f.usages == [0x38])
        .unwrap();
    assert_eq!((xy.size, xy.count, xy.flags), (8, 2, 2));
    assert_eq!((wheel.size, wheel.count, wheel.flags), (8, 1, 6));
}
#[test]
fn touch_report_uses_slot_ids_rounding_and_two_contact_limit() {
    let contact = |id, x, y, down| AirPlayContact { id, x, y, down };
    assert_eq!(
        AirPlayHid::touch_report(&[
            contact(9, 1919.6, -1.0, true),
            contact(8, 300.4, 700.5, false),
            contact(7, 1.0, 2.0, true)
        ])
        .unwrap(),
        vec![0, 1, 0x80, 7, 0, 0, 1, 0, 0x2c, 1, 0xbd, 2]
    );
}
#[test]
fn local_and_loopback_peers_are_internal() {
    assert!(
        carplay_airplay::air_play_tcp_accepted::is_internal_air_play_peer(
            "127.0.0.1".parse().unwrap(),
            "192.168.1.1".parse().unwrap()
        )
    );
    assert!(
        carplay_airplay::air_play_tcp_accepted::is_internal_air_play_peer(
            "::1".parse().unwrap(),
            "::".parse().unwrap()
        )
    );
    assert!(
        !carplay_airplay::air_play_tcp_accepted::is_internal_air_play_peer(
            "192.168.1.2".parse().unwrap(),
            "192.168.1.1".parse().unwrap()
        )
    );
}

#[test]
fn identity_generation_and_listener_identity_keep_distinct_ids() {
    let identity = AirPlayIdentity::generate().unwrap();
    assert_eq!(identity.public_key_hex().len(), 64);
    assert_eq!(identity.pairing_id.len(), 36);
    assert_eq!(&identity.pairing_id[14..15], "4");
    assert!(matches!(
        &identity.pairing_id[19..20],
        "8" | "9" | "a" | "b"
    ));
    let signature = AirPlayCrypto::ed25519_sign(&identity.private_key, b"identity").unwrap();
    assert!(AirPlayCrypto::ed25519_verify(
        &identity.public_key,
        b"identity",
        &signature
    ));
    let a = AirPlayListenerIdentity::new(1);
    let b = AirPlayListenerIdentity::new(1);
    assert_eq!(a.generation, 1);
    assert_ne!(a.id, b.id);
}
