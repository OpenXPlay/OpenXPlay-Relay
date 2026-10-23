// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23

use crate::admin::AdminHotspotStatus as Status;
use crate::{
    host_backends::{AccessPointCredentials, AccessPointRequest, WifiBand},
    hotspot_lifetime::HotspotLifetime,
    network_roles::AdminHotspotPlan,
    platform::CommandRunner,
    Result, WirelessHotspotManager,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(PartialEq)]
struct Inputs {
    interface: String,
    mac: String,
    ssid: String,
    password: String,
    band: String,
    channel: u8,
}
struct State {
    inputs: Option<Inputs>,
    shared: bool,
    joined: bool,
    manager: Option<Arc<dyn WirelessHotspotManager>>,
    status: Status,
}
pub struct AdminHotspot {
    state: Mutex<State>,
    lifetime: HotspotLifetime,
    state_dir: PathBuf,
    runner: Arc<dyn CommandRunner>,
}
impl AdminHotspot {
    pub fn new(state_dir: PathBuf, gap: Duration, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            lifetime: HotspotLifetime::with_down_gap(
                gap,
                Some(state_dir.join("admin-hotspot-down")),
            ),
            state_dir,
            runner,
            state: Mutex::new(State {
                inputs: None,
                shared: false,
                joined: false,
                manager: None,
                status: Status {
                    state: "off".into(),
                    reason: None,
                },
            }),
        }
    }
    pub fn status(&self) -> Status {
        self.state.lock().unwrap().status.clone()
    }
    pub fn prepare(&self) -> Result<()> {
        self.lifetime
            .remove_stale_once(|| {
                crate::linux_hotspot::remove_stale_prefix(self.runner.as_ref(), "oxp-admin-")
            })
            .transpose()?;
        Ok(())
    }
    fn down(&self, state: &mut State) -> Result<()> {
        self.lifetime.close()?;
        state.manager = None;
        Ok(())
    }
    /// Only an actual join holds the radio; Bluetooth scanning and pairing leave the AP up.
    pub fn observe(&self, event: &serde_json::Value, interface: &str) {
        let mut state = self.state.lock().unwrap();
        if event["type"] == "link"
            && event["state"] == "joined"
            && event["network"]["interface"] == interface
        {
            state.joined = true;
        } else if crate::relay_admin::carlink_follow::carlink_link_left(event) {
            state.joined = false;
        } else {
            return;
        }
        if state.shared && state.joined {
            self.lifetime.hold_for_carlink(true);
            state.status = match self.down(&mut state) {
                Ok(()) => Status {
                    state: "paused-carlink".into(),
                    reason: None,
                },
                Err(_) => Status {
                    state: "failed".into(),
                    reason: Some("shutdown-failed".into()),
                },
            };
        } else {
            self.lifetime.hold_for_carlink(false);
        }
    }
    pub fn reconcile(
        &self,
        plan: &AdminHotspotPlan,
        password: Option<&str>,
        inventory: &crate::net_inventory::Inventory,
    ) {
        self.reconcile_with(plan, password, inventory, |inputs| {
            let request = AccessPointRequest {
                credentials: AccessPointCredentials::Supplied {
                    ssid: inputs.ssid.clone(),
                    password: inputs.password.clone(),
                },
                band: if inputs.band == "5" {
                    WifiBand::Ghz5
                } else {
                    WifiBand::Ghz2
                },
                channel: inputs.channel,
            };
            Arc::new(
                crate::linux_hotspot::LinuxHotspot::new(
                    self.state_dir.clone(),
                    self.runner.clone(),
                    inputs.interface.clone(),
                    inputs.channel,
                )
                .with_admin(inputs.mac.clone(), request),
            )
        });
    }
    fn reconcile_with(
        &self,
        plan: &AdminHotspotPlan,
        password: Option<&str>,
        inventory: &crate::net_inventory::Inventory,
        factory: impl FnOnce(&Inputs) -> Arc<dyn WirelessHotspotManager>,
    ) {
        let mut state = self.state.lock().unwrap();
        state.shared = plan.time_shared_with.as_deref() == Some("carlink");
        let interface = inventory
            .interfaces
            .iter()
            .find(|i| Some(i.name.as_str()) == plan.role.name());
        let desired = if !plan.enabled {
            Some(("off", None))
        } else if interface.is_none() || plan.role.missing {
            Some(("waiting-interface", None))
        } else if !password.is_some_and(crate::network_roles::valid_password) {
            Some(("failed", Some("admin-hotspot-password")))
        } else if interface.and_then(|i| i.wifi.as_ref()).is_some_and(|w| {
            !w.capabilities.ap
                || !w.bands.iter().any(|b| {
                    b.band == plan.band && b.ap_channels.contains(&u16::from(plan.channel))
                })
        }) {
            Some(("failed", Some("no-ap-channel")))
        } else if state.shared && state.joined {
            Some(("paused-carlink", None))
        } else if interface.is_some_and(|i| i.activity.kind == "other-ap") {
            Some(("failed", Some("interface-busy")))
        } else {
            None
        };
        self.lifetime.hold_for_carlink(state.shared && state.joined);
        if let Some((target, reason)) = desired {
            state.status = match self.down(&mut state) {
                Ok(()) => Status {
                    state: target.into(),
                    reason: reason.map(str::to_string),
                },
                Err(_) => Status {
                    state: "failed".into(),
                    reason: Some("shutdown-failed".into()),
                },
            };
            state.inputs = None;
            return;
        }
        let interface = interface.unwrap();
        let Some(mac) = interface.mac.clone() else {
            state.status = Status {
                state: "failed".into(),
                reason: Some("mac-unavailable".into()),
            };
            return;
        };
        let inputs = Inputs {
            interface: interface.name.clone(),
            mac,
            ssid: plan.ssid.clone(),
            password: password.unwrap().into(),
            band: plan.band.clone(),
            channel: plan.channel,
        };
        if state.inputs.as_ref() != Some(&inputs)
            || state
                .manager
                .as_ref()
                .is_some_and(|ap| ap.validate_ready().is_err())
        {
            if self.down(&mut state).is_err() {
                state.status = Status {
                    state: "failed".into(),
                    reason: Some("shutdown-failed".into()),
                };
                return;
            }
            state.inputs = None;
        }
        if state.manager.is_some() {
            return;
        }
        if !self.lifetime.down_remaining().is_zero() {
            state.status = Status {
                state: "off".into(),
                reason: Some("down-gap".into()),
            };
            return;
        }
        let result = self
            .lifetime
            .create(|| factory(&inputs))
            .and_then(|manager| {
                manager.start(Duration::from_secs(40))?;
                state.manager = Some(manager);
                Ok(())
            });
        state.status = match result {
            Ok(()) => {
                state.inputs = Some(inputs);
                Status {
                    state: "up".into(),
                    reason: None,
                }
            }
            Err(_) => {
                let _ = self.down(&mut state);
                Status {
                    state: "failed".into(),
                    reason: Some("activation-failed".into()),
                }
            }
        };
    }
    pub fn close(&self) -> Result<()> {
        self.down(&mut self.state.lock().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hotspot_lifetime::tests::Fake,
        net_inventory::{Interface, Inventory},
        platform::CommandResult,
    };
    use std::sync::atomic::Ordering;
    struct Runner;
    impl CommandRunner for Runner {
        fn run(&self, _: &[String], _: Duration) -> Result<CommandResult> {
            unreachable!()
        }
    }
    fn fixture(gap: Duration) -> (AdminHotspot, AdminHotspotPlan, Inventory, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "nr4-lifetime-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let inventory = Inventory {
            interfaces: vec![Interface {
                name: "wlan9".into(),
                mac: Some("02:00:00:00:00:09".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let plan = serde_json::from_value(serde_json::json!({"enabled":true,"interface":{"ref":"02:00:00:00:00:09","mac":"02:00:00:00:00:09","name":"wlan9"},"missing":false,"source":"custom","ssid":"Synthetic Admin","band":"5","channel":165,"passwordSet":true,"timeSharedWith":"carlink"})).unwrap();
        (
            AdminHotspot::new(dir.clone(), gap, Arc::new(Runner)),
            plan,
            inventory,
            dir,
        )
    }
    #[test]
    fn scan_pair_join_leave_use_only_the_admin_gap_and_record() {
        let (admin, plan, inventory, dir) = fixture(Duration::from_millis(80));
        let fake = Arc::new(Fake::default());
        let carplay =
            HotspotLifetime::with_down_gap(Duration::from_secs(99), Some(dir.join("hotspot-down")));
        let carplay_fake = Arc::new(Fake::default());
        let cp = carplay.create(|| carplay_fake.clone()).unwrap();
        cp.start(Duration::from_secs(1)).unwrap();
        let reconcile = || {
            admin.reconcile_with(&plan, Some("synthetic-password"), &inventory, |_| {
                fake.clone()
            })
        };
        reconcile();
        assert_eq!(admin.status().state, "up");
        for event in [
            serde_json::json!({"type":"state","state":"scanning"}),
            serde_json::json!({"type":"state","state":"pairing"}),
            serde_json::json!({"type":"link","state":"joined","network":{"interface":"wlan8"}}),
        ] {
            admin.observe(&event, "wlan9");
            reconcile();
            assert_eq!(admin.status().state, "up");
        }
        admin.observe(
            &serde_json::json!({"type":"link","state":"joined","network":{"interface":"wlan9"}}),
            "wlan9",
        );
        assert_eq!(admin.status().state, "paused-carlink");
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        assert!(dir.join("admin-hotspot-down").exists());
        assert!(!dir.join("hotspot-down").exists());
        assert!(carplay.down_remaining().is_zero());
        assert_eq!(carplay_fake.closes.load(Ordering::SeqCst), 0);
        reconcile();
        assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
        admin.observe(&serde_json::json!({"type":"link","state":"lost"}), "wlan9");
        reconcile();
        assert_eq!(admin.status().reason.as_deref(), Some("down-gap"));
        std::thread::sleep(Duration::from_millis(90));
        reconcile();
        assert_eq!(admin.status().state, "up");
        assert_eq!(fake.starts.load(Ordering::SeqCst), 2);
        carplay.close().unwrap();
        assert!(!carplay.down_remaining().is_zero());
        reconcile();
        assert_eq!(
            admin.status().state,
            "up",
            "CarPlay's gap does not stop admin"
        );
        admin.close().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn missing_busy_disabled_and_admin_changes_do_not_reclaim_a_foreign_ap() {
        let (admin, mut plan, mut inventory, dir) = fixture(Duration::ZERO);
        let fake = Arc::new(Fake::default());
        admin.reconcile_with(&plan, None, &inventory, |_| panic!("no password"));
        assert_eq!(
            admin.status().reason.as_deref(),
            Some("admin-hotspot-password")
        );
        inventory.interfaces[0].activity.kind = "other-ap".into();
        admin.reconcile_with(&plan, Some("synthetic-password"), &inventory, |_| {
            panic!("foreign AP")
        });
        assert_eq!(admin.status().reason.as_deref(), Some("interface-busy"));
        inventory.interfaces[0].activity.kind = "idle".into();
        admin.reconcile_with(&plan, Some("synthetic-password"), &inventory, |_| {
            fake.clone()
        });
        plan.ssid = "Changed Admin".into();
        admin.reconcile_with(&plan, Some("synthetic-password"), &inventory, |_| {
            fake.clone()
        });
        assert_eq!(fake.starts.load(Ordering::SeqCst), 2);
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        inventory.interfaces.clear();
        admin.reconcile_with(
            &plan,
            Some("synthetic-password"),
            &inventory,
            |_| unreachable!(),
        );
        assert_eq!(admin.status().state, "waiting-interface");
        plan.enabled = false;
        admin.reconcile_with(
            &plan,
            Some("synthetic-password"),
            &inventory,
            |_| unreachable!(),
        );
        assert_eq!(admin.status().state, "off");
        let _ = std::fs::remove_dir_all(dir);
    }
}
