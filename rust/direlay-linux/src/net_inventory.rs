// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

use crate::{
    platform::{command, spawn_worker, CommandRunner},
    wifi_capabilities::{parse_phy, PhyCapabilities, WifiSummary},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Country {
    pub global: Option<String>,
    pub configured: Option<String>,
    pub phys: Vec<PhyCountry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhyCountry {
    pub phy: String,
    pub country: Option<String>,
    pub self_managed: bool,
}
pub fn parse_country(text: &str) -> Country {
    let mut result = Country::default();
    let mut scope = None;
    for line in text.lines().map(str::trim) {
        if line == "global" {
            scope = None;
        } else if let Some(phy) = line.strip_prefix("phy#") {
            result.phys.push(PhyCountry {
                phy: format!("phy{}", phy.split_whitespace().next().unwrap_or("")),
                country: None,
                self_managed: line.contains("self-managed"),
            });
            scope = Some(result.phys.len() - 1);
        } else if let Some(country) = line
            .strip_prefix("country ")
            .and_then(|s| s.split_once(':'))
        {
            if let Some(index) = scope {
                result.phys[index].country = Some(country.0.into());
            } else {
                result.global = Some(country.0.into());
            }
        }
    }
    result
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    pub band: String,
    pub station_channels: Vec<u16>,
    pub ap_channels: Vec<u16>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Radio {
    #[serde(flatten)]
    pub capabilities: PhyCapabilities,
    pub bands: Vec<Band>,
    pub p2p_client: bool,
    pub p2p_go: bool,
    pub concurrency: String,
}
impl Radio {
    pub fn from_phy(capabilities: PhyCapabilities) -> Self {
        let summary = WifiSummary::from_phy(Some(&capabilities));
        let concurrency = match (summary.ap_station, summary.ap_station_channels) {
            (Some(false), _) => "none",
            (Some(true), Some(1)) => "ap-station-same-channel",
            (Some(true), Some(n)) if n > 1 => "ap-station-multi-channel",
            _ => "unknown",
        };
        let bands = ["2.4", "5", "6"]
            .into_iter()
            .filter_map(|band| {
                let channels: Vec<_> = capabilities
                    .channels
                    .iter()
                    .filter(|c| match band {
                        "2.4" => c.two_ghz(),
                        "5" => c.five_ghz(),
                        _ => (5925..=7125).contains(&c.frequency),
                    })
                    .collect();
                (!channels.is_empty()).then(|| Band {
                    band: band.into(),
                    station_channels: channels
                        .iter()
                        .filter(|c| !c.disabled)
                        .map(|c| c.channel)
                        .collect(),
                    ap_channels: channels
                        .iter()
                        .filter(|c| c.access_point())
                        .map(|c| c.channel)
                        .collect(),
                })
            })
            .collect();
        Self {
            p2p_client: capabilities.modes.iter().any(|m| m == "P2P-client"),
            p2p_go: capabilities.modes.iter().any(|m| m == "P2P-GO"),
            capabilities,
            bands,
            concurrency: concurrency.into(),
        }
    }
    pub fn station5(&self) -> Vec<u16> {
        if !self.capabilities.station {
            return vec![];
        }
        self.bands
            .iter()
            .find(|b| b.band == "5")
            .map(|b| b.station_channels.clone())
            .unwrap_or_default()
    }
    pub fn ap_band(&self, band: &str) -> bool {
        self.bands
            .iter()
            .any(|b| b.band == band && !b.ap_channels.is_empty())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub kind: String,
    pub profile: Option<String>,
    pub ssid: Option<String>,
    pub mode: Option<String>,
    pub channel: Option<u16>,
    pub width: Option<String>,
}
pub fn parse_activity(text: &str, profile: Option<&str>, ethernet: bool) -> Activity {
    let mut activity = Activity {
        profile: profile.map(str::to_string),
        ..Default::default()
    };
    for line in text.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("type ") {
            activity.mode = Some(value.into());
        }
        if let Some(value) = line.strip_prefix("ssid ") {
            activity.ssid = Some(value.into());
        }
        if let Some(value) = line.strip_prefix("channel ") {
            activity.channel = value.split_whitespace().next().and_then(|n| n.parse().ok());
            activity.width = value
                .split_once("width: ")
                .map(|(_, tail)| tail.split(',').next().unwrap_or(tail).trim().into());
        }
    }
    activity.kind = if ethernet {
        "ethernet"
    } else if profile.is_some_and(|p| p.starts_with("oxp-carplay-")) {
        "relay-hotspot"
    } else if profile.is_some_and(|p| p.starts_with("oxp-admin-")) {
        "admin-hotspot"
    } else if profile.is_some_and(|p| p.starts_with("oxp-setup-")) {
        "setup-hotspot"
    } else if profile.is_some_and(|p| p.starts_with("oxp-client-")) {
        "saved-network"
    } else if profile.is_some_and(|p| p.starts_with("oxp-clink-")) {
        "carlink"
    } else if activity.mode.as_deref() == Some("AP") {
        "other-ap"
    } else if profile.is_some() || activity.ssid.is_some() {
        "other-network"
    } else {
        "idle"
    }
    .into();
    activity
}
/// nmcli's terse fields escape both colons and backslashes.
pub fn parse_active(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut fields = vec![String::new()];
            let mut escaped = false;
            for c in line.chars() {
                if escaped {
                    fields.last_mut()?.push(c);
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == ':' {
                    fields.push(String::new());
                } else {
                    fields.last_mut()?.push(c);
                }
            }
            (fields.len() == 3 && !fields[0].is_empty())
                .then(|| (fields[0].clone(), fields[1].clone()))
        })
        .collect()
}
pub fn normalized_mac(mac: &str) -> Option<String> {
    let mac = mac.trim();
    crate::linux_bluetooth::valid_mac(mac).then(|| mac.to_ascii_lowercase())
}
pub fn parse_permanent(text: &str) -> Option<String> {
    let mac = normalized_mac(text.trim().strip_prefix("Permanent address: ")?)?;
    (mac != "00:00:00:00:00:00").then_some(mac)
}
pub fn parse_product(text: &str) -> Option<String> {
    let properties: BTreeMap<_, _> = text.lines().filter_map(|l| l.split_once('=')).collect();
    properties
        .get("ID_MODEL_FROM_DATABASE")
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| {
            let value = [
                properties.get("ID_VENDOR_FROM_DATABASE"),
                properties.get("ID_MODEL"),
            ]
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
            (!value.is_empty()).then_some(value)
        })
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub name: String,
    pub mac: Option<String>,
    pub current_mac: Option<String>,
    pub mac_source: String,
    pub driver: Option<String>,
    pub bus: String,
    pub product: Option<String>,
    pub kind: String,
    pub phy: Option<String>,
    pub operstate: Option<String>,
    pub ipv4: Vec<String>,
    pub wifi: Option<Radio>,
    pub activity: Activity,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub probed_at: Option<u64>,
    pub country: Country,
    pub interfaces: Vec<Interface>,
}
pub fn interface_names(root: &Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(root.join("sys/class/net"))
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}
fn link_name(path: &Path) -> Option<String> {
    std::fs::read_link(path)
        .ok()?
        .file_name()?
        .to_str()
        .map(str::to_string)
}
fn bus(path: &Path, root: &Path) -> String {
    let mut subsystems = Vec::new();
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.into());
    if let Ok(device) = std::fs::canonicalize(path.join("device")) {
        for parent in device.ancestors().take_while(|p| p.starts_with(&root)) {
            if let Some(subsystem) = link_name(&parent.join("subsystem")) {
                subsystems.push(subsystem);
            }
        }
    }
    for known in ["usb", "sdio", "pci", "platform"] {
        if subsystems.iter().any(|s| s == known) {
            return known.into();
        }
    }
    subsystems
        .into_iter()
        .next()
        .unwrap_or_else(|| "other".into())
}
fn run(runner: &dyn CommandRunner, args: &[&str]) -> Option<String> {
    command(runner, args, Duration::from_secs(3)).ok()
}
/// All filesystem and command inputs can be replaced by fake sysfs trees and runners.
pub fn probe(
    runner: &dyn CommandRunner,
    root: &Path,
    addresses: &[(String, std::net::Ipv4Addr, u8)],
) -> Inventory {
    let active = parse_active(
        &run(
            runner,
            &[
                "nmcli",
                "-t",
                "-f",
                "DEVICE,NAME,UUID",
                "connection",
                "show",
                "--active",
            ],
        )
        .unwrap_or_default(),
    );
    let country = parse_country(&run(runner, &["iw", "reg", "get"]).unwrap_or_default());
    let mut phys: BTreeMap<String, Option<Radio>> = BTreeMap::new();
    let interfaces = interface_names(root)
        .into_iter()
        .filter_map(|name| {
            let path = root.join("sys/class/net").join(&name);
            if !std::fs::symlink_metadata(path.join("device"))
                .is_ok_and(|m| m.file_type().is_symlink())
                || path.join("bridge").exists()
            {
                return None;
            }
            let wifi = path.join("phy80211").exists() || path.join("wireless").exists();
            if !wifi && std::fs::read_to_string(path.join("type")).ok()?.trim() != "1" {
                return None;
            }
            let info = if wifi {
                run(runner, &["iw", "dev", &name, "info"]).unwrap_or_default()
            } else {
                String::new()
            };
            let phy = wifi
                .then(|| {
                    link_name(&path.join("phy80211")).or_else(|| {
                        info.lines().find_map(|line| {
                            line.trim()
                                .strip_prefix("wiphy ")?
                                .parse::<u16>()
                                .ok()
                                .map(|n| format!("phy{n}"))
                        })
                    })
                })
                .flatten();
            let radio = phy.as_ref().and_then(|phy| {
                phys.entry(phy.clone())
                    .or_insert_with(|| {
                        run(runner, &["iw", "phy", phy, "info"])
                            .map(|text| parse_phy(&text))
                            .filter(|p| !p.modes.is_empty() || !p.channels.is_empty())
                            .map(Radio::from_phy)
                    })
                    .clone()
            });
            let current_mac = std::fs::read_to_string(path.join("address"))
                .ok()
                .and_then(|s| normalized_mac(&s));
            let permanent =
                run(runner, &["ethtool", "-P", &name]).and_then(|s| parse_permanent(&s));
            Some(Interface {
                mac_source: if permanent.is_some() {
                    "permanent"
                } else {
                    "current"
                }
                .into(),
                mac: permanent.or_else(|| current_mac.clone()),
                current_mac,
                driver: link_name(&path.join("device/driver")),
                bus: bus(&path, root),
                product: run(
                    runner,
                    &[
                        "udevadm",
                        "info",
                        "-q",
                        "property",
                        "-p",
                        &format!("/sys/class/net/{name}"),
                    ],
                )
                .and_then(|s| parse_product(&s)),
                kind: if wifi { "wifi" } else { "ethernet" }.into(),
                phy,
                operstate: std::fs::read_to_string(path.join("operstate"))
                    .ok()
                    .map(|s| s.trim().into()),
                ipv4: addresses
                    .iter()
                    .filter(|(n, _, _)| n == &name)
                    .map(|(_, ip, _)| ip.to_string())
                    .collect(),
                wifi: radio,
                activity: parse_activity(&info, active.get(&name).map(String::as_str), !wifi),
                name,
            })
        })
        .collect();
    Inventory {
        probed_at: Some(crate::platform::wall_clock().as_secs()),
        country,
        interfaces,
    }
}
/// State-only refresh: identity, regulatory channels and phy capabilities stay cached.
pub fn refresh_activity(
    runner: &dyn CommandRunner,
    root: &Path,
    snapshot: &mut Inventory,
    addresses: &[(String, std::net::Ipv4Addr, u8)],
) {
    let active = parse_active(
        &run(
            runner,
            &[
                "nmcli",
                "-t",
                "-f",
                "DEVICE,NAME,UUID",
                "connection",
                "show",
                "--active",
            ],
        )
        .unwrap_or_default(),
    );
    for interface in &mut snapshot.interfaces {
        let wifi = interface.kind == "wifi";
        let info = if wifi {
            run(runner, &["iw", "dev", &interface.name, "info"]).unwrap_or_default()
        } else {
            String::new()
        };
        interface.activity = parse_activity(
            &info,
            active.get(&interface.name).map(String::as_str),
            !wifi,
        );
        interface.operstate = std::fs::read_to_string(
            root.join("sys/class/net")
                .join(&interface.name)
                .join("operstate"),
        )
        .ok()
        .map(|s| s.trim().into());
        interface.ipv4 = addresses
            .iter()
            .filter(|(name, _, _)| name == &interface.name)
            .map(|(_, ip, _)| ip.to_string())
            .collect();
    }
}
#[derive(Default)]
struct CacheState {
    names: Option<Vec<String>>,
    refreshed: Option<Instant>,
    full_refreshed: Option<Instant>,
    full_requested: bool,
    running: bool,
    requested: bool,
    snapshot: Inventory,
}
pub struct InventoryCache {
    state: Mutex<CacheState>,
    probed: Condvar,
    runner: Arc<dyn CommandRunner>,
    root: PathBuf,
    addresses: crate::runtime_paths::InterfaceAddresses,
    refresh_enabled: bool,
}
impl InventoryCache {
    pub fn new(runner: Arc<dyn CommandRunner>, root: PathBuf) -> Self {
        Self {
            state: Mutex::default(),
            probed: Condvar::new(),
            runner,
            root,
            addresses: if cfg!(target_os = "linux") {
                Arc::new(crate::linux_hotspot::interface_addresses)
            } else {
                Arc::new(|| Ok(Vec::new()))
            },
            refresh_enabled: true,
        }
    }
    pub fn with_addresses(mut self, addresses: crate::runtime_paths::InterfaceAddresses) -> Self {
        self.addresses = addresses;
        self
    }
    pub fn with_refresh(mut self, enabled: bool) -> Self {
        self.refresh_enabled = enabled;
        self
    }
    pub fn get(&self) -> Inventory {
        self.state.lock().unwrap().snapshot.clone()
    }
    pub fn wait_for_probe(&self, timeout: Duration) -> bool {
        let (state, _) = self
            .probed
            .wait_timeout_while(self.state.lock().unwrap(), timeout, |state| {
                state.snapshot.probed_at.is_none()
            })
            .unwrap();
        state.snapshot.probed_at.is_some()
    }
    #[cfg(test)]
    pub(crate) fn set(&self, snapshot: Inventory) {
        let mut state = self.state.lock().unwrap();
        while state.running {
            drop(state);
            std::thread::sleep(Duration::from_millis(1));
            state = self.state.lock().unwrap();
        }
        state.snapshot = snapshot;
        state.names = Some(interface_names(&self.root));
        state.refreshed = Some(Instant::now());
        state.full_refreshed = Some(Instant::now());
        self.probed.notify_all();
    }
    /// A country change must refresh channel capabilities, even during the activity-only interval.
    pub fn reprobe(&self) {
        if !self.refresh_enabled {
            return;
        }
        let mut state = self.state.lock().unwrap();
        while state.running {
            state = self.probed.wait(state).unwrap();
        }
        let addresses = (self.addresses)().unwrap_or_default();
        state.snapshot = probe(self.runner.as_ref(), &self.root, &addresses);
        state.names = Some(interface_names(&self.root));
        state.full_refreshed = Some(Instant::now());
        state.refreshed = Some(Instant::now());
        self.probed.notify_all();
    }
    pub fn refresh(self: &Arc<Self>) {
        self.request(false);
    }
    pub fn refresh_now(self: &Arc<Self>) {
        self.request(true);
    }
    fn request(self: &Arc<Self>, force: bool) {
        if !self.refresh_enabled {
            return;
        }
        let names = interface_names(&self.root);
        let mut state = self.state.lock().unwrap();
        let full = state.names.as_ref() != Some(&names)
            || state
                .full_refreshed
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60));
        if !full
            && !force
            && state
                .refreshed
                .is_some_and(|t| t.elapsed() < Duration::from_secs(10))
        {
            return;
        }
        state.names = Some(names);
        state.refreshed = Some(Instant::now());
        if full {
            state.full_refreshed = Some(Instant::now());
        }
        state.full_requested |= full;
        state.requested = true;
        if state.running {
            return;
        }
        state.running = true;
        drop(state);
        let cache = self.clone();
        if spawn_worker(
            "network-inventory",
            Box::new(move || loop {
                let (full, mut snapshot) = {
                    let mut state = cache.state.lock().unwrap();
                    if !state.requested {
                        state.running = false;
                        cache.probed.notify_all();
                        break;
                    }
                    state.requested = false;
                    (
                        std::mem::take(&mut state.full_requested),
                        state.snapshot.clone(),
                    )
                };
                let addresses = (cache.addresses)().unwrap_or_default();
                if full {
                    snapshot = probe(cache.runner.as_ref(), &cache.root, &addresses);
                } else {
                    refresh_activity(
                        cache.runner.as_ref(),
                        &cache.root,
                        &mut snapshot,
                        &addresses,
                    );
                }
                cache.state.lock().unwrap().snapshot = snapshot;
                cache.probed.notify_all();
            }),
        )
        .is_err()
        {
            let mut state = self.state.lock().unwrap();
            state.running = false;
            state.refreshed = None;
            state.full_refreshed = None;
            self.probed.notify_all();
        }
    }
}

#[cfg(test)]
#[path = "net_inventory_tests.rs"]
mod tests;
