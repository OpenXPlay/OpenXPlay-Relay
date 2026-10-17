// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
use crate::*;
use carplay_airplay::*;
use carplay_iap2::{
    mfi::MfiAuthenticator, DuplexStream, Iap2Session, Iap2WirelessCarPlayEndpoint,
    Iap2WirelessSecurity, Receive,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// `WirelessHotspotInfo` protocol data.
#[derive(Clone, Debug)]
pub struct WirelessHotspotInfo {
    pub ssid: String,

    pub passphrase: String,

    pub channel: u8,

    pub security: Iap2WirelessSecurity,

    pub host_address: Option<std::net::Ipv4Addr>,

    pub bssid: Option<String>,

    pub access_point_bssid: Option<String>,

    pub interface_name: Option<String>,
}
/// Platform interface for `WirelessHotspotManager`. Implementations must support bounded shutdown.
pub trait WirelessHotspotManager: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn start(&self, timeout: Duration) -> Result<WirelessHotspotInfo>;
    /// Errors propagate to the lifecycle owner.
    fn validate_ready(&self) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_car_play_confirmed(&self) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn close(&self) -> Result<()>;
}
