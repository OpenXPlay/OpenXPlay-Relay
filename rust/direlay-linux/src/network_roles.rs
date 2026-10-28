// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

use crate::{
    net_inventory::{Interface, Inventory},
    settings::Settings,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkRoles {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iphone: Option<Iphone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub car_browser: Option<CarBrowser>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carlink: Option<Carlink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_hotspot: Option<AdminHotspot>,
}
impl Default for NetworkRoles {
    fn default() -> Self {
        Self {
            mode: "auto".into(),
            iphone: None,
            car_browser: None,
            carlink: None,
            admin_hotspot: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Iphone {
    pub link: String,
    pub interface: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CarBrowser {
    pub network: String,
    pub interface: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Carlink {
    pub interface: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AdminHotspot {
    pub enabled: bool,
    pub interface: Option<String>,
    pub ssid: String,
    pub band: String,
    pub channel: u8,
}
impl Default for AdminHotspot {
    fn default() -> Self {
        Self {
            enabled: false,
            interface: None,
            ssid: "OpenXPlay-Admin".into(),
            band: "auto".into(),
            channel: 0,
        }
    }
}
pub fn valid_ifref(value: &str) -> bool {
    if value.contains(':') {
        crate::net_inventory::normalized_mac(value).as_deref() == Some(value)
    } else {
        !value.is_empty()
            && value.len() <= 15
            && value != "."
            && value != ".."
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    }
}
pub fn valid_password(value: &str) -> bool {
    (8..=63).contains(&value.len()) && value.bytes().all(|b| (32..=126).contains(&b))
}
pub fn valid_country(value: &str) -> bool {
    // ISO 3166-1 alpha-2 assignments, plus the world regulatory domain.
    const COUNTRIES: &str = "AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW";
    value == "system" || value == "00" || COUNTRIES.split_whitespace().any(|code| code == value)
}
impl NetworkRoles {
    pub fn validate(&self) -> std::io::Result<()> {
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid networkRoles");
        if !["auto", "custom"].contains(&self.mode.as_str()) {
            return Err(bad());
        }
        if self.mode == "custom"
            && (self.iphone.is_none()
                || self.car_browser.is_none()
                || self.carlink.is_none()
                || self.admin_hotspot.is_none())
        {
            return Err(bad());
        }
        if self.iphone.as_ref().is_some_and(|r| {
            !["hotspot", "saved-network"].contains(&r.link.as_str()) || !valid_ifref(&r.interface)
        }) {
            return Err(bad());
        }
        if self.car_browser.as_ref().is_some_and(|r| {
            !["iphone", "interface"].contains(&r.network.as_str())
                || (r.network == "interface" && r.interface.is_none())
        }) {
            return Err(bad());
        }
        let refs = [
            self.car_browser
                .as_ref()
                .and_then(|r| r.interface.as_deref()),
            self.carlink.as_ref().and_then(|r| r.interface.as_deref()),
            self.admin_hotspot
                .as_ref()
                .and_then(|r| r.interface.as_deref()),
        ];
        if refs.into_iter().flatten().any(|r| !valid_ifref(r)) {
            return Err(bad());
        }
        if self.admin_hotspot.as_ref().is_some_and(|r| {
            !["auto", "2.4", "5"].contains(&r.band.as_str())
                || !(1..=32).contains(&r.ssid.len())
                || r.ssid.contains(['\0', '\r', '\n'])
                || (r.channel != 0
                    && !match r.band.as_str() {
                        "5" => (32..=177).contains(&r.channel),
                        "2.4" => (1..=14).contains(&r.channel),
                        _ => (1..=14).contains(&r.channel) || (32..=177).contains(&r.channel),
                    })
        }) {
            return Err(bad());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub name: Option<String>,
    pub mac: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub interface: Option<InterfaceRef>,
    pub missing: bool,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IphonePlan {
    #[serde(flatten)]
    pub role: Role,
    pub link: String,
    pub band: String,
    pub channel: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarBrowserPlan {
    #[serde(flatten)]
    pub role: Role,
    pub network: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarlinkPlan {
    #[serde(flatten)]
    pub role: Role,
    pub follow_over: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminHotspotPlan {
    #[serde(flatten)]
    pub role: Role,
    pub enabled: bool,
    pub ssid: String,
    pub band: String,
    pub channel: u8,
    pub time_shared_with: Option<String>,
    pub password_set: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub source: String,
    pub iphone: IphonePlan,
    pub car_browser: CarBrowserPlan,
    pub carlink: CarlinkPlan,
    pub admin_hotspot: AdminHotspotPlan,
    pub output: String,
}
/// Effective legacy inputs already include host.json, settings.json and command-line precedence.
pub struct Legacy<'a> {
    pub wifi_interface: &'a str,
    pub carlink_interface: &'a str,
    pub network_mode: &'a str,
    pub hotspot_band: &'a str,
    pub hotspot_channel: u8,
    pub cli: &'a BTreeMap<String, String>,
}
#[derive(Clone, PartialEq, Eq)]
struct InterfaceCapabilities {
    kind: String,
    phy: Option<String>,
    wifi: Option<crate::wifi_capabilities::PhyCapabilities>,
}
/// Automatic selections survive activity, address and probe timestamp changes, only in memory.
#[derive(Clone, Default)]
pub struct AutomaticChoices {
    topology: BTreeMap<String, InterfaceCapabilities>,
    inputs: Value,
    plan: Option<Plan>,
}
impl AutomaticChoices {
    pub fn resolve(
        &mut self,
        inventory: &Inventory,
        settings: &Settings,
        legacy: &Legacy<'_>,
        preset: &str,
        output: &str,
    ) -> Plan {
        let topology = inventory
            .interfaces
            .iter()
            .map(|i| {
                (
                    i.name.clone(),
                    InterfaceCapabilities {
                        kind: i.kind.clone(),
                        phy: i.phy.clone(),
                        wifi: i.wifi.as_ref().map(|w| w.capabilities.clone()),
                    },
                )
            })
            .collect();
        let inputs = json!({
            "roles": if settings.network_roles.mode == "auto" { json!({"mode":"auto"}) } else { json!({ "mode": settings.network_roles.mode, "iphone": settings.network_roles.iphone, "carBrowser": settings.network_roles.car_browser, "carlink": settings.network_roles.carlink }) },
            "iphone": legacy.wifi_interface,
            "carlink": legacy.carlink_interface,
            "cli": legacy.cli,
        });
        let kept = (self.topology == topology && self.inputs == inputs)
            .then_some(self.plan.as_ref())
            .flatten();
        let plan = match kept {
            Some(kept) => {
                resolve_with_choices(inventory, settings, legacy, preset, output, Some(kept))
            }
            None => resolve_plan(inventory, settings, legacy, preset, output),
        };
        // An empty, not-yet-probed inventory must never choose the startup roles.
        if inventory.probed_at.is_some() {
            self.topology = topology;
            self.inputs = inputs;
            self.plan = Some(plan.clone());
        }
        plan
    }
}
fn resolve_interface(inventory: &Inventory, reference: Option<&str>, source: &str) -> Role {
    let interface = reference.map(|reference| {
        let found = inventory.interfaces.iter().find(|i| {
            if reference.contains(':') {
                i.mac.as_deref() == Some(reference) || i.current_mac.as_deref() == Some(reference)
            } else {
                i.name == reference
            }
        });
        InterfaceRef {
            reference: reference.into(),
            name: found
                .map(|i| i.name.clone())
                .or_else(|| (!reference.contains(':')).then(|| reference.into())),
            mac: found.and_then(|i| i.mac.clone()),
        }
    });
    let missing = reference.is_some()
        && interface
            .as_ref()
            .and_then(|r| r.name.as_ref())
            .is_none_or(|n| !inventory.interfaces.iter().any(|i| &i.name == n));
    Role {
        interface,
        missing,
        source: source.into(),
    }
}
fn entry<'a>(inventory: &'a Inventory, role: &Role) -> Option<&'a Interface> {
    if role.missing {
        return None;
    }
    let name = role.interface.as_ref()?.name.as_deref()?;
    inventory.interfaces.iter().find(|i| i.name == name)
}
fn same_interface(a: &Role, b: &Role) -> bool {
    match (&a.interface, &b.interface) {
        (Some(a), Some(b)) => a.reference == b.reference || (a.name.is_some() && a.name == b.name),
        _ => false,
    }
}
fn same_phy(inventory: &Inventory, a: &Role, b: &Role) -> bool {
    match (
        entry(inventory, a).and_then(|i| i.phy.as_ref()),
        entry(inventory, b).and_then(|i| i.phy.as_ref()),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
/// Resolve compatibility inputs and stable interface references into the running links.
pub fn resolve_plan(
    inventory: &Inventory,
    settings: &Settings,
    legacy: &Legacy<'_>,
    preset: &str,
    output: &str,
) -> Plan {
    resolve_with_choices(inventory, settings, legacy, preset, output, None)
}
fn resolve_with_choices(
    inventory: &Inventory,
    settings: &Settings,
    legacy: &Legacy<'_>,
    preset: &str,
    output: &str,
    kept: Option<&Plan>,
) -> Plan {
    let roles = &settings.network_roles;
    let custom = roles.mode == "custom";
    let iphone_cli =
        legacy.cli.contains_key("--wifi-interface") || legacy.cli.contains_key("--network-mode");
    let iphone_ref = legacy
        .cli
        .get("--wifi-interface")
        .map(String::as_str)
        .or_else(|| {
            custom
                .then(|| roles.iphone.as_ref().map(|r| r.interface.as_str()))
                .flatten()
        })
        .unwrap_or(legacy.wifi_interface);
    let link = legacy
        .cli
        .get("--network-mode")
        .map(String::as_str)
        .map(|mode| {
            if mode == "shared-wifi" {
                "saved-network"
            } else {
                "hotspot"
            }
        })
        .or_else(|| {
            custom
                .then(|| roles.iphone.as_ref().map(|r| r.link.as_str()))
                .flatten()
        })
        .unwrap_or(if legacy.network_mode == "shared-wifi" {
            "saved-network"
        } else {
            "hotspot"
        });
    let iphone = resolve_interface(
        inventory,
        Some(iphone_ref),
        if iphone_cli {
            "cli"
        } else if custom {
            "custom"
        } else {
            "legacy"
        },
    );
    let iphone_entry = entry(inventory, &iphone);
    let hotspot = crate::device_preset::resolve_hotspot(
        legacy.hotspot_band,
        legacy.hotspot_channel,
        preset,
        iphone_entry
            .and_then(|i| i.wifi.as_ref())
            .map(|w| &w.capabilities),
    );
    let different_phy = |i: &&Interface| {
        i.kind == "wifi"
            && i.phy.is_some()
            && iphone_entry
                .and_then(|p| p.phy.as_ref())
                .is_some_and(|phy| i.phy.as_ref() != Some(phy))
    };
    let mut candidates: Vec<_> = inventory
        .interfaces
        .iter()
        .filter(different_phy)
        .filter(|i| !foreign_busy(i))
        .filter(|i| i.wifi.as_ref().is_some_and(|w| !w.station5().is_empty()))
        .collect();
    candidates.sort_by_key(|i| {
        (
            std::cmp::Reverse(i.wifi.as_ref().map_or(0, |w| w.station5().len())),
            &i.name,
        )
    });
    let (carlink_ref, carlink_source) =
        if let Some(interface) = legacy.cli.get("--carlink-interface") {
            (
                Some(interface.as_str())
                    .filter(|s| !s.is_empty())
                    .or(Some(iphone_ref)),
                "cli",
            )
        } else if custom {
            (
                roles
                    .carlink
                    .as_ref()
                    .and_then(|r| r.interface.as_deref())
                    .or(Some(iphone_ref)),
                "custom",
            )
        } else if !legacy.carlink_interface.is_empty() {
            (Some(legacy.carlink_interface), "legacy")
        } else {
            (
                kept.and_then(|p| p.carlink.role.name())
                    .or_else(|| Some(candidates.first().map_or(iphone_ref, |i| i.name.as_str()))),
                "auto",
            )
        };
    let carlink = resolve_interface(inventory, carlink_ref, carlink_source);
    let browser = custom
        .then(|| roles.car_browser.clone())
        .flatten()
        .unwrap_or(CarBrowser {
            network: "iphone".into(),
            interface: None,
        });
    let browser_role = if browser.network == "iphone" {
        let mut role = iphone.clone();
        role.source = if custom { "custom" } else { "auto" }.into();
        role
    } else {
        resolve_interface(inventory, browser.interface.as_deref(), "custom")
    };
    let admin = roles.admin_hotspot.clone().unwrap_or_default();
    let mut admin_candidates: Vec<_> = inventory
        .interfaces
        .iter()
        .filter(different_phy)
        .filter(|i| {
            i.wifi.as_ref().is_some_and(|w| {
                w.capabilities.ap && (admin.band == "auto" || w.ap_band(&admin.band))
            })
        })
        .collect();
    admin_candidates.sort_by_key(|i| {
        (
            entry(inventory, &carlink).is_none_or(|c| c.phy != i.phy),
            &i.name,
        )
    });
    let admin_ref = if !admin.enabled {
        None
    } else if admin.interface.is_some() {
        admin.interface.as_deref()
    } else {
        kept.and_then(|p| p.admin_hotspot.role.name())
            .filter(|name| admin_candidates.iter().any(|i| i.name == *name))
            .or_else(|| admin_candidates.first().map(|i| i.name.as_str()))
    };
    let admin_role = resolve_interface(
        inventory,
        admin_ref,
        if custom || admin.interface.is_some() {
            "custom"
        } else {
            "auto"
        },
    );
    let radio = entry(inventory, &admin_role).and_then(|i| i.wifi.as_ref());
    let band = if admin.band == "auto" {
        if radio.is_some_and(|w| w.ap_band("5")) {
            "5"
        } else {
            "2.4"
        }
    } else {
        &admin.band
    };
    let channel = if admin.channel == 0 {
        radio
            .and_then(|w| automatic_admin_channel(w, band, &hotspot.band, hotspot.channel))
            .unwrap_or(0)
    } else {
        admin.channel
    };
    let time_shared_with =
        (admin.enabled && same_phy(inventory, &admin_role, &carlink)).then(|| "carlink".into());
    let follow_over = same_interface(&iphone, &carlink);
    Plan {
        source: roles.mode.clone(),
        output: output.into(),
        iphone: IphonePlan {
            role: iphone,
            link: link.into(),
            band: hotspot.band,
            channel: hotspot.channel,
        },
        car_browser: CarBrowserPlan {
            role: browser_role,
            network: browser.network,
        },
        carlink: CarlinkPlan {
            role: carlink,
            follow_over,
        },
        admin_hotspot: AdminHotspotPlan {
            role: admin_role,
            enabled: admin.enabled,
            ssid: admin.ssid,
            band: band.into(),
            channel,
            time_shared_with,
            password_set: settings
                .admin_hotspot_password
                .as_deref()
                .is_some_and(valid_password),
        },
    }
}
/// Prefer the highest sufficiently separated AP channel, otherwise the farthest available one.
fn automatic_admin_channel(
    radio: &crate::net_inventory::Radio,
    band: &str,
    iphone_band: &str,
    iphone_channel: u8,
) -> Option<u8> {
    let channels = &radio.bands.iter().find(|b| b.band == band)?.ap_channels;
    if band == "5" && iphone_band == "5" && iphone_channel != 0 {
        let distance = |c: u16| c.abs_diff(u16::from(iphone_channel)) * 5;
        channels
            .iter()
            .filter(|&&c| distance(c) >= 80)
            .max()
            .or_else(|| channels.iter().max_by_key(|&&c| (distance(c), c)))
            .and_then(|&c| c.try_into().ok())
    } else {
        channels.iter().max().and_then(|&c| c.try_into().ok())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub level: String,
    pub code: String,
    pub roles: Vec<String>,
    pub interfaces: Vec<String>,
    pub params: Value,
}
pub fn checks(plan: &Plan, inventory: &Inventory) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut add = |level: &str, code: &str, roles: &[(&str, &Role)], params: Value| {
        let mut interfaces: Vec<_> = roles
            .iter()
            .filter_map(|(_, r)| {
                r.interface
                    .as_ref()
                    .map(|i| i.name.as_deref().unwrap_or(&i.reference).to_string())
            })
            .collect();
        interfaces.sort();
        interfaces.dedup();
        checks.push(Check {
            level: level.into(),
            code: code.into(),
            roles: roles.iter().map(|(n, _)| (*n).into()).collect(),
            interfaces,
            params,
        });
    };
    for (name, role) in [
        ("iphone", &plan.iphone.role),
        ("carBrowser", &plan.car_browser.role),
        ("carlink", &plan.carlink.role),
        ("adminHotspot", &plan.admin_hotspot.role),
    ] {
        if role.missing {
            add(
                "warning",
                "interface-missing",
                &[(name, role)],
                json!({"ref": role.interface.as_ref().map(|i| &i.reference)}),
            );
        }
    }
    for interface in inventory
        .interfaces
        .iter()
        .filter(|i| i.kind == "wifi" && foreign_busy(i))
    {
        let role = resolve_interface(inventory, Some(&interface.name), "auto");
        let mut roles = Vec::new();
        if plan.carlink.role.source == "auto" {
            roles.push(("carlink", &role));
        }
        if plan.admin_hotspot.enabled
            && plan.admin_hotspot.role.name() == Some(interface.name.as_str())
        {
            roles.push(("adminHotspot", &role));
        }
        if !roles.is_empty() {
            add(
                if roles.iter().any(|(name, _)| *name == "adminHotspot")
                    && interface.activity.kind == "other-ap"
                {
                    "warning"
                } else {
                    "info"
                },
                "interface-busy",
                &roles,
                json!({
                    "connection": interface.activity.profile.as_ref().or(interface.activity.ssid.as_ref()).cloned().unwrap_or_else(|| interface.activity.kind.clone())
                }),
            );
        }
    }
    let aps = [
        (
            "iphone",
            &plan.iphone.role,
            plan.iphone.link == "hotspot",
            plan.iphone.band.as_str(),
            plan.iphone.channel,
        ),
        (
            "adminHotspot",
            &plan.admin_hotspot.role,
            plan.admin_hotspot.enabled,
            plan.admin_hotspot.band.as_str(),
            plan.admin_hotspot.channel,
        ),
    ];
    for (name, role, enabled, band, channel) in aps {
        if !enabled {
            continue;
        }
        if role.interface.is_none() {
            add(
                "error",
                "no-ap-channel",
                &[(name, role)],
                json!({"band": band}),
            );
        } else if let Some(interface) = entry(inventory, role) {
            if interface.kind != "wifi"
                || interface.wifi.as_ref().is_some_and(|w| !w.capabilities.ap)
            {
                add("error", "no-ap-mode", &[(name, role)], json!({}));
            } else if interface.wifi.as_ref().is_some_and(|w| {
                !w.ap_band(band)
                    || (channel != 0
                        && !w
                            .bands
                            .iter()
                            .any(|b| b.band == band && b.ap_channels.contains(&u16::from(channel))))
            }) {
                add(
                    "error",
                    "no-ap-channel",
                    &[(name, role)],
                    json!({"band": band}),
                );
            }
        }
    }
    for (name, role, enabled) in [
        (
            "iphone",
            &plan.iphone.role,
            plan.iphone.link == "saved-network",
        ),
        ("carlink", &plan.carlink.role, plan.output != "web"),
    ] {
        if enabled
            && entry(inventory, role).is_some_and(|i| {
                i.kind != "wifi" || i.wifi.as_ref().is_some_and(|w| !w.capabilities.station)
            })
        {
            add("error", "no-station-mode", &[(name, role)], json!({}));
        }
    }
    if plan.carlink.follow_over {
        add(
            "info",
            "follow-over",
            &[
                ("iphone", &plan.iphone.role),
                ("carlink", &plan.carlink.role),
            ],
            json!({}),
        );
    }
    if let Some(radio) = entry(inventory, &plan.carlink.role).and_then(|i| i.wifi.as_ref()) {
        let channels = radio.station5();
        if channels.is_empty() {
            add(
                "warning",
                "carlink-no-5ghz",
                &[("carlink", &plan.carlink.role)],
                json!({}),
            );
        } else {
            let missing: Vec<_> = [36, 40, 44, 48, 149, 153, 157, 161, 165]
                .into_iter()
                .filter(|c| !channels.contains(c))
                .collect();
            if !missing.is_empty() {
                add(
                    "warning",
                    "carlink-band-gap",
                    &[("carlink", &plan.carlink.role)],
                    json!({"channels": missing}),
                );
            }
        }
    }
    if plan.admin_hotspot.enabled {
        add(
            "info",
            "admin-hotspot-nat",
            &[("adminHotspot", &plan.admin_hotspot.role)],
            json!({}),
        );
        if plan.admin_hotspot.time_shared_with.is_some() {
            add(
                "info",
                "time-shared",
                &[
                    ("adminHotspot", &plan.admin_hotspot.role),
                    ("carlink", &plan.carlink.role),
                ],
                json!({}),
            );
        }
    }
    let mut simultaneous = vec![(
        "iphone",
        &plan.iphone.role,
        if plan.iphone.link == "hotspot" {
            "AP"
        } else {
            "managed"
        },
    )];
    if plan.output != "web" && !plan.carlink.follow_over {
        simultaneous.push(("carlink", &plan.carlink.role, "managed"));
    }
    if plan.car_browser.network == "interface"
        && !same_interface(&plan.car_browser.role, &plan.iphone.role)
        && !same_interface(&plan.car_browser.role, &plan.carlink.role)
    {
        simultaneous.push(("carBrowser", &plan.car_browser.role, "managed"));
    }
    if plan.admin_hotspot.enabled {
        simultaneous.push(("adminHotspot", &plan.admin_hotspot.role, "AP"));
    }
    for (index, (a, ar, am)) in simultaneous.iter().enumerate() {
        for (b, br, bm) in &simultaneous[index + 1..] {
            if !same_phy(inventory, ar, br)
                || ((*a == "carlink" && *b == "adminHotspot")
                    || (*b == "carlink" && *a == "adminHotspot"))
            {
                continue;
            }
            if let Some(radio) = entry(inventory, ar).and_then(|i| i.wifi.as_ref()) {
                let ap_channel = |name: &str| {
                    aps.iter()
                        .find(|(role, _, enabled, _, _)| *role == name && *enabled)
                        .map(|(_, _, _, band, channel)| (*band, *channel))
                };
                let channels_needed = match (ap_channel(a), ap_channel(b)) {
                    (Some((ab, ac)), Some((bb, bc)))
                        if ac != 0 && bc != 0 && (ab != bb || ac != bc) =>
                    {
                        2
                    }
                    _ => 1,
                };
                let allowed = radio.capabilities.combinations.iter().any(|c| {
                    c.allows(&[am, bm]) && c.channels.is_none_or(|n| n >= channels_needed)
                });
                if !allowed || same_interface(ar, br) {
                    add(
                        "error",
                        "roles-conflict",
                        &[(a, ar), (b, br)],
                        json!({"phy": entry(inventory, ar).and_then(|i| i.phy.as_ref())}),
                    );
                }
            }
        }
    }
    if aps[0].2 && aps[1].2 && aps[0].3 == "5" && aps[1].3 == "5" && aps[0].4 != 0 && aps[1].4 != 0
    {
        let frequency = |role: &Role, channel: u8| {
            entry(inventory, role)
                .and_then(|i| i.wifi.as_ref())
                .and_then(|w| {
                    w.capabilities
                        .channels
                        .iter()
                        .find(|c| c.five_ghz() && c.channel == u16::from(channel))
                })
                .map_or(5000 + u32::from(channel) * 5, |c| c.frequency)
        };
        let spacing = frequency(aps[0].1, aps[0].4).abs_diff(frequency(aps[1].1, aps[1].4));
        if spacing < 80 {
            add(
                "warning",
                "ap-spacing",
                &[("iphone", aps[0].1), ("adminHotspot", aps[1].1)],
                json!({"mhz": spacing, "channels": [aps[0].4, aps[1].4]}),
            );
        }
    }
    if let Some(configured) = inventory
        .country
        .configured
        .as_deref()
        .filter(|c| *c != "system")
    {
        if inventory.country.global.as_deref() != Some(configured) {
            add(
                "warning",
                "country-not-applied",
                &[],
                json!({"configured": configured, "effective": inventory.country.global}),
            );
        }
    }
    checks
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    #[serde(flatten)]
    pub inventory: Inventory,
    pub plan: Plan,
    pub checks: Vec<Check>,
    pub applied: bool,
    pub probing: bool,
}
pub enum SaveResult {
    Saved(Box<Proposal>),
    Probing,
    Invalid(String),
    Checks(Vec<Check>),
    Storage,
}
/// The same proposal is returned by preview and save; only save applies it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub plan: Plan,
    pub checks: Vec<Check>,
    pub changes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<bool>,
}
fn foreign_busy(interface: &Interface) -> bool {
    matches!(
        interface.activity.kind.as_str(),
        "other-ap" | "other-network"
    )
}
impl Role {
    pub fn name(&self) -> Option<&str> {
        self.interface
            .as_ref()
            .map(|i| i.name.as_deref().unwrap_or(&i.reference))
    }
}
/// One mapping for the controller, provisioning and output supervisor.
pub fn apply_plan(plan: &Plan, options: &mut crate::Options) {
    if let Some(name) = plan.iphone.role.name() {
        options.wifi_interface = name.into();
        options.settings.wifi_interface = name.into();
    }
    options.settings.network_mode = if plan.iphone.link == "saved-network" {
        "shared-wifi"
    } else {
        "device-hotspot"
    }
    .into();
    options.settings.carlink_interface = if plan.carlink.follow_over {
        String::new()
    } else {
        plan.carlink.role.name().unwrap_or("").into()
    };
}
pub fn changes(before: &Plan, after: &Plan) -> Vec<String> {
    let mut changes = RunningLinks::from_plan(before, |_| true)
        .changes(&RunningLinks::from_plan(after, |_| true));
    if admin_inputs(&before.admin_hotspot) != admin_inputs(&after.admin_hotspot) {
        changes.push("adminHotspot".into());
    }
    changes
}
fn admin_inputs(p: &AdminHotspotPlan) -> (bool, Option<&str>, &str, &str, u8, bool) {
    (
        p.enabled,
        p.role.name(),
        &p.ssid,
        &p.band,
        p.channel,
        p.password_set,
    )
}
/// Actual component inputs, including interface presence rather than diagnostic missing flags.
#[derive(Clone)]
pub struct RunningLinks {
    iphone: Option<String>,
    link: String,
    hotspot: Option<(String, u8)>,
    carlink: Option<String>,
    follow_over: bool,
    browser: Option<String>,
    browser_network: String,
}
impl RunningLinks {
    pub fn from_plan(plan: &Plan, present: impl Fn(&str) -> bool) -> Self {
        let name = |role: &Role| role.name().filter(|n| present(n)).map(str::to_string);
        Self {
            iphone: name(&plan.iphone.role),
            link: plan.iphone.link.clone(),
            hotspot: Some((plan.iphone.band.clone(), plan.iphone.channel)),
            carlink: name(&plan.carlink.role),
            follow_over: plan.carlink.follow_over,
            browser: name(&plan.car_browser.role),
            browser_network: plan.car_browser.network.clone(),
        }
    }
    pub fn legacy(options: &crate::Options, present: impl Fn(&str) -> bool) -> Self {
        let name = |n: &str| present(n).then(|| n.to_string());
        Self {
            iphone: name(&options.wifi_interface),
            link: if options.settings.network_mode == "shared-wifi" {
                "saved-network"
            } else {
                "hotspot"
            }
            .into(),
            hotspot: None,
            carlink: name(options.carlink_interface()),
            follow_over: options.shared_carlink_radio(),
            browser: name(&options.wifi_interface),
            browser_network: "iphone".into(),
        }
    }
    pub fn changes(&self, after: &Self) -> Vec<String> {
        let mut changes = Vec::new();
        let iphone = self.iphone != after.iphone
            || self.link != after.link
            || (after.link == "hotspot" && self.hotspot.is_some() && self.hotspot != after.hotspot);
        if iphone {
            changes.push("controller".into());
        }
        if self.carlink != after.carlink || self.follow_over != after.follow_over {
            changes.push("carlink".into());
        }
        if self.browser != after.browser
            || self.browser_network != after.browser_network
            || (iphone && after.browser_network == "iphone")
        {
            changes.push("carBrowserAddress".into());
        }
        changes
    }
}
/// A radio hold is wider than follow-over: two virtual interfaces can share one phy.
pub fn shared_radio(inventory: &Inventory, plan: &Plan) -> bool {
    same_interface(&plan.iphone.role, &plan.carlink.role)
        || same_phy(inventory, &plan.iphone.role, &plan.carlink.role)
}
pub fn browser_address(
    plan: &Plan,
    iphone: Option<std::net::Ipv4Addr>,
    addresses: &[(String, std::net::Ipv4Addr, u8)],
) -> Option<std::net::Ipv4Addr> {
    if plan.car_browser.network == "iphone" {
        return iphone;
    }
    if plan.car_browser.role.missing {
        return None;
    }
    addresses
        .iter()
        .find(|(name, ip, _)| {
            Some(name.as_str()) == plan.car_browser.role.name()
                && !ip.is_loopback()
                && !ip.is_unspecified()
        })
        .map(|(_, ip, _)| *ip)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Update {
    pub network_roles: NetworkRoles,
    pub admin_hotspot_password: Option<String>,
    pub wifi_country: Option<String>,
}

#[cfg(test)]
#[path = "network_roles_tests.rs"]
mod tests;
