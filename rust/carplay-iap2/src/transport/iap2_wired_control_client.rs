// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Wired identification, authentication, charging, subscriptions, and availability responses.
use crate::*;
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct Iap2WiredCarPlayEndpoint {
    pub ipv6_addresses: Vec<String>,
    pub air_play_port: u16,
    pub device_identifier: Option<String>,
    pub public_key: String,
    pub source_version: String,
}
impl Iap2WiredCarPlayEndpoint {
    /// Standard wired endpoint on the NCM link; the AirPlay listener must already be bound.
    pub fn new(
        air_play_port: u16,
        host_mac: [u8; 6],
        public_key: String,
        source_version: String,
    ) -> Result<Self> {
        let endpoint = Self {
            ipv6_addresses: vec!["fe80::2".into()],
            air_play_port,
            device_identifier: Some(
                host_mac
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(":"),
            ),
            public_key,
            source_version,
        };
        endpoint.validate()?;
        Ok(endpoint)
    }
    pub fn validate(&self) -> Result<()> {
        if self.ipv6_addresses.is_empty() {
            return Err(invalid("At least one wired IPv6 address is required"));
        }
        if self
            .ipv6_addresses
            .iter()
            .any(|s| s.contains('%') || s.parse::<std::net::Ipv6Addr>().is_err())
        {
            return Err(invalid("Every wired address must be an IPv6 text literal"));
        }
        if self.air_play_port == 0 {
            return Err(invalid("airPlayPort must be in 1..65535"));
        }
        for (value, name) in [
            (&self.public_key, "publicKey"),
            (&self.source_version, "sourceVersion"),
        ] {
            if value.is_empty() {
                return Err(invalid(format!("{name} is required and must not be empty")));
            }
            if value.contains('\0') {
                return Err(invalid(format!("{name} must not contain U+0000")));
            }
        }
        if self
            .device_identifier
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.contains('\0'))
        {
            return Err(invalid("Invalid deviceIdentifier"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2WiredControlStage {
    Identified,
    Authenticated,
    Subscribed,
    CarPlayStartSent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2WiredControlTerminal {
    ChannelClosed,
    TimedOut,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Iap2WiredControlResult {
    pub terminal: Iap2WiredControlTerminal,
    pub stage: Iap2WiredControlStage,
    pub forwarded_frames: u64,
    pub car_play_start_sessions_sent: u64,
}
/// Notifications run on the control worker, after each successful protocol operation.
pub trait WiredControlEvents: Send + Sync {
    fn on_stage_changed(&self, _stage: Iap2WiredControlStage) {}
    fn on_authenticated(&self) {}
    /// Called exactly once, only after the first successful 0x4301 send.
    fn on_start_session_sent(&self, _event: Iap2StartSessionSent) {}
    fn on_incoming(&self, _frame: &Iap2Frame) {}
    fn on_progress(&self, _message: &str) {}
}
pub struct Iap2WiredControlClient {
    session: Arc<Iap2Session>,
    mfi: Iap2MfiAuthenticationClient,
}
impl Iap2WiredControlClient {
    pub fn new(session: Arc<Iap2Session>, mfi: Arc<dyn MfiAuthenticator>) -> Self {
        Self {
            session,
            mfi: Iap2MfiAuthenticationClient::new(mfi),
        }
    }
    /// `None` bounds the handshake to 60 seconds, then runs indefinitely with <=30-second polls.
    /// A finite timeout covers the entire operation. The caller retains session ownership.
    pub fn run(
        &self,
        identification: &Iap2IdentificationConfig,
        endpoint: &Iap2WiredCarPlayEndpoint,
        available_current_milliamps: u16,
        timeout: Option<Duration>,
        events: &dyn WiredControlEvents,
    ) -> Result<Iap2WiredControlResult> {
        if identification.wireless.is_some() {
            return Err(invalid("Wired control requires a wired identification"));
        }
        if timeout.is_some_and(|t| t.is_zero() || t > Duration::from_secs(86400)) {
            return Err(invalid(
                "timeoutMillis must be in 1..86400000 or NO_TIMEOUT_MILLIS",
            ));
        }
        endpoint.validate()?;
        let mut deadline =
            Iap2ControlDeadline::new(timeout.unwrap_or(Duration::MAX), Instant::now());
        let progress = |line: &str| events.on_progress(line);
        Iap2IdentificationClient::new(self.session.clone()).identify_with_progress(
            identification,
            remaining(&deadline)?,
            &progress,
        )?;
        progress("iap2 identification accepted");
        events.on_stage_changed(Iap2WiredControlStage::Identified);
        self.mfi
            .run_with_progress(&self.session, remaining(&deadline)?, &progress)?;
        deadline.authenticated();
        progress("iap2 authentication accepted");
        events.on_stage_changed(Iap2WiredControlStage::Authenticated);
        events.on_authenticated();
        self.session.send(
            &Self::power_source_update(available_current_milliamps)?,
            remaining(&deadline)?,
        )?;
        for subscription in Self::subscriptions()? {
            self.session.send(&subscription, remaining(&deadline)?)?;
        }
        events.on_stage_changed(Iap2WiredControlStage::Subscribed);
        progress(&format!(
            "iap2 power/subscriptions sent availableCurrentMa={available_current_milliamps}"
        ));
        let mut result = Iap2WiredControlResult {
            terminal: Iap2WiredControlTerminal::TimedOut,
            stage: Iap2WiredControlStage::Subscribed,
            forwarded_frames: 0,
            car_play_start_sessions_sent: 0,
        };
        let mut log = Iap2RxLog::default();
        loop {
            let millis = deadline.remaining_millis(Instant::now());
            if millis == 0 {
                return Ok(result);
            }
            let incoming = match self.session.recv(Duration::from_millis(millis)) {
                Ok(Some(f)) => f,
                Ok(None) => continue,
                Err(e) => {
                    if self.session.is_closed()
                        && (e.kind() == io::ErrorKind::BrokenPipe
                            || e.to_string() == "iAP2 link ended")
                    {
                        result.terminal = Iap2WiredControlTerminal::ChannelClosed;
                        return Ok(result);
                    }
                    return Err(e);
                }
            };
            if incoming.message_id == 0x4300 {
                progress("iap2 rx=0x4300 carplay-availability");
                progress(&Self::car_play_availability_summary(&incoming.payload));
                self.session.send(
                    &Self::car_play_start_session(endpoint)?,
                    remaining(&deadline)?,
                )?;
                if result.car_play_start_sessions_sent == 0 {
                    events.on_start_session_sent(Iap2StartSessionSent {
                        sent_at_nanos:
                            crate::transport::iap2_wireless_control_client::monotonic_nanos(),
                    });
                    events.on_stage_changed(Iap2WiredControlStage::CarPlayStartSent);
                }
                result.car_play_start_sessions_sent += 1;
                result.stage = Iap2WiredControlStage::CarPlayStartSent;
                progress("iap2 tx=0x4301 carplay-start-session");
            } else {
                if let Some(line) = log.line(incoming.message_id) {
                    progress(&line);
                }
                events.on_incoming(&incoming);
                result.forwarded_frames += 1;
            }
        }
    }
    pub fn power_source_update(milliamps: u16) -> Result<Iap2Frame> {
        Iap2ControlMessages::power_source_update_with_options(
            milliamps,
            Some(true),
            None,
            None,
            false,
        )
    }
    pub fn subscriptions() -> Result<Vec<Iap2Frame>> {
        Iap2ControlMessages::subscriptions_with_route_guidance(true)
    }
    pub fn car_play_start_session(endpoint: &Iap2WiredCarPlayEndpoint) -> Result<Iap2Frame> {
        Iap2CarPlayMessages::start_wired_session(endpoint)
    }
    pub fn car_play_availability_summary(payload: &[u8]) -> String {
        let decoded = (|| {
            let frame = Iap2Frame {
                message_id: 0x4300,
                payload: payload.to_vec(),
            };
            Iap2CarPlayMessages::availability(&frame)?;
            let body = Iap2BodyReader::of(&frame)?;
            let group = body.optional_group(0)?;
            let available = group
                .as_ref()
                .map(|g| g.optional_u8(0))
                .transpose()?
                .flatten()
                .map(|v| (v != 0).to_string())
                .unwrap_or_else(|| "null".into());
            let transport = group
                .as_ref()
                .map(|g| g.optional_string(1))
                .transpose()?
                .flatten()
                .unwrap_or_else(|| "none".into());
            Ok::<_, io::Error>(format!(
                "iap2 4300 wiredAvailable={available} usbTransport={transport}"
            ))
        })();
        decoded.unwrap_or_else(|e| format!("iap2 4300 decode failed: {e}"))
    }
}
fn remaining(deadline: &Iap2ControlDeadline) -> Result<Duration> {
    let millis = deadline.remaining_millis(Instant::now());
    if millis == 0 {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Timed out during wired iAP2 control bring-up",
        ))
    } else {
        Ok(Duration::from_millis(millis))
    }
}
