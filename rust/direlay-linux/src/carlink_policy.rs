// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use crate::display::Size;

pub const PRESETS: [Size; 6] = [
    Size {
        width: 1280,
        height: 720,
    },
    Size {
        width: 1920,
        height: 1080,
    },
    Size {
        width: 1280,
        height: 800,
    },
    Size {
        width: 1920,
        height: 720,
    },
    Size {
        width: 1024,
        height: 600,
    },
    Size {
        width: 1024,
        height: 768,
    },
];
pub fn valid_picture(s: Size) -> bool {
    let (w, h) = if s.height > s.width {
        (s.height, s.width)
    } else {
        (s.width, s.height)
    };
    s.width.is_multiple_of(2)
        && s.height.is_multiple_of(2)
        && (640..=1920).contains(&w)
        && (360..=1080).contains(&h)
}
pub fn validate_pictures(sizes: &[Size], default: Option<Size>) -> crate::Result<()> {
    let mut seen = Vec::new();
    if sizes.len() > 5
        || sizes.iter().any(|s| {
            let invalid = !valid_picture(*s) || PRESETS.contains(s) || seen.contains(s);
            seen.push(*s);
            invalid
        })
        || default.is_some_and(|s| !PRESETS.contains(&s) && !sizes.contains(&s))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid Carlink picture sizes or default",
        ));
    }
    Ok(())
}
/// None = persisted default; Some(None) = explicitly try Automatic. Never serialized.
#[derive(Default)]
pub struct PictureSelection {
    pub trial: Option<Option<Size>>,
    pub applied: Option<Size>,
    pub automatic_canvas: Option<Size>,
}
impl PictureSelection {
    pub fn effective(&self, default: Option<Size>) -> Option<Size> {
        self.trial.unwrap_or(default)
    }
    pub fn choose(&mut self, size: Option<Size>, default: Option<Size>) -> crate::Result<bool> {
        if size.is_some_and(|s| !valid_picture(s)) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid Carlink picture size",
            ));
        }
        let old = self.effective(default);
        self.trial = Some(size);
        Ok(old != size)
    }
    pub fn stopped(&mut self) {
        *self = Self::default();
    }
}
pub fn bluetooth_address(value: &str) -> Option<String> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 6
        || parts
            .iter()
            .any(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return None;
    }
    let bytes: Vec<_> = parts
        .iter()
        .map(|s| u8::from_str_radix(s, 16).unwrap())
        .collect();
    (bytes[0] & 1 == 0 && bytes.iter().any(|b| *b != 0) && bytes != [2, 0, 0, 0, 0, 0])
        .then(|| value.to_ascii_lowercase())
}
pub fn failure_code(text: &str) -> &'static str {
    let s = text.to_ascii_lowercase();
    for code in [
        "ble_timeout",
        "ble_unavailable",
        "ble_scan_failed",
        "gatt_connect_failed",
        "gatt_services_failed",
        "ble_request_rejected",
        "ble_cccd_timeout",
        "ble_discovery_failed",
        "ble_service_missing",
        "ble_subscribe_failed",
        "ble_cccd_failed",
        "ble_write_failed",
        "ble_reply_invalid",
        "ble_verify_timeout",
        "ble_exchange_failed",
        "ble_exchange_timeout",
        "wifi_join_p2p_timeout",
        "wifi_off",
        "wifi_join_unavailable",
        "wifi_bind_failed",
        "wifi_join_timeout",
        "wifi_link_lost",
        "auth_rejected",
        "auth_pin_proof_failed",
        "auth_timeout",
        "auth_closed",
        "auth_internal_error",
        "auth_signature_failed",
        "authentication_failed",
        "auth_pin_required",
        "auth_confirmation_required",
        "auth_version_unsupported",
        "peer_key_changed",
        "pin_cancelled",
        "pairing_confirmation_cancelled",
        "pin_required",
        "invalid_auth_header",
        "invalid_auth_signature",
        "link_lost",
        "heartbeat_timeout",
        "vehicle_channel_closed",
    ] {
        if s.split(':').next() == Some(code) {
            return code;
        }
    }
    if s.contains("band_unsupported") {
        "head_unit_band_unsupported"
    } else if s.contains("pin") && (s.contains("incorrect") || s.contains("stale")) {
        "pin_incorrect"
    } else if s.contains("pin") && s.contains("timeout") {
        "pin_timeout"
    } else if s.contains("auth") {
        "authentication_failed"
    } else if s.contains("wifi") || s.contains("wi-fi") || s.contains("network") {
        "network_failed"
    } else if s.contains("bluetooth") || s.contains("gatt") || s.contains("adapter") {
        "bluetooth_failed"
    } else {
        "connection_failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pictures_validate_bounds_presets_duplicates_and_default_membership() {
        for s in PRESETS {
            assert!(valid_picture(s));
            assert!(validate_pictures(&[], Some(s)).is_ok());
        }
        let s = Size {
            width: 1440,
            height: 900,
        };
        assert!(validate_pictures(&[s], Some(s)).is_ok());
        assert!(validate_pictures(&[s, s], None).is_err());
        assert!(validate_pictures(&[], Some(s)).is_err());
        assert!(validate_pictures(&[PRESETS[0]], None).is_err());
        for s in [
            Size {
                width: 639,
                height: 360,
            },
            Size {
                width: 1922,
                height: 1080,
            },
            Size {
                width: 1280,
                height: 721,
            },
            Size {
                width: 318,
                height: 1920,
            },
        ] {
            assert!(!valid_picture(s));
        }
        assert!(valid_picture(Size {
            width: 720,
            height: 1280
        }));
    }
    #[test]
    fn trial_is_not_default_and_survives_restarts_until_stop() {
        let mut p = PictureSelection::default();
        let s = Some(PRESETS[0]);
        assert_eq!(p.effective(s), s);
        assert!(p.choose(None, s).unwrap());
        assert_eq!(p.effective(s), None);
        assert!(!p.choose(None, s).unwrap());
        assert!(p.choose(Some(PRESETS[1]), s).unwrap());
        p.applied = p.effective(s);
        assert_eq!(p.effective(None), Some(PRESETS[1]));
        p.stopped();
        assert_eq!(p.effective(s), s);
    }
    #[test]
    fn bluetooth_override_validation() {
        assert_eq!(
            bluetooth_address("AA:BB:CC:DD:EE:FF").as_deref(),
            Some("aa:bb:cc:dd:ee:ff")
        );
        for a in [
            "",
            "0:11:22:33:44:55",
            "01:11:22:33:44:55",
            "00:00:00:00:00:00",
            "02:00:00:00:00:00",
            "aa:bb:cc:dd:ee:gg",
        ] {
            assert!(bluetooth_address(a).is_none());
        }
        assert_eq!(
            failure_code("head_unit_band_unsupported"),
            "head_unit_band_unsupported"
        );
        assert_eq!(failure_code("PIN incorrect"), "pin_incorrect");
    }
}
