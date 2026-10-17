// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
mod common;
use carplay_iap2::message::iap2_messages::{Iap2ClusterAsset, Iap2StartSessionOptions};
use carplay_iap2::*;
use common::*;
#[test]
fn catalog_registers_every_reference_endpoint_once() {
    assert_eq!(
        (0..=65535)
            .filter(|id| Iap2Endpoints::by_id(*id).is_some())
            .count(),
        151
    );
}
#[test]
fn framer_accepts_fragmented_and_concatenated_frames() {
    let first = frame(0x4300, &[(0, &[])]);
    let second = frame(0x4301, &[(0, &[1])]);
    let b = [
        first.encoded_frame().unwrap(),
        second.encoded_frame().unwrap(),
    ]
    .concat();
    let mut f = Iap2CsmFramer::new();
    assert!(f.offer(&b[..3]).unwrap().is_empty());
    assert_eq!(f.offer(&b[3..]).unwrap(), vec![first, second]);
}
#[test]
fn ordered_parameter_list_preserves_repeated_and_unknown_ids() {
    let f = frame(0, &[(0, &[1]), (0, &[2]), (0x7fff, &[3, 4])]);
    let p = f.body().unwrap();
    assert_eq!(
        p.parameters.iter().map(|p| p.id).collect::<Vec<_>>(),
        [0, 0, 0x7fff]
    );
    assert_eq!(p.all(0)[1].payload, [2]);
    assert_eq!(p.first(0x7fff).unwrap().payload, [3, 4]);
    assert_eq!(p.encode().unwrap(), f.payload);
}
#[test]
fn generic_builder_constructs_nested_unmodeled_body() {
    let mut b = Iap2BodyBuilder::opaque(0x0d01);
    let mut g = Iap2BodyBuilder::opaque(0);
    g.u32(0, 7).unwrap().u8(1, 2).unwrap();
    b.u16(0, 1).unwrap().group(4, g).unwrap();
    let r = Iap2BodyReader::of(&b.build().unwrap()).unwrap();
    assert_eq!(r.u16(0).unwrap(), 1);
    assert_eq!(r.group(4).unwrap().u32(0).unwrap(), 7);
    assert_eq!(r.group(4).unwrap().u8(1).unwrap(), 2);
}
#[test]
fn start_session_supports_reference_optional_fields() {
    let mut e = endpoint();
    e.ip_addresses.push("192.168.1.2".into());
    let o = Iap2StartSessionOptions {
        wired_ipv6_addresses: vec!["fe80::2".into()],
        wired_reserved: Some(3),
        sdk_version: Some("27.0".into()),
        cluster_asset: Some(Iap2ClusterAsset {
            asset_id: "cluster".into(),
            asset_version: 4,
        }),
        mutual_auth: Some(true),
    };
    let b = Iap2BodyReader::of(&Iap2CarPlayMessages::start_session_with_options(&e, &o).unwrap())
        .unwrap();
    assert_eq!(b.group(0).unwrap().string(0).unwrap(), "fe80::2");
    assert_eq!(b.group(0).unwrap().u32(1).unwrap(), 3);
    assert_eq!(b.group(1).unwrap().all(3).len(), 2);
    assert_eq!(b.u32(2).unwrap(), 49152);
    assert_eq!(b.string(6).unwrap(), "27.0");
    assert_eq!(b.group(7).unwrap().string(0).unwrap(), "cluster");
    assert_eq!(b.group(7).unwrap().u32(1).unwrap(), 4);
    assert_eq!(b.u8(8).unwrap(), 1);
}
#[test]
fn wireless_availability_uses_boolean_semantics() {
    for (v, want) in [(0, false), (1, true)] {
        assert_eq!(
            Iap2WirelessMessages::wireless_car_play_availability(&frame(0x4e0d, &[(0, &[v])]))
                .unwrap(),
            want
        );
    }
    assert!(
        Iap2WirelessMessages::wireless_car_play_availability(&frame(0x4e0d, &[(0, &[2])])).is_err()
    );
}
#[test]
fn accessory_wifi_configuration_matches_livi_vector() {
    assert_eq!(
        hex(
            &Iap2WirelessControlClient::accessory_wifi_configuration(&endpoint())
                .unwrap()
                .encoded_frame()
                .unwrap()
        ),
        "404000275703000900014c49564900000e00027365637265743132330000050003030005000424"
    );
}
#[test]
fn start_session_matches_livi_wireless_vector() {
    assert_eq!(
        hex(
            &Iap2WirelessControlClient::car_play_start_session(&endpoint())
                .unwrap()
                .encoded_frame()
                .unwrap()
        ),
        concat!(
            "40400060430100350001000900004c49564900000e000173656372657431323300000500022400100003",
            "3139322e3136382e322e31000005000403000800020000c000000a00036465762d3100000b000461",
            "61626263630000080005312e3000"
        )
    );
}
#[test]
fn ap_hint_is_only_in_wifi_configuration() {
    let before = Iap2CarPlayMessages::start_session(&endpoint()).unwrap();
    let mut e = endpoint();
    e.access_point_bssid = Some([2, 17, 34, 51, 68, 85]);
    assert_eq!(
        Iap2BodyReader::of(&Iap2WirelessMessages::accessory_wifi_configuration(&e).unwrap())
            .unwrap()
            .bytes(0)
            .unwrap(),
        [2, 17, 34, 51, 68, 85]
    );
    assert_eq!(before, Iap2CarPlayMessages::start_session(&e).unwrap());
    assert!(!Iap2BodyReader::of(
        &Iap2WirelessMessages::accessory_wifi_configuration(&endpoint()).unwrap()
    )
    .unwrap()
    .has(0));
}
#[test]
fn same_lan_hint_does_not_leak_into_p2p_frames() {
    let before = Iap2WirelessMessages::accessory_wifi_configuration(&endpoint()).unwrap();
    let mut e = endpoint();
    e.access_point_bssid = Some([2, 3, 4, 5, 6, 7]);
    for _ in 0..2 {
        assert_eq!(
            Iap2WirelessMessages::accessory_wifi_configuration(&e)
                .unwrap()
                .body()
                .unwrap()
                .parameters
                .len(),
            5
        );
    }
    assert_eq!(
        before,
        Iap2WirelessMessages::accessory_wifi_configuration(&endpoint()).unwrap()
    );
}
#[test]
fn open_network_encodes_empty_credential_in_both_messages() {
    let mut e = endpoint();
    e.security = Iap2WirelessSecurity::None;
    e.passphrase.clear();
    let wifi = Iap2BodyReader::of(&Iap2WirelessMessages::accessory_wifi_configuration(&e).unwrap())
        .unwrap();
    assert_eq!(wifi.bytes(2).unwrap(), [0]);
    assert_eq!(wifi.u8(3).unwrap(), 0);
    let s = Iap2BodyReader::of(&Iap2CarPlayMessages::start_session(&e).unwrap())
        .unwrap()
        .group(1)
        .unwrap();
    assert_eq!(s.bytes(1).unwrap(), [0]);
    assert_eq!(s.u8(4).unwrap(), 0);
}
#[test]
fn secured_session_rejects_empty_credential() {
    let mut e = endpoint();
    e.passphrase.clear();
    assert!(Iap2CarPlayMessages::start_session(&e).is_err());
}
#[test]
fn formatter_shows_names_fields_and_groups() {
    let f = Iap2CarPlayMessages::start_session(&endpoint()).unwrap();
    let s = Iap2FrameFormatter::format_with_context(Iap2TraceDirection::Tx, "wired", &f);
    assert!(s.starts_with("IAP2 TX [wired] 0x4301 CarPlayStartSession"));
    assert!(s.contains("wireless:"));
    assert!(s.contains("airPlayPort: u32=49152"));
    assert!(s.contains("raw-body="));
}
#[test]
fn formatter_falls_back_for_malformed_bodies() {
    let f = Iap2Frame {
        message_id: 0x4300,
        payload: vec![0, 1, 2],
    };
    let s = Iap2FrameFormatter::format(Iap2TraceDirection::Rx, &f);
    assert!(s.contains("body=malformed TLV"));
    assert!(s.contains("raw-body=00 01 02"));
}
#[test]
fn formatter_escapes_line_breaks() {
    let mut b = Iap2BodyBuilder::opaque(0xfffb);
    b.string(0, "$GPGGA,1\r\n").unwrap();
    assert!(
        Iap2FrameFormatter::format(Iap2TraceDirection::Tx, &b.build().unwrap()).contains("\\r\\n")
    );
}
#[test]
fn framer_resynchronizes_invalid_lengths_and_bounds_large_chunks() {
    let mut f = Iap2CsmFramer::new();
    let valid = frame(0, &[]).encoded_frame().unwrap();
    let b = [vec![1, 2, 64, 64, 0, 1, 0, 0], valid].concat();
    assert_eq!(f.offer(&b).unwrap().len(), 1);
    let max = Iap2CsmFramer::encode_frame(3, &vec![0; 65529]).unwrap();
    for _ in 0..3 {
        assert_eq!(f.offer(&max).unwrap().len(), 1);
    }
    assert!(Iap2CsmFramer::encode_frame(3, &vec![0; 65530]).is_err());
}
#[test]
fn parameter_parser_rejects_truncation_and_invalid_lengths() {
    for b in [&[0][..], &[0, 3, 0, 1], &[0, 5, 0, 1]] {
        assert!(Iap2ParameterList::parse(b).is_err());
    }
}
#[test]
fn schema_validates_required_duplicate_and_wire_types() {
    let e = Iap2Endpoints::require(0xaa01).unwrap();
    assert!(Iap2BodyBuilder::of(&e).build().is_err());
    let mut b = Iap2BodyBuilder::of(&e);
    b.u8(0, 1).unwrap();
    assert!(b.build().is_err());
    let mut b = Iap2BodyBuilder::of(&e);
    b.bytes(0, &[1]).unwrap().bytes(0, &[2]).unwrap();
    assert!(b.build().is_err());
}
#[test]
fn codecs_preserve_signed_values_and_string_lists() {
    assert_eq!(Iap2WireCodec::i16(-2).unwrap(), [255, 254]);
    assert_eq!(Iap2WireCodec::read_i32(&[255; 4]).unwrap(), -1);
    assert_eq!(
        Iap2WireCodec::read_strings(b"a\0\0b\0").unwrap(),
        ["a", "", "b"]
    );
    assert!(Iap2WireCodec::string("bad\0").is_err());
    assert!(Iap2WireCodec::read_string(b"bad").is_err());
    assert!(Iap2WireCodec::read_u16_list(&[1]).is_err());
}
#[test]
fn wifi_information_retains_status_and_optional_credentials() {
    let f = frame(
        0x5701,
        &[(0, &[0]), (1, &[3]), (2, b"SSID\0"), (3, b"pw\0")],
    );
    let v = Iap2WirelessMessages::wifi_information(&f).unwrap();
    assert!(v.successful());
    assert_eq!(v.ssid.as_deref(), Some("SSID"));
    assert_eq!(v.passphrase.as_deref(), Some("pw"));
    assert!(
        !Iap2WirelessMessages::wifi_information(&frame(0x5701, &[(0, &[1])]))
            .unwrap()
            .successful()
    );
}
