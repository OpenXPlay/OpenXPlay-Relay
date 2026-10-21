// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_PATH: &str = "/etc/openxplay/host.json";
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SecureConnectionsPolicy {
    #[default]
    PairingOff,
    Untouched,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HostProfile {
    pub model: Option<String>,
    pub wifi_interface: String,
    pub hotspot_channel: u8,
    pub bluetooth_adapter: Option<String>,
    pub hotspot_down_gap_seconds: u64,
    pub secure_connections: SecureConnectionsPolicy,
    pub bluetooth_guard_status: PathBuf,
}
impl Default for HostProfile {
    fn default() -> Self {
        Self {
            model: None,
            wifi_interface: "wlan0".into(),
            hotspot_channel: 36,
            bluetooth_adapter: None,
            hotspot_down_gap_seconds: crate::hotspot_lifetime::AP_DOWN_GAP.as_secs(),
            secure_connections: SecureConnectionsPolicy::PairingOff,
            bluetooth_guard_status: crate::bluetooth_guard::DEFAULT_STATUS_PATH.into(),
        }
    }
}
impl HostProfile {
    pub fn settings_defaults(&self) -> crate::settings::Settings {
        crate::settings::Settings {
            wifi_interface: self.wifi_interface.clone(),
            hotspot_channel: self.hotspot_channel,
            ..Default::default()
        }
    }
    fn parse(bytes: &[u8]) -> Result<(Self, Vec<String>), String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let object = value.as_object().ok_or("expected an object")?;
        let keys = [
            "model",
            "wifiInterface",
            "hotspotChannel",
            "bluetoothAdapter",
            "hotspotDownGapSeconds",
            "secureConnections",
            "bluetoothGuardStatus",
        ];
        let unknown = object
            .keys()
            .filter(|k| !keys.contains(&k.as_str()))
            .cloned()
            .collect();
        // Optional means omitted; null is not a value of any profile field.
        if object
            .iter()
            .any(|(k, v)| keys.contains(&k.as_str()) && v.is_null())
        {
            return Err("profile fields cannot be null".into());
        }
        let p: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        p.settings_defaults()
            .validate()
            .map_err(|e| e.to_string())?;
        if p.model.as_ref().is_some_and(|s| {
            !(1..=64).contains(&s.chars().count()) || s.chars().any(char::is_control)
        }) || p.bluetooth_adapter.as_ref().is_some_and(|s| {
            adapter_name_index(s).is_none() && !crate::linux_bluetooth::valid_mac(s)
        }) || p.hotspot_down_gap_seconds > 60
            || !p.bluetooth_guard_status.is_absolute()
        {
            return Err("invalid host profile value".into());
        }
        Ok((p, unknown))
    }
}
pub fn adapter_name_index(name: &str) -> Option<u16> {
    if !name.get(..3)?.eq_ignore_ascii_case("hci") {
        return None;
    }
    let n = name.get(3..)?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok()
}
#[derive(Clone, Debug)]
pub struct LoadedHostProfile {
    pub path: PathBuf,
    pub profile: HostProfile,
    pub loaded: bool,
    pub error: Option<String>,
    pub unknown: Vec<String>,
}
impl LoadedHostProfile {
    pub fn load(path: &Path) -> Self {
        let mut result = Self {
            path: path.into(),
            profile: HostProfile::default(),
            loaded: false,
            error: None,
            unknown: Vec::new(),
        };
        match std::fs::read(path) {
            Ok(bytes) => match HostProfile::parse(&bytes) {
                Ok((profile, unknown)) => {
                    result.profile = profile;
                    result.unknown = unknown;
                    result.loaded = true;
                }
                Err(e) => result.error = Some(e),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => result.error = Some(e.to_string()),
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_profile_and_unknown_fields() {
        let (p, unknown) = HostProfile::parse(br#"{"model":"Phicomm N1","wifiInterface":"wlan1","hotspotChannel":6,"bluetoothAdapter":"AA:BB:CC:DD:EE:FF","hotspotDownGapSeconds":0,"secureConnections":"untouched","bluetoothGuardStatus":"/run/n1.json","future":42}"#).unwrap();
        assert_eq!(p.model.as_deref(), Some("Phicomm N1"));
        assert_eq!(p.wifi_interface, "wlan1");
        assert_eq!(p.hotspot_channel, 6);
        assert_eq!(p.bluetooth_adapter.as_deref(), Some("AA:BB:CC:DD:EE:FF"));
        assert_eq!(p.hotspot_down_gap_seconds, 0);
        assert_eq!(p.secure_connections, SecureConnectionsPolicy::Untouched);
        assert_eq!(p.bluetooth_guard_status, Path::new("/run/n1.json"));
        assert_eq!(unknown, ["future"]);
        assert_eq!(
            HostProfile::parse(br#"{"hotspotDownGapSeconds":60}"#)
                .unwrap()
                .0
                .hotspot_down_gap_seconds,
            60
        );
    }
    #[test]
    fn invalid_profiles_are_ignored_as_a_whole() {
        let dir = std::env::temp_dir().join(format!(
            "host-profile-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("host.json");
        let absent = LoadedHostProfile::load(&path);
        assert!(!absent.loaded && absent.error.is_none());
        assert_eq!(absent.profile.wifi_interface, "wlan0");
        assert_eq!(absent.profile.hotspot_channel, 36);
        assert_eq!(absent.profile.bluetooth_adapter, None);
        assert_eq!(absent.profile.hotspot_down_gap_seconds, 15);
        assert_eq!(
            absent.profile.secure_connections,
            SecureConnectionsPolicy::PairingOff
        );
        assert_eq!(
            absent.profile.bluetooth_guard_status,
            Path::new(crate::bluetooth_guard::DEFAULT_STATUS_PATH)
        );
        for invalid in [
            "{",
            "[]",
            "null",
            r#"{"model":""}"#,
            r#"{"model":"\n"}"#,
            r#"{"model":4}"#,
            r#"{"wifiInterface":"../wlan0"}"#,
            r#"{"wifiInterface":null}"#,
            r#"{"hotspotChannel":0}"#,
            r#"{"hotspotChannel":15}"#,
            r#"{"hotspotChannel":178}"#,
            r#"{"hotspotChannel":"36"}"#,
            r#"{"bluetoothAdapter":"hciX"}"#,
            r#"{"hotspotDownGapSeconds":61}"#,
            r#"{"hotspotDownGapSeconds":-1}"#,
            r#"{"hotspotDownGapSeconds":1.5}"#,
            r#"{"secureConnections":"off"}"#,
            r#"{"bluetoothGuardStatus":"relative"}"#,
        ] {
            let value = if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(invalid) {
                if let Some(o) = value.as_object_mut() {
                    o.insert("wifiInterface".into(), "wlan9".into());
                }
                // Preserve invalid Wi-Fi cases too.
                if invalid.contains("wifiInterface") {
                    invalid.to_string()
                } else {
                    value.to_string()
                }
            } else {
                invalid.to_string()
            };
            std::fs::write(&path, value).unwrap();
            let p = LoadedHostProfile::load(&path);
            assert!(!p.loaded && p.error.is_some(), "{invalid}");
            assert_eq!(p.profile.wifi_interface, "wlan0");
        }
        assert!(
            HostProfile::parse(format!("{{\"model\":\"{}\"}}", "x".repeat(65)).as_bytes()).is_err()
        );
        assert!(LoadedHostProfile::load(&dir).error.is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn settings_and_cli_precedence_and_partial_settings_fallback() {
        let dir = std::env::temp_dir().join(format!(
            "host-precedence-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("host.json");
        std::fs::write(&path, br#"{"wifiInterface":"wlan1","hotspotChannel":6,"bluetoothGuardStatus":"/run/profile.json"}"#).unwrap();
        let args = vec![
            "--state-dir".into(),
            dir.to_string_lossy().into_owned(),
            "--host-profile".into(),
            path.to_string_lossy().into_owned(),
        ];
        let options = crate::Options::parse(&args).unwrap();
        assert_eq!(options.wifi_interface, "wlan1");
        assert_eq!(options.wifi_channel, 6);
        assert_eq!(
            options.bluetooth_guard_status,
            Path::new("/run/profile.json")
        );
        std::fs::write(dir.join("settings.json"), br#"{"hotspotChannel":11}"#).unwrap();
        let options = crate::Options::parse(&args).unwrap();
        assert_eq!(options.wifi_interface, "wlan1");
        assert_eq!(options.wifi_channel, 11);
        std::fs::write(
            dir.join("settings.json"),
            br#"{"wifiInterface":"wlan2","hotspotChannel":11}"#,
        )
        .unwrap();
        let options = crate::Options::parse(&args).unwrap();
        assert_eq!(options.wifi_interface, "wlan2");
        let mut args = args;
        args.extend([
            "--wifi-interface".into(),
            "wlan3".into(),
            "--wifi-channel".into(),
            "44".into(),
            "--bluetooth-guard-status".into(),
            "/run/cli.json".into(),
        ]);
        let options = crate::Options::parse(&args).unwrap();
        assert_eq!(options.wifi_interface, "wlan3");
        assert_eq!(options.wifi_channel, 44);
        assert_eq!(options.bluetooth_guard_status, Path::new("/run/cli.json"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
