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

/// `AirPlayIdentity` protocol data.
#[derive(Clone, Debug)]
pub struct AirPlayIdentity {
    pub private_key: [u8; 32],

    pub public_key: [u8; 32],

    pub pairing_id: String,
}
impl AirPlayIdentity {
    pub fn public_key_hex(&self) -> String {
        crate::bytes::to_hex_string(&self.public_key)
    }
}

impl AirPlayIdentity {
    /// Create the long-term signing key and a random version-4 pairing UUID; persistence is caller-owned.
    pub fn generate() -> Result<Self> {
        use rand_core::RngCore;
        let key = AirPlayCrypto::ed25519_generate()?;
        let mut uuid = [0; 16];
        rand_core::OsRng
            .try_fill_bytes(&mut uuid)
            .map_err(std::io::Error::other)?;
        uuid[6] = (uuid[6] & 15) | 0x40;
        uuid[8] = (uuid[8] & 0x3f) | 0x80;
        let hex = crate::bytes::to_hex_string(&uuid);
        let pairing_id = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        Ok(Self {
            private_key: key.private_key,
            public_key: key.public_key,
            pairing_id,
        })
    }
}
