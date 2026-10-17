// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::{sync::Arc, time::Duration};

/// `Iap2WirelessIdentification` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2WirelessIdentification {
    pub bluetooth_mac: String,

    pub ssid: String,
}
/// `Iap2WirelessLinkRole` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2WirelessLinkRole {
    BluetoothBootstrap,
    RuntimeTunnel,
}
/// `Iap2IdentificationConfig` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2IdentificationConfig {
    /// Opt in to DiPlay route guidance identification and subscriptions.
    pub route_guidance: bool,
    pub name: String,

    pub model_identifier: String,

    pub manufacturer: String,

    pub serial_number: String,

    pub firmware_version: String,

    pub hardware_version: String,

    pub wireless: Option<Iap2WirelessIdentification>,
    /// Identification language (default en).
    pub language: String,
    /// Accessory EA identifier; do not claim a CarPlay EA flag.
    pub external_accessory_protocol: String,
}
impl Iap2IdentificationConfig {
    pub fn for_wireless_link(
        &self,
        role: Iap2WirelessLinkRole,
        wireless: Iap2WirelessIdentification,
    ) -> Self {
        let _ = role;
        Self {
            wireless: Some(wireless),
            ..self.clone()
        }
    }
}
/// State owned by `Iap2IdentificationClient`; construct through its factory.
pub struct Iap2IdentificationClient {
    session: Arc<Iap2Session>,
}
impl Iap2IdentificationClient {
    pub fn new(session: Arc<Iap2Session>) -> Self {
        Self { session }
    }

    pub fn identify(&self, config: &Iap2IdentificationConfig, timeout: Duration) -> Result<()> {
        self.identify_with_progress(config, timeout, &|_| {})
    }

    pub fn identification_information(
        config: &Iap2IdentificationConfig,
        omitted: &[u16],
    ) -> Result<Iap2Frame> {
        let mut b = Iap2BodyBuilder::of(&Iap2Endpoints::require(0x1d01)?);
        for (id, value) in [
            (0, &config.name),
            (1, &config.model_identifier),
            (2, &config.manufacturer),
            (3, &config.serial_number),
            (4, &config.firmware_version),
            (5, &config.hardware_version),
        ] {
            if value.trim().is_empty() {
                return Err(invalid("Identification identity strings must not be blank"));
            }
            b.string(id, value)?;
        }
        if config.language.trim().is_empty() {
            return Err(invalid("Identification language must not be blank"));
        }
        if config.external_accessory_protocol.trim().is_empty() {
            return Err(invalid("External accessory protocol must not be blank"));
        }
        // Vehicle and location providers remain excluded. Route guidance is opt-in.
        let mut sent = vec![
            0xaa01, 0xaa03, 0x5000, 0x5002, 0xae00, 0xae02, 0x4157, 0x4159, 0x4154, 0x4156, 0xae03,
            0x4301,
        ];
        let mut received = vec![
            0xaa00, 0xaa02, 0xaa04, 0xaa05, 0xea00, 0xea01, 0x5001, 0xae01, 0x4158, 0x4155, 0x4300,
        ];
        if config.route_guidance {
            sent.splice(4..4, [0x5200, 0x5203]);
            received.splice(7..7, [0x5201, 0x5202]);
        }
        if config.wireless.is_some() {
            sent.retain(|v| *v != 0xae03);
            sent.push(0x5703);
            received.extend([0x4e0d, 0x4e0e, 0x5702]);
        }
        sent.retain(|v| !omitted.contains(v));
        received.retain(|v| !omitted.contains(v));
        b.u16_list(6, &sent)?
            .u16_list(7, &received)?
            .u8(8, if config.wireless.is_some() { 0 } else { 2 })?
            .u16(9, 20)?;
        let mut ea = Iap2BodyBuilder::opaque(0);
        ea.u8(0, 1)?
            .string(1, &config.external_accessory_protocol)?
            .u8(2, 0)?;
        b.group(10, ea)?
            .string(12, &config.language)?
            .strings(13, std::slice::from_ref(&config.language))?;
        if let Some(w) = &config.wireless {
            if w.ssid.trim().is_empty() {
                return Err(invalid("Wireless SSID must not be blank"));
            }
            let pieces: Vec<_> = w.bluetooth_mac.split(':').collect();
            if pieces.len() != 6
                || pieces
                    .iter()
                    .any(|s| s.len() != 2 || !s.bytes().all(|v| v.is_ascii_hexdigit()))
            {
                return Err(invalid(
                    "bluetoothMac must be six colon-separated hexadecimal bytes",
                ));
            }
            let mac: Vec<u8> = pieces
                .iter()
                .map(|s| u8::from_str_radix(s, 16).unwrap())
                .collect();
            let mut blue = Iap2BodyBuilder::opaque(0);
            blue.u16(0, 0)?
                .string(1, "blue")?
                .void(2)?
                .bytes(3, &mac)?
                .string(4, "blue")?
                .void(5)?;
            b.group(17, blue)?;
            let mut wifi = Iap2BodyBuilder::opaque(0);
            wifi.u16(0, 1)?
                .string(1, &w.ssid)?
                .void(2)?
                .u16(3, 1)?
                .void(4)?
                .void(5)?;
            b.group(24, wifi)?;
        } else {
            let mut usb = Iap2BodyBuilder::opaque(0);
            usb.u16(0, 0)?
                .string(1, "USBHostTransport")?
                .void(2)?
                .u8(3, 0)?
                .void(4)?;
            b.group(16, usb)?;
        }
        if config.route_guidance {
            let mut route = Iap2BodyBuilder::opaque(0);
            route
                .u16(0, 42)?
                .string(1, "RouteGuidance")?
                .u16(2, 64)?
                .u16(4, 64)?
                .u16(6, 8)?;
            b.group(30, route)?;
        }
        b.build()
    }
}

