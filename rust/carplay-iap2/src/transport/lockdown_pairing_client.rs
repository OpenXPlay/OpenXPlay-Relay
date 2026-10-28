// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Plaintext Lockdown pairing with bounded Trust-dialog retries and typed failures.
use super::{
    iap2_usb_mux_host::{Iap2UsbMuxHost, LOCKDOWN_PORT},
    lockdown_pair_record::LockdownPairRecord,
    lockdown_plist_channel::{dictionary, LockdownPlistChannel},
};
use crate::invalid;
use plist::{Dictionary, Value};
use std::{
    fmt, io,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
#[derive(Debug)]
pub enum LockdownPairingError {
    UserDenied,
    PasswordProtected,
    DialogPending,
    Timeout,
    InvalidPairRecord,
    InvalidHostId,
    Cancelled,
    Remote(String),
    InvalidResponse(&'static str),
    Io(io::Error),
}
impl fmt::Display for LockdownPairingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserDenied => f.write_str("The user denied Lockdown pairing"),
            Self::PasswordProtected => f.write_str("The iPhone is password protected"),
            Self::DialogPending => f.write_str("Lockdown pairing dialog response pending"),
            Self::Timeout => f.write_str("Lockdown pairing timed out"),
            Self::InvalidPairRecord => f.write_str("InvalidPairRecord"),
            Self::InvalidHostId => f.write_str("InvalidHostID"),
            Self::Cancelled => f.write_str("Lockdown pairing was cancelled"),
            Self::Remote(c) => write!(f, "Lockdown pairing failed: {c}"),
            Self::InvalidResponse(s) => write!(f, "Invalid Lockdown pairing response: {s}"),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for LockdownPairingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(e) = self {
            Some(e)
        } else {
            None
        }
    }
}
impl From<io::Error> for LockdownPairingError {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::TimedOut {
            Self::Timeout
        } else {
            Self::Io(e)
        }
    }
}
pub type PairingResult<T> = std::result::Result<T, LockdownPairingError>;
impl LockdownPairingError {
    pub fn from_code(code: &str) -> Self {
        match code {
            "UserDeniedPairing" => Self::UserDenied,
            "PasswordProtected" => Self::PasswordProtected,
            "PairingDialogResponsePending" => Self::DialogPending,
            "InvalidPairRecord" => Self::InvalidPairRecord,
            "InvalidHostID" => Self::InvalidHostId,
            c => Self::Remote(c.to_owned()),
        }
    }
    pub fn invalidates_record(&self) -> bool {
        matches!(self, Self::InvalidPairRecord | Self::InvalidHostId)
    }
}
pub(crate) fn reject_error(response: &Dictionary) -> PairingResult<()> {
    let Some(value) = response.get("Error") else {
        return Ok(());
    };
    let code = match value {
        Value::String(c) => c.clone(),
        Value::Integer(i) => response
            .get("ErrorString")
            .and_then(Value::as_string)
            .map(str::to_owned)
            .unwrap_or_else(|| i.to_string()),
        _ => {
            return Err(LockdownPairingError::InvalidResponse(
                "Error was not text or integer",
            ))
        }
    };
    Err(LockdownPairingError::from_code(&code))
}
pub struct LockdownPairingClient {
    host: Arc<Iap2UsbMuxHost>,
}
impl LockdownPairingClient {
    pub fn new(host: Arc<Iap2UsbMuxHost>) -> Self {
        Self { host }
    }
    /// Keeps the generated record across pending-dialog reconnects. Owns only its mux connection.
    pub fn pair(
        &self,
        label: &str,
        host_id: &str,
        system_buid: &str,
        total_timeout: Duration,
        is_cancelled: &dyn Fn() -> bool,
    ) -> PairingResult<LockdownPairRecord> {
        if [label, host_id, system_buid]
            .iter()
            .any(|s| s.trim().is_empty())
            || total_timeout.is_zero()
            || total_timeout > Duration::from_secs(300)
        {
            return Err(invalid("Invalid Lockdown pairing arguments").into());
        }
        let deadline = Instant::now() + total_timeout;
        let mut record = None;
        loop {
            check_cancelled(is_cancelled)?;
            let connection = self.host.connect(LOCKDOWN_PORT, step_timeout(deadline)?)?;
            let channel = LockdownPlistChannel::new(connection);
            let response = channel.request(
                &dictionary([
                    ("Label", Value::String(label.into())),
                    ("Request", Value::String("SetValue".into())),
                    ("Key", Value::String("UntrustedHostBUID".into())),
                    ("Value", Value::String(system_buid.into())),
                ]),
                step_timeout(deadline)?,
            )?;
            check_cancelled(is_cancelled)?;
            reject_error(&response)?;
            if record.is_none() {
                let public = get_value(&channel, label, "DevicePublicKey", deadline, is_cancelled)?;
                let wifi = get_value(&channel, label, "WiFiAddress", deadline, is_cancelled)?;
                let public = public
                    .as_data()
                    .ok_or(LockdownPairingError::InvalidResponse(
                        "DevicePublicKey was not data",
                    ))?;
                let wifi = wifi
                    .as_string()
                    .ok_or(LockdownPairingError::InvalidResponse(
                        "WiFiAddress was not text",
                    ))?;
                record = Some(LockdownPairRecord::generate(
                    public,
                    wifi,
                    host_id,
                    system_buid,
                )?);
            }
            let r = record.as_mut().unwrap();
            let response = channel.request(
                &dictionary([
                    ("Label", Value::String(label.into())),
                    (
                        "PairRecord",
                        Value::Dictionary(r.to_pair_request_dictionary()),
                    ),
                    ("Request", Value::String("Pair".into())),
                    ("ProtocolVersion", Value::String("2".into())),
                    (
                        "PairingOptions",
                        Value::Dictionary(dictionary([(
                            "ExtendedPairingErrors",
                            Value::Boolean(true),
                        )])),
                    ),
                ]),
                step_timeout(deadline)?,
            )?;
            check_cancelled(is_cancelled)?;
            match reject_error(&response) {
                Ok(()) => {
                    r.escrow_bag = match response.get("EscrowBag") {
                        None => None,
                        Some(Value::Data(b)) => Some(b.clone()),
                        _ => {
                            return Err(LockdownPairingError::InvalidResponse(
                                "EscrowBag was not data",
                            ))
                        }
                    };
                    return Ok(record.unwrap());
                }
                Err(LockdownPairingError::DialogPending) => (),
                Err(e) => return Err(e),
            }
            channel.close()?;
            let retry = Instant::now() + Duration::from_secs(1);
            while Instant::now() < retry {
                check_cancelled(is_cancelled)?;
                let remaining = step_timeout(deadline)?;
                thread::sleep(
                    remaining
                        .min(Duration::from_millis(100))
                        .min(retry.saturating_duration_since(Instant::now())),
                );
            }
        }
    }
}
fn get_value(
    channel: &LockdownPlistChannel,
    label: &str,
    key: &str,
    deadline: Instant,
    cancelled: &dyn Fn() -> bool,
) -> PairingResult<Value> {
    check_cancelled(cancelled)?;
    let mut response = channel.request(
        &dictionary([
            ("Label", Value::String(label.into())),
            ("Request", Value::String("GetValue".into())),
            ("Key", Value::String(key.into())),
        ]),
        step_timeout(deadline)?,
    )?;
    check_cancelled(cancelled)?;
    reject_error(&response)?;
    response
        .remove("Value")
        .ok_or(LockdownPairingError::InvalidResponse(
            "GetValue response omitted Value",
        ))
}
fn check_cancelled(cancelled: &dyn Fn() -> bool) -> PairingResult<()> {
    if cancelled() {
        Err(LockdownPairingError::Cancelled)
    } else {
        Ok(())
    }
}
fn step_timeout(deadline: Instant) -> PairingResult<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(LockdownPairingError::Timeout)
    } else {
        Ok(remaining.min(Duration::from_secs(5)))
    }
}
