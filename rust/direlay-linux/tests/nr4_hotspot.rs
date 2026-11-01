// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23

use openxplay_relay::{
    host_backends::{AccessPointCredentials, AccessPointRequest, WifiBand},
    linux_hotspot::{remove_stale_prefix, LinuxHotspot},
    platform::{CommandResult, CommandRunner},
    Result,
};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
#[derive(Default)]
struct Runner {
    calls: Mutex<Vec<Vec<String>>>,
    keyfiles: Mutex<Vec<String>>,
    rows: String,
    foreign: bool,
    fail_up: bool,
    fail_load: bool,
    fail_delete: bool,
    active_profile: AtomicBool,
    inactive_profile: bool,
    fail_list: bool,
}
impl CommandRunner for Runner {
    fn run(&self, args: &[String], _: Duration) -> Result<CommandResult> {
        self.calls.lock().unwrap().push(args.to_vec());
        if args.get(2).is_some_and(|s| s == "load") {
            let path = PathBuf::from(&args[3]);
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            self.keyfiles
                .lock()
                .unwrap()
                .push(std::fs::read_to_string(path).unwrap());
        }
        let mut exit_code = 0;
        let output = if args.iter().any(|s| s == "NAME,UUID,ACTIVE") {
            if self.fail_list {
                exit_code = 1;
            }
            let keyfiles = self.keyfiles.lock().unwrap();
            if self.active_profile.load(Ordering::SeqCst) || self.inactive_profile {
                let id = keyfiles
                    .last()
                    .unwrap()
                    .lines()
                    .find_map(|line| line.strip_prefix("id="))
                    .unwrap();
                let active = if self.active_profile.load(Ordering::SeqCst) {
                    "yes"
                } else {
                    "no"
                };
                format!("{id}:synthetic-uuid:{active}\n")
            } else {
                self.rows.clone()
            }
        } else if args[0] == "iw" {
            format!(
                "Interface wlan9\naddr 02:00:00:00:00:09\ntype {}\nchannel 165 (5825 MHz)\n",
                if self.foreign { "AP" } else { "managed" }
            )
        } else if args.iter().any(|s| s == "GENERAL.CONNECTION") {
            "synthetic-foreign-ap".into()
        } else if args.iter().any(|s| s == "IP4.ADDRESS") {
            "10.79.0.1/24\n".into()
        } else if args.iter().any(|s| s == "GENERAL.STATE") {
            "30 (disconnected)".into()
        } else {
            if args.get(2).is_some_and(|s| {
                (self.fail_up && s == "up")
                    || (self.fail_load && s == "load")
                    || (self.fail_delete && (s == "down" || s == "delete"))
            }) {
                exit_code = 10;
            }
            String::new()
        };
        Ok(CommandResult { exit_code, output })
    }
}
fn root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "nr4-keyfile-{}",
        openxplay_relay::admin::auth::random_hex(8).unwrap()
    ))
}
fn admin(root: &std::path::Path, runner: Arc<Runner>) -> LinuxHotspot {
    LinuxHotspot::new(root.into(), runner, "wlan9".into(), 165)
        .with_runtime_dirs(
            root.join("nm-keyfiles"),
            openxplay_relay::public_name::local_dns::Paths::in_dir(root.join("dnsmasq-shared.d")),
        )
        .with_admin(
            "02:00:00:00:00:09".into(),
            AccessPointRequest {
                credentials: AccessPointCredentials::Supplied {
                    ssid: "Synthetic Admin".into(),
                    password: " synthetic;\\password ".into(),
                },
                band: WifiBand::Ghz5,
                channel: 165,
            },
        )
}
#[test]
fn admin_keyfile_is_mac_bound_private_and_fixed_without_a_password_in_argv() {
    let root = root();
    let runner = Arc::new(Runner::default());
    let hotspot = admin(&root, runner.clone());
    hotspot.start(Duration::from_secs(1)).unwrap();
    let keyfiles = runner.keyfiles.lock().unwrap();
    assert_eq!(keyfiles.len(), 1);
    let text = &keyfiles[0];
    for setting in [
        "mac-address=02:00:00:00:00:09\n",
        "autoconnect=false\n",
        "mode=ap\n",
        "band=a\n",
        "channel=165\n",
        "powersave=2\n",
        "key-mgmt=wpa-psk\n",
        "proto=rsn;\n",
        "pairwise=ccmp;\n",
        "group=ccmp;\n",
        "pmf=1\n",
        "[ipv4]\nmethod=shared\naddress1=10.79.0.1/24\n",
        "[ipv6]\nmethod=disabled\n",
        "psk=\\ssynthetic;\\\\password\\s\n",
    ] {
        assert!(text.contains(setting), "missing {setting:?}");
    }
    assert!(!text.lines().any(|line| line.starts_with("interface-name=")));
    assert!(!text.lines().any(|line| line.ends_with('=')));
    let id = text.lines().find_map(|l| l.strip_prefix("id=")).unwrap();
    assert_eq!(id.len(), "oxp-admin-".len() + 16);
    assert!(id
        .strip_prefix("oxp-admin-")
        .unwrap()
        .bytes()
        .all(|b| b.is_ascii_hexdigit()));
    let calls = runner.calls.lock().unwrap();
    assert!(calls.iter().flatten().all(|s| !s.contains("password")));
    assert!(!calls.iter().flatten().any(|s| s == "add"));
    let conf =
        std::fs::read_to_string(root.join("dnsmasq-shared.d/openxplay-public-name.conf")).unwrap();
    assert!(conf.contains("addn-hosts=") && conf.contains("localise-queries\n"));
    assert_eq!(
        std::fs::read_to_string(root.join("dnsmasq-shared.d/hotspot-hosts")).unwrap(),
        "10.79.0.1 openxplay-linux.local\n"
    );
    drop(calls);
    drop(keyfiles);
    hotspot.close().unwrap();
    assert_eq!(
        std::fs::read_dir(root.join("nm-keyfiles")).unwrap().count(),
        0
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn admin_cleanup_deletes_only_its_prefix_and_foreign_ap_is_never_replaced() {
    let runner = Runner { rows: "oxp-admin-0123456789abcdef:admin-active:yes\noxp-admin-fedcba9876543210:admin-old:no\noxp-carplay-0123456789abcdef:carplay:yes\nopenxplay-n1-admin-ap:foreign:yes\noxp-setup-test:setup:yes\nHome:home:yes\n".into(), ..Default::default() };
    assert!(remove_stale_prefix(&runner, "oxp-admin-").unwrap());
    let deleted: Vec<_> = runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|a| a.get(2).is_some_and(|s| s == "delete"))
        .map(|a| a[4].clone())
        .collect();
    assert_eq!(deleted, ["admin-active", "admin-old"]);
    let root = root();
    let runner = Arc::new(Runner {
        foreign: true,
        ..Default::default()
    });
    assert!(admin(&root, runner.clone())
        .start(Duration::from_secs(1))
        .is_err());
    assert!(!runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.get(1).is_some_and(|s| s == "connection")));
    assert!(!root.exists());
}
#[test]
fn admin_does_not_silently_fall_back_to_another_band() {
    let root = root();
    let runner = Arc::new(Runner {
        fail_up: true,
        ..Default::default()
    });
    assert!(admin(&root, runner.clone())
        .start(Duration::from_secs(1))
        .is_err());
    assert!(!runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .any(|s| s == "modify"));
    assert!(
        std::fs::read_to_string(root.join("dnsmasq-shared.d/hotspot-hosts"))
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn carplay_keyfile_binds_by_interface_without_empty_properties() {
    let root = root();
    let runner = Arc::new(Runner::default());
    let hotspot = LinuxHotspot::new(root.clone(), runner.clone(), "wlan9".into(), 165)
        .with_runtime_dirs(
            root.join("nm-keyfiles"),
            openxplay_relay::public_name::local_dns::Paths::in_dir(root.join("dnsmasq-shared.d")),
        );
    hotspot.start(Duration::from_secs(1)).unwrap();
    let keyfiles = runner.keyfiles.lock().unwrap();
    assert_eq!(keyfiles.len(), 1);
    let text = &keyfiles[0];
    assert!(text.contains("interface-name=wlan9\n"));
    assert!(!text.contains("mac-address="));
    assert!(!text.lines().any(|line| line.ends_with('=')));
    hotspot.close().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn close_succeeds_when_nm_already_removed_or_deactivated_the_connection() {
    for inactive in [false, true] {
        let root = root();
        let runner = Arc::new(Runner {
            fail_delete: true,
            inactive_profile: inactive,
            ..Default::default()
        });
        let hotspot = admin(&root, runner);
        hotspot.start(Duration::from_secs(1)).unwrap();
        hotspot.close().unwrap();
        hotspot.close().unwrap();
        assert_eq!(
            std::fs::read_dir(root.join("nm-keyfiles")).unwrap().count(),
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn rejected_profile_load_keeps_start_error_and_can_close() {
    let root = root();
    let runner = Arc::new(Runner {
        fail_load: true,
        fail_delete: true,
        ..Default::default()
    });
    let hotspot = admin(&root, runner);
    let error = hotspot.start(Duration::from_secs(1)).unwrap_err();
    assert_eq!(error.to_string(), "nmcli failed (exit 10)");
    hotspot.close().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn close_retries_a_still_active_connection_and_does_not_hide_query_errors() {
    for fail_list in [false, true] {
        let root = root();
        let runner = Arc::new(Runner {
            fail_delete: true,
            active_profile: AtomicBool::new(true),
            fail_list,
            ..Default::default()
        });
        let hotspot = admin(&root, runner.clone());
        hotspot.start(Duration::from_secs(1)).unwrap();
        assert!(hotspot.close().is_err());
        assert_eq!(
            std::fs::read_dir(root.join("nm-keyfiles")).unwrap().count(),
            1
        );
        if !fail_list {
            runner.active_profile.store(false, Ordering::SeqCst);
            hotspot.close().unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
