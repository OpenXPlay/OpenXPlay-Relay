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
fn cached_plan(choices: &mut AutomaticChoices, inv: &Inventory, settings: &Settings) -> Plan {
    choices.resolve(
        inv,
        settings,
        &Legacy {
            wifi_interface: &settings.wifi_interface,
            carlink_interface: &settings.carlink_interface,
            network_mode: &settings.network_mode,
            hotspot_band: &settings.hotspot_band,
            hotspot_channel: settings.hotspot_channel,
            cli: &BTreeMap::new(),
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
    for value in ["system", "00", "CN", "US", "GB", "AX"] {
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

#[test]
fn plan_applies_effective_options_table() {
    struct Case {
        radios: Vec<Interface>,
        roles: NetworkRoles,
        legacy: &'static str,
        cli: BTreeMap<String, String>,
        expected: (&'static str, &'static str, &'static str),
    }
    let mut busy = usb();
    busy.activity.kind = "other-ap".into();
    busy.activity.profile = Some("Synthetic foreign AP".into());
    let cases = [
        Case {
            radios: vec![n1()],
            roles: Default::default(),
            legacy: "",
            cli: BTreeMap::new(),
            expected: ("wlan0", "device-hotspot", ""),
        },
        Case {
            radios: vec![n1(), usb()],
            roles: Default::default(),
            legacy: "",
            cli: BTreeMap::new(),
            expected: ("wlan0", "device-hotspot", "wlan1"),
        },
        Case {
            radios: vec![n1(), usb()],
            roles: Default::default(),
            legacy: "wlan0",
            cli: BTreeMap::new(),
            expected: ("wlan0", "device-hotspot", ""),
        },
        Case {
            radios: vec![n1(), usb()],
            roles: custom("02:00:00:00:00:02", Some("wlan0"), Default::default()),
            legacy: "",
            cli: BTreeMap::new(),
            expected: ("wlan1", "device-hotspot", "wlan0"),
        },
        Case {
            radios: vec![n1(), usb()],
            roles: custom("missing", Some("missing"), Default::default()),
            legacy: "",
            cli: BTreeMap::from([
                ("--wifi-interface".into(), "wlan1".into()),
                ("--carlink-interface".into(), "wlan0".into()),
                ("--network-mode".into(), "shared-wifi".into()),
            ]),
            expected: ("wlan1", "shared-wifi", "wlan0"),
        },
        Case {
            radios: vec![n1()],
            roles: custom("wlan0", Some("wlan9"), Default::default()),
            legacy: "",
            cli: BTreeMap::new(),
            expected: ("wlan0", "device-hotspot", "wlan9"),
        },
        Case {
            radios: vec![n1(), busy],
            roles: Default::default(),
            legacy: "",
            cli: BTreeMap::new(),
            expected: ("wlan0", "device-hotspot", ""),
        },
    ];
    let dir = std::env::temp_dir().join(format!(
        "roles-apply-{}",
        crate::admin::auth::random_hex(8).unwrap()
    ));
    let options =
        crate::Options::parse(&["--state-dir".into(), dir.to_string_lossy().into_owned()]).unwrap();
    for case in cases {
        let settings = Settings {
            network_roles: case.roles,
            carlink_interface: case.legacy.into(),
            ..Default::default()
        };
        let inv = inventory(case.radios);
        let p = plan(&inv, &settings, &case.cli);
        let mut options = options.clone();
        apply_plan(&p, &mut options);
        assert_eq!(
            (
                options.wifi_interface.as_str(),
                options.settings.network_mode.as_str(),
                options.settings.carlink_interface.as_str()
            ),
            case.expected
        );
        if inv.interfaces.iter().any(foreign_busy) {
            let check = checks(&p, &inv)
                .into_iter()
                .find(|c| c.code == "interface-busy")
                .unwrap();
            assert_eq!(check.level, "info");
            assert_eq!(check.params["connection"], "Synthetic foreign AP");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn restarts_follow_interface_link_and_presence_changes_only() {
    let inv = inventory(vec![n1(), usb()]);
    let original = plan(&inv, &Settings::default(), &BTreeMap::new());
    let mut next = original.clone();
    next.car_browser.network = "interface".into();
    next.car_browser.role = resolve_interface(&inv, Some("wlan1"), "custom");
    assert_eq!(changes(&original, &next), ["carBrowserAddress"]);
    next = original.clone();
    next.iphone.link = "saved-network".into();
    assert_eq!(
        changes(&original, &next),
        ["controller", "carBrowserAddress"]
    );
    next = original.clone();
    next.carlink.role.interface.as_mut().unwrap().name = Some("wlan2".into());
    assert_eq!(changes(&original, &next), ["carlink"]);
    next = original.clone();
    next.carlink.role.missing = true;
    next.iphone.role.missing = true;
    next.car_browser.role.missing = true;
    assert!(changes(&original, &next).is_empty());
    let running = RunningLinks::from_plan(&original, |_| true);
    let absent = RunningLinks::from_plan(&next, |name| name != "wlan1");
    assert_eq!(running.changes(&absent), ["carlink"]);
    assert_eq!(absent.changes(&running), ["carlink"]);
    next = original.clone();
    next.source = "custom".into();
    assert!(changes(&original, &next).is_empty());
    next.iphone.role.interface.as_mut().unwrap().name = Some("wlan3".into());
    assert_eq!(
        changes(&original, &next),
        ["controller", "carBrowserAddress"]
    );
}

#[test]
fn automatic_choices_survive_activity_but_reselect_on_adapter_and_capability_changes() {
    let settings = Settings {
        network_roles: NetworkRoles {
            admin_hotspot: Some(admin(None, 0)),
            ..Default::default()
        },
        output: "both".into(),
        ..Default::default()
    };
    let mut inv = inventory(vec![n1(), usb()]);
    let mut choices = AutomaticChoices::default();
    // Reads before the probe do not pin the empty-inventory follow-over.
    cached_plan(&mut choices, &Inventory::default(), &settings);
    inv.probed_at = Some(1);
    let mut previous = cached_plan(&mut choices, &inv, &settings);
    for round in 0..32 {
        for activity in ["idle", "other-ap", "idle", "other-network", "carlink"] {
            let usb = inv
                .interfaces
                .iter_mut()
                .find(|i| i.name == "wlan1")
                .unwrap();
            usb.activity.kind = activity.into();
            usb.activity.profile = Some(format!("Synthetic profile {round}"));
            usb.current_mac = Some(format!("02:00:00:00:01:{round:02x}"));
            usb.operstate = Some(if activity == "idle" { "down" } else { "up" }.into());
            usb.ipv4 = vec![format!("192.0.2.{round}/24")];
            inv.probed_at = Some(round + 2);
            inv.interfaces.reverse();
            let next = cached_plan(&mut choices, &inv, &settings);
            assert_eq!(next.carlink.role.name(), Some("wlan1"), "{activity}");
            assert_eq!(next.admin_hotspot.role.name(), Some("wlan1"), "{activity}");
            assert!(changes(&previous, &next).is_empty(), "{activity}");
            let busy: Vec<_> = checks(&next, &inv)
                .into_iter()
                .filter(|c| c.code == "interface-busy")
                .collect();
            assert_eq!(
                busy.len(),
                usize::from(matches!(activity, "other-ap" | "other-network"))
            );
            for check in busy {
                assert_eq!(
                    check.level,
                    if activity == "other-ap" {
                        "warning"
                    } else {
                        "info"
                    }
                );
                assert_eq!(check.roles, ["carlink", "adminHotspot"]);
            }
            previous = next;
        }
    }
    inv.interfaces.retain(|i| i.name != "wlan1");
    let next = cached_plan(&mut choices, &inv, &settings);
    assert_eq!(next.carlink.role.name(), Some("wlan0"));
    assert!(next.carlink.follow_over);
    assert_eq!(next.admin_hotspot.role.name(), None);
    assert_eq!(changes(&previous, &next), ["carlink", "adminHotspot"]);
    previous = next;
    inv.interfaces.push(usb());
    let next = cached_plan(&mut choices, &inv, &settings);
    assert_eq!(next.carlink.role.name(), Some("wlan1"));
    assert_eq!(next.admin_hotspot.role.name(), Some("wlan1"));
    assert_eq!(changes(&previous, &next), ["carlink", "adminHotspot"]);
    previous = next;
    inv.interfaces[1].wifi = n1().wifi;
    inv.interfaces[1]
        .wifi
        .as_mut()
        .unwrap()
        .capabilities
        .station = false;
    inv.interfaces[1].wifi.as_mut().unwrap().capabilities.ap = false;
    let next = cached_plan(&mut choices, &inv, &settings);
    assert_eq!(next.carlink.role.name(), Some("wlan0"));
    assert_eq!(next.admin_hotspot.role.name(), None);
    assert_eq!(changes(&previous, &next), ["carlink", "adminHotspot"]);
    inv.interfaces[1] = usb();
    let restored = cached_plan(&mut choices, &inv, &settings);
    assert_eq!(restored.carlink.role.name(), Some("wlan1"));
    assert_eq!(restored.admin_hotspot.role.name(), Some("wlan1"));
    assert_eq!(changes(&next, &restored), ["carlink", "adminHotspot"]);
}

#[test]
fn automatic_follow_over_stays_until_a_fresh_selection() {
    let settings = Settings::default();
    let mut inv = inventory(vec![n1(), usb()]);
    inv.probed_at = Some(1);
    inv.interfaces[1].activity.kind = "other-ap".into();
    let mut choices = AutomaticChoices::default();
    let before = cached_plan(&mut choices, &inv, &settings);
    assert!(before.carlink.follow_over);
    inv.interfaces[1].activity.kind = "idle".into();
    let kept = cached_plan(&mut choices, &inv, &settings);
    assert!(changes(&before, &kept).is_empty());
    assert_eq!(kept.carlink.role.name(), Some("wlan0"));
    let fresh = cached_plan(&mut AutomaticChoices::default(), &inv, &settings);
    assert_eq!(fresh.carlink.role.name(), Some("wlan1"));
    assert_eq!(changes(&kept, &fresh), ["carlink"]);
}
#[test]
fn browser_address_and_phy_hold_follow_selected_network() {
    let mut ethernet = Interface {
        name: "eth0".into(),
        kind: "ethernet".into(),
        ..Default::default()
    };
    ethernet.ipv4 = vec!["192.0.2.2".into()];
    let inv = inventory(vec![n1(), usb(), ethernet]);
    let mut p = plan(&inv, &Settings::default(), &BTreeMap::new());
    let iphone = Some("192.0.2.1".parse().unwrap());
    let rows = vec![("eth0".into(), "192.0.2.2".parse().unwrap(), 24)];
    assert_eq!(browser_address(&p, iphone, &rows), iphone);
    p.car_browser.network = "interface".into();
    p.car_browser.role = resolve_interface(&inv, Some("eth0"), "custom");
    assert_eq!(
        browser_address(&p, iphone, &rows),
        Some("192.0.2.2".parse().unwrap())
    );
    assert_eq!(browser_address(&p, iphone, &[]), None);
    assert!(!shared_radio(&inv, &p));
    p.carlink.role = p.iphone.role.clone();
    assert!(shared_radio(&inv, &p));
    let mut virtual_radio = usb();
    virtual_radio.phy = n1().phy;
    let inv = inventory(vec![n1(), virtual_radio]);
    p.carlink.role = resolve_interface(&inv, Some("wlan1"), "custom");
    assert!(shared_radio(&inv, &p));
}

#[test]
fn admin_automatic_channels_follow_radio_band_and_eighty_mhz_spacing() {
    let defaults: AdminHotspot = serde_json::from_value(json!({"enabled":true})).unwrap();
    assert_eq!(defaults.ssid, "OpenXPlay-Admin");
    assert_eq!(defaults.band, "auto");
    assert_eq!(defaults.channel, 0);
    let mut settings = Settings {
        hotspot_band: "5".into(),
        hotspot_channel: 149,
        admin_hotspot_password: Some("synthetic-password".into()),
        ..Default::default()
    };
    settings.network_roles.admin_hotspot = Some(AdminHotspot {
        enabled: true,
        ..Default::default()
    });
    let mut inv = inventory(vec![n1(), usb()]);
    let p = plan(&inv, &settings, &BTreeMap::new());
    assert_eq!(p.admin_hotspot.ssid, "OpenXPlay-Admin");
    assert_eq!(
        (
            p.iphone.channel,
            p.admin_hotspot.band.as_str(),
            p.admin_hotspot.channel
        ),
        (149, "5", 165)
    );
    assert_eq!(p.admin_hotspot.time_shared_with.as_deref(), Some("carlink"));
    assert!(!checks(&p, &inv).iter().any(|c| c.code == "ap-spacing"));
    // With no 80 MHz choice, the farthest channel wins and remains a warning.
    inv.interfaces[1].wifi = Some(Radio::from_phy(parse_phy("Supported interface modes:\n * managed\n * AP\nBand 2:\n * 5745 MHz [149] (20.0 dBm)\n * 5785 MHz [157] (20.0 dBm)\n")));
    let p = plan(&inv, &settings, &BTreeMap::new());
    assert_eq!(p.admin_hotspot.channel, 157);
    assert!(checks(&p, &inv)
        .iter()
        .any(|c| c.code == "ap-spacing" && c.level == "warning"));
    // An explicitly selected 2.4-only radio supplies the default band and a usable AP channel.
    settings
        .network_roles
        .admin_hotspot
        .as_mut()
        .unwrap()
        .interface = Some("wlan1".into());
    inv.interfaces[1].wifi = Some(Radio::from_phy(parse_phy(include_str!(
        "../tests/fixtures/iw-phy-ufi003.txt"
    ))));
    let p = plan(&inv, &settings, &BTreeMap::new());
    assert_eq!(p.admin_hotspot.band, "2.4");
    assert!(inv.interfaces[1]
        .wifi
        .as_ref()
        .unwrap()
        .bands
        .iter()
        .any(|b| b.band == "2.4" && b.ap_channels.contains(&u16::from(p.admin_hotspot.channel))));
}
#[test]
fn admin_password_busy_country_and_restart_checks_are_independent() {
    let mut inv = inventory(vec![n1(), usb()]);
    inv.country.configured = Some("CN".into());
    inv.country.global = Some("00".into());
    let mut settings = Settings::default();
    settings.network_roles.admin_hotspot = Some(admin(Some("wlan1"), 165));
    let p = plan(&inv, &settings, &BTreeMap::new());
    let found = checks(&p, &inv);
    assert!(found
        .iter()
        .any(|c| c.code == "admin-hotspot-password" && c.level == "error"));
    assert!(found
        .iter()
        .any(|c| c.code == "country-not-applied" && c.level == "warning"));
    inv.country.configured = Some("system".into());
    inv.interfaces[1].activity.kind = "other-ap".into();
    assert!(checks(&p, &inv).iter().any(|c| c.code == "interface-busy"
        && c.level == "warning"
        && c.roles.contains(&"adminHotspot".into())));
    assert!(!checks(&p, &inv)
        .iter()
        .any(|c| c.code == "country-not-applied"));
    let mut next = p.clone();
    next.admin_hotspot.ssid = "Changed".into();
    assert_eq!(changes(&p, &next), ["adminHotspot"]);
    next = p.clone();
    next.iphone.channel = 165;
    assert_eq!(changes(&p, &next), ["controller", "carBrowserAddress"]);
}
