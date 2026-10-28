// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::{platform::private_state_file, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
/// Entries stay strict: a new field here would make older relays reject settings.json.
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
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub plugins: bool,
    pub plugins_enabled: Vec<String>,
    pub network_mode: String,
    pub network_roles: crate::network_roles::NetworkRoles,
    pub admin_hotspot_password: Option<String>,
    pub wifi_country: String,
    pub device_preset: String,
    pub shared_wifi_networks: Vec<SharedWifiNetwork>,
    pub wifi_interface: String,
    pub carlink_interface: String,
    pub hotspot_channel: u8,
    pub hotspot_band: String,
    pub iphone: Option<String>,
    pub rfcomm_channel: Option<u8>,
    pub receiver_port: u16,
    pub require_pairing_code: bool,
    pub output: String,
    pub tesla_browser: crate::tesla_address::Mode,
    pub tesla_virtual_address: String,
    pub tesla_public_address: String,
    pub tesla_mirror_address: String,
    pub tesla_android_address: String,
    pub carlink_name: String,
    pub carlink_model: String,
    pub carlink_auto_reconnect: bool,
    pub carlink_reconnect_on_resize: bool,
    pub carlink_hevc: String,
    pub carlink_trace: String,
    pub vehicle_information: bool,
    pub frame_rate: String,
    pub carlink_picture_size: String,
    pub carlink_picture_sizes: Vec<crate::display::Size>,
    pub carlink_picture_default: Option<crate::display::Size>,
    pub car_brand: String,
    pub car_brand_text: String,
    pub car_model: String,
    pub car_model_text: String,
    pub car_year: String,
    pub experimental_active: bool,
    pub experimental_send_bluetooth_address: bool,
    pub experimental_bluetooth_address: String,
    #[serde(alias = "autoStartWhenReady")]
    pub auto_start: bool,
    #[serde(alias = "autoStartOnBoot")]
    pub boot_start: bool,
    pub video_stats: String,
    #[serde(rename = "carPlayViewAreas")]
    pub car_play_view_areas: bool,
    pub car_bluetooth_audio_defaulted: bool,
    pub carlink_link: String,
    pub carlink_address: Option<String>,
    pub carlink_head_unit: String,
    pub carlink_local: String,
    pub carlink_pin: Option<String>,
    pub carlink_ports: BTreeMap<String, u16>,
    /// Offer the iPhone no CarPlay audio, so music, navigation and calls stay on its Bluetooth link with the car.
    pub car_bluetooth_audio: bool,
    /// Keys this version does not know, written by a newer relay. Kept and saved back unchanged, so
    /// the relay still starts after a rollback and the newer relay finds its values again.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            plugins: false,
            plugins_enabled: Vec::new(),
            network_mode: "device-hotspot".into(),
            network_roles: Default::default(),
            admin_hotspot_password: None,
            wifi_country: "system".into(),
            device_preset: "auto".into(),
            shared_wifi_networks: Vec::new(),
            wifi_interface: "wlan0".into(),
            carlink_interface: String::new(),
            hotspot_channel: 0,
            hotspot_band: "auto".into(),
            iphone: None,
            rfcomm_channel: None,
            receiver_port: 8443,
            require_pairing_code: false,
            output: "web".into(),
            tesla_browser: Default::default(),
            tesla_virtual_address: "169.254.88.1".into(),
            tesla_public_address: "169.254.88.1".into(),
            tesla_mirror_address: "100.88.0.1".into(),
            tesla_android_address: "3.3.3.1".into(),
            carlink_name: crate::branding::DEVICE_NAME.into(),
            carlink_model: crate::branding::MODEL.into(),
            carlink_auto_reconnect: true,
            carlink_reconnect_on_resize: false,
            carlink_hevc: "auto".into(),
            carlink_trace: "off".into(),
            vehicle_information: false,
            frame_rate: "auto".into(),
            carlink_picture_size: "window".into(),
            carlink_picture_sizes: vec![],
            carlink_picture_default: None,
            car_brand: String::new(),
            car_brand_text: String::new(),
            car_model: String::new(),
            car_model_text: String::new(),
            car_year: String::new(),
            experimental_active: true,
            experimental_send_bluetooth_address: false,
            experimental_bluetooth_address: String::new(),
            auto_start: true,
            boot_start: false,
            video_stats: "off".into(),
            car_play_view_areas: true,
            car_bluetooth_audio_defaulted: true,
            carlink_link: "ble".into(),
            carlink_address: None,
            carlink_head_unit: "192.168.43.1".into(),
            carlink_local: "192.168.43.2".into(),
            carlink_pin: None,
            carlink_ports: BTreeMap::new(),
            car_bluetooth_audio: true,
            unknown: BTreeMap::new(),
        }
    }
}
fn persist_loaded_settings(dir: &Path, bytes: &[u8]) {
    // Loading must also work with a read-only state directory.
    let result = if std::fs::metadata(dir).is_ok_and(|m| m.permissions().readonly()) {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "State directory is read-only",
        ))
    } else {
        private_state_file(&dir.join("settings.json"), bytes)
    };
    if let Err(e) = result {
        eprintln!("Could not persist loaded settings: {e}");
    }
}
impl Settings {
    pub fn tesla_address(&self) -> Option<&str> {
        use crate::tesla_address::Mode;
        match self.tesla_browser {
            Mode::Off => None,
            Mode::VirtualName => Some(&self.tesla_virtual_address),
            Mode::PublicAddress => Some(&self.tesla_public_address),
            Mode::Teslamirror => Some(&self.tesla_mirror_address),
            Mode::TeslaAndroid => Some(&self.tesla_android_address),
        }
    }
    pub fn car(&self) -> crate::car_models::CarSelection {
        crate::car_models::CarSelection {
            brand: self.car_brand.clone(),
            brand_text: self.car_brand_text.clone(),
            model: self.car_model.clone(),
            model_text: self.car_model_text.clone(),
            year: self.car_year.clone(),
        }
        .normalized()
    }
    pub fn normalize_car(&mut self) {
        let c = self.car();
        self.car_brand = c.brand;
        self.car_brand_text = c.brand_text;
        self.car_model = c.model;
        self.car_model_text = c.model_text;
        self.car_year = c.year;
    }
    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_with_defaults(dir, &Self::default())
    }
    pub fn load_with_defaults(dir: &Path, defaults: &Self) -> Result<Self> {
        match std::fs::read(dir.join("settings.json")) {
            Ok(b) => {
                let mut value: serde_json::Value = serde_json::from_slice(&b)?;
                let migrate_audio = value
                    .as_object()
                    .is_some_and(|o| !o.contains_key("carBluetoothAudioDefaulted"));
                if migrate_audio {
                    value["carBluetoothAudio"] = true.into();
                    value["carBluetoothAudioDefaulted"] = true.into();
                }
                let migrate_startup =
                    value.get("bootStart").is_none() && value.get("autoStartOnBoot").is_none();
                if migrate_startup {
                    value["bootStart"] = value
                        .get("autoStart")
                        .cloned()
                        .unwrap_or(defaults.boot_start.into());
                }
                // Persist only migrations, leaving absent profile-derived settings absent.
                let migrated = (migrate_audio || migrate_startup).then(|| value.clone());
                if let Some(object) = value.as_object_mut() {
                    object
                        .entry("devicePreset")
                        .or_insert_with(|| defaults.device_preset.clone().into());
                    object
                        .entry("networkMode")
                        .or_insert_with(|| defaults.network_mode.clone().into());
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
                if let Some(object) = value.as_object_mut() {
                    object
                        .entry("wifiInterface")
                        .or_insert_with(|| defaults.wifi_interface.clone().into());
                    object
                        .entry("carlinkInterface")
                        .or_insert_with(|| defaults.carlink_interface.clone().into());
                    object
                        .entry("hotspotChannel")
                        .or_insert_with(|| defaults.hotspot_channel.into());
                }
                let mut s: Self = serde_json::from_value(value)?;
                s.normalize_car();
                s.validate()?;
                if let Some(migrated) = migrated {
                    persist_loaded_settings(dir, &serde_json::to_vec_pretty(&migrated)?);
                }
                Ok(s)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut s = defaults.clone();
                s.car_bluetooth_audio = true;
                s.car_bluetooth_audio_defaulted = true;
                persist_loaded_settings(
                    dir,
                    br#"{"carBluetoothAudio":true,"carBluetoothAudioDefaulted":true}"#,
                );
                Ok(s)
            }
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
        if crate::carplay_policy::frame_rate_setting(&self.frame_rate).is_none() {
            return Err(invalid("Invalid frameRate"));
        }
        if !["window", "mode"].contains(&self.carlink_picture_size.as_str()) {
            return Err(invalid("Invalid carlinkPictureSize"));
        }
        crate::carlink_policy::validate_pictures(
            &self.carlink_picture_sizes,
            self.carlink_picture_default,
        )?;
        if self.experimental_active
            && self.experimental_send_bluetooth_address
            && !self.experimental_bluetooth_address.is_empty()
            && crate::carlink_policy::bluetooth_address(&self.experimental_bluetooth_address)
                .is_none()
        {
            return Err(invalid("Invalid experimentalBluetoothAddress"));
        }
        for text in [
            &self.car_brand,
            &self.car_brand_text,
            &self.car_model,
            &self.car_model_text,
            &self.car_year,
        ] {
            if text.chars().count() > 128 || text.chars().any(char::is_control) {
                return Err(invalid("Invalid car selection"));
            }
        }
        if self
            .tesla_address()
            .is_some_and(|a| !crate::tesla_address::valid(a))
        {
            return Err(invalid("error.teslaAddress"));
        }
        self.network_roles.validate()?;
        if !crate::network_roles::valid_country(&self.wifi_country) {
            return Err(invalid("Invalid wifiCountry"));
        }
        if self
            .admin_hotspot_password
            .as_deref()
            .is_some_and(|p| !crate::network_roles::valid_password(p))
        {
            return Err(invalid("Invalid adminHotspotPassword"));
        }
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
        if !self
            .carlink_interface
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(invalid("Invalid Carlink interface"));
        }
        if !crate::device_preset::valid_selection(&self.device_preset) {
            return Err(invalid("Invalid devicePreset"));
        }
        if !["auto", "2.4", "5"].contains(&self.hotspot_band.as_str()) {
            return Err(invalid("Invalid hotspotBand"));
        }
        if self.hotspot_channel != 0
            && !(1..=14).contains(&self.hotspot_channel)
            && !(32..=177).contains(&self.hotspot_channel)
        {
            return Err(invalid(
                "Preferred Wi-Fi channel must be 0 (automatic), in 1..14 or 32..177",
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
        for (key, value) in [
            ("carlinkName", &self.carlink_name),
            ("carlinkModel", &self.carlink_model),
        ] {
            if !(1..=128).contains(&value.len()) || value.chars().any(char::is_control) {
                return Err(invalid(key));
            }
        }
        if !["auto", "off", "signal"].contains(&self.carlink_hevc.as_str()) {
            return Err(invalid("Invalid carlinkHevc"));
        }
        if !["off", "on"].contains(&self.carlink_trace.as_str()) {
            return Err(invalid("Invalid carlinkTrace"));
        }
        if !["off", "5", "15"].contains(&self.video_stats.as_str()) {
            return Err(invalid("Invalid videoStats"));
        }
        if self.receiver_port == 0 || self.receiver_port == 6975 {
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
    fn legacy_auto_start_migrates_boot_preference_without_coupling_future_edits() {
        let dir = std::env::temp_dir().join(format!(
            "w11-settings-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), br#"{"autoStart":false}"#).unwrap();
        let mut settings = Settings::load(&dir).unwrap();
        assert!(!settings.auto_start && !settings.boot_start);
        settings.boot_start = true;
        settings.save(&dir).unwrap();
        let settings = Settings::load(&dir).unwrap();
        assert!(!settings.auto_start && settings.boot_start);
        let aliases: Settings = serde_json::from_value(
            serde_json::json!({"autoStartOnBoot":false,"autoStartWhenReady":true}),
        )
        .unwrap();
        assert!(!aliases.boot_start && aliases.auto_start);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn linux_car_settings_defaults_validation_and_round_trip() {
        let defaults = Settings::default();
        let json = serde_json::to_value(&defaults).unwrap();
        for (key, value) in [
            (
                "carlinkName",
                serde_json::json!(crate::branding::DEVICE_NAME),
            ),
            ("carlinkModel", serde_json::json!(crate::branding::MODEL)),
            ("carlinkAutoReconnect", serde_json::json!(true)),
            ("carlinkReconnectOnResize", serde_json::json!(false)),
            ("carlinkHevc", serde_json::json!("auto")),
            ("carlinkTrace", serde_json::json!("off")),
            ("autoStart", serde_json::json!(true)),
            ("bootStart", serde_json::json!(false)),
            ("videoStats", serde_json::json!("off")),
            ("carPlayViewAreas", serde_json::json!(true)),
            ("carBluetoothAudio", serde_json::json!(true)),
            ("carBluetoothAudioDefaulted", serde_json::json!(true)),
            ("receiverPort", serde_json::json!(8443)),
        ] {
            assert_eq!(json[key], value, "{key}");
        }
        assert_eq!(serde_json::from_str::<Settings>("{}").unwrap(), defaults);
        for key in ["carlinkName", "carlinkModel"] {
            for value in [
                "".to_owned(),
                "x".repeat(129),
                "é".repeat(65),
                "bad\nname".into(),
                "bad\u{7f}name".into(),
            ] {
                let mut v = json.clone();
                v[key] = value.into();
                assert!(
                    serde_json::from_value::<Settings>(v)
                        .unwrap()
                        .validate()
                        .is_err(),
                    "{key}"
                );
            }
            let mut v = json.clone();
            v[key] = "é".repeat(64).into();
            assert!(serde_json::from_value::<Settings>(v)
                .unwrap()
                .validate()
                .is_ok());
        }
        for key in [
            "carlinkAutoReconnect",
            "carlinkReconnectOnResize",
            "autoStart",
            "carPlayViewAreas",
            "carBluetoothAudioDefaulted",
        ] {
            let mut v = json.clone();
            v[key] = "false".into();
            assert!(serde_json::from_value::<Settings>(v).is_err());
        }
        for (key, values) in [
            ("carlinkHevc", vec!["auto", "off", "signal"]),
            ("carlinkTrace", vec!["off", "on"]),
            ("videoStats", vec!["off", "5", "15"]),
        ] {
            for value in values.into_iter().chain(["", "ON", "30"]) {
                let mut v = json.clone();
                v[key] = value.into();
                let valid = if key == "carlinkHevc" {
                    ["auto", "off", "signal"].contains(&value)
                } else if key == "carlinkTrace" {
                    ["off", "on"].contains(&value)
                } else {
                    ["off", "5", "15"].contains(&value)
                };
                assert_eq!(
                    serde_json::from_value::<Settings>(v)
                        .unwrap()
                        .validate()
                        .is_ok(),
                    valid
                );
            }
        }
        for port in [0, 80, 1023, 1024, 6975, 8443, 65535] {
            let s = Settings {
                receiver_port: port,
                ..defaults.clone()
            };
            assert_eq!(s.validate().is_ok(), port != 0 && port != 6975);
        }
    }
    #[test]
    fn trace_setting_persists_and_rejects_non_strings() {
        let dir = std::env::temp_dir().join(format!(
            "trace-settings-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let settings = Settings {
            carlink_trace: "on".into(),
            ..Default::default()
        };
        settings.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).unwrap().carlink_trace, "on");
        assert!(
            serde_json::from_value::<Settings>(serde_json::json!({"carlinkTrace":true})).is_err()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn low_receiver_port_still_loads() {
        let dir = std::env::temp_dir().join(format!(
            "settings-low-port-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), br#"{"receiverPort":443}"#).unwrap();
        assert_eq!(Settings::load(&dir).unwrap().receiver_port, 443);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn read_only_state_keeps_migration_and_fresh_defaults_in_memory() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "settings-read-only-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let original = br#"{"carBluetoothAudio":false,"receiverPort":443}"#;
        std::fs::write(&path, original).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let migrated = Settings::load(&dir).unwrap();
        assert!(migrated.car_bluetooth_audio && migrated.car_bluetooth_audio_defaulted);
        assert_eq!(migrated.receiver_port, 443);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let fresh = Settings::load(&dir).unwrap();
        assert!(fresh.car_bluetooth_audio && fresh.car_bluetooth_audio_defaulted);
        assert!(!path.exists());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bluetooth_audio_migration_runs_once_and_fresh_defaults_are_true() {
        let dir = std::env::temp_dir().join(format!(
            "audio-default-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let fresh = Settings::load(&dir).unwrap();
        assert!(fresh.car_bluetooth_audio && fresh.car_bluetooth_audio_defaulted);
        let fresh_file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(
            fresh_file,
            serde_json::json!({"carBluetoothAudio":true,"carBluetoothAudioDefaulted":true})
        );
        std::fs::create_dir_all(&dir).unwrap();
        for marker in [None, Some(false), Some(true)] {
            let mut value = serde_json::json!({"carBluetoothAudio":false});
            if let Some(marker) = marker {
                value["carBluetoothAudioDefaulted"] = marker.into();
            }
            std::fs::write(
                dir.join("settings.json"),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
            let settings = Settings::load(&dir).unwrap();
            assert_eq!(settings.car_bluetooth_audio, marker.is_none());
            assert_eq!(
                settings.car_bluetooth_audio_defaulted,
                marker.unwrap_or(true)
            );
            let saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
            assert_eq!(saved["carBluetoothAudio"], marker.is_none());
            assert_eq!(saved["carBluetoothAudioDefaulted"], marker.unwrap_or(true));
            assert_eq!(Settings::load(&dir).unwrap(), settings);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn legacy_wifi_migrates_and_unknown_fields_survive_a_save() {
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
        // A newer relay's keys load, stay out of the known fields and are saved back unchanged.
        std::fs::write(
            dir.join("settings.json"),
            br#"{"wifiSsid":"Home","output":"both","futureFlag":true,"futureMap":{"a":[1,2]}}"#,
        )
        .unwrap();
        let mut settings = Settings::load(&dir).unwrap();
        assert_eq!(settings.output, "both");
        assert_eq!(settings.shared_wifi_networks.len(), 1);
        assert_eq!(
            settings.unknown.keys().collect::<Vec<_>>(),
            ["futureFlag", "futureMap"]
        );
        settings.output = "web".into();
        settings.save(&dir).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(json["output"], "web");
        assert_eq!(json["futureFlag"], true);
        assert_eq!(json["futureMap"], serde_json::json!({"a": [1, 2]}));
        assert_eq!(Settings::load(&dir).unwrap(), settings);
        // Known keys still have to be valid.
        std::fs::write(dir.join("settings.json"), br#"{"output":"radio"}"#).unwrap();
        assert!(Settings::load(&dir).is_err());
        std::fs::write(dir.join("settings.json"), br#"{"hotspotChannel":"six"}"#).unwrap();
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
        assert!(settings.carlink_interface.is_empty());
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
