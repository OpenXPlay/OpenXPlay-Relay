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

/// State owned by `AirPlayControlDiagnostics`; construct through its factory.
pub struct AirPlayControlDiagnostics {
    _private: (),
}
impl AirPlayControlDiagnostics {
    pub fn request(method: &str, path: &str, bytes: usize) -> String {
        let verb = match method {
            "GET" | "POST" | "SETUP" | "RECORD" | "TEARDOWN" | "OPTIONS" | "GET_PARAMETER"
            | "SET_PARAMETER" => method,
            _ => "OTHER",
        };
        let route = path
            .split('?')
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let route = match route.as_str() {
            "info" | "pair-setup" | "pair-verify" | "auth-setup" | "fp-setup" => route.as_str(),
            _ => "other",
        };
        format!("airplay control request method={verb} route={route} contentBytes={bytes}")
    }
}