/// Protocol message, field, or framing constant.
pub const START_IDENTIFICATION: u64 = 0x1d00;
/// Protocol message, field, or framing constant.
pub const IDENTIFICATION_INFORMATION: u64 = 0x1d01;
/// Protocol message, field, or framing constant.
pub const IDENTIFICATION_ACCEPTED: u64 = 0x1d02;
/// Protocol message, field, or framing constant.
pub const IDENTIFICATION_REJECTED: u64 = 0x1d03;
/// Protocol message, field, or framing constant.
pub const DEFAULT_TIMEOUT_MILLIS: u64 = 10_000;
/// Protocol message, field, or framing constant.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;
/// Protocol message, field, or framing constant.
pub const MAX_REJECTED_MESSAGE_IDS: u64 = 32;
/// Protocol message, field, or framing constant.
pub const POWER_SOURCE_UPDATE: u64 = 0xae03;
/// Protocol message, field, or framing constant.
pub const CARPLAY_AVAILABILITY: u64 = 0x4300;
/// Protocol message, field, or framing constant.
pub const CARPLAY_START_SESSION: u64 = 0x4301;
/// Protocol message, field, or framing constant.
pub const ACCESSORY_WIFI_CONFIGURATION_INFORMATION: u64 = 0x5703;
/// Protocol message, field, or framing constant.
pub const LOCATION_INFORMATION: u64 = 0xfffb;
/// Protocol message, field, or framing constant.
pub const START_LOCATION_INFORMATION: u64 = 0xfffa;
/// Protocol message, field, or framing constant.
pub const STOP_LOCATION_INFORMATION: u64 = 0xfffc;

/// Constant expression: `5 * 60 * 1_000`.
pub const MAXIMUM_TIMEOUT_MILLIS: u64 = 300000;

