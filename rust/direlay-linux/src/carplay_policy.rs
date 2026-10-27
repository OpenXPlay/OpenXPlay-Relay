// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use oxp_core::model::FrameRatePreference;

pub fn frame_rate_setting(value: &str) -> Option<FrameRatePreference> {
    Some(match value {
        "auto" => FrameRatePreference::Auto,
        "30" => FrameRatePreference::Fps30,
        "35" => FrameRatePreference::Fps35,
        "40" => FrameRatePreference::Fps40,
        "45" => FrameRatePreference::Fps45,
        "50" => FrameRatePreference::Fps50,
        "55" => FrameRatePreference::Fps55,
        "60" => FrameRatePreference::Fps60,
        _ => return None,
    })
}
pub fn frame_rate_value(value: FrameRatePreference) -> &'static str {
    match value {
        FrameRatePreference::Auto => "auto",
        FrameRatePreference::Fps30 => "30",
        FrameRatePreference::Fps35 => "35",
        FrameRatePreference::Fps40 => "40",
        FrameRatePreference::Fps45 => "45",
        FrameRatePreference::Fps50 => "50",
        FrameRatePreference::Fps55 => "55",
        FrameRatePreference::Fps60 => "60",
    }
}
pub fn requested_frame_rate(setting: FrameRatePreference, negotiated: Option<f64>) -> u32 {
    if setting == FrameRatePreference::Auto {
        negotiated
            .filter(|v| v.is_finite())
            .unwrap_or(60.)
            .clamp(30., 60.)
            .round() as u32
    } else {
        frame_rate_value(setting).parse().unwrap()
    }
}
pub fn media_button(key: u16) -> Option<u8> {
    use oxp_core::model::KeyCode;
    match key {
        k if k == KeyCode::PlayPause as u16 => Some(3),
        k if k == KeyCode::Next as u16 => Some(4),
        k if k == KeyCode::Previous as u16 => Some(5),
        _ => None,
    }
}
pub fn classify_failure(message: &str) -> &'static str {
    let m = message.to_ascii_lowercase();
    if m.contains("first_tcp_timeout") {
        "first_tcp_timeout"
    } else if m.contains("hotspot")
        && (m.contains("configuration") || m.contains("security") || m.contains("ssid differs"))
    {
        "hotspot_configuration"
    } else if m.contains("hotspot") || m.contains("network is not ready") {
        "hotspot_not_ready"
    } else if m.contains("bluetooth") && (m.contains("off") || m.contains("powered")) {
        "bluetooth_off"
    } else if m.contains("no sockets")
        || m.contains("unreal")
        || m.contains("socket manager")
        || m.contains("ibluetoothsocketmanager")
    {
        "bluetooth_unusable"
    } else if m.contains("no longer paired") || m.contains("no bonded") || m.contains("not paired")
    {
        "iphone_unpaired"
    } else if m.contains("rfcomm") && (m.contains("timeout") || m.contains("timed out")) {
        "rfcomm_timeout"
    } else if m.contains("turn on wi-fi") {
        "wifi_off"
    } else if m.contains("userdeniedpairing") || m.contains("user denied lockdown pairing") {
        "usb_trust_denied"
    } else if m.contains("password protected") || m.contains("passwordprotected") {
        "iphone_locked"
    } else {
        "unknown"
    }
}
/// Only fixed, safe wording is exposed; command errors can contain network credentials.
pub fn failure_text(code: &str) -> &'static str {
    match code {
        "first_tcp_timeout" => "The iPhone did not connect over Wi-Fi in time",
        "hotspot_configuration" => "Check the wireless network settings",
        "hotspot_not_ready" => "The wireless network is not ready",
        "bluetooth_off" => "Turn on Bluetooth",
        "bluetooth_unusable" => "Bluetooth is unavailable",
        "iphone_unpaired" => "Pair the iPhone again",
        "rfcomm_timeout" => "The Bluetooth connection timed out",
        "wifi_off" => "Turn on Wi-Fi",
        _ => "The iPhone connection ended",
    }
}
pub fn startup_stage(stage: &str) -> Option<(&'static str, &'static str)> {
    Some(match stage {
        "DiscoveringMfi" | "MfiReady" | "StartingHotspot" | "HotspotReady" => {
            ("STARTING", "connecting")
        }
        "WaitingForBluetooth" => ("BLUETOOTH", "connecting"),
        "WaitingForPairedIphone" | "AttachingNetwork" => ("WAITING_IPHONE", "connecting"),
        "ConnectingBluetooth" => ("BLUETOOTH", "authenticating"),
        "RunningWireless" | "Identified" | "Authenticated" | "Subscribed" => {
            ("AUTH", "authenticating")
        }
        "WifiConfigSent" | "PostTransportWifiConfigSent" => ("WIFI_SENT", "joining-network"),
        "CarplayStartSent" | "TransportNotified" | "WirelessActive" => {
            ("JOINING", "joining-network")
        }
        "Session" => ("SESSION", "waiting-media"),
        "Failed" => ("ENDED", "failed"),
        "Stopped" => ("ENDED", "disconnected"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolver_uses_every_setting_and_finite_negotiation() {
        for n in (30..=60).step_by(5) {
            let s = n.to_string();
            let p = frame_rate_setting(&s).unwrap();
            assert_eq!(requested_frame_rate(p, Some(31.)), n);
            assert_eq!(frame_rate_value(p), s);
        }
        for (v, expected) in [
            (None, 60),
            (Some(f64::NAN), 60),
            (Some(f64::INFINITY), 60),
            (Some(20.), 30),
            (Some(80.), 60),
            (Some(44.5), 45),
        ] {
            assert_eq!(requested_frame_rate(FrameRatePreference::Auto, v), expected);
        }
        assert!(frame_rate_setting("31").is_none());
    }
    #[test]
    fn key_indices_and_failure_markers() {
        assert_eq!(
            (
                media_button(7),
                media_button(8),
                media_button(9),
                media_button(1)
            ),
            (Some(3), Some(4), Some(5), None)
        );
        for (text, code) in [
            ("RFCOMM timed out", "rfcomm_timeout"),
            ("Bluetooth powered off", "bluetooth_off"),
            ("iPhone no longer paired", "iphone_unpaired"),
            ("first_tcp_timeout", "first_tcp_timeout"),
            ("Hotspot address changed", "hotspot_not_ready"),
            ("IBluetoothSocketManager", "bluetooth_unusable"),
            ("Turn on Wi-Fi", "wifi_off"),
            ("other", "unknown"),
        ] {
            assert_eq!(classify_failure(text), code);
        }
    }
    #[test]
    fn startup_reason_mapping() {
        for (stage, reason) in [
            ("MfiReady", "connecting"),
            ("ConnectingBluetooth", "authenticating"),
            ("Authenticated", "authenticating"),
            ("WifiConfigSent", "joining-network"),
            ("Session", "waiting-media"),
            ("Failed", "failed"),
            ("Stopped", "disconnected"),
        ] {
            assert_eq!(startup_stage(stage).unwrap().1, reason);
        }
    }
}
