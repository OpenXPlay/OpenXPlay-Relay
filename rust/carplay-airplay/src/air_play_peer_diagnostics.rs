// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use carplay_iap2::{
    mfi::MfiAuthenticator, DuplexStream, Iap2Session, Iap2WirelessCarPlayEndpoint,
    Iap2WirelessSecurity, Receive,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// State owned by `AirPlayPeerDiagnostics`; construct through its factory.
pub struct AirPlayPeerDiagnostics {
    _private: (),
}
impl AirPlayPeerDiagnostics {
    pub fn summary(model: &str, os_version: &str, source_version: &str) -> String {
        fn version(s: &str) -> &str {
            if !s.is_empty()
                && s.len() <= 40
                && s.as_bytes()[0].is_ascii_digit()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                s
            } else {
                "not_reported"
            }
        }
        let valid_model = ["iPhone", "iPad", "iPod"].iter().any(|prefix| {
            model.strip_prefix(prefix).is_some_and(|s| {
                s.split_once(',').is_some_and(|(a, b)| {
                    [a, b].iter().all(|s| {
                        !s.is_empty() && s.len() <= 3 && s.bytes().all(|b| b.is_ascii_digit())
                    })
                })
            })
        });
        format!(
            "Phone display peer model={} ios={} airplay={}",
            if valid_model { model } else { "not_reported" },
            version(os_version),
            version(source_version)
        )
    }
}
