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

/// Platform interface for `PairingStore`. Implementations must support bounded shutdown.
pub trait PairingStore: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn save(&self, identifier: &str, long_term_public_key: &[u8; 32]) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn get(&self, identifier: &str) -> Result<Option<[u8; 32]>>;
    /// Errors propagate to the lifecycle owner.
    fn clear(&self) -> Result<()>;
}
