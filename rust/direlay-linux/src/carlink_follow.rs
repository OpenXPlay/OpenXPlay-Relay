// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

use super::*;

/// The Carlink link left the head unit's network: it went down, its session ended (oxp-relay then
/// leaves the network and exits) or the child stopped. Errors and PIN timeouts alone leave the link
/// up; the status shows them, and a real departure follows when they end the session.
fn carlink_link_left(event: &serde_json::Value) -> bool {
    (event["type"] == "link" && event["state"] == "lost")
        || event["type"] == "receiver-stopped"
        || (event["type"] == "state"
            && matches!(
                event["state"].as_str(),
                Some("ended" | "disconnected" | "Ended" | "Disconnected")
            ))
}
// This mailbox contains only public link data; credentials stay in the worker's override.
#[derive(Default)]
pub(super) struct CarlinkFollow {
    pub(super) target: Option<String>,
    pub(super) changed: Option<std::time::Instant>,
    pub(super) generation: u64,
}
impl CarlinkFollow {
    pub(super) fn observe(&mut self, event: &serde_json::Value, interface: &str) {
        let target = if event["type"] == "link" && event["state"] == "joined" {
            if event["network"]["interface"] != interface {
                return;
            }
            let Some(ssid) = event["network"]["ssid"].as_str() else {
                return;
            };
            Some(ssid.to_owned())
        } else if carlink_link_left(event) {
            None
        } else {
            return;
        };
        self.target = target;
        self.changed = Some(std::time::Instant::now());
        self.generation = self.generation.wrapping_add(1);
    }
    pub(super) fn apply(
        &mut self,
        event: &serde_json::Value,
        interface: &str,
        shared_radio: bool,
        events: &JsonEvents,
        hotspot: &crate::hotspot_lifetime::HotspotLifetime,
    ) {
        if !shared_radio {
            return;
        }
        if event["type"] == "link"
            && event["state"] == "joined"
            && event["network"]["interface"] == interface
            && self.target.as_deref() != event["network"]["ssid"].as_str()
        {
            events.follow_over("link-joined");
        }
        self.observe(event, interface);
        // Until the debounced follow-over stops the controller, retries must not reclaim the radio.
        if self.target.is_some() {
            hotspot.hold_for_carlink(true);
        }
    }
    pub(super) fn ready(&self) -> bool {
        self.changed
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(750))
    }
}
impl LinuxRelayAdmin {
    pub(super) fn reconcile_carlink_network(&self) -> Result<()> {
        if !self.effective_options()?.shared_carlink_radio() {
            *self.follow.lock().unwrap() = CarlinkFollow::default();
            let following = self.network_override.lock().unwrap().take().is_some();
            self.hotspot_lifetime.hold_for_carlink(false);
            let held = self
                .provision
                .lock()
                .unwrap()
                .as_mut()
                .is_some_and(|provision| provision.hold_for_carlink(false));
            if following || held {
                self.stop_controller();
                self.operation(Operation::Initialize)?;
            }
            return Ok(());
        }
        if self.provision.lock().unwrap().is_none() {
            return Ok(());
        }
        let (target, generation) = {
            let follow = self.follow.lock().unwrap();
            if !follow.ready() {
                return Ok(());
            }
            (follow.target.clone(), follow.generation)
        };
        let current = self
            .network_override
            .lock()
            .unwrap()
            .as_ref()
            .map(|n| n.ssid.clone());
        if current == target {
            let mut follow = self.follow.lock().unwrap();
            if follow.generation == generation {
                follow.changed = None;
                if target.is_none() {
                    self.hotspot_lifetime.hold_for_carlink(false);
                    let held = self
                        .provision
                        .lock()
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .hold_for_carlink(false);
                    drop(follow);
                    if held {
                        self.operation(Operation::Initialize)?;
                    }
                }
            }
            return Ok(());
        }
        // Hold the provisioner even if reading the secret fails; never reconnect home here.
        if target.is_some() {
            self.provision
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .hold_for_carlink(true);
        }
        let network = match target
            .as_ref()
            .map(|ssid| {
                self.provision
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .carlink_network(ssid)
            })
            .transpose()
        {
            Ok(network) => network,
            Err(error) => {
                // Do not let the old controller retry its hotspot while Carlink owns the radio.
                // tick remains blocked until the target credentials have been resolved.
                self.stop_controller();
                let mut follow = self.follow.lock().unwrap();
                if follow.generation == generation {
                    follow.changed = Some(std::time::Instant::now());
                }
                return Err(error);
            }
        };
        let mut follow = self.follow.lock().unwrap();
        if follow.generation != generation {
            return Ok(());
        }
        *self.network_override.lock().unwrap() = network;
        follow.changed = None;
        drop(follow);
        // Publish the resolved target before stopping; tick shares the lifecycle lock.
        self.events.follow_over("controller-stop");
        self.stop_controller();
        if target.is_none() {
            self.events.follow_over("link-left");
            {
                // A newer join keeps the radio held until its own follow-over settles.
                let follow = self.follow.lock().unwrap();
                if follow.target.is_none() {
                    self.hotspot_lifetime.hold_for_carlink(false);
                }
            }
            self.provision
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .hold_for_carlink(false);
            self.operation(Operation::Initialize)?;
        }
        Ok(())
    }
}
