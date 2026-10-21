// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// NetworkManager access point lifecycle tests.
use openxplay_relay::*;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<Vec<String>>>,
    attempts: Mutex<usize>,
    fail_5g: bool,
    fail_all: bool,
    iw_missing: bool,
    /// What NetworkManager reports for the Wi-Fi device's state, one answer per query; `None`
    /// when it does not list the device yet.
    device_states: Mutex<Vec<Option<&'static str>>>,
}
impl CommandRunner for Fake {
    fn run(&self, a: &[String], timeout: Duration) -> openxplay_relay::Result<CommandResult> {
        assert!(!timeout.is_zero());
        self.calls.lock().unwrap().push(a.to_vec());
        let output = if a.starts_with(&["nmcli".into(), "connection".into(), "up".into()]) {
            let mut n = self.attempts.lock().unwrap();
            *n += 1;
            if self.fail_all || (self.fail_5g && *n == 1) {
                return Ok(CommandResult {
                    exit_code: 10,
                    output: "activation failed".into(),
                });
            }
            "Connection successfully activated".into()
        } else if a[0] == "iw" {
            if self.iw_missing {
                return Ok(CommandResult {
                    exit_code: 127,
                    output: "missing".into(),
                });
            }
            include_str!("fixtures/iw-info.txt").replace(
                "36 (5180",
                if self.fail_5g { "6 (2437" } else { "36 (5180" },
            )
        } else if a.iter().any(|s| s == "GENERAL.STATE")
            && !self.device_states.lock().unwrap().is_empty()
        {
            match self.device_states.lock().unwrap().remove(0) {
                Some(state) => state.into(),
                None => {
                    return Ok(CommandResult {
                        exit_code: 10,
                        output: "Error: Device 'wlan0' not found.".into(),
                    })
                }
            }
        } else if a.iter().any(|s| s == "IP4.ADDRESS") {
            include_str!("fixtures/nmcli-ipv4.txt").into()
        } else if a.iter().any(|s| s == "IN-USE,CHAN,BSSID") {
            include_str!("fixtures/nmcli-wifi.txt").into()
        } else {
            "success".into()
        };
        Ok(CommandResult {
            exit_code: 0,
            output,
        })
    }
}
fn temp() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "wp4a-ap-{}",
        openxplay_relay::admin::auth::random_hex(10).unwrap()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn manager(p: &std::path::Path, runner: Arc<Fake>) -> LinuxHotspot {
    LinuxHotspot::new(p.into(), runner, "wlan0".into(), 36)
}
#[test]
fn hotspot_persists_credentials_and_deletes_owned_connection() {
    let p = temp();
    let runner = Arc::new(Fake::default());
    let h = manager(&p, runner.clone());
    let info = h.start(Duration::from_secs(60)).unwrap();
    assert_eq!(info.channel, 36);
    assert_eq!(info.host_address.unwrap().to_string(), "10.42.0.1");
    assert_eq!(
        info.access_point_bssid.as_deref(),
        Some("dc:a6:32:11:22:33")
    );
    h.validate_ready().unwrap();
    h.close().unwrap();
    h.close().unwrap();
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[calls.len() - 2][2], "down");
    assert_eq!(calls[calls.len() - 1][2], "delete");
    let add = calls
        .iter()
        .find(|a| a.get(2).is_some_and(|s| s == "add"))
        .unwrap();
    for v in ["ap", "rsn", "ccmp", "shared", "no"] {
        assert!(add.iter().any(|s| s == v));
    }
    assert!(calls.iter().all(|a| a[0] != "ip"));
    drop(calls);
    let second = manager(&p, Arc::new(Fake::default()));
    assert_eq!(
        second.start(Duration::from_secs(60)).unwrap().passphrase,
        info.passphrase
    );
    second.close().unwrap();
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn hotspot_falls_back_to_24ghz_channel_6() {
    let p = temp();
    let runner = Arc::new(Fake {
        fail_5g: true,
        ..Default::default()
    });
    let h = manager(&p, runner.clone());
    assert_eq!(h.start(Duration::from_secs(60)).unwrap().channel, 6);
    assert!(runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.iter().any(|s| s == "modify")
            && a.iter().any(|s| s == "bg")
            && a.iter().any(|s| s == "6")));
    h.close().unwrap();
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn hotspot_uses_nmcli_radio_when_iw_missing() {
    let p = temp();
    let h = manager(
        &p,
        Arc::new(Fake {
            iw_missing: true,
            ..Default::default()
        }),
    );
    assert_eq!(
        h.start(Duration::from_secs(60)).unwrap().bssid.as_deref(),
        Some("DC:A6:32:11:22:33")
    );
    h.close().unwrap();
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn hotspot_failure_cleans_up_and_close_prevents_restart() {
    let p = temp();
    let runner = Arc::new(Fake {
        fail_all: true,
        ..Default::default()
    });
    let h = manager(&p, runner.clone());
    assert!(h.start(Duration::from_secs(60)).is_err());
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls[calls.len() - 2][2], "down");
    assert_eq!(calls[calls.len() - 1][2], "delete");
    drop(calls);
    h.close().unwrap();
    assert!(h.start(Duration::from_secs(60)).is_err());
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn channel_selection_respects_radio_capabilities() {
    use openxplay_relay::linux_hotspot::hotspot_channel;
    assert_eq!(hotspot_channel(36, false), 6);
    assert_eq!(hotspot_channel(36, true), 36);
    assert_eq!(hotspot_channel(11, false), 11);
    assert_eq!(hotspot_channel(14, true), 14);
}

#[test]
fn session_restart_reuses_ap_without_networkmanager_activation() {
    use openxplay_relay::hotspot_lifetime::HotspotLifetime;
    let p = temp();
    let runner = Arc::new(Fake::default());
    let lifetime = HotspotLifetime::default();
    let first = lifetime
        .create(|| Arc::new(manager(&p, runner.clone())))
        .unwrap();
    let info = first.start(Duration::from_secs(1)).unwrap();
    lifetime.keep_for_restart();
    first.close().unwrap();
    let before = runner.calls.lock().unwrap().len();
    let second = lifetime.create(|| panic!("AP must be reused")).unwrap();
    let reused = second.start(Duration::from_secs(1)).unwrap();
    assert!(lifetime.reused());
    assert_eq!(info.ssid, reused.ssid);
    assert_eq!(info.channel, reused.channel);
    assert_eq!(info.host_address, reused.host_address);
    let calls = runner.calls.lock().unwrap();
    // Reuse only checks the address; it never changes the AP connection.
    assert_eq!(calls.len(), before + 1);
    assert!(calls[before].iter().any(|a| a == "IP4.ADDRESS"));
    assert_eq!(*runner.attempts.lock().unwrap(), 1);
    assert!(!calls
        .iter()
        .any(|a| a.iter().any(|s| s == "down" || s == "delete")));
    drop(calls);
    second.close().unwrap();
    lifetime.close().unwrap();
    assert!(runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.iter().any(|s| s == "down")));
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn shared_wifi_restart_recreates_attachment_without_disconnect() {
    use openxplay_relay::{hotspot_lifetime::HotspotLifetime, settings::SharedWifiNetwork};
    struct SharedWifi(Mutex<Vec<Vec<String>>>);
    impl CommandRunner for SharedWifi {
        fn run(&self, args: &[String], _: Duration) -> openxplay_relay::Result<CommandResult> {
            self.0.lock().unwrap().push(args.to_vec());
            Ok(CommandResult {
                exit_code: 0,
                output: if args.iter().any(|a| a == "IN-USE,SSID") {
                    "*:Home\n".into()
                } else {
                    "*:Home:6:AA\\:BB\\:CC\\:DD\\:EE\\:FF:--\n".into()
                },
            })
        }
    }
    let p = temp();
    let state = PersistentState::open(&p).unwrap();
    let mut options = Options::parse(&["--state-dir".into(), p.to_str().unwrap().into()]).unwrap();
    options.settings.network_mode = "shared-wifi".into();
    options.settings.shared_wifi_networks = vec![SharedWifiNetwork {
        ssid: "Home".into(),
        password: String::new(),
    }];
    options.wifi_interface = "test-no-such-wifi".into();
    let runner = Arc::new(SharedWifi(Mutex::new(Vec::new())));
    let lifetime = Arc::new(HotspotLifetime::default());
    let events = Arc::new(JsonEvents::new(Arc::new(|_| {})));
    let input = Arc::new(DiPlayInput::new(events.clone()));
    let platform = || {
        LinuxWirelessPlatform::new(
            options.clone(),
            openxplay_relay::host_backends::HostBackends::linux(
                &options,
                runner.clone(),
                events.clone(),
            ),
            state.identity().unwrap(),
            state.pairings().unwrap(),
            Arc::new(RecordingMediaSink::discard(events.clone())),
            events.clone(),
            input.clone(),
        )
        .with_hotspot_lifetime(lifetime.clone())
    };
    let first = platform().create_hotspot().unwrap();
    lifetime.keep_for_restart();
    first.close().unwrap();
    assert!(first
        .validate_ready()
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    let next_platform = platform();
    let second = next_platform.create_hotspot().unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(!next_platform.hotspot_reused());
    // Snapshot still runs on the new attachment; no physical interface is needed by this test.
    assert!(!second
        .validate_ready()
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    second.close().unwrap();
    lifetime.close().unwrap();
    assert!(runner
        .0
        .lock()
        .unwrap()
        .iter()
        .all(|a| a[0] == "nmcli" && a.iter().any(|s| s == "list")));
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn station_dump_lists_associated_stations() {
    use openxplay_relay::linux_hotspot::{associated_stations, WifiOutput};
    struct Dump(Mutex<Vec<Vec<String>>>, i32, &'static str);
    impl CommandRunner for Dump {
        fn run(&self, a: &[String], _: Duration) -> openxplay_relay::Result<CommandResult> {
            self.0.lock().unwrap().push(a.to_vec());
            Ok(CommandResult {
                exit_code: self.1,
                output: self.2.into(),
            })
        }
    }
    let dump = "Station 02:1A:2B:3C:4D:5E (on wlan0)\n\tinactive time:\t304 ms\n\trx bytes:\t18816\n\tsignal:  \t-52 dBm\n\tauthorized:\tyes\n\tassociated:\tyes\nStation 02:00:00:00:00:01 (on wlan0)\n\tinactive time:\t12 ms\n";
    let runner = Dump(Mutex::default(), 0, dump);
    assert_eq!(
        associated_stations(&runner, "wlan0").unwrap(),
        ["02:1a:2b:3c:4d:5e", "02:00:00:00:00:01"]
    );
    assert_eq!(
        *runner.0.lock().unwrap(),
        [["iw", "dev", "wlan0", "station", "dump"]]
    );
    // An AP without stations prints nothing; a failed query is not an empty AP.
    assert!(WifiOutput::stations("").is_empty());
    assert!(WifiOutput::stations("Station not-a-mac (on wlan0)\n").is_empty());
    assert!(associated_stations(&Dump(Mutex::default(), 237, "command failed"), "wlan0").is_err());
}
#[test]
fn startup_deletes_the_aps_a_killed_run_left_behind() {
    use openxplay_relay::linux_hotspot::remove_stale_hotspots;
    /// `nmcli connection show` lists `rows`; deleting `fail` fails.
    struct Profiles {
        calls: Mutex<Vec<Vec<String>>>,
        rows: Option<&'static str>,
        fail: &'static str,
    }
    impl CommandRunner for Profiles {
        fn run(&self, a: &[String], _: Duration) -> openxplay_relay::Result<CommandResult> {
            self.calls.lock().unwrap().push(a.to_vec());
            let (exit_code, output) = if a.iter().any(|s| s == "show") {
                self.rows.map_or((8, ""), |rows| (0, rows))
            } else if a.last().is_some_and(|uuid| uuid == self.fail) {
                (10, "Error: cannot delete")
            } else {
                (0, "deleted")
            };
            Ok(CommandResult {
                exit_code,
                output: output.into(),
            })
        }
    }
    let profiles = |rows, fail| Profiles {
        calls: Mutex::default(),
        rows,
        fail,
    };
    let rows = "Home\\: 5G:home-uuid:yes\noxp-carplay-0123456789abcdef:ap-old:no\noxp-setup-ab12:setup-uuid:no\noxp-carplay-fedcba9876543210:ap-live:yes\n";
    let runner = profiles(Some(rows), "");
    // The AP that was still up went down only now: the caller starts the gap.
    assert!(remove_stale_hotspots(&runner).unwrap());
    assert_eq!(
        *runner.calls.lock().unwrap(),
        [
            &[
                "nmcli",
                "-t",
                "-f",
                "NAME,UUID,ACTIVE",
                "connection",
                "show"
            ][..],
            &["nmcli", "connection", "delete", "uuid", "ap-old"],
            &["nmcli", "connection", "delete", "uuid", "ap-live"],
        ]
    );
    // Only inactive leftovers, or none: nothing went down.
    let inactive = "oxp-carplay-0123456789abcdef:ap-old:no\n";
    assert!(!remove_stale_hotspots(&profiles(Some(inactive), "")).unwrap());
    assert!(!remove_stale_hotspots(&profiles(Some("Home:home-uuid:yes\n"), "")).unwrap());
    // An inactive leftover that cannot be deleted stays for the next start ...
    assert!(remove_stale_hotspots(&profiles(Some(rows), "ap-old")).unwrap());
    // ... but an AP that stays up, or a failed listing, is an error.
    assert!(remove_stale_hotspots(&profiles(Some(rows), "ap-live")).is_err());
    assert!(remove_stale_hotspots(&profiles(None, "")).is_err());
}

#[test]
fn access_point_starts_once_networkmanager_can_use_the_wifi_device() {
    let p = temp();
    // At boot NetworkManager first lists the device, then sets it up.
    let runner = Arc::new(Fake {
        device_states: Mutex::new(vec![
            None,
            Some("10 (unmanaged)\n"),
            Some("20 (unavailable)\n"),
            Some("30 (disconnected)\n"),
        ]),
        ..Default::default()
    });
    let h = manager(&p, runner.clone());
    h.start(Duration::from_secs(60)).unwrap();
    let calls = runner.calls.lock().unwrap();
    let position = |test: &dyn Fn(&Vec<String>) -> bool| calls.iter().position(test).unwrap();
    let add = position(&|a| a.get(2).is_some_and(|s| s == "add"));
    let up = position(&|a| a.get(2).is_some_and(|s| s == "up"));
    let states: Vec<_> = calls
        .iter()
        .enumerate()
        .filter(|(_, a)| a.iter().any(|s| s == "GENERAL.STATE"))
        .map(|(i, _)| i)
        .collect();
    // The profile is added meanwhile; the activation waits for the usable device.
    assert_eq!(states.len(), 4);
    assert!(add < states[0] && *states.last().unwrap() < up);
    drop(calls);
    assert_eq!(linux_hotspot::device_state("30 (disconnected)"), Some(30));
    assert_eq!(linux_hotspot::device_state("success"), None);
    assert!(linux_hotspot::device_available(100) && !linux_hotspot::device_available(20));
    h.close().unwrap();
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn activation_waits_for_the_wifi_provisioning_but_the_profile_does_not() {
    let p = temp();
    let runner = Arc::new(Fake::default());
    let gate = Arc::new(openxplay_relay::provision::ProvisioningGate::default());
    // The startup provisioning takes the saved client connections on wlan0 down meanwhile.
    let provisioning = gate.hold();
    let h = Arc::new(manager(&p, runner.clone()).with_provisioning(gate.clone()));
    let start = {
        let h = h.clone();
        std::thread::spawn(move || h.start(Duration::from_secs(60)).map(|i| i.ssid))
    };
    let called = |verb: &str| {
        runner
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|a| a.get(2).is_some_and(|s| s == verb))
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !called("add") {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(150));
    assert!(!called("up"));
    assert!(!start.is_finished());
    drop(provisioning);
    start.join().unwrap().unwrap();
    assert!(called("up"));
    h.close().unwrap();
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn the_provisioning_gate_opens_with_its_last_hold() {
    let gate = Arc::new(openxplay_relay::provision::ProvisioningGate::default());
    assert!(gate.wait(Duration::ZERO));
    let (first, second) = (gate.hold(), gate.hold());
    drop(first);
    assert!(!gate.wait(Duration::from_millis(10)));
    let waiter = {
        let gate = gate.clone();
        std::thread::spawn(move || gate.wait(Duration::from_secs(5)))
    };
    std::thread::sleep(Duration::from_millis(20));
    drop(second);
    assert!(waiter.join().unwrap());
}

#[test]
fn ap_channel_rejects_no_ir_radar_and_missing_while_station_can_join() {
    use openxplay_relay::{linux_hotspot::access_point_channel, wifi_capabilities::parse_phy};
    for flags in ["(disabled)", "(no IR)", "(radar detection)"] {
        let phy = parse_phy(&format!(" * 5180 MHz [36] (20.0 dBm) {flags}"));
        assert_eq!(access_point_channel(36, Some(&phy)), 6);
        assert_eq!(phy.station_5ghz(), flags != "(disabled)");
    }
    let country_set = parse_phy(" * 5180 MHz [36] (20.0 dBm)");
    assert_eq!(access_point_channel(36, Some(&country_set)), 36);
    assert_eq!(access_point_channel(44, Some(&country_set)), 6);
    let ufi = parse_phy(" * 2437 MHz [6] (20.0 dBm)");
    assert_eq!(access_point_channel(36, Some(&ufi)), 6);
    assert_eq!(access_point_channel(11, Some(&ufi)), 11);
    assert_eq!(access_point_channel(36, None), 6);
}

#[test]
fn semantic_access_point_request_supplies_credentials_band_and_channel() {
    use openxplay_relay::host_backends::{
        AccessPointBackend, AccessPointCredentials, AccessPointRequest, WifiBand,
    };
    let root = temp();
    let runner = Arc::new(Fake::default());
    let hotspot = manager(&root, runner.clone());
    let backend: &dyn AccessPointBackend = &hotspot;
    let request = AccessPointRequest {
        credentials: AccessPointCredentials::Supplied {
            ssid: "Semantic AP".into(),
            password: "password123".into(),
        },
        band: WifiBand::Ghz2,
        channel: 11,
    };
    backend.start(&request, Duration::from_secs(60)).unwrap();
    backend.validate().unwrap();
    assert_eq!(backend.radio().unwrap().channel, Some(36));
    backend.stations().unwrap();
    backend.stop().unwrap();
    let calls = runner.calls.lock().unwrap();
    let add = calls
        .iter()
        .find(|args| args.get(2).is_some_and(|s| s == "add"))
        .unwrap();
    for pair in [
        ["ssid", "Semantic AP"],
        ["802-11-wireless-security.psk", "password123"],
        ["802-11-wireless.band", "bg"],
        ["802-11-wireless.channel", "11"],
    ] {
        assert!(add.windows(2).any(|w| w == pair));
    }
    assert!(!root.join("hotspot.credentials").exists());
    std::fs::remove_dir_all(root).unwrap();
}
