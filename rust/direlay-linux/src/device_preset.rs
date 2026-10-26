// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21

use crate::wifi_capabilities::{PhyCapabilities, WifiSummary};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub struct DevicePreset {
    pub id: &'static str,
    pub label: &'static str,
    compatible: Option<&'static str>,
    band: Option<&'static str>,
    pub network_mode: &'static str,
    pub hotspot_down_gap_seconds: u64,
}
pub const PRESETS: [DevicePreset; 3] = [
    DevicePreset {
        id: "msm8916",
        label: "Qualcomm Snapdragon 410 (MSM8916)",
        compatible: Some("qcom,msm8916"),
        band: Some("2.4"),
        network_mode: "device-hotspot",
        hotspot_down_gap_seconds: 15,
    },
    DevicePreset {
        id: "phicomm-n1",
        label: "Phicomm N1",
        compatible: Some("phicomm,n1"),
        band: Some("5"),
        network_mode: "device-hotspot",
        hotspot_down_gap_seconds: crate::hotspot_lifetime::AP_DOWN_GAP.as_secs(),
    },
    DevicePreset {
        id: "generic",
        label: "Generic Linux device",
        compatible: None,
        band: None,
        network_mode: "device-hotspot",
        hotspot_down_gap_seconds: crate::hotspot_lifetime::AP_DOWN_GAP.as_secs(),
    },
];
pub fn preset(id: &str) -> &'static DevicePreset {
    PRESETS.iter().find(|p| p.id == id).unwrap_or(&PRESETS[2])
}
pub fn valid_selection(id: &str) -> bool {
    id == "auto" || PRESETS.iter().any(|p| p.id == id)
}
pub fn detect(root: &Path) -> &'static str {
    let Ok(bytes) = std::fs::read(root.join("proc/device-tree/compatible")) else {
        return "generic";
    };
    PRESETS
        .iter()
        .find(|p| {
            p.compatible
                .is_some_and(|name| bytes.split(|b| *b == 0).any(|s| s == name.as_bytes()))
        })
        .map_or("generic", |p| p.id)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresetSelection {
    pub id: String,
    pub label: String,
    pub source: String,
    pub detected: String,
}
pub fn select(root: &Path, requested: &str, source: &str) -> PresetSelection {
    let detected = detect(root);
    let (id, source) = if requested == "auto" {
        (
            detected,
            if detected == "generic" {
                "default"
            } else {
                "detected"
            },
        )
    } else {
        (requested, source)
    };
    PresetSelection {
        id: id.into(),
        label: preset(id).label.into(),
        source: source.into(),
        detected: detected.into(),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HotspotResolution {
    pub band: String,
    pub auto_band: String,
    pub channel: u8,
    pub configured_band: String,
    pub configured_channel: u8,
    pub reason: Option<String>,
}
impl HotspotResolution {
    pub fn change_message(&self) -> Option<String> {
        let reason = self.reason.as_ref()?;
        let configured = if self.configured_band == "auto" {
            "Automatic".into()
        } else {
            format!("{} GHz", self.configured_band)
        };
        Some(format!(
            "Wi-Fi hotspot: configured {configured} channel {} -> {} GHz channel {} ({reason})",
            self.configured_channel, self.band, self.channel
        ))
    }
}
/// Resolves preferences without starting an AP or probing the host.
pub fn resolve_hotspot(
    configured_band: &str,
    configured_channel: u8,
    preset_id: &str,
    phy: Option<&PhyCapabilities>,
) -> HotspotResolution {
    let summary = WifiSummary::from_phy(phy);
    let automatic = preset(preset_id).band.unwrap_or_else(|| {
        // Unprobed hardware keeps the historical 5 GHz/channel 36 attempt and start-failure fallback.
        if summary.ap5 == Some(false) {
            "2.4"
        } else {
            "5"
        }
    });
    let auto_band = if automatic == "5" && summary.ap5 == Some(false) {
        "2.4"
    } else {
        automatic
    };
    let mut band = if configured_band == "auto" {
        automatic
    } else {
        configured_band
    };
    let mut reason = None;
    if band == "5" && summary.ap5 == Some(false) {
        band = "2.4";
        reason = Some("no-5ghz-ap".into());
    }
    let in_band = match band {
        "5" => (32..=177).contains(&configured_channel),
        _ => (1..=14).contains(&configured_channel),
    };
    let capable = phy.is_none_or(|p| p.ap_channel(configured_channel));
    let channel = if in_band && capable {
        configured_channel
    } else {
        if configured_channel != 0 && reason.is_none() {
            reason = Some(
                if in_band {
                    "channel-not-ap-capable"
                } else {
                    "channel-not-in-band"
                }
                .into(),
            );
        }
        if band == "2.4" {
            6
        } else {
            summary
                .ap5_channels
                .as_ref()
                .and_then(|c| c.first())
                .copied()
                .unwrap_or(36) as u8
        }
    };
    HotspotResolution {
        band: band.into(),
        auto_band: auto_band.into(),
        channel,
        configured_band: configured_band.into(),
        configured_channel,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{settings::Settings, wifi_capabilities::parse_phy, Options};
    fn root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "device-preset-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(root.join("proc/device-tree")).unwrap();
        root
    }
    #[test]
    fn detects_compatible_strings_missing_and_unreadable_trees() {
        let root = root();
        assert_eq!(detect(&root), "generic");
        let path = root.join("proc/device-tree/compatible");
        for (bytes, expected) in [
            (b"handsome,openstick\0qcom,msm8916\0".as_slice(), "msm8916"),
            (
                b"phicomm,n1\0amlogic,s905d\0amlogic,meson-gxl\0",
                "phicomm-n1",
            ),
            (b"qcom,msm89160\0other,device\0", "generic"),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(detect(&root), expected);
            assert_eq!(
                select(&root, "auto", "default").source,
                if expected == "generic" {
                    "default"
                } else {
                    "detected"
                }
            );
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(detect(&root), "generic");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn selection_precedence_and_explicit_profile_gap_and_network_mode() {
        let root = root();
        std::fs::write(root.join("proc/device-tree/compatible"), b"phicomm,n1\0").unwrap();
        let host = root.join("host.json");
        let mut args = vec![
            "--state-dir".into(),
            root.to_str().unwrap().into(),
            "--host-profile".into(),
            host.to_str().unwrap().into(),
        ];
        let selection = |args: &[String]| Options::parse(args).unwrap().device_preset(&root);
        let detected = selection(&args);
        assert_eq!(
            (
                detected.id.as_str(),
                detected.source.as_str(),
                detected.detected.as_str()
            ),
            ("phicomm-n1", "detected", "phicomm-n1")
        );
        std::fs::write(
            &host,
            br#"{"devicePreset":"msm8916","hotspotDownGapSeconds":0}"#,
        )
        .unwrap();
        let options = Options::parse(&args).unwrap();
        assert_eq!(options.settings.network_mode, "device-hotspot");
        assert_eq!(options.hotspot_down_gap_seconds(), 0);
        let selected = selection(&args);
        assert_eq!(
            (selected.id.as_str(), selected.source.as_str()),
            ("msm8916", "host-profile")
        );
        std::fs::write(
            root.join("settings.json"),
            br#"{"networkMode":"shared-wifi","hotspotChannel":36}"#,
        )
        .unwrap();
        assert_eq!(selection(&args).source, "host-profile");
        assert_eq!(
            Options::parse(&args).unwrap().settings.network_mode,
            "shared-wifi"
        );
        std::fs::write(
            root.join("settings.json"),
            br#"{"devicePreset":"generic","networkMode":"shared-wifi","hotspotChannel":36}"#,
        )
        .unwrap();
        let selected = selection(&args);
        assert_eq!(
            (
                selected.id.as_str(),
                selected.source.as_str(),
                selected.detected.as_str()
            ),
            ("generic", "settings", "phicomm-n1")
        );
        args.extend(["--device-preset".into(), "phicomm-n1".into()]);
        let selected = selection(&args);
        assert_eq!(
            (selected.id.as_str(), selected.source.as_str()),
            ("phicomm-n1", "command-line")
        );
        assert_eq!(Options::parse(&args).unwrap().settings.hotspot_channel, 36);
        *args.last_mut().unwrap() = "auto".into();
        assert_eq!(selection(&args).source, "detected");
        *args.last_mut().unwrap() = "unknown".into();
        assert!(Options::parse(&args).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn band_channel_resolution_table() {
        let n1 = parse_phy(include_str!("../tests/fixtures/iw-phy-n1.txt"));
        let ufi = parse_phy(include_str!("../tests/fixtures/iw-phy-ufi003.txt"));
        let other = parse_phy(" * 5745 MHz [149]\n * 2437 MHz [6]");
        for (band, channel, preset, phy, expected_band, expected_channel, reason) in [
            (
                "auto",
                36,
                "msm8916",
                Some(&ufi),
                "2.4",
                6,
                Some("channel-not-in-band"),
            ),
            ("auto", 0, "phicomm-n1", Some(&n1), "5", 36, None),
            ("2.4", 0, "phicomm-n1", Some(&n1), "2.4", 6, None),
            (
                "2.4",
                36,
                "phicomm-n1",
                Some(&n1),
                "2.4",
                6,
                Some("channel-not-in-band"),
            ),
            ("5", 36, "msm8916", Some(&ufi), "2.4", 6, Some("no-5ghz-ap")),
            ("auto", 0, "generic", Some(&ufi), "2.4", 6, None),
            ("auto", 0, "generic", Some(&n1), "5", 36, None),
            ("auto", 44, "generic", Some(&n1), "5", 44, None),
            (
                "5",
                52,
                "generic",
                Some(&n1),
                "5",
                36,
                Some("channel-not-ap-capable"),
            ),
            (
                "2.4",
                12,
                "generic",
                Some(&ufi),
                "2.4",
                6,
                Some("channel-not-ap-capable"),
            ),
            (
                "2.4",
                14,
                "generic",
                Some(&n1),
                "2.4",
                6,
                Some("channel-not-ap-capable"),
            ),
            ("2.4", 11, "generic", Some(&ufi), "2.4", 11, None),
            ("auto", 0, "generic", Some(&other), "5", 149, None),
            (
                "5",
                36,
                "generic",
                Some(&other),
                "5",
                149,
                Some("channel-not-ap-capable"),
            ),
            ("auto", 0, "generic", None, "5", 36, None),
            ("auto", 44, "generic", None, "5", 44, None),
            ("auto", 0, "msm8916", None, "2.4", 6, None),
            ("2.4", 11, "phicomm-n1", None, "2.4", 11, None),
            ("5", 0, "msm8916", None, "5", 36, None),
        ] {
            let resolved = resolve_hotspot(band, channel, preset, phy);
            assert_eq!(
                (
                    resolved.band.as_str(),
                    resolved.channel,
                    resolved.reason.as_deref()
                ),
                (expected_band, expected_channel, reason),
                "{band}/{channel}/{preset}"
            );
            assert_eq!(resolved.configured_band, band);
            assert_eq!(resolved.configured_channel, channel);
            assert_eq!(
                resolved.auto_band,
                resolve_hotspot("auto", channel, preset, phy).band
            );
        }
    }
    #[test]
    fn validates_band_preset_and_automatic_channel_in_settings_files() {
        let root = root();
        for value in [
            serde_json::json!({"hotspotBand":"other"}),
            serde_json::json!({"hotspotBand":null}),
            serde_json::json!({"devicePreset":"unknown"}),
            serde_json::json!({"devicePreset":null}),
            serde_json::json!({"hotspotChannel":15}),
            serde_json::json!({"hotspotChannel":178}),
            serde_json::json!({"hotspotChannel":-1}),
            serde_json::json!({"hotspotChannel":"0"}),
        ] {
            std::fs::write(root.join("settings.json"), value.to_string()).unwrap();
            assert!(Settings::load(&root).is_err(), "{value}");
        }
        for preset in ["auto", "msm8916", "phicomm-n1", "generic"] {
            for band in ["auto", "2.4", "5"] {
                for channel in [0, 1, 14, 32, 177] {
                    let value = serde_json::json!({"devicePreset":preset,"hotspotBand":band,"hotspotChannel":channel});
                    std::fs::write(root.join("settings.json"), value.to_string()).unwrap();
                    let settings = Settings::load(&root).unwrap();
                    settings.save(&root).unwrap();
                    assert_eq!(settings, Settings::load(&root).unwrap());
                }
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
