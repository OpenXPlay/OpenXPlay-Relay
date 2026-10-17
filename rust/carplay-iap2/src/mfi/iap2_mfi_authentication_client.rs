// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::{sync::Arc, time::Duration};

/// State owned by `Iap2MfiAuthenticationClient`; construct through its factory.
pub struct Iap2MfiAuthenticationClient {
    mfi: Arc<dyn MfiAuthenticator>,
}
impl Iap2MfiAuthenticationClient {
    pub fn new(mfi: Arc<dyn MfiAuthenticator>) -> Self {
        Self { mfi }
    }

    pub fn run(&self, session: &Iap2Session, timeout: Duration) -> Result<()> {
        self.run_with_progress(session, timeout, &|_| {})
    }
}

/// Protocol message, field, or framing constant.
pub const REQUEST_CERTIFICATE: u64 = 0xaa00;
/// Protocol message, field, or framing constant.
pub const CERTIFICATE: u64 = 0xaa01;
/// Protocol message, field, or framing constant.
pub const REQUEST_CHALLENGE: u64 = 0xaa02;
/// Protocol message, field, or framing constant.
pub const RESPONSE: u64 = 0xaa03;
/// Protocol message, field, or framing constant.
pub const AUTHENTICATION_FAILED: u64 = 0xaa04;
/// Protocol message, field, or framing constant.
pub const AUTHENTICATION_SUCCEEDED: u64 = 0xaa05;
/// Protocol message, field, or framing constant.
pub const MAXIMUM_CHALLENGE_BYTES: u64 = 128;
/// Protocol message, field, or framing constant.
pub const MAX_CSM_CERTIFICATE_BYTES: u64 = 65_525;
/// Protocol message, field, or framing constant.
pub const DEFAULT_TIMEOUT_MILLIS: u64 = 30_000;
/// Protocol message, field, or framing constant.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;

/// Constant expression: `5 * 60 * 1_000`.
pub const MAX_TIMEOUT_MILLIS: u64 = 300000;

impl Iap2MfiAuthenticationClient {
    pub(crate) fn run_with_progress(
        &self,
        session: &Iap2Session,
        timeout: Duration,
        progress: &dyn Fn(&str),
    ) -> Result<()> {
        if timeout.is_zero() || timeout > Duration::from_secs(300) {
            return Err(invalid("timeoutMillis must be in 1..300000"));
        }
        let started = std::time::Instant::now();
        let remaining = || timeout.saturating_sub(started.elapsed());
        let cert = self.mfi.read_certificate(65525)?;
        let frame = match self.mfi.certificate_type() {
            MfiCertificateType::Mfi => Iap2AuthenticationMessages::accessory_certificate(&cert)?,
            MfiCertificateType::Baa => {
                Iap2AuthenticationMessages::accessory_certificate_body(&cert)?
            }
        };
        progress(&format!(
            "mfi type={} certificate loaded bytes={} payload bytes={}",
            if self.mfi.certificate_type() == MfiCertificateType::Mfi {
                "MFI"
            } else {
                "BAA"
            },
            cert.len(),
            frame.payload.len()
        ));
        loop {
            if remaining().is_zero() {
                return Err(invalid("Timed out waiting for iAP2 MFi authentication"));
            }
            let incoming = session
                .recv(remaining())?
                .ok_or_else(|| invalid("Timed out waiting for iAP2 MFi authentication"))?;
            let reply = match incoming.message_id {
                0xaa00 => {
                    progress("iap2 mfi rx=0xaa00 request-certificate");
                    frame.clone()
                }
                0xaa02 => {
                    let challenge = Iap2AuthenticationMessages::challenge(&incoming)
                        .map_err(|_| invalid("iAP2 MFi challenge is missing parameter 0"))?;
                    if challenge.is_empty() || challenge.len() > 128 {
                        return Err(invalid(format!(
                            "Invalid iAP2 MFi challenge length {}",
                            challenge.len()
                        )));
                    }
                    progress(&format!(
                        "iap2 mfi rx=0xaa02 challenge bytes={}",
                        challenge.len()
                    ));
                    Iap2AuthenticationMessages::response(&self.mfi.sign_challenge(&challenge)?)?
                }
                0xaa05 => {
                    progress("iap2 mfi rx=0xaa05 authentication-succeeded");
                    return Ok(());
                }
                0xaa04 => return Err(invalid("iPhone sent AuthenticationFailed")),
                id => return Err(invalid(format!("Unexpected iAP2 MFi message 0x{id:04x}"))),
            };
            if remaining().is_zero() {
                return Err(invalid("Timed out sending iAP2 MFi authentication reply"));
            }
            session.send(&reply, remaining())?;
            if reply.message_id == 0xaa01 {
                progress(&format!(
                    "iap2 mfi tx=0xaa01 certificate bytes={}",
                    reply.payload.len()
                ));
            } else {
                progress(&format!(
                    "iap2 mfi tx=0xaa03 signature bytes={}",
                    reply.payload.len().saturating_sub(4)
                ));
            }
        }
    }
}
