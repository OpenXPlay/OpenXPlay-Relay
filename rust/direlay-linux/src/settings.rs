// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::{platform::private_state_file, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedWifiNetwork {
    pub ssid: String,
    pub password: String,
}
impl SharedWifiNetwork {
    pub fn valid(&self) -> bool {
        (1..=32).contains(&self.ssid.len())
            && !self.ssid.contains(['\r', '\n', '\0'])
            && (self.password.is_empty() || (8..=63).contains(&self.password.len()))
            && !self.password.contains('\0')
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub plugins: bool,
    pub plugins_enabled: Vec<String>,
    pub network_mode: String,
    pub shared_wifi_networks: Vec<SharedWifiNetwork>,
    pub wifi_interface: String,
    pub hotspot_channel: u8,
    pub iphone: Option<String>,
    pub rfcomm_channel: Option<u8>,
    pub receiver_port: u16,
    pub require_pairing_code: bool,
    pub output: String,
    pub carlink_link: String,
    pub carlink_address: Option<String>,
    pub carlink_head_unit: String,
    pub carlink_local: String,
    pub carlink_pin: Option<String>,
    pub carlink_ports: BTreeMap<String, u16>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            plugins: false,
            plugins_enabled: Vec::new(),
            network_mode: "device-hotspot".into(),
            shared_wifi_networks: Vec::new(),
            wifi_interface: "wlan0".into(),
            hotspot_channel: 36,
            iphone: None,
            rfcomm_channel: None,
            receiver_port: 8443,
            require_pairing_code: false,
            output: "web".into(),
            carlink_link: "ble".into(),
            carlink_address: None,
            carlink_head_unit: "192.168.43.1".into(),
            carlink_local: "192.168.43.2".into(),
            carlink_pin: None,
            carlink_ports: BTreeMap::new(),
        }
    }
}
impl Settings {
    pub fn load(dir: &Path) -> Result<Self> {
        match std::fs::read(dir.join("settings.json")) {
            Ok(b) => {
                let mut value: serde_json::Value = serde_json::from_slice(&b)?;
                if let Some(object) = value.as_object_mut() {
                    let ssid = object.remove("wifiSsid");
                    let password = object.remove("wifiPassword");
                    if !object.contains_key("sharedWifiNetworks") {
                        if let Some(ssid) = ssid {
                            let ssid: String = serde_json::from_value(ssid)?;
                            let password: String = password
                                .map(serde_json::from_value)
                                .transpose()?
                                .unwrap_or_default();
                            if !ssid.is_empty() {
                                object.insert(
                                    "sharedWifiNetworks".into(),
                                    serde_json::json!([{ "ssid": ssid, "password": password }]),
                                );
                            }
                        }
                    }
                }
                let s: Self = serde_json::from_value(value)?;
                s.validate()?;
                Ok(s)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }
    pub fn save(&self, dir: &Path) -> Result<()> {
        self.validate()?;
        private_state_file(
            &dir.join("settings.json"),
            &serde_json::to_vec_pretty(self)?,
        )
    }
    pub fn validate(&self) -> Result<()> {
        let invalid = |s: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, s);
        if !["device-hotspot", "shared-wifi"].contains(&self.network_mode.as_str()) {
            return Err(invalid("Invalid networkMode"));
        }
        if self.wifi_interface.is_empty()
            || !self
                .wifi_interface
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(invalid("Invalid Wi-Fi interface"));
        }
        if !(1..=14).contains(&self.hotspot_channel) && !(32..=177).contains(&self.hotspot_channel)
        {
            return Err(invalid(
                "Preferred Wi-Fi channel must be in 1..14 or 32..177",
            ));
        }
        if self
            .iphone
            .as_ref()
            .is_some_and(|a| !crate::linux_bluetooth::valid_mac(a))
        {
            return Err(invalid("Invalid --iphone address"));
        }
        if self.rfcomm_channel.is_some_and(|n| !(1..=30).contains(&n)) {
            return Err(invalid("RFCOMM channel must be in 1..30"));
        }
        if !["web", "carlink", "both"].contains(&self.output.as_str()) {
            return Err(invalid("Invalid output"));
        }
        if !["ble", "static"].contains(&self.carlink_link.as_str()) {
            return Err(invalid("Invalid carlinkLink"));
        }
        if self
            .carlink_address
            .as_ref()
            .is_some_and(|a| !crate::linux_bluetooth::valid_mac(a))
        {
            return Err(invalid("Invalid carlinkAddress"));
        }
        if self.receiver_port == 0 {
            return Err(invalid("Invalid receiverPort"));
        }
        let mut ids = std::collections::BTreeSet::new();
        if self
            .plugins_enabled
            .iter()
            .any(|id| !crate::plugins::valid_id(id) || !ids.insert(id))
        {
            return Err(invalid("error.plugins"));
        }
        let mut names = std::collections::BTreeSet::new();
        if self.shared_wifi_networks.len() > 10
            || self
                .shared_wifi_networks
                .iter()
                .any(|n| !n.valid() || !names.insert(&n.ssid))
        {
            return Err(invalid("error.networks"));
        }
        if self.output != "web"
            && self.carlink_link == "static"
            && (self
                .carlink_head_unit
                .parse::<std::net::Ipv4Addr>()
                .is_err()
                || self.carlink_local.parse::<std::net::Ipv4Addr>().is_err())
        {
            return Err(invalid("Invalid Carlink addresses"));
        }
        if self.carlink_ports.values().any(|p| *p == 0) {
            return Err(invalid("Invalid Carlink port"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_wifi_migrates_and_unknown_fields_stay_rejected() {
        let dir = std::env::temp_dir().join(format!(
            "settings-migrate-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            br#"{"wifiSsid":"Home","wifiPassword":"password"}"#,
        )
        .unwrap();
        let settings = Settings::load(&dir).unwrap();
        assert_eq!(
            settings.shared_wifi_networks,
            vec![SharedWifiNetwork {
                ssid: "Home".into(),
                password: "password".into()
            }]
        );
        settings.save(&dir).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert!(json.get("wifiSsid").is_none());
        assert!(json.get("wifiPassword").is_none());
        assert_eq!(json["sharedWifiNetworks"][0]["password"], "password");
        std::fs::write(
            dir.join("settings.json"),
            br#"{"wifiSsid":"Home","unknown":true}"#,
        )
        .unwrap();
        assert!(Settings::load(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn saved_network_validation_uses_bytes_and_rejects_duplicates() {
        let valid = |ssid: &str, password: &str| {
            SharedWifiNetwork {
                ssid: ssid.into(),
                password: password.into(),
            }
            .valid()
        };
        for ssid in ["", "bad\rname", "bad\nname", "bad\0name", &"é".repeat(17)] {
            assert!(!valid(ssid, "password"));
        }
        assert!(valid(&"é".repeat(16), ""));
        for password in ["short", "1234567\0", &"x".repeat(64)] {
            assert!(!valid("Home", password));
        }
        assert!(valid("Home", &"x".repeat(63)));
        let mut settings = Settings {
            shared_wifi_networks: (0..10)
                .map(|i| SharedWifiNetwork {
                    ssid: i.to_string(),
                    password: String::new(),
                })
                .collect(),
            ..Default::default()
        };
        assert!(settings.validate().is_ok());
        settings.shared_wifi_networks.push(SharedWifiNetwork {
            ssid: "extra".into(),
            password: String::new(),
        });
        assert!(settings.validate().is_err());
        settings.shared_wifi_networks = vec![
            SharedWifiNetwork {
                ssid: "Home".into(),
                password: String::new()
            };
            2
        ];
        assert!(settings.validate().is_err());
    }
    #[test]
    fn old_settings_load_with_ble_defaults_and_static_fields_preserved() {
        let dir = std::env::temp_dir().join(format!(
            "carlink-settings-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), br#"{"output":"carlink","carlinkHeadUnit":"192.168.43.1","carlinkLocal":"192.168.43.2","carlinkPin":"1234","carlinkPorts":{"video":1234}}"#).unwrap();
        let settings = Settings::load(&dir).unwrap();
        assert_eq!(settings.carlink_link, "ble");
        assert_eq!(settings.carlink_address, None);
        assert_eq!(settings.carlink_pin.as_deref(), Some("1234"));
        assert_eq!(settings.carlink_ports["video"], 1234);
        settings.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).unwrap(), settings);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn validates_carlink_link_mac_and_static_addresses() {
        let mut settings = Settings {
            output: "both".into(),
            ..Default::default()
        };
        for link in ["", "BLE", "unknown"] {
            settings.carlink_link = link.into();
            assert!(settings.validate().is_err());
        }
        settings.carlink_link = "ble".into();
        for address in ["", "AA:BB", "GG:BB:CC:DD:EE:FF", "AA:BB:CC:DD:EE:FF\n"] {
            settings.carlink_address = Some(address.into());
            assert!(settings.validate().is_err());
        }
        settings.carlink_address = Some("aA:bB:00:11:22:fF".into());
        settings.carlink_local = "unused".into();
        assert!(settings.validate().is_ok());
        settings.carlink_link = "static".into();
        assert!(settings.validate().is_err());
        settings.carlink_local = "192.168.43.2".into();
        assert!(settings.validate().is_ok());
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::*;
    #[test]
    fn plugins_default_off_and_persist_without_ui() {
        assert!(!Settings::default().plugins);
        assert!(!serde_json::from_str::<Settings>("{}").unwrap().plugins);
        let dir = std::env::temp_dir().join(format!(
            "plugins-settings-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let enabled = Settings {
            plugins: true,
            plugins_enabled: vec!["org.openxplay.second".into(), "org.openxplay.first".into()],
            ..Default::default()
        };
        for legacy in [false, true] {
            std::fs::write(
                dir.join("settings.json"),
                format!(r#"{{"plugins":{legacy}}}"#),
            )
            .unwrap();
            let migrated = Settings::load(&dir).unwrap();
            assert_eq!(migrated.plugins, legacy);
            assert!(migrated.plugins_enabled.is_empty());
        }
        enabled.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).unwrap(), enabled);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
