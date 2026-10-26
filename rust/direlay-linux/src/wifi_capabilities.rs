// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub frequency: u32,
    pub channel: u16,
    pub disabled: bool,
    pub no_ir: bool,
    pub radar: bool,
}
impl Channel {
    pub fn two_ghz(&self) -> bool {
        (2400..=2500).contains(&self.frequency)
    }
    pub fn five_ghz(&self) -> bool {
        (4900..5900).contains(&self.frequency)
    }
    pub fn access_point(&self) -> bool {
        !self.disabled && !self.no_ir && !self.radar
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Combination {
    pub modes: Vec<String>,
    pub channels: Option<u16>,
    pub ap_station: bool,
    pub description: String,
}
impl Combination {
    /// Check group and total limits, including chips that allow only one AP.
    pub fn allows(&self, requested: &[&str]) -> bool {
        if number_after(&self.description, "total <=")
            .is_none_or(|n| usize::from(n) < requested.len())
        {
            return false;
        }
        let mut remaining = self.description.as_str();
        let mut covered = vec![false; requested.len()];
        while let Some((_, tail)) = remaining.split_once("#{") {
            let Some((names, tail)) = tail.split_once('}') else {
                return false;
            };
            let names: Vec<_> = names.split(',').map(str::trim).collect();
            let count = requested
                .iter()
                .enumerate()
                .filter(|(i, mode)| {
                    if names.contains(mode) {
                        covered[*i] = true;
                        true
                    } else {
                        false
                    }
                })
                .count();
            if number_after(tail, "<=").is_none_or(|n| usize::from(n) < count) {
                return false;
            }
            remaining = tail;
        }
        covered.iter().all(|b| *b)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhyCapabilities {
    pub modes: Vec<String>,
    pub ap: bool,
    pub station: bool,
    pub combinations: Vec<Combination>,
    pub channels: Vec<Channel>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WifiSummary {
    pub mode: String,
    pub station24: Option<bool>,
    pub ap24: Option<bool>,
    pub station5: Option<bool>,
    pub ap5: Option<bool>,
    pub ap_station: Option<bool>,
    pub ap_station_channels: Option<u16>,
    pub p2p_client: Option<bool>,
    pub ap5_channels: Option<Vec<u16>>,
}
impl WifiSummary {
    pub fn from_phy(phy: Option<&PhyCapabilities>) -> Self {
        let Some(phy) = phy else {
            return Self {
                mode: "unknown".into(),
                station24: None,
                ap24: None,
                station5: None,
                ap5: None,
                ap_station: None,
                ap_station_channels: None,
                p2p_client: None,
                ap5_channels: None,
            };
        };
        let concurrent = phy.combinations.iter().any(|c| c.ap_station);
        let preferred = [36, 40, 44, 48, 149, 153, 157, 161, 165];
        let mut channels = phy
            .channels
            .iter()
            .filter(|c| c.five_ghz() && c.access_point())
            .map(|c| c.channel)
            .collect::<Vec<_>>();
        channels.sort_unstable_by_key(|c| {
            (
                preferred
                    .iter()
                    .position(|p| p == c)
                    .unwrap_or(preferred.len()),
                *c,
            )
        });
        channels.dedup();
        Self {
            mode: if concurrent {
                "concurrent"
            } else {
                "single-radio"
            }
            .into(),
            station24: Some(phy.channels.iter().any(|c| c.two_ghz() && !c.disabled)),
            ap24: Some(phy.channels.iter().any(|c| c.two_ghz() && c.access_point())),
            station5: Some(phy.station_5ghz()),
            ap5: Some(!channels.is_empty()),
            ap_station: Some(concurrent),
            ap_station_channels: phy
                .combinations
                .iter()
                .filter(|c| c.ap_station)
                .filter_map(|c| c.channels)
                .max(),
            p2p_client: Some(phy.modes.iter().any(|m| m == "P2P-client")),
            ap5_channels: Some(channels),
        }
    }
}
impl PhyCapabilities {
    pub fn station_5ghz(&self) -> bool {
        self.channels.iter().any(|c| c.five_ghz() && !c.disabled)
    }
    pub fn ap_channel_reason(&self, channel: u8) -> Option<&'static str> {
        let Some(c) = self
            .channels
            .iter()
            .find(|c| c.channel == u16::from(channel) && c.five_ghz())
        else {
            return Some("channel not offered by radio");
        };
        if c.disabled {
            Some("channel disabled")
        } else if c.no_ir {
            Some("no-IR prohibits initiating radiation")
        } else if c.radar {
            Some("channel requires radar detection")
        } else {
            None
        }
    }
    pub fn ap_channel(&self, channel: u8) -> bool {
        self.channels.iter().any(|c| {
            c.channel == u16::from(channel)
                && (if channel <= 14 {
                    c.two_ghz()
                } else {
                    c.five_ghz()
                })
                && c.access_point()
        })
    }
}
fn number_after(text: &str, marker: &str) -> Option<u16> {
    text.split_once(marker)?
        .1
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}
fn combination(text: &str) -> Combination {
    let mut groups = Vec::new();
    let mut remaining = text;
    while let Some((_, tail)) = remaining.split_once("#{") {
        let Some((names, tail)) = tail.split_once('}') else {
            break;
        };
        groups.push((
            names
                .split(',')
                .map(|s| s.trim().to_string())
                .collect::<Vec<_>>(),
            number_after(tail, "<="),
        ));
        remaining = tail;
    }
    let ap = groups
        .iter()
        .position(|(m, n)| m.iter().any(|m| m == "AP") && n.is_some_and(|n| n > 0));
    let station = groups
        .iter()
        .position(|(m, n)| m.iter().any(|m| m == "managed") && n.is_some_and(|n| n > 0));
    let ap_station = match (ap, station) {
        (Some(a), Some(s)) => {
            (a != s || groups[a].1.is_some_and(|n| n >= 2))
                && number_after(text, "total <=").is_some_and(|n| n >= 2)
        }
        _ => false,
    };
    Combination {
        modes: groups.into_iter().flat_map(|(m, _)| m).collect(),
        channels: number_after(text, "#channels <="),
        ap_station,
        description: text.trim().into(),
    }
}
/// Reads only public `iw` output. AP transmission must also permit initiating radiation.
pub fn parse_phy(text: &str) -> PhyCapabilities {
    let mut result = PhyCapabilities::default();
    let mut modes = false;
    let mut combinations = false;
    let mut current = String::new();
    for line in text.lines().map(str::trim) {
        if line == "Supported interface modes:" {
            modes = true;
            continue;
        }
        if modes {
            if let Some(mode) = line.strip_prefix("* ") {
                result.modes.push(mode.into());
                continue;
            }
            modes = false;
        }
        if line == "valid interface combinations:" || line == "Valid interface combinations:" {
            combinations = true;
            continue;
        }
        if combinations {
            if line.starts_with("* #{") {
                if !current.is_empty() {
                    result.combinations.push(combination(&current));
                }
                current = line.into();
                continue;
            }
            if line.starts_with("#{")
                || line.starts_with("total <=")
                || line.starts_with("#channels <=")
                || line.starts_with("STA/AP")
            {
                current.push(' ');
                current.push_str(line);
                continue;
            }
            if !current.is_empty() {
                result.combinations.push(combination(&current));
                current.clear();
            }
            combinations = false;
        }
        let Some(rest) = line.strip_prefix("* ") else {
            continue;
        };
        let Some((frequency, tail)) = rest.split_once(" MHz [") else {
            continue;
        };
        let Some((channel, _)) = tail.split_once(']') else {
            continue;
        };
        if let (Ok(frequency), Ok(channel)) = (frequency.parse::<f64>(), channel.parse()) {
            if !frequency.is_finite()
                || frequency.fract() != 0.0
                || !(0.0..=u32::MAX as f64).contains(&frequency)
            {
                continue;
            }
            result.channels.push(Channel {
                frequency: frequency as u32,
                channel,
                disabled: line.contains("disabled"),
                no_ir: line.contains("no IR")
                    || line.contains("no-IR")
                    || line.contains("passive scan"),
                radar: line.contains("radar detection"),
            });
        }
    }
    if !current.is_empty() {
        result.combinations.push(combination(&current));
    }
    result.ap = result.modes.iter().any(|m| m == "AP");
    result.station = result.modes.iter().any(|m| m == "managed");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_device_summaries_and_unknown_probe() {
        let n1 = parse_phy(include_str!("../tests/fixtures/iw-phy-n1.txt"));
        assert_eq!(
            WifiSummary::from_phy(Some(&n1)),
            WifiSummary {
                mode: "concurrent".into(),
                station24: Some(true),
                ap24: Some(true),
                station5: Some(true),
                ap5: Some(true),
                ap_station: Some(true),
                ap_station_channels: Some(1),
                p2p_client: Some(true),
                ap5_channels: Some(vec![36, 40, 44, 48, 149, 153, 157, 161, 165]),
            }
        );
        let ufi = parse_phy(include_str!("../tests/fixtures/iw-phy-ufi003.txt"));
        assert_eq!(
            WifiSummary::from_phy(Some(&ufi)),
            WifiSummary {
                mode: "single-radio".into(),
                station24: Some(true),
                ap24: Some(true),
                station5: Some(false),
                ap5: Some(false),
                ap_station: Some(false),
                ap_station_channels: None,
                p2p_client: Some(false),
                ap5_channels: Some(vec![]),
            }
        );
        let unknown = serde_json::to_value(WifiSummary::from_phy(None)).unwrap();
        assert_eq!(unknown["mode"], "unknown");
        for (key, value) in unknown.as_object().unwrap() {
            if key != "mode" {
                assert!(value.is_null(), "{key}");
            }
        }
    }
    #[test]
    fn summary_prefers_common_channels_and_largest_concurrent_channel_count() {
        let phy = parse_phy(" * 5845 MHz [169]\n * 5220 MHz [44]\n * 5180 MHz [36]\n * 4920 MHz [184]\n * 5180 MHz [36]\n * 5975 MHz [5]\n * 2412 MHz [1] (disabled)\nvalid interface combinations:\n * #{ managed } <= 1, #{ AP } <= 1, total <= 2, #channels <= 1\n * #{ managed } <= 1, #{ AP } <= 1, total <= 2, #channels <= 2\n");
        let summary = WifiSummary::from_phy(Some(&phy));
        assert_eq!(summary.ap5_channels, Some(vec![36, 44, 169, 184]));
        assert_eq!(summary.ap_station_channels, Some(2));
        assert_eq!(summary.station24, Some(false));
        assert_eq!(summary.ap24, Some(false));
    }
    #[test]
    fn bands_flags_and_single_channel_concurrency() {
        let ufi = parse_phy(
            "Supported interface modes:\n * managed\n * AP\nBand 1:\n * 2437 MHz [6] (20.0 dBm)",
        );
        assert!(ufi.ap && ufi.station && ufi.ap_channel(6));
        assert!(!ufi.station_5ghz());
        let n1 = parse_phy("Supported interface modes:\n * managed\n * AP\nBand 2:\n * 5180 MHz [36] (20.0 dBm) (no IR)\n * 5200 MHz [40] (disabled)\n * 5260 MHz [52] (20.0 dBm) (radar detection)\nvalid interface combinations:\n * #{ managed } <= 1, #{ AP } <= 1,\n total <= 2, #channels <= 1\n");
        assert!(n1.station_5ghz());
        assert!(!n1.ap_channel(36) && !n1.ap_channel(40) && !n1.ap_channel(52));
        assert!(n1.combinations[0].ap_station);
        assert_eq!(n1.combinations[0].channels, Some(1));
        assert!(parse_phy(" * 5180.0 MHz [36] (20.0 dBm)").ap_channel(36));
        assert!(!parse_phy("valid interface combinations:\n * #{ managed, AP } <= 1, total <= 1, #channels <= 1").combinations[0].ap_station);
    }
}
