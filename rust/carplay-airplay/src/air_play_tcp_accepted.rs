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

/// `AirPlayListenerIdentity` protocol data.
#[derive(Clone, Debug)]
pub struct AirPlayListenerIdentity {
    pub generation: u64,

    pub id: u64,
}
/// `AirPlayTcpAccepted` protocol data.
#[derive(Clone, Debug)]
pub struct AirPlayTcpAccepted {
    pub listener: AirPlayListenerIdentity,

    pub internal: bool,

    pub accepted_at_nanos: u64,
}
/// Distinguish local tunnel peers from externally accepted control connections.
pub fn is_internal_air_play_peer(peer: std::net::IpAddr, local: std::net::IpAddr) -> bool {
    peer.is_loopback() || peer == local
}

impl AirPlayListenerIdentity {
    pub fn new(generation: u64) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self {
            generation,
            id: NEXT_ID
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .wrapping_add(1),
        }
    }
}