impl Iap2IdentificationClient {
    pub(crate) fn identify_with_progress(
        &self,
        config: &Iap2IdentificationConfig,
        timeout: Duration,
        progress: &dyn Fn(&str),
    ) -> Result<()> {
        if timeout.is_zero() || timeout > Duration::from_secs(300) {
            return Err(invalid("timeoutMillis must be in 1..300000"));
        }
        let started = std::time::Instant::now();
        let remaining = || -> Result<Duration> {
            let r = timeout.saturating_sub(started.elapsed());
            if r.is_zero() {
                Err(invalid("iAP2 identification timed out"))
            } else {
                Ok(r)
            }
        };
        if !self.session.await_ready(remaining()?)? {
            return Err(invalid(
                "Timed out waiting for iAP2 control session readiness",
            ));
        }
        let mut omitted = None;
        loop {
            let frame = self
                .session
                .recv(remaining()?)?
                .ok_or_else(|| invalid("Timed out waiting for iAP2 identification"))?;
            match frame.message_id {
                0x1d00 => self.session.send(
                    &Self::identification_information(config, omitted.as_deref().unwrap_or(&[]))?,
                    remaining()?,
                )?,
                0x1d02 => return Ok(()),
                0x1d03 => {
                    let rejection = identification_rejection(&frame)?;
                    let Some(ids) = rejection.omittable_messages().filter(|_| omitted.is_none())
                    else {
                        return Err(invalid(rejection.diagnostic()));
                    };
                    progress(&format!(
                        "iap2 identification retry without unsupported messages {}",
                        hex_ids(&ids)
                    ));
                    self.session.send(
                        &Self::identification_information(config, &ids)?,
                        remaining()?,
                    )?;
                    omitted = Some(ids);
                }
                id => {
                    return Err(invalid(format!(
                        "Unexpected iAP2 identification message 0x{id:04x}"
                    )))
                }
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Iap2RejectedMessageList {
    pub message_ids: Vec<u16>,
    pub payload_count: usize,
    pub empty_payload_count: usize,
    pub malformed_payload_count: usize,
    pub omitted_message_count: usize,
}
#[derive(Clone, Debug)]
pub struct Iap2IdentificationRejection {
    pub parameter_ids: Vec<u16>,
    pub unsupported_messages_sent: Option<Iap2RejectedMessageList>,
    pub unsupported_messages_received: Option<Iap2RejectedMessageList>,
}
fn hex_ids(ids: &[u16]) -> String {
    format!(
        "[{}]",
        ids.iter()
            .map(|id| format!("0x{id:04x}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
impl Iap2RejectedMessageList {
    pub fn diagnostic(&self) -> String {
        let unique: std::collections::HashSet<_> = self.message_ids.iter().collect();
        format!("{} payloads={} empty={} malformed={} duplicateParameters={} duplicateRetainedIds={} omitted={}",hex_ids(&self.message_ids),self.payload_count,self.empty_payload_count,self.malformed_payload_count,self.payload_count>1,unique.len()!=self.message_ids.len(),self.omitted_message_count)
    }
}
impl Iap2IdentificationRejection {
    pub fn diagnostic(&self) -> String {
        let d = |l: &Option<Iap2RejectedMessageList>| {
            l.as_ref()
                .map(|v| v.diagnostic())
                .unwrap_or_else(|| "absent".into())
        };
        format!("iAP2 identification rejected before MFi authentication; parameters {}; unsupported messages sent={} received={}",hex_ids(&self.parameter_ids),d(&self.unsupported_messages_sent),d(&self.unsupported_messages_received))
    }
    pub fn omittable_messages(&self) -> Option<Vec<u16>> {
        if self.parameter_ids.is_empty() || self.parameter_ids.iter().any(|id| *id != 6 && *id != 7)
        {
            return None;
        }
        let mut ids = Vec::new();
        for l in [
            &self.unsupported_messages_sent,
            &self.unsupported_messages_received,
        ]
        .into_iter()
        .flatten()
        {
            if l.malformed_payload_count > 0 || l.omitted_message_count > 0 {
                return None;
            }
            ids.extend(&l.message_ids);
        }
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() || ids.iter().any(|id| (0xaa00..=0xaa05).contains(id)) {
            None
        } else {
            Some(ids)
        }
    }
}
pub fn identification_rejection(frame: &Iap2Frame) -> Result<Iap2IdentificationRejection> {
    let params = frame.body()?;
    let mut ids: Vec<_> = params.parameters.iter().map(|p| p.id).collect();
    ids.sort_unstable();
    ids.dedup();
    let list = |id| {
        let payloads = params.all(id);
        if payloads.is_empty() {
            return None;
        }
        let mut result = Iap2RejectedMessageList {
            message_ids: Vec::new(),
            payload_count: payloads.len(),
            empty_payload_count: 0,
            malformed_payload_count: 0,
            omitted_message_count: 0,
        };
        for p in payloads {
            if p.payload.is_empty() {
                result.empty_payload_count += 1;
            } else if p.payload.len() % 2 != 0 {
                result.malformed_payload_count += 1;
            } else {
                for &pair in p.payload.as_chunks::<2>().0 {
                    if result.message_ids.len() < 32 {
                        result.message_ids.push(u16::from_be_bytes(pair));
                    } else {
                        result.omitted_message_count += 1;
                    }
                }
            }
        }
        Some(result)
    };
    Ok(Iap2IdentificationRejection {
        parameter_ids: ids,
        unsupported_messages_sent: list(6),
        unsupported_messages_received: list(7),
    })
}
