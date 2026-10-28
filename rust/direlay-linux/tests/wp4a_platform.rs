// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Linux platform, media sink, audio, and iPhone selection tests.
use carplay_airplay::{AudioCodecKind, AudioFormat, AudioStreamId, MediaSink, VideoCodec};
use openxplay_relay::*;
use openxplay_relay::{
    linux_bluetooth::{select_iphone, valid_mac, BluetoothOutput},
    linux_hotspot::WifiOutput,
    relay_admin::{settings_schema, update_settings},
    settings::Settings,
};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
fn temp() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "wp4a-{}",
        openxplay_relay::admin::auth::random_hex(10).unwrap()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn quiet() -> Arc<JsonEvents> {
    Arc::new(JsonEvents::new(Arc::new(|_| {})))
}
macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/", $name, ".txt"))
    };
}
#[test]
fn parses_observed_wifi_details() {
    let r = WifiOutput::iw_info(fixture!("iw-info"));
    assert_eq!(r.bssid.as_deref(), Some("dc:a6:32:11:22:33"));
    assert_eq!(r.channel, Some(36));
    assert_eq!(r.frequency_mhz, Some(5180));
    assert_eq!(
        WifiOutput::ipv4(fixture!("nmcli-ipv4"))
            .unwrap()
            .to_string(),
        "10.42.0.1"
    );
    assert_eq!(
        WifiOutput::ipv4("IP4.ADDRESS[1]: 10.42.0.1/24")
            .unwrap()
            .to_string(),
        "10.42.0.1"
    );
    assert!(WifiOutput::ipv4("127.0.0.1/8\n--").is_none());
    assert!(WifiOutput::ipv4("999.42.0.1/24").is_none());
    assert!(WifiOutput::iw_info("Interface wlan0\n type managed")
        .channel
        .is_none());
}
#[test]
fn nmcli_ipv4_values_support_device_multivalue_and_line_output() {
    for text in [
        "10.99.99.1/32 | 10.79.0.1/24",
        "10.99.99.1/32\n10.79.0.1/24",
        "IP4.ADDRESS[1]: 10.99.99.1/32\nIP4.ADDRESS[2]: 10.79.0.1/24",
    ] {
        assert_eq!(
            WifiOutput::ipv4_networks(text),
            [
                ("10.99.99.1".parse().unwrap(), 32),
                ("10.79.0.1".parse().unwrap(), 24),
            ],
            "{text}"
        );
        assert_eq!(WifiOutput::ipv4(text), Some("10.99.99.1".parse().unwrap()));
    }
    for text in [
        "10.79.0.1/24 | 10.99.99.1/32",
        "10.79.0.1/24\n10.99.99.1/32",
    ] {
        assert_eq!(
            WifiOutput::ipv4_networks(text),
            [
                ("10.79.0.1".parse().unwrap(), 24),
                ("10.99.99.1".parse().unwrap(), 32),
            ],
            "{text}"
        );
        assert_eq!(WifiOutput::ipv4(text), Some("10.79.0.1".parse().unwrap()));
    }
}
#[test]
fn nmcli_multivalue_escapes_are_decoded_after_splitting() {
    use openxplay_relay::provision::nmcli_values;
    assert_eq!(
        nmcli_values("\r\n 10.99.99.1/32 | 10.79.0.1/24\r\n | \n10.77.0.1/24\n"),
        ["10.99.99.1/32", "10.79.0.1/24", "10.77.0.1/24"]
    );
    assert_eq!(
        nmcli_values(r"name\:value | literal \| value | back\\slash | dangling\"),
        ["name:value", "literal | value", "back\\slash", "dangling\\"]
    );
    assert_eq!(
        nmcli_values(r"2001\:db8\:\:1/64 | fe80\:\:2/64"),
        ["2001:db8::1/64", "fe80::2/64"]
    );
    assert_eq!(nmcli_values(r"back\\|next"), ["back\\", "next"]);
    assert!(nmcli_values("\n | \r\n").is_empty());
}
#[test]
fn nmcli_ipv4_ignores_escaped_non_addresses_and_unusable_values() {
    let text = r"bad\:10.99.99.1/32 | bad\|10.99.99.1/32 | 0.0.0.0/0 | 127.0.0.1/8 | 999.1.1.1/24 | 10.77.0.1/24";
    let ip = "10.77.0.1".parse().unwrap();
    assert_eq!(WifiOutput::ipv4(text), Some(ip));
    assert_eq!(WifiOutput::ipv4_networks(text), [(ip, 24)]);
    assert!(WifiOutput::ipv4_networks("10.77.0.1/33 | 10.77.0.2/no | 10.77.0.3").is_empty());
    assert_eq!(WifiOutput::ipv4("-- | 10.77.0.1"), Some(ip));
}
#[test]
fn nmcli_escaped_radio() {
    let f = fixture!("nmcli-wifi")
        .lines()
        .next()
        .unwrap()
        .splitn(3, ':')
        .collect::<Vec<_>>();
    let r = WifiOutput::nmcli_radio(f[1], f[2]);
    assert_eq!(r.bssid.as_deref(), Some("DC:A6:32:11:22:33"));
    assert_eq!(WifiOutput::unescape(r"a\:b\\c"), "a:b\\c");
}
#[test]
fn parses_bluez_fixtures() {
    let entries = BluetoothOutput::devices(fixture!("bluetoothctl-paired"));
    assert_eq!(entries.len(), 2);
    let phone = BluetoothOutput::info(&entries[0].0, &entries[0].1, fixture!("bluetoothctl-info"));
    assert!(phone.paired && phone.connected);
    assert_eq!(
        BluetoothOutput::select(std::slice::from_ref(&phone), None)
            .unwrap()
            .address,
        phone.address
    );
    assert_eq!(
        BluetoothOutput::adapter_address("Controller DC:A6:32:44:55:66 relay [default]").unwrap(),
        "DC:A6:32:44:55:66"
    );
}
#[test]
fn parses_sdp_scoped_records() {
    assert_eq!(
        BluetoothOutput::rfcomm_channel(fixture!("sdptool-search")).unwrap(),
        12
    );
    assert_eq!(
        BluetoothOutput::rfcomm_channel(fixture!("sdptool-records")).unwrap(),
        12
    );
    assert!(BluetoothOutput::rfcomm_channel("\"RFCOMM\" (0x0003)\n Channel: 7").is_err());
    assert!(BluetoothOutput::rfcomm_channel(
        &fixture!("sdptool-records").replace("Channel: 12", "Channel: 31")
    )
    .is_err());
}
#[test]
fn sdp_ambiguity_does_not_pick_handsfree() {
    let r = fixture!("sdptool-records");
    assert!(BluetoothOutput::rfcomm_channel(&format!(
        "{r}\n{}",
        r.replace("Channel: 12", "Channel: 13")
    ))
    .is_err());
}
fn phone(n: u8) -> BondedDevice {
    BondedDevice {
        address: format!("AA:BB:CC:DD:EE:{n:02X}"),
        name: "iPhone".into(),
        paired: true,
        ..Default::default()
    }
}
#[test]
fn configured_phone_wins_case_insensitively() {
    let a = phone(1);
    let mut b = phone(2);
    b.connected = true;
    assert_eq!(
        select_iphone(&[a.clone(), b], Some(&a.address.to_lowercase()))
            .unwrap()
            .address,
        a.address
    );
}
#[test]
fn configured_unpaired_never_falls_back() {
    assert_eq!(
        select_iphone(&[phone(2)], Some(&phone(1).address))
            .unwrap_err()
            .to_string(),
        "The configured iPhone is no longer paired"
    );
}
#[test]
fn strong_uuid_beats_phone_class() {
    let mut a = phone(1);
    a.name = "Alice".into();
    a.iap2 = true;
    let mut b = phone(2);
    b.name = "Android".into();
    b.phone_class = true;
    b.connected = true;
    assert_eq!(
        select_iphone(&[a.clone(), b], None).unwrap().address,
        a.address
    );
}
#[test]
fn phone_class_fallback() {
    let mut a = phone(1);
    a.name = "Alice".into();
    a.phone_class = true;
    let mut b = phone(2);
    b.name = "Car".into();
    assert_eq!(
        select_iphone(&[a.clone(), b], None).unwrap().address,
        a.address
    );
}
#[test]
fn connected_candidate_wins() {
    let a = phone(1);
    let mut b = phone(2);
    b.connected = true;
    assert_eq!(
        select_iphone(&[a, b.clone()], None).unwrap().address,
        b.address
    );
}
#[test]
fn multiple_connected_is_ambiguous() {
    let mut a = phone(1);
    a.connected = true;
    let mut b = phone(2);
    b.connected = true;
    assert!(select_iphone(&[a, b], None)
        .unwrap_err()
        .to_string()
        .contains("More than one"));
}
#[test]
fn single_candidate_wins() {
    let a = phone(1);
    let mut b = phone(2);
    b.name = "Car".into();
    assert_eq!(
        select_iphone(&[a.clone(), b], None).unwrap().address,
        a.address
    );
}
#[test]
fn multiple_candidates_is_ambiguous() {
    assert!(select_iphone(&[phone(1), phone(2)], None).is_err());
}
#[test]
fn single_bond_fallback() {
    let mut a = phone(1);
    a.name = "Alice".into();
    assert_eq!(
        select_iphone(&[a.clone()], None).unwrap().address,
        a.address
    );
}
#[test]
fn no_phone_or_no_bonds() {
    assert!(select_iphone(&[], None).is_err());
    let mut a = phone(1);
    a.paired = false;
    assert!(select_iphone(&[a], None).is_err());
}
#[test]
fn hotspot_subnet_avoids_every_network() {
    let nets=WifiOutput::ip_networks("lo inet 127.0.0.1/8\neth0 inet 192.168.1.20/24\nwlan1 inet 10.42.0.1/24\ndocker0 inet 10.77.0.1/16");
    assert_eq!(nets.len(), 4);
    assert_eq!(
        WifiOutput::free_hotspot_address(&nets).unwrap(),
        "172.31.200.1/24"
    );
    assert_eq!(
        WifiOutput::free_hotspot_address(&["10.42.0.1/24".into()]).unwrap(),
        "10.77.0.1/24"
    );
    assert_eq!(
        WifiOutput::free_hotspot_address(&["10.77.0.9/24".into()]).unwrap(),
        "10.77.1.1/24"
    );
    assert!(WifiOutput::free_hotspot_address(&["0.0.0.0/0".into()]).is_err());
}
#[test]
fn rfcomm_override_skips_sdp() {
    struct NoCalls;
    impl CommandRunner for NoCalls {
        fn run(&self, _: &[String], _: Duration) -> openxplay_relay::Result<CommandResult> {
            panic!("SDP must be skipped")
        }
    }
    let b = LinuxBluetooth::new(Arc::new(NoCalls), None, Some(12));
    assert_eq!(b.channel("A0:B1:C2:D3:E4:F5").unwrap(), 12);
}
#[test]
fn options_parse_valid() {
    let p = temp();
    let args = [
        "--state-dir",
        p.to_str().unwrap(),
        "--identity-dir",
        "/tmp/mfi",
        "--wifi-interface",
        "wlan1",
        "--pair",
        "30",
    ];
    let o = Options::parse(&args.map(str::to_string)).unwrap();
    assert_eq!(o.wifi_interface, "wlan1");
    assert_eq!(o.pair_seconds, Some(30));
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn options_reject_unknown_missing_duplicate_and_invalid() {
    for args in [
        vec!["--wat", "x"],
        vec!["--i2c", "/dev/i2c-1"],
        vec!["--iphone", "no"],
        vec!["--rfcomm-channel", "0"],
        vec!["--pair", "3601"],
        vec!["--pair"],
        vec!["--wifi-channel", "15"],
        vec!["--device-preset", "unknown"],
        vec!["--wifi-interface", "a;bad"],
        vec!["--pair", "1", "--pair", "2"],
    ] {
        assert!(Options::parse(&args.into_iter().map(str::to_string).collect::<Vec<_>>()).is_err());
    }
}
#[test]
fn supports_openstick_channels_and_cli_precedence() {
    let p = temp();
    let s = Settings {
        hotspot_channel: 6,
        wifi_interface: "wlan1".into(),
        ..Default::default()
    };
    s.save(&p).unwrap();
    let o = Options::parse(&[
        "--state-dir".into(),
        p.to_str().unwrap().into(),
        "--wifi-interface".into(),
        "wlan2".into(),
        "--pairing".into(),
    ])
    .unwrap();
    assert_eq!(o.wifi_channel, 6);
    assert_eq!(o.wifi_interface, "wlan2");
    assert!(o.settings.require_pairing_code);
    assert_eq!(Settings::load(&p).unwrap().wifi_interface, "wlan1");
    let automatic = Options::parse(&[
        "--state-dir".into(),
        p.to_str().unwrap().into(),
        "--wifi-channel".into(),
        "0".into(),
    ])
    .unwrap();
    assert_eq!(automatic.wifi_channel, 0);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn json_events_escape_and_trace_redacts() {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let out = lines.clone();
    let e = JsonEvents::new(Arc::new(move |s| out.lock().unwrap().push(s)));
    e.emit("stage", &serde_json::json!({"message":"line\n\"\\\t"}));
    e.trace("IAP2 TX [wireless] 0x5703 frame=80B\n passphrase=private");
    let l = lines.lock().unwrap();
    assert!(!l[0].contains('\n'));
    let v: serde_json::Value = serde_json::from_str(&l[0]).unwrap();
    assert_eq!(v["message"], "line\n\"\\\t");
    assert!(v["time"].as_str().unwrap().ends_with('Z'));
    assert!(l[1].contains("0x5703"));
    assert!(!l[1].contains("private"));
}
#[test]
fn settings_round_trip_private_permissions() {
    let p = temp();
    let mut s = Settings {
        network_mode: "shared-wifi".into(),
        shared_wifi_networks: vec![openxplay_relay::settings::SharedWifiNetwork {
            ssid: "Car: Wi-Fi".into(),
            password: "secret123".into(),
        }],
        iphone: Some(phone(1).address),
        ..Default::default()
    };
    s.carlink_ports.insert("control".into(), 1234);
    s.save(&p).unwrap();
    assert_eq!(Settings::load(&p).unwrap(), s);
    assert_eq!(
        std::fs::metadata(p.join("settings.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o700
    );
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn admin_mapping_and_schema() {
    let s = Settings::default();
    let next=update_settings(&s,serde_json::json!({"networkMode":"shared-wifi","sharedWifiNetworks":[{"ssid":"Car","password":"password"}],"hotspotChannel":6,"iphone":"auto","wifiInterface":"wlan1","rfcommChannel":12,"requirePairingCode":true,"output":"both","carBluetoothAudio":true}).as_object().unwrap().clone()).unwrap();
    assert!(next.iphone.is_none());
    assert!(next.car_bluetooth_audio);
    assert_eq!(next.rfcomm_channel, Some(12));
    assert!(next.require_pairing_code);
    let keys = settings_schema()
        .into_iter()
        .flat_map(|s| s.fields.into_iter().map(|f| f.key))
        .collect::<Vec<_>>();
    for k in [
        "networkMode",
        "sharedWifiNetworks",
        "hotspotChannel",
        "iphone",
        "wifiInterface",
        "rfcommChannel",
        "requirePairingCode",
        "output",
        "carBluetoothAudio",
    ] {
        assert!(keys.iter().any(|s| s == k));
    }
}
#[test]
fn invalid_admin_update_is_atomic() {
    let s = Settings::default();
    assert!(update_settings(
        &s,
        serde_json::json!({"networkMode":"invalid","output":"both"})
            .as_object()
            .unwrap()
            .clone()
    )
    .is_err());
    assert!(update_settings(
        &s,
        serde_json::json!({"unknown":1})
            .as_object()
            .unwrap()
            .clone()
    )
    .is_err());
    assert_eq!(s.output, "web");
    // Keys kept from a newer relay stay out of reach of the admin page.
    let mut kept = Settings::default();
    kept.unknown
        .insert("futureFlag".into(), serde_json::json!(true));
    let result = update_settings(
        &kept,
        serde_json::json!({"futureFlag":false})
            .as_object()
            .unwrap()
            .clone(),
    );
    assert_eq!(
        result.unwrap_err().get("futureFlag").map(String::as_str),
        Some("error.setting")
    );
    let next = update_settings(
        &kept,
        serde_json::json!({"output":"both"})
            .as_object()
            .unwrap()
            .clone(),
    )
    .unwrap();
    assert_eq!(next.unknown, kept.unknown);
}
#[test]
fn identity_pairings_restart_and_exclusive_lock() {
    let p = temp();
    let state = PersistentState::open(&p).unwrap();
    let identity = state.identity().unwrap();
    state
        .pairings()
        .unwrap()
        .save("controller\t\n", &[7; 32])
        .unwrap();
    assert!(PersistentState::open(&p).is_err());
    state.close().unwrap();
    let next = PersistentState::open(&p).unwrap();
    assert_eq!(next.identity().unwrap().private_key, identity.private_key);
    assert_eq!(next.identity().unwrap().pairing_id, identity.pairing_id);
    let pairs = next.pairings().unwrap();
    assert_eq!(pairs.get("controller\t\n").unwrap(), Some([7; 32]));
    pairs.clear().unwrap();
    assert!(next
        .pairings()
        .unwrap()
        .get("controller\t\n")
        .unwrap()
        .is_none());
    drop(next);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn private_file_atomically_replaces_and_is_private() {
    let p = temp();
    let f = p.join("state");
    platform::private_state_file(&f, b"one").unwrap();
    platform::private_state_file(&f, b"two").unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"two");
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn preferred_port_falls_back_in_priority_order() {
    let tried = Mutex::new(Vec::new());
    let p = AirPlayPortSelector::select(7000, &|p| {
        tried.lock().unwrap().push(p);
        if p == 7003 {
            Ok(p)
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::AddrInUse))
        }
    })
    .unwrap();
    assert_eq!(p, 7003);
    assert_eq!(*tried.lock().unwrap(), [7000, 7001, 7002, 7003]);
}
#[test]
fn port_other_errors_propagate() {
    assert!(AirPlayPortSelector::select(7000, &|_| Err(
        std::io::ErrorKind::PermissionDenied.into()
    ))
    .is_err());
}
#[test]
fn wireless_address_skips_unusable() {
    let a = [
        "127.0.0.1",
        "0.0.0.0",
        "169.254.1.1",
        "224.0.0.1",
        "10.77.0.1",
    ]
    .map(|s| s.parse().unwrap());
    assert_eq!(WirelessHostAddress::select(&a), Some(a[4]));
}
#[test]
fn mac_validation() {
    assert!(valid_mac("a0:B1:C2:d3:e4:F5"));
    for a in ["a0:B1:C2:d3:e4", "a0:B1:C2:d3:e4:fg", "AA:BB:CC:DD:EE:FF\n"] {
        assert!(!valid_mac(a));
    }
}
fn format(codec: AudioCodecKind, rate: u32, channels: u8) -> AudioFormat {
    AudioFormat {
        codec,
        sample_rate: rate,
        channels,
        payload_type: 96,
        audio_type: "media".into(),
        rtp_clock_rate: rate,
    }
}
#[test]
fn media_only_aac_48k_stereo_with_asc() {
    let id = AudioStreamId {
        kind: 102,
        audio_type: "media".into(),
    };
    let source = format(AudioCodecKind::AacLc, 48000, 2);
    let f = AudioPolicy::format(&id, &source).unwrap();
    assert_eq!(f.stream, 0);
    assert_eq!(f.config.unwrap().as_ref(), [0x11, 0x90]);
    let (p, n) = AudioNormalizer::new(source).convert(&[1, 2, 3]).unwrap();
    assert_eq!(p, [1, 2, 3]);
    assert_eq!(n, 1024);
    for codec in [
        AudioCodecKind::AacLc,
        AudioCodecKind::Lpcm,
        AudioCodecKind::Opus,
        AudioCodecKind::Alac {
            bits_per_sample: 16,
        },
    ] {
        for rate in [16000, 44100, 48000] {
            for ch in [1, 2] {
                if codec == AudioCodecKind::AacLc && rate == 48000 && ch == 2 {
                    continue;
                }
                assert!(AudioPolicy::format(&id, &format(codec, rate, ch)).is_err());
            }
        }
    }
}
#[test]
fn role_stream_mapping_and_big_endian_lpcm() {
    for name in [
        "alert",
        "speechrecognition",
        "telephony",
        "default",
        "compatibility",
    ] {
        for (kind, stream) in [(102, 0), (100, 1), (101, 2)] {
            let id = AudioStreamId {
                kind,
                audio_type: name.into(),
            };
            let f = AudioPolicy::format(&id, &format(AudioCodecKind::Lpcm, 16000, 2)).unwrap();
            assert_eq!(f.stream, stream);
            let (b, n) = AudioNormalizer::new(format(AudioCodecKind::Lpcm, 16000, 2))
                .convert(&[0x12, 0x34, 0xab, 0xcd])
                .unwrap();
            assert_eq!(b, [0x34, 0x12, 0xcd, 0xab]);
            assert_eq!(n, 1);
        }
    }
    assert!(AudioPolicy::swap(&[1]).is_err());
    assert!(AudioPolicy::stream(&AudioStreamId {
        kind: 103,
        audio_type: "media".into()
    })
    .is_none());
}
#[test]
fn opus_passthrough_and_duration() {
    let b = [0xf8, 0xff, 0xfe];
    let source = format(AudioCodecKind::Opus, 24000, 1);
    let f = AudioPolicy::format(
        &AudioStreamId {
            kind: 100,
            audio_type: "speechrecognition".into(),
        },
        &source,
    )
    .unwrap();
    assert_eq!(f.codec, oxp_core::model::AudioCodec::Opus);
    assert_eq!(f.sample_rate, 48000);
    let (data, n) = AudioNormalizer::new(source).convert(&b).unwrap();
    assert_eq!(data, b);
    assert_eq!(n, 960);
}
#[test]
fn alac_is_dropped_with_diagnostic_reason() {
    let e = AudioPolicy::format(
        &AudioStreamId {
            kind: 100,
            audio_type: "default".into(),
        },
        &format(
            AudioCodecKind::Alac {
                bits_per_sample: 24,
            },
            48000,
            2,
        ),
    )
    .unwrap_err();
    assert!(e.to_string().contains("Stream wire has no ALAC codec"));
}
#[test]
fn recording_parameter_sets_avc_and_hevc() {
    let avcc = [
        1, 66, 0, 30, 255, 225, 0, 2, 0x67, 0x42, 1, 0, 2, 0x68, 0x11,
    ];
    let annex = [0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1, 0x68, 0x11];
    assert_eq!(
        RecordingMediaSink::parameter_sets(VideoCodec::H264, &avcc).unwrap(),
        annex
    );
    assert_eq!(
        RecordingMediaSink::parameter_sets(VideoCodec::H264, &annex).unwrap(),
        annex
    );
    let mut hvcc = vec![0; 23];
    hvcc[0] = 1;
    hvcc[22] = 2;
    hvcc.extend_from_slice(&[0xa0, 0, 1, 0, 2, 0x40, 1, 0xa1, 0, 1, 0, 2, 0x42, 1]);
    assert_eq!(
        RecordingMediaSink::parameter_sets(VideoCodec::H265, &hvcc).unwrap(),
        [0, 0, 0, 1, 0x40, 1, 0, 0, 0, 1, 0x42, 1]
    );
    assert!(RecordingMediaSink::parameter_sets(VideoCodec::H264, &avcc[..8]).is_err());
    assert!(RecordingMediaSink::parameter_sets(VideoCodec::H265, &hvcc[..24]).is_err());
}
#[test]
fn recording_audio_boundaries_and_metadata() {
    let p = temp();
    let sink = RecordingMediaSink::new(p.clone(), quiet()).unwrap();
    let id = AudioStreamId {
        kind: 100,
        audio_type: "media".into(),
    };
    let f = format(AudioCodecKind::Lpcm, 48000, 2);
    sink.on_audio_started(&id, &f, 11).unwrap();
    sink.on_audio_rtp(&id, &f, &[0x80, 96, 0, 1, 2], 12)
        .unwrap();
    sink.close().unwrap();
    sink.close().unwrap();
    let files = std::fs::read_dir(&p)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    let rtp = files
        .iter()
        .find(|p| p.extension().unwrap() == "rtp")
        .unwrap();
    assert_eq!(
        std::fs::read(rtp).unwrap(),
        [0, 0, 0, 5, 0, 0, 0, 12, 0x80, 96, 0, 1, 2]
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(rtp.with_extension("rtp.json")).unwrap()).unwrap();
    assert_eq!(metadata["sampleRate"], 48000);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn recording_video_writes_a_timing_index() {
    let p = temp();
    let sink = RecordingMediaSink::new(p.clone(), quiet()).unwrap();
    sink.on_video_codec(110, VideoCodec::H264).unwrap();
    sink.on_video_config(110, &[0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2])
        .unwrap();
    sink.on_video_frame(110, &[0, 0, 0, 1, 0x65, 9, 9], 1_000, 2_000)
        .unwrap();
    sink.on_video_frame(110, &[0, 0, 1, 0x41, 7], 17_000, 18_500)
        .unwrap();
    sink.close().unwrap();
    let index = std::fs::read_dir(&p)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().ends_with(".h264.jsonl"))
        .unwrap();
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&index)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["format"], "oxp-annexb-index/1");
    assert_eq!(lines[0]["codec"], "H264");
    assert_eq!(lines[1]["kind"], "config");
    assert_eq!(
        (lines[1]["offset"].as_u64(), lines[1]["len"].as_u64()),
        (Some(0), Some(12))
    );
    assert_eq!(lines[2]["offset"], 12);
    assert_eq!(lines[2]["senderNs"], 1_000);
    assert_eq!(lines[2]["arrivalNs"], 2_000);
    assert_eq!(lines[2]["nal"], serde_json::json!([5]));
    assert_eq!(
        (lines[3]["offset"].as_u64(), lines[3]["len"].as_u64()),
        (Some(19), Some(5))
    );
    assert_eq!(lines[3]["nal"], serde_json::json!([1]));
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn process_command_captures_failure_and_timeout() {
    let r = ProcessCommandRunner::new();
    let o = r
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "printf out; printf err >&2; exit 7".into(),
            ],
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(o.exit_code, 7);
    assert!(o.output.contains("out") && o.output.contains("err"));
    assert!(o
        .checked("test")
        .unwrap_err()
        .to_string()
        .contains("test failed (exit 7)"));
    let before = std::time::Instant::now();
    assert!(r
        .run(
            &["/bin/sleep".into(), "2".into()],
            Duration::from_millis(30)
        )
        .is_err());
    assert!(before.elapsed() < Duration::from_secs(1));
}

#[test]
fn shared_wifi_rejects_open_network_with_saved_password() {
    assert_eq!(
        openxplay_relay::linux_hotspot::existing_wifi_security("--", "password")
            .unwrap_err()
            .to_string(),
        "Connected Wi-Fi is open; clear the saved Wi-Fi password"
    );
}
#[test]
fn shared_wifi_rejects_secured_network_without_password() {
    assert_eq!(
        openxplay_relay::linux_hotspot::existing_wifi_security("WPA2", "")
            .unwrap_err()
            .to_string(),
        "Connected Wi-Fi is secured; enter its WPA2 password"
    );
}
#[test]
fn shared_wifi_requires_personal_security() {
    for security in ["WPA2 802.1X", "WEP", "EAP"] {
        assert_eq!(
            openxplay_relay::linux_hotspot::existing_wifi_security(security, "password")
                .unwrap_err()
                .to_string(),
            "Existing Wi-Fi requires an open or WPA2-Personal network"
        );
    }
}
#[test]
fn shared_wifi_open_and_personal_mapping() {
    use carplay_iap2::Iap2WirelessSecurity;
    assert_eq!(
        openxplay_relay::linux_hotspot::existing_wifi_security("--", "").unwrap(),
        Iap2WirelessSecurity::None
    );
    assert_eq!(
        openxplay_relay::linux_hotspot::existing_wifi_security("WPA2 WPA3", "password").unwrap(),
        Iap2WirelessSecurity::WpaWpa2
    );
}

#[test]
fn bonjour_probe_request_exact_bytes() {
    use openxplay_relay::linux_bonjour::CarPlayBonjourProtocol as P;
    assert_eq!(P::connect_probe_request("10.77.0.2",7000,"950.7.1","AA:BB:CC:DD:EE:FF").unwrap(), "GET /ctrl-int/1/connect HTTP/1.1\r\nHost: 10.77.0.2:7000\r\nUser-Agent: AirPlay/950.7.1\r\nAirPlay-Receiver-Device-ID: AABBCCDDEEFF\r\nConnection: close\r\n\r\n");
}
#[test]
fn bonjour_probe_normalizes_ipv6_scope() {
    use openxplay_relay::linux_bonjour::CarPlayBonjourProtocol as P;
    assert!(
        P::connect_probe_request("[fe80::1%wlan0]", 7000, "950.7.1", "AA:BB")
            .unwrap()
            .contains("Host: [fe80::1]:7000\r\n")
    );
}
#[test]
fn bonjour_probe_rejects_header_injection() {
    use openxplay_relay::linux_bonjour::CarPlayBonjourProtocol as P;
    for (host, port, version, id) in [
        ("", 7000, "950.7.1", "AA"),
        ("host", 0, "950.7.1", "AA"),
        ("host\r\nx:1", 7000, "950.7.1", "AA"),
        ("host", 7000, "950\n", "AA"),
        ("host", 7000, "950", "AA\n"),
        ("host", 7000, "950", ":"),
    ] {
        assert!(P::connect_probe_request(host, port, version, id).is_err());
    }
}

#[test]
fn iphone_selection_retains_stable_reason_codes() {
    use openxplay_relay::linux_bluetooth::IphoneSelection as S;
    let d = phone(1);
    assert_eq!(
        S::select(Some(&d.address), std::slice::from_ref(&d)).reason(),
        "Selected"
    );
    assert_eq!(
        S::select(Some(&d.address), &[]).reason(),
        "SelectedUnpaired"
    );
    assert_eq!(S::select(None, &[]).reason(), "NoIphone");
    assert_eq!(S::Connected(d.clone()).reason(), "Connected");
    assert_eq!(
        S::MultipleConnected(vec![d.clone()]).reason(),
        "MultipleConnected"
    );
    assert_eq!(S::OnlyCandidate(d.clone()).reason(), "OnlyCandidate");
    assert_eq!(
        S::MultipleCandidates(vec![d.clone()]).reason(),
        "MultipleCandidates"
    );
    assert_eq!(S::OnlyBonded(d).reason(), "OnlyBonded");
}

#[test]
fn five_ghz_capability_comes_from_enabled_iw_frequencies() {
    let ufi003 = "\tBand 1:\n\t\tFrequencies:\n\t\t\t* 2412 MHz [1] (20.0 dBm)\n\t\t\t* 2484 MHz [14] (disabled)\n";
    assert!(!openxplay_relay::linux_hotspot::lists_5ghz(ufi003));
    let dual = "\tBand 2:\n\t\tFrequencies:\n\t\t\t* 5180.0 MHz [36] (20.0 dBm)\n";
    assert!(openxplay_relay::linux_hotspot::lists_5ghz(dual));
    let disabled = "\tBand 2:\n\t\tFrequencies:\n\t\t\t* 5180 MHz [36] (disabled)\n";
    assert!(!openxplay_relay::linux_hotspot::lists_5ghz(disabled));
}
