// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

use super::*;
use crate::CommandResult;
use std::{fs, os::unix::fs::symlink};

#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<Vec<String>>>,
}
impl CommandRunner for Fake {
    fn run(&self, args: &[String], timeout: Duration) -> std::io::Result<CommandResult> {
        assert!(timeout <= Duration::from_secs(3));
        self.calls.lock().unwrap().push(args.to_vec());
        let args: Vec<_> = args.iter().map(String::as_str).collect();
        let output = match args.as_slice() {
            ["iw", "reg", "get"] => include_str!("../tests/fixtures/iw-reg-cn.txt"),
            ["iw", "phy", "phy0", "info"] => include_str!("../tests/fixtures/iw-phy-n1-cn.txt"),
            ["iw", "dev", _, "info"] => "type AP\nssid Synthetic-Test\nchannel 149 (5745 MHz), width: 40 MHz, center1: 5755 MHz",
            ["ethtool", "-P", "wlan0"] => "Permanent address: 02:00:00:00:00:01",
            ["ethtool", "-P", _] => "Permanent address: 00:00:00:00:00:00",
            ["udevadm", "info", "-q", "property", "-p", _] => "ID_MODEL_FROM_DATABASE=Synthetic Wi-Fi\nID_MODEL=Other",
            ["nmcli", "-t", "-f", "DEVICE,NAME,UUID", "connection", "show", "--active"] => "wlan0:oxp-carplay-test:synthetic-id\nwlan1:Foreign\\:profile:synthetic-id",
            _ => return Err(std::io::Error::other("tool absent")),
        };
        Ok(CommandResult {
            exit_code: 0,
            output: output.into(),
        })
    }
}
fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "net-inventory-{}",
        crate::admin::auth::random_hex(8).unwrap()
    ));
    fs::create_dir_all(root.join("sys/class/net")).unwrap();
    root
}
fn device(root: &Path, name: &str, wifi: bool, bus_name: &str) {
    let path = root.join("sys/class/net").join(name);
    let device = root.join("sys/devices").join(name);
    fs::create_dir_all(&path).unwrap();
    fs::create_dir_all(&device).unwrap();
    symlink(&device, path.join("device")).unwrap();
    symlink(format!("/sys/bus/{bus_name}"), device.join("subsystem")).unwrap();
    symlink("/sys/bus/test/drivers/synthetic", device.join("driver")).unwrap();
    fs::write(path.join("type"), "1\n").unwrap();
    fs::write(path.join("address"), "02:00:00:00:00:02\n").unwrap();
    fs::write(path.join("operstate"), "up\n").unwrap();
    if wifi {
        fs::create_dir_all(root.join("sys/class/ieee80211/phy0")).unwrap();
        symlink(root.join("sys/class/ieee80211/phy0"), path.join("phy80211")).unwrap();
    }
}
#[test]
fn inventory_filters_virtual_devices_caches_phy_and_reads_identity() {
    let root = root();
    device(&root, "wlan0", true, "sdio");
    device(&root, "wlan1", true, "usb");
    device(&root, "eth0", false, "platform");
    device(&root, "oxpusb0", false, "usb");
    device(&root, "oxpusb1", false, "usb");
    for name in ["lo", "sit0", "ip6tnl0", "tun0", "veth0", "br0"] {
        let path = root.join("sys/class/net").join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("type"), "1").unwrap();
    }
    device(&root, "bridge-with-device", false, "other");
    fs::create_dir(root.join("sys/class/net/bridge-with-device/bridge")).unwrap();
    device(&root, "non-ethernet", false, "other");
    fs::write(root.join("sys/class/net/non-ethernet/type"), "768").unwrap();
    let runner = Fake::default();
    let inv = probe(
        &runner,
        &root,
        &[("eth0".into(), "192.0.2.1".parse().unwrap(), 24)],
    );
    assert_eq!(
        inv.interfaces
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>(),
        ["eth0", "wlan0", "wlan1"]
    );
    let eth = &inv.interfaces[0];
    assert_eq!(eth.ipv4, ["192.0.2.1"]);
    assert!(eth.wifi.is_none());
    assert_eq!(eth.activity.kind, "ethernet");
    let wlan = &inv.interfaces[1];
    assert_eq!(wlan.mac.as_deref(), Some("02:00:00:00:00:01"));
    assert_eq!(wlan.current_mac.as_deref(), Some("02:00:00:00:00:02"));
    assert_eq!(wlan.mac_source, "permanent");
    assert_eq!(wlan.bus, "sdio");
    assert_eq!(wlan.driver.as_deref(), Some("synthetic"));
    assert_eq!(wlan.product.as_deref(), Some("Synthetic Wi-Fi"));
    assert_eq!(wlan.activity.kind, "relay-hotspot");
    assert_eq!(wlan.activity.channel, Some(149));
    assert_eq!(wlan.activity.width.as_deref(), Some("40 MHz"));
    let usb = &inv.interfaces[2];
    assert_eq!(usb.bus, "usb");
    assert_eq!(usb.mac_source, "current");
    assert_eq!(usb.activity.kind, "other-ap");
    assert_eq!(usb.activity.profile.as_deref(), Some("Foreign:profile"));
    assert_eq!(
        wlan.wifi.as_ref().unwrap().concurrency,
        "ap-station-same-channel"
    );
    let count = runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| {
            c.iter()
                .map(String::as_str)
                .eq(["iw", "phy", "phy0", "info"])
        })
        .count();
    assert_eq!(count, 1);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn country_and_station_channels_include_no_ir_and_radar() {
    let country = parse_country(include_str!("../tests/fixtures/iw-reg-cn.txt"));
    assert_eq!(country.global.as_deref(), Some("CN"));
    assert_eq!(country.phys[0].phy, "phy0");
    assert_eq!(country.phys[0].country.as_deref(), Some("99"));
    let country =
        parse_country("global\ncountry 00: DFS-UNSET\nphy#2 (self-managed)\ncountry CN: DFS-FCC\n");
    assert!(country.phys[0].self_managed);
    let cn = Radio::from_phy(parse_phy(include_str!(
        "../tests/fixtures/iw-phy-rtl8822bu-cn.txt"
    )));
    let world = Radio::from_phy(parse_phy(include_str!(
        "../tests/fixtures/iw-phy-rtl8822bu-world.txt"
    )));
    for c in [36, 40, 44, 48] {
        assert!(cn.station5().contains(&c));
        assert!(!cn
            .bands
            .iter()
            .find(|b| b.band == "5")
            .unwrap()
            .ap_channels
            .contains(&c));
    }
    assert_eq!(cn.concurrency, "none");
    assert!(!cn.p2p_client && !cn.p2p_go);
    assert!(!world.bands.is_empty());
    assert_eq!(parse_country("").global, None);
}
#[test]
fn optional_tools_identity_and_activity_parsers() {
    assert_eq!(
        parse_permanent("Permanent address: 00:00:00:00:00:00"),
        None
    );
    assert_eq!(
        parse_permanent("Permanent address: 02:00:00:00:00:AB").as_deref(),
        Some("02:00:00:00:00:ab")
    );
    assert_eq!(
        parse_product("ID_VENDOR_FROM_DATABASE=Vendor\nID_MODEL=Model").as_deref(),
        Some("Vendor Model")
    );
    assert_eq!(parse_product(""), None);
    for (profile, expected) in [
        (Some("oxp-carplay-test"), "relay-hotspot"),
        (Some("oxp-setup-test"), "setup-hotspot"),
        (Some("oxp-client-test"), "saved-network"),
        (Some("oxp-clink-test"), "carlink"),
        (Some("Foreign"), "other-network"),
        (None, "idle"),
    ] {
        assert_eq!(
            parse_activity("type managed", profile, false).kind,
            expected
        );
    }
    struct Absent;
    impl CommandRunner for Absent {
        fn run(&self, _: &[String], _: Duration) -> std::io::Result<CommandResult> {
            Err(std::io::Error::other("absent"))
        }
    }
    let root = root();
    device(&root, "wlan0", true, "usb");
    let inv = probe(&Absent, &root, &[]);
    assert_eq!(inv.interfaces[0].mac_source, "current");
    assert!(inv.interfaces[0].wifi.is_none());
    assert!(inv.interfaces[0].product.is_none());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn cache_is_independent_of_controller_refreshes_names_and_never_blocks_reads() {
    use std::sync::mpsc;
    struct Slow {
        entered: mpsc::Sender<()>,
        gate: Mutex<mpsc::Receiver<()>>,
    }
    impl CommandRunner for Slow {
        fn run(&self, _: &[String], _: Duration) -> std::io::Result<CommandResult> {
            self.entered.send(()).unwrap();
            self.gate.lock().unwrap().recv().unwrap();
            Err(std::io::Error::other("absent"))
        }
    }
    let root = root();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let cache = Arc::new(InventoryCache::new(
        Arc::new(Slow {
            entered,
            gate: Mutex::new(gate),
        }),
        root.clone(),
    ));
    cache.refresh();
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(cache.get().probed_at.is_none()); // Reading while an external command waits uses the last snapshot.
    for _ in 0..2 {
        release.send(()).unwrap();
    }
    let wait = || {
        let deadline = Instant::now() + Duration::from_secs(3);
        while cache.state.lock().unwrap().running {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait();
    while waiting.try_recv().is_ok() {}
    assert!(cache.get().probed_at.is_some());
    cache.refresh();
    assert!(waiting.try_recv().is_err());
    fs::create_dir(root.join("sys/class/net/virtual-added")).unwrap();
    cache.refresh();
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    for _ in 0..2 {
        release.send(()).unwrap();
    }
    wait();
    while waiting.try_recv().is_ok() {}
    cache.state.lock().unwrap().refreshed = Some(Instant::now() - Duration::from_secs(61));
    cache.refresh();
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    for _ in 0..2 {
        release.send(()).unwrap();
    }
    wait();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn activity_refresh_keeps_identity_and_phy_without_capability_probes() {
    let root = root();
    device(&root, "wlan0", true, "sdio");
    let runner = Fake::default();
    let mut inv = probe(&runner, &root, &[]);
    runner.calls.lock().unwrap().clear();
    let stamp = inv.probed_at;
    refresh_activity(
        &runner,
        &root,
        &mut inv,
        &[("wlan0".into(), "192.0.2.5".parse().unwrap(), 24)],
    );
    assert_eq!(inv.interfaces[0].ipv4, ["192.0.2.5"]);
    assert_eq!(inv.interfaces[0].activity.kind, "relay-hotspot");
    assert!(inv.interfaces[0].wifi.is_some());
    assert_eq!(inv.probed_at, stamp);
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(!calls.iter().any(|c| c
        .iter()
        .any(|arg| arg == "phy" || arg == "reg" || arg == "ethtool" || arg == "udevadm")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn country_reprobe_ignores_the_fresh_cache_and_refreshes_capabilities() {
    let root = root();
    device(&root, "wlan0", true, "sdio");
    let runner = Arc::new(Fake::default());
    let cache = InventoryCache::new(runner.clone(), root.clone());
    cache.reprobe();
    assert_eq!(cache.get().country.global.as_deref(), Some("CN"));
    runner.calls.lock().unwrap().clear();
    cache.reprobe();
    let calls = runner.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.as_slice() == ["iw", "reg", "get"])
            .count(),
        1
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.as_slice() == ["iw", "phy", "phy0", "info"])
            .count(),
        1
    );
    assert!(cache.get().interfaces[0]
        .wifi
        .as_ref()
        .unwrap()
        .ap_band("5"));
    fs::remove_dir_all(root).unwrap();
}
