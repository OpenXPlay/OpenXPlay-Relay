// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

use super::*;
use crate::net_inventory::{Interface, Radio};
use crate::wifi_capabilities::parse_phy;

fn radio(name: &str, phy: &str, index: u8, capture: &str) -> Interface {
    Interface {
        name: name.into(),
        phy: Some(phy.into()),
        kind: "wifi".into(),
        mac: Some(format!("02:00:00:00:00:{index:02x}")),
        current_mac: Some(format!("02:00:00:00:00:{index:02x}")),
        wifi: Some(Radio::from_phy(parse_phy(capture))),
        ..Default::default()
    }
}
fn n1() -> Interface {
    radio(
        "wlan0",
        "phy0",
        1,
        include_str!("../tests/fixtures/iw-phy-n1-cn.txt"),
    )
}
fn usb() -> Interface {
    radio(
        "wlan1",
        "phy2",
        2,
        include_str!("../tests/fixtures/iw-phy-rtl8822bu-cn.txt"),
    )
}
fn inventory(interfaces: Vec<Interface>) -> Inventory {
    Inventory {
        interfaces,
        ..Default::default()
    }
}
fn plan(inv: &Inventory, settings: &Settings, cli: &BTreeMap<String, String>) -> Plan {
    resolve_plan(
        inv,
        settings,
        &Legacy {
            wifi_interface: &settings.wifi_interface,
            carlink_interface: &settings.carlink_interface,
            network_mode: &settings.network_mode,
            hotspot_band: &settings.hotspot_band,
            hotspot_channel: settings.hotspot_channel,
            cli,
        },
        "phicomm-n1",
        &settings.output,
    )
}
fn admin(interface: Option<&str>, channel: u8) -> AdminHotspot {
    AdminHotspot {
        enabled: true,
        interface: interface.map(str::to_string),
        band: "5".into(),
        channel,
        ..Default::default()
    }
}
fn custom(iphone: &str, carlink: Option<&str>, admin: AdminHotspot) -> NetworkRoles {
    NetworkRoles {
        mode: "custom".into(),
        iphone: Some(Iphone {
            link: "hotspot".into(),
            interface: iphone.into(),
        }),
        car_browser: Some(CarBrowser {
            network: "iphone".into(),
            interface: None,
        }),
        carlink: Some(Carlink {
            interface: carlink.map(str::to_string),
        }),
        admin_hotspot: Some(admin),
    }
}
#[test]
fn hardware_plan_table() {
    struct Case {
        name: &'static str,
        interfaces: Vec<Interface>,
        roles: NetworkRoles,
        legacy_carlink: &'static str,
        cli: BTreeMap<String, String>,
        iphone: &'static str,
        carlink: &'static str,
        admin: Option<&'static str>,
        follow: bool,
        present: Vec<&'static str>,
        absent: Vec<&'static str>,
    }
    let cases = vec![
        Case {
            name: "N1 onboard",
            interfaces: vec![n1()],
            roles: Default::default(),
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan0",
            admin: None,
            follow: true,
            present: vec!["follow-over", "carlink-band-gap"],
            absent: vec!["roles-conflict", "admin-hotspot-nat"],
        },
        Case {
            name: "N1 and USB CN",
            interfaces: vec![n1(), usb()],
            roles: NetworkRoles {
                admin_hotspot: Some(admin(None, 0)),
                ..Default::default()
            },
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan1",
            admin: Some("wlan1"),
            follow: false,
            present: vec!["time-shared", "admin-hotspot-nat"],
            absent: vec![
                "carlink-band-gap",
                "roles-conflict",
                "follow-over",
                "ap-spacing",
            ],
        },
        Case {
            name: "UFI003",
            interfaces: vec![radio(
                "wlan0",
                "phy0",
                1,
                include_str!("../tests/fixtures/iw-phy-ufi003.txt"),
            )],
            roles: Default::default(),
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan0",
            admin: None,
            follow: true,
            present: vec!["carlink-no-5ghz"],
            absent: vec!["no-ap-channel", "no-ap-mode"],
        },
        Case {
            name: "missing USB",
            interfaces: vec![n1()],
            roles: custom("wlan0", Some("02:00:00:00:00:02"), Default::default()),
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "02:00:00:00:00:02",
            admin: None,
            follow: false,
            present: vec!["interface-missing"],
            absent: vec!["no-ap-mode", "roles-conflict"],
        },
        Case {
            name: "single radio conflict",
            interfaces: vec![n1()],
            roles: custom("wlan0", Some("wlan0"), admin(Some("wlan0"), 0)),
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan0",
            admin: Some("wlan0"),
            follow: true,
            present: vec!["roles-conflict"],
            absent: vec!["no-ap-mode"],
        },
        Case {
            name: "AP spacing",
            interfaces: vec![n1(), usb()],
            roles: custom("wlan0", Some("wlan1"), admin(Some("wlan1"), 157)),
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan1",
            admin: Some("wlan1"),
            follow: false,
            present: vec!["ap-spacing", "time-shared"],
            absent: vec!["roles-conflict"],
        },
        Case {
            name: "legacy Carlink",
            interfaces: vec![n1(), usb()],
            roles: Default::default(),
            legacy_carlink: "wlan0",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan0",
            admin: None,
            follow: true,
            present: vec!["follow-over", "carlink-band-gap"],
            absent: vec!["roles-conflict"],
        },
        Case {
            name: "CLI overrides custom",
            interfaces: vec![n1(), usb()],
            roles: custom("missing", Some("missing"), Default::default()),
            legacy_carlink: "",
            cli: BTreeMap::from([
                ("--wifi-interface".into(), "wlan0".into()),
                ("--carlink-interface".into(), "wlan1".into()),
                ("--network-mode".into(), "shared-wifi".into()),
            ]),
            iphone: "wlan0",
            carlink: "wlan1",
            admin: None,
            follow: false,
            present: vec![],
            absent: vec!["interface-missing", "roles-conflict"],
        },
        Case {
            name: "MAC after rename",
            interfaces: vec![
                n1(),
                Interface {
                    name: "wlan2".into(),
                    ..usb()
                },
            ],
            roles: custom(
                "02:00:00:00:00:01",
                Some("02:00:00:00:00:02"),
                Default::default(),
            ),
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan2",
            admin: None,
            follow: false,
            present: vec![],
            absent: vec!["interface-missing", "carlink-band-gap"],
        },
        Case {
            name: "admin unavailable",
            interfaces: vec![n1()],
            roles: NetworkRoles {
                admin_hotspot: Some(admin(None, 0)),
                ..Default::default()
            },
            legacy_carlink: "",
            cli: BTreeMap::new(),
            iphone: "wlan0",
            carlink: "wlan0",
            admin: None,
            follow: true,
            present: vec!["no-ap-channel"],
            absent: vec!["roles-conflict"],
        },
    ];
    for case in cases {
        let inv = inventory(case.interfaces);
        let settings = Settings {
            network_roles: case.roles,
            carlink_interface: case.legacy_carlink.into(),
            output: "both".into(),
            ..Default::default()
        };
        let p = plan(&inv, &settings, &case.cli);
        let name = |role: &Role| {
            role.interface
                .as_ref()
                .map(|i| i.name.as_deref().unwrap_or(&i.reference).to_string())
        };
        assert_eq!(
            name(&p.iphone.role).as_deref(),
            Some(case.iphone),
            "{}",
            case.name
        );
        assert_eq!(
            name(&p.carlink.role).as_deref(),
            Some(case.carlink),
            "{}",
            case.name
        );
        assert_eq!(
            name(&p.admin_hotspot.role).as_deref(),
            case.admin,
            "{}",
            case.name
        );
        assert_eq!(p.carlink.follow_over, case.follow, "{}", case.name);
        let checks = checks(&p, &inv);
        for code in case.present {
            assert!(
                checks.iter().any(|c| c.code == code),
                "{} lacks {code}",
                case.name
            );
        }
        for code in case.absent {
            assert!(
                !checks.iter().any(|c| c.code == code),
                "{} has {code}",
                case.name
            );
        }
        if case.name == "CLI overrides custom" {
            assert_eq!(p.iphone.role.source, "cli");
            assert_eq!(p.carlink.role.source, "cli");
            assert_eq!(p.iphone.link, "saved-network");
        }
        if case.name == "legacy Carlink" {
            assert_eq!(p.carlink.role.source, "legacy");
        }
        if case.name == "AP spacing" {
            assert_eq!(
                checks
                    .iter()
                    .find(|c| c.code == "ap-spacing")
                    .unwrap()
                    .params["mhz"],
                40
            );
        }
    }
}
#[test]
fn auto_ranks_station_channels_then_names_and_requires_a_different_phy() {
    let mut more = usb();
    more.name = "wlan9".into();
    let mut less = usb();
    less.name = "wlan1".into();
    less.wifi = Some(Radio::from_phy(parse_phy(
        "Supported interface modes:\n * managed\n * AP\n * 5745 MHz [149]",
    )));
    let mut shared = usb();
    shared.name = "wlan3".into();
    shared.phy = Some("phy0".into());
    let inv = inventory(vec![less, more.clone(), shared, n1()]);
    assert_eq!(
        plan(&inv, &Settings::default(), &BTreeMap::new())
            .carlink
            .role
            .interface
            .unwrap()
            .name
            .as_deref(),
        Some("wlan9")
    );
    more.name = "wlan2".into();
    let inv = inventory(vec![more, usb(), n1()]);
    assert_eq!(
        plan(&inv, &Settings::default(), &BTreeMap::new())
            .carlink
            .role
            .interface
            .unwrap()
            .name
            .as_deref(),
        Some("wlan1")
    );
}
#[test]
fn missing_name_is_retained_and_current_mac_also_resolves() {
    let mut interface = usb();
    interface.current_mac = Some("02:00:00:00:00:03".into());
    let inv = inventory(vec![n1(), interface]);
    let settings = Settings {
        network_roles: custom("missing", Some("02:00:00:00:00:03"), Default::default()),
        ..Default::default()
    };
    let p = plan(&inv, &settings, &BTreeMap::new());
    assert!(p.iphone.role.missing);
    assert_eq!(
        p.iphone.role.interface.unwrap().name.as_deref(),
        Some("missing")
    );
    assert_eq!(
        p.carlink.role.interface.unwrap().mac.as_deref(),
        Some("02:00:00:00:00:02")
    );
}
#[test]
fn ap_mode_channels_and_unknown_are_distinct() {
    for (capture, code) in [
        (Some("Supported interface modes:\n * managed\n * 2437 MHz [6]"), Some("no-ap-mode")),
        (Some("Supported interface modes:\n * managed\n * AP\n * 2437 MHz [6] (no IR)\n * 5745 MHz [149] (radar detection)"), Some("no-ap-channel")),
        (None, None),
    ] {
        let mut interface = n1(); interface.wifi = capture.map(|s| Radio::from_phy(parse_phy(s)));
        let inv = inventory(vec![interface]);
        let p = plan(&inv, &Settings::default(), &BTreeMap::new());
        let checks = checks(&p, &inv);
        assert_eq!(checks.iter().find(|c| c.level == "error").map(|c| c.code.as_str()), code);
    }
}
#[test]
fn combination_limits_do_not_confuse_two_aps_with_ap_station() {
    for (description, requested, allowed) in [
        (
            "* #{ managed } <= 1, #{ AP } <= 1, total <= 2, #channels <= 1",
            vec!["AP", "AP"],
            false,
        ),
        (
            "* #{ managed } <= 1, #{ AP } <= 1, total <= 2, #channels <= 1",
            vec!["AP", "managed"],
            true,
        ),
        (
            "* #{ managed, AP } <= 1, total <= 1",
            vec!["AP", "managed"],
            false,
        ),
        (
            "* #{ managed, AP } <= 2, total <= 2",
            vec!["AP", "AP"],
            true,
        ),
    ] {
        let phy = parse_phy(&format!("valid interface combinations:\n{description}\n"));
        assert_eq!(phy.combinations[0].allows(&requested), allowed);
    }
}
#[test]
fn validation_covers_country_password_shape_and_refs() {
    assert_eq!(
        serde_json::to_value(NetworkRoles::default()).unwrap(),
        json!({"mode":"auto"})
    );
    for value in ["00", "CN", "US", "GB", "AX"] {
        assert!(valid_country(value));
    }
    for value in ["cn", "ZZ", "USA", "UK", "0", "99"] {
        assert!(!valid_country(value));
    }
    for value in ["12345678", "a printable password!", &"x".repeat(63)] {
        assert!(valid_password(value));
    }
    for value in ["short", "1234567\n", "1234567é", &"x".repeat(64)] {
        assert!(!valid_password(value));
    }
    for value in ["wlan2", "enp0s1", "02:00:00:00:00:0a"] {
        assert!(valid_ifref(value));
    }
    for value in ["", "../wlan0", ".", "02:00:00:00:00:0A", "wlan0;id"] {
        assert!(!valid_ifref(value));
    }
    for value in [
        json!({"mode":"invalid"}),
        json!({"mode":"auto","extra":true}),
        json!({"mode":"custom"}),
        json!({"mode":"auto","adminHotspot":{"enabled":true,"band":"5","channel":6,"ssid":"Test"}}),
    ] {
        assert!(
            serde_json::from_value::<NetworkRoles>(value).map_or(true, |r| r.validate().is_err())
        );
    }
}

#[test]
fn two_ap_combinations_respect_channel_limits() {
    for (channels, conflict) in [(1, true), (2, false)] {
        let capture = format!("Supported interface modes:\n * managed\n * AP\n * 5745 MHz [149]\n * 5785 MHz [157]\nvalid interface combinations:\n * #{{ AP }} <= 2, total <= 2, #channels <= {channels}\n");
        let inv = inventory(vec![
            radio("wlan0", "phy0", 1, &capture),
            radio("wlan1", "phy0", 2, &capture),
        ]);
        let settings = Settings {
            network_roles: custom("wlan0", None, admin(Some("wlan1"), 157)),
            ..Default::default()
        };
        let p = plan(&inv, &settings, &BTreeMap::new());
        assert_eq!(
            checks(&p, &inv).iter().any(|c| c.code == "roles-conflict"),
            conflict
        );
    }
}
