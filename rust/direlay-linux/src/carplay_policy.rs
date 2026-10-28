// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use oxp_core::model::FrameRatePreference;

pub const DISCOVERING_MFI: &str = "DiscoveringMfi";
pub const MFI_READY: &str = "MfiReady";
pub const WAITING_FOR_IPHONE_USB: &str = "WaitingForIphoneUsb";
pub const REQUESTING_CARPLAY_MODE: &str = "RequestingCarPlayMode";
pub const WAITING_FOR_REENUMERATION: &str = "WaitingForReenumeration";
pub const SELECTING_USB_CONFIGURATION: &str = "SelectingUsbConfiguration";
pub const OPENING_USBMUX: &str = "OpeningUsbmux";
pub const PAIRING: &str = "Pairing";
pub const CONNECTING_CONTROL: &str = "ConnectingControl";
pub const RUNNING_CONTROL: &str = "RunningControl";
pub const START_SESSION_SENT: &str = "StartSessionSent";

pub fn screen_session_policy(
    settings: &crate::settings::Settings,
) -> carplay_airplay::ScreenSessionPolicy {
    // The receiver captures this policy when constructing each engine.
    carplay_airplay::ScreenSessionPolicy {
        isolate_sessions: settings.experimental_active
            && settings.experimental_screen_session_isolation,
        reset_on_reconnect: settings.experimental_active
            && settings.experimental_screen_reconnect_reset,
        serialize_delivery: settings.experimental_active
            && settings.experimental_screen_delivery_serialization,
    }
}

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
/// Only use before RFCOMM connects. The same OS error after connection spends the startup budget.
pub fn iphone_unreachable(error: &std::io::Error) -> bool {
    if classify_failure(&error.to_string()) == "iphone_unplugged" {
        return true;
    }
    use std::io::ErrorKind;
    matches!(
        error.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::HostUnreachable
            | ErrorKind::NetworkUnreachable
            | ErrorKind::TimedOut
    ) || error.raw_os_error() == Some(libc::EHOSTDOWN)
        || error
            .get_ref()
            .is_some_and(|cause| cause.is::<crate::linux_bluetooth::IphoneUnavailable>())
}
pub fn classify_failure(message: &str) -> &'static str {
    let m = message.to_ascii_lowercase();
    if m.contains("iphone unplugged") {
        "iphone_unplugged"
    } else if m.contains("usb interface busy") {
        "usb_interface_busy"
    } else if m.contains("carlink output did not become ready")
        || m.contains("carlink output was disabled while waiting")
    {
        "carlink_not_ready"
    } else if m.contains("wifi_reset_required") {
        "wifi_reset_required"
    } else if m.contains("first_tcp_timeout") {
        "first_tcp_timeout"
    } else if m.contains("hotspot")
        && (m.contains("configuration") || m.contains("security") || m.contains("ssid differs"))
    {
        "hotspot_configuration"
    } else if m.contains("hotspot") || m.contains("network is not ready") {
        "hotspot_not_ready"
    } else if m.contains("bluetooth")
        && (m.contains("off")
            || m.contains("powered")
            || m.contains("adapter") && m.contains("unavailable"))
    {
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
        "iphone_unplugged" => "The iPhone was unplugged. Connect it to the relay again",
        "usb_interface_busy" => "Another program (usbmuxd?) holds the iPhone USB interface",
        "usb_trust_denied" => "Reconnect the iPhone and tap Trust",
        "iphone_locked" => "Unlock the iPhone and connect it again",
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
        DISCOVERING_MFI | MFI_READY | "StartingHotspot" | "HotspotReady" => {
            ("STARTING", "connecting")
        }
        WAITING_FOR_IPHONE_USB
        | REQUESTING_CARPLAY_MODE
        | WAITING_FOR_REENUMERATION
        | SELECTING_USB_CONFIGURATION
        | OPENING_USBMUX => ("USB_DETECT", "connecting"),
        PAIRING => ("USB_TRUST", "authenticating"),
        CONNECTING_CONTROL | RUNNING_CONTROL | START_SESSION_SENT => {
            ("USB_CONTROL", "authenticating")
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
    fn reachability_uses_error_kinds_and_typed_selection_and_sdp_failures() {
        use std::io::{Error, ErrorKind};
        for code in [
            libc::ECONNREFUSED,
            libc::EHOSTDOWN,
            libc::EHOSTUNREACH,
            libc::ENETUNREACH,
            libc::ETIMEDOUT,
        ] {
            assert!(iphone_unreachable(&Error::from_raw_os_error(code)));
        }
        assert!(iphone_unreachable(&Error::new(
            ErrorKind::TimedOut,
            "no message marker"
        )));
        assert!(iphone_unreachable(
            &crate::linux_bluetooth::select_iphone(&[], None).unwrap_err()
        ));
        assert!(iphone_unreachable(
            &crate::linux_bluetooth::rfcomm_channel("").unwrap_err()
        ));
        for error in [
            Error::other("Connection refused (os error 111)"),
            Error::other("sdptool failed (exit 255)"),
            Error::new(ErrorKind::PermissionDenied, "Bluetooth rejected"),
            Error::new(ErrorKind::NotFound, "Missing local file"),
            crate::linux_bluetooth::select_iphone(&[], Some("AA:BB:CC:DD:EE:01")).unwrap_err(),
        ] {
            assert!(!iphone_unreachable(&error), "{error}");
        }
    }
    #[test]
    fn screen_policy_captures_each_choice_and_master_without_erasing_choices() {
        for bits in 0..16 {
            let settings = crate::settings::Settings {
                experimental_active: bits & 8 != 0,
                experimental_screen_session_isolation: bits & 1 != 0,
                experimental_screen_reconnect_reset: bits & 2 != 0,
                experimental_screen_delivery_serialization: bits & 4 != 0,
                ..Default::default()
            };
            let policy = screen_session_policy(&settings);
            assert_eq!(policy.isolate_sessions, bits & 9 == 9);
            assert_eq!(policy.reset_on_reconnect, bits & 10 == 10);
            assert_eq!(policy.serialize_delivery, bits & 12 == 12);
            let mut next = settings.clone();
            next.experimental_active = !next.experimental_active;
            let _ = screen_session_policy(&next);
            assert_eq!(policy, screen_session_policy(&settings));
        }
    }

    #[test]
    fn screen_session_defaults_enable_all_protections() {
        let policy = screen_session_policy(&crate::settings::Settings::default());
        assert!(policy.isolate_sessions);
        assert!(policy.reset_on_reconnect);
        assert!(policy.serialize_delivery);
    }
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
            ("Bluetooth adapter address unavailable", "bluetooth_off"),
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
    fn wired_stages_failures_and_unplug_reachability() {
        for stage in [
            WAITING_FOR_IPHONE_USB,
            REQUESTING_CARPLAY_MODE,
            WAITING_FOR_REENUMERATION,
            SELECTING_USB_CONFIGURATION,
            OPENING_USBMUX,
        ] {
            assert_eq!(startup_stage(stage).unwrap().0, "USB_DETECT");
        }
        assert_eq!(startup_stage(PAIRING).unwrap().0, "USB_TRUST");
        for stage in [CONNECTING_CONTROL, RUNNING_CONTROL, START_SESSION_SENT] {
            assert_eq!(startup_stage(stage).unwrap().0, "USB_CONTROL");
        }
        for (message, code) in [
            ("iPhone unplugged", "iphone_unplugged"),
            ("USB interface busy", "usb_interface_busy"),
            ("UserDeniedPairing", "usb_trust_denied"),
            ("PasswordProtected", "iphone_locked"),
        ] {
            assert_eq!(classify_failure(message), code);
            assert_ne!(failure_text(code), failure_text("unknown"));
        }
        assert!(iphone_unreachable(&std::io::Error::other(
            "iPhone unplugged"
        )));
        let mut budget = crate::start_policy::RetryBudget::default();
        for _ in 0..20 {
            assert!(budget.failed("iphone_unplugged", false));
        }
        for _ in 0..crate::start_policy::MAX_STARTUP_RETRIES {
            assert!(budget.failed("unknown", false));
        }
        assert!(!budget.failed("unknown", false));
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
