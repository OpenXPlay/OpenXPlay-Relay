// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::collections::VecDeque;
use std::time::Instant;
use std::{sync::Arc, time::Duration};

/// `Iap2WirelessSecurity` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Iap2WirelessSecurity {
    None = 0,
    Wep = 1,
    WpaWpa2 = 2,
    Wpa3Transition = 3,
    Wpa3Only = 4,
}
/// `Iap2WirelessCarPlayEndpoint` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2WirelessCarPlayEndpoint {
    pub ssid: String,

    pub passphrase: String,

    pub channel: u8,

    pub security: Iap2WirelessSecurity,

    pub ip_addresses: Vec<String>,

    pub air_play_port: u16,

    pub device_identifier: String,

    pub public_key: String,

    pub source_version: String,

    pub access_point_bssid: Option<[u8; 6]>,
}
/// `Iap2WirelessControlResult` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2WirelessControlResult {
    /// Counters and observations for protocol diagnostics.
    pub forwarded_frames: u32,
    /// Wi-Fi configuration sends before transport notification.
    pub wifi_configurations_sent: u32,
    /// StartSession sends.
    pub car_play_start_sessions_sent: u32,
    /// Whether a device transport notification was received.
    pub transport_notification_seen: bool,
    /// Wi-Fi sends following transport notification.
    pub post_transport_wifi_configurations_sent: u32,
    /// Whether wireless availability was reported.
    pub wireless_car_play_available_seen: bool,

    pub terminal: Iap2WirelessControlTerminal,

    pub stage: Iap2WirelessControlStage,
}
/// `Iap2WirelessControlTerminal` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2WirelessControlTerminal {
    ChannelClosed,
    TimedOut,
}
/// `Iap2WirelessControlStage` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Iap2WirelessControlStage {
    Identified,
    Authenticated,
    Subscribed,
    WifiConfigSent,
    CarplayStartSent,
    TransportNotified,
    PostTransportWifiConfigSent,
}
/// `Iap2StartSessionSent` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2StartSessionSent {
    pub sent_at_nanos: u64,
}
/// Platform interface for `WirelessControlEvents`. Implementations must support bounded shutdown.
pub trait WirelessControlEvents: Send + Sync {
    /// Observe each successfully reached bring-up stage in increasing order; default leaves host reporting unchanged.
    fn on_stage_changed(&self, _stage: Iap2WirelessControlStage) {}
    /// Validate the host network immediately before each permitted Wi-Fi configuration send; errors abort bring-up.
    fn before_wifi_configuration(&self) -> Result<()> {
        Ok(())
    }
    /// Errors propagate to the lifecycle owner.
    fn before_start_session(&self) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn on_start_session_sent(&self, event: Iap2StartSessionSent);
    /// Errors propagate to the lifecycle owner.
    fn on_ready(&self);
    /// Errors propagate to the lifecycle owner.
    fn on_progress(&self, message: &str);
    /// Errors propagate to the lifecycle owner.
    fn keep_alive(&self) -> bool;
    fn on_incoming(&self, _frame: &Iap2Frame) {}
    /// An accessory Wi-Fi configuration (0x5703) went out.
    fn on_wifi_configuration_sent(&self) {}
}
/// Supplies the accessory's wireless endpoint once its network is up. Identification and MFi
/// authentication need no endpoint, so they can run while the access point is still starting;
/// the answers that carry the network's credentials or address, the Wi-Fi configuration (0x5703)
/// and StartSession (0x4301), wait for it.
pub trait WirelessEndpointSource: Send + Sync {
    /// The endpoint once the network is up, `Ok(None)` while it is still starting. It is polled
    /// without blocking while an answer waits for it; an error ends the bring-up.
    fn endpoint(&self) -> Result<Option<Iap2WirelessCarPlayEndpoint>>;
}
impl WirelessEndpointSource for Iap2WirelessCarPlayEndpoint {
    fn endpoint(&self) -> Result<Option<Iap2WirelessCarPlayEndpoint>> {
        Ok(Some(self.clone()))
    }
}
/// How long an answer may wait for the endpoint before the bring-up fails. The iPhone asks for
/// the Wi-Fi configuration right after authentication; the access point normally needs about
/// 1.5 s more, and an iPhone left waiting much longer may give up on the accessory.
pub const DEFAULT_ENDPOINT_WAIT: Duration = Duration::from_secs(10);
/// How often a waiting answer checks the endpoint while no frame arrives.
pub const ENDPOINT_POLL: Duration = Duration::from_millis(20);
/// An iPhone request whose answer carries the endpoint, kept in arrival order until it is known.
#[derive(Clone, Copy, Debug)]
enum Answer {
    /// 0x5702; `post` when it followed the device transport notification.
    WifiConfiguration { post: bool },
    /// 0x4300.
    StartSession,
    /// 0x4e0e.
    PostTransportWifiConfiguration,
}
/// State owned by `Iap2WirelessControlClient`; construct through its factory.
pub struct Iap2WirelessControlClient {
    session: Arc<Iap2Session>,
    mfi: Iap2MfiAuthenticationClient,
    endpoint_wait: Duration,
}
impl Iap2WirelessControlClient {
    pub fn new(session: Arc<Iap2Session>, mfi: Arc<dyn MfiAuthenticator>) -> Self {
        Self {
            session,
            mfi: Iap2MfiAuthenticationClient::new(mfi),
            endpoint_wait: DEFAULT_ENDPOINT_WAIT,
        }
    }
    /// How long an answer may wait for a deferred endpoint (default [`DEFAULT_ENDPOINT_WAIT`]).
    pub fn with_endpoint_wait(mut self, wait: Duration) -> Self {
        self.endpoint_wait = wait;
        self
    }

    pub fn run(
        &self,
        identification: &Iap2IdentificationConfig,
        endpoint: &Iap2WirelessCarPlayEndpoint,
        timeout: Option<Duration>,
        events: &dyn WirelessControlEvents,
    ) -> Result<Iap2WirelessControlResult> {
        Self::check_arguments(identification, timeout)?;
        endpoint.validate()?;
        self.run_deferred(identification, endpoint, timeout, events)
    }

    fn check_arguments(
        identification: &Iap2IdentificationConfig,
        timeout: Option<Duration>,
    ) -> Result<()> {
        if identification.wireless.is_none() {
            return Err(invalid(
                "Wireless control requires an Iap2IdentificationConfig with wireless transport",
            ));
        }
        if timeout.is_some_and(|t| t.is_zero() || t > Duration::from_secs(86400)) {
            return Err(invalid(
                "timeoutMillis must be in 1..86400000 or NO_TIMEOUT_MILLIS",
            ));
        }
        Ok(())
    }

    /// Like [`Self::run`], but takes the endpoint from `source` only when an answer needs it:
    /// identification and authentication proceed while the accessory's network is starting, and
    /// the answers that need it go out in request order once it is up, at most
    /// [`Self::with_endpoint_wait`] after the first of them was requested.
    pub fn run_deferred(
        &self,
        identification: &Iap2IdentificationConfig,
        source: &dyn WirelessEndpointSource,
        timeout: Option<Duration>,
        events: &dyn WirelessControlEvents,
    ) -> Result<Iap2WirelessControlResult> {
        Self::check_arguments(identification, timeout)?;
        let mut deadline = WirelessControlDeadline::new(timeout, Instant::now());
        let progress = |message: &str| events.on_progress(message);
        Iap2IdentificationClient::new(self.session.clone()).identify_with_progress(
            identification,
            deadline.require_remaining(Instant::now(), false)?,
            &progress,
        )?;
        progress("iap2 identification accepted");
        events.on_stage_changed(Iap2WirelessControlStage::Identified);
        self.mfi.run_with_progress(
            &self.session,
            deadline.require_remaining(Instant::now(), false)?,
            &progress,
        )?;
        progress("iap2 authentication accepted");
        events.on_stage_changed(Iap2WirelessControlStage::Authenticated);
        for subscription in
            Iap2ControlMessages::subscriptions_with_route_guidance(identification.route_guidance)?
        {
            self.session.send(
                &subscription,
                deadline.require_remaining(Instant::now(), false)?,
            )?;
        }
        progress("iap2 subscriptions sent");
        events.on_stage_changed(Iap2WirelessControlStage::Subscribed);
        events.on_ready();
        let mut result = Iap2WirelessControlResult {
            forwarded_frames: 0,
            wifi_configurations_sent: 0,
            car_play_start_sessions_sent: 0,
            transport_notification_seen: false,
            post_transport_wifi_configurations_sent: 0,
            wireless_car_play_available_seen: false,
            terminal: Iap2WirelessControlTerminal::TimedOut,
            stage: Iap2WirelessControlStage::Subscribed,
        };
        let mut rx = Iap2RxLog::default();
        let mut pre_transport = 0;
        let mut endpoint = None;
        let mut answers = VecDeque::new();
        let mut waiting: Option<Instant> = None;
        loop {
            if !answers.is_empty() {
                if endpoint.is_none() {
                    if let Some(e) = source.endpoint()? {
                        e.validate()?;
                        if let Some(since) = waiting.take() {
                            progress(&format!(
                                "iap2 accessory network ready afterMs={}",
                                since.elapsed().as_millis()
                            ));
                        }
                        endpoint = Some(e);
                    }
                }
                match &endpoint {
                    Some(e) => {
                        while let Some(answer) = answers.pop_front() {
                            self.answer(
                                answer,
                                e,
                                &mut result,
                                &mut pre_transport,
                                &mut deadline,
                                events,
                            )?;
                        }
                    }
                    None => {
                        let since = *waiting.get_or_insert_with(|| {
                            progress("iap2 answer waits for the accessory network");
                            Instant::now()
                        });
                        if since.elapsed() >= self.endpoint_wait {
                            return Err(invalid(format!(
                                "Accessory network not ready {} ms after the iPhone's request",
                                since.elapsed().as_millis()
                            )));
                        }
                    }
                }
            }
            let remaining = deadline.remaining_with(Instant::now(), || events.keep_alive());
            if remaining.is_zero() {
                return Ok(result);
            }
            // A waiting answer checks the endpoint again after each frame and each poll.
            let wait = if answers.is_empty() {
                remaining
            } else {
                remaining.min(ENDPOINT_POLL)
            };
            let incoming = match self.session.recv(wait) {
                Ok(Some(f)) => f,
                Ok(None) => continue,
                Err(e) => {
                    if self.session.is_closed()
                        && (e.kind() == std::io::ErrorKind::BrokenPipe
                            || e.to_string() == "iAP2 link ended")
                    {
                        result.terminal = Iap2WirelessControlTerminal::ChannelClosed;
                        return Ok(result);
                    }
                    return Err(e);
                }
            };
            match incoming.message_id {
                0x5702 => {
                    progress("iap2 rx=0x5702 request-wifi-configuration");
                    answers.push_back(Answer::WifiConfiguration {
                        post: result.transport_notification_seen,
                    });
                }
                0x4300 => {
                    progress("iap2 rx=0x4300 carplay-availability");
                    progress(&car_play_availability_diagnostic(&incoming));
                    answers.push_back(Answer::StartSession);
                }
                0x4e0d => {
                    let available =
                        Iap2WirelessMessages::wireless_car_play_availability(&incoming)?;
                    result.wireless_car_play_available_seen |= available;
                    progress(&format!(
                        "iap2 rx=0x4e0d wireless-carplay-update available={available}"
                    ));
                }
                0x4e0e => {
                    let body = Iap2BodyReader::of(&incoming)?;
                    let bluetooth = body.optional_string(0)?.unwrap_or_else(|| "none".into());
                    let usb = body.optional_string(1)?.unwrap_or_else(|| "none".into());
                    result.transport_notification_seen = true;
                    result.advance(Iap2WirelessControlStage::TransportNotified, events);
                    progress(&format!("iap2 rx=0x4e0e device-transport-identifier bluetooth={bluetooth} usb={usb}; resending 0x5703"));
                    answers.push_back(Answer::PostTransportWifiConfiguration);
                }
                _ => {
                    if let Some(line) = rx.line(incoming.message_id) {
                        progress(&line);
                    }
                    events.on_incoming(&incoming);
                    result.forwarded_frames += 1;
                }
            }
        }
    }

    /// Answers one request with the known endpoint.
    fn answer(
        &self,
        answer: Answer,
        endpoint: &Iap2WirelessCarPlayEndpoint,
        result: &mut Iap2WirelessControlResult,
        pre_transport: &mut u32,
        deadline: &mut WirelessControlDeadline,
        events: &dyn WirelessControlEvents,
    ) -> Result<()> {
        let progress = |message: &str| events.on_progress(message);
        match answer {
            Answer::WifiConfiguration { post } => {
                let count = if post {
                    result.post_transport_wifi_configurations_sent
                } else {
                    *pre_transport
                };
                let limit = if post { 2 } else { 5 };
                if count >= limit {
                    progress("iap2 0x5703 ignored: maximum Wi-Fi configuration sends reached");
                    return Ok(());
                }
                events.before_wifi_configuration()?;
                self.session.send(
                    &Self::accessory_wifi_configuration(endpoint)?,
                    deadline.require_remaining_with(Instant::now(), || events.keep_alive())?,
                )?;
                result.wifi_configurations_sent += 1;
                if post {
                    result.post_transport_wifi_configurations_sent += 1;
                    result.advance(
                        Iap2WirelessControlStage::PostTransportWifiConfigSent,
                        events,
                    );
                } else {
                    *pre_transport += 1;
                    result.advance(Iap2WirelessControlStage::WifiConfigSent, events);
                }
                progress("iap2 tx=0x5703 accessory-wifi-configuration");
                events.on_wifi_configuration_sent();
            }
            Answer::StartSession => {
                events.before_start_session()?;
                send_start_session(
                    endpoint,
                    |frame| {
                        self.session.send(
                            &frame,
                            deadline
                                .require_remaining_with(Instant::now(), || events.keep_alive())?,
                        )
                    },
                    |event| events.on_start_session_sent(event),
                )?;
                result.advance(Iap2WirelessControlStage::CarplayStartSent, events);
                result.car_play_start_sessions_sent += 1;
                progress("iap2 tx=0x4301 carplay-start-session");
            }
            Answer::PostTransportWifiConfiguration => {
                if result.post_transport_wifi_configurations_sent >= 2 {
                    progress("iap2 post-transport 0x5703 ignored: maximum Wi-Fi configuration sends reached");
                    return Ok(());
                }
                events.before_wifi_configuration()?;
                self.session.send(
                    &Self::accessory_wifi_configuration(endpoint)?,
                    deadline.require_remaining_with(Instant::now(), || events.keep_alive())?,
                )?;
                result.wifi_configurations_sent += 1;
                result.post_transport_wifi_configurations_sent += 1;
                result.advance(
                    Iap2WirelessControlStage::PostTransportWifiConfigSent,
                    events,
                );
                progress("iap2 tx=0x5703 post-transport accessory-wifi-configuration");
                events.on_wifi_configuration_sent();
            }
        }
        Ok(())
    }

    pub fn accessory_wifi_configuration(
        endpoint: &Iap2WirelessCarPlayEndpoint,
    ) -> Result<Iap2Frame> {
        Iap2WirelessMessages::accessory_wifi_configuration(endpoint)
    }

    pub fn car_play_start_session(endpoint: &Iap2WirelessCarPlayEndpoint) -> Result<Iap2Frame> {
        Iap2CarPlayMessages::start_session(endpoint)
    }
}

impl Iap2WirelessControlResult {
    fn advance(&mut self, stage: Iap2WirelessControlStage, events: &dyn WirelessControlEvents) {
        if stage > self.stage {
            self.stage = stage;
            events.on_stage_changed(stage);
        }
    }
}

/// Protocol message, field, or framing constant.
pub const REQUEST_ACCESSORY_WIFI_CONFIGURATION: u64 = 0x5702;
/// Protocol message, field, or framing constant.
pub const ACCESSORY_WIFI_CONFIGURATION: u64 = 0x5703;
/// Protocol message, field, or framing constant.
pub const CARPLAY_AVAILABILITY: u64 = 0x4300;
/// Protocol message, field, or framing constant.
pub const CARPLAY_START_SESSION: u64 = 0x4301;
/// Protocol message, field, or framing constant.
pub const WIRELESS_CARPLAY_UPDATE: u64 = 0x4e0d;
/// Protocol message, field, or framing constant.
pub const DEVICE_TRANSPORT_IDENTIFIER_NOTIFICATION: u64 = 0x4e0e;
/// Protocol message, field, or framing constant.
pub const DEFAULT_TIMEOUT_MILLIS: u64 = 60_000;
/// Protocol message, field, or framing constant.
pub const MAX_PRE_TRANSPORT_WIFI_CONFIGURATION_SENDS: u64 = 5;
/// Protocol message, field, or framing constant.
pub const MAX_POST_TRANSPORT_WIFI_CONFIGURATION_SENDS: u64 = 2;
/// Protocol message, field, or framing constant.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;

/// Constant expression: `9223372036854775807`.
pub const NO_TIMEOUT_MILLIS: u64 = 9223372036854775807;

/// Constant expression: `24 * 60 * 60 * 1_000`.
pub const MAX_TIMEOUT_MILLIS: u64 = 86400000;

/// Constant expression: `5 * 60 * 1_000`.
pub const MAX_POLL_MILLIS: u64 = 300000;

impl Iap2WirelessCarPlayEndpoint {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.ssid.trim().is_empty() {
            return Err(invalid("ssid is required and must not be blank"));
        }
        if self.security != Iap2WirelessSecurity::None && self.passphrase.is_empty() {
            return Err(invalid("passphrase is required for secured Wi-Fi"));
        }
        if self.ip_addresses.is_empty() {
            return Err(invalid("At least one wireless IP address is required"));
        }
        if self.air_play_port == 0 {
            return Err(invalid("airPlayPort must be in 1..65535"));
        }
        for (name, value) in [
            ("deviceIdentifier", &self.device_identifier),
            ("publicKey", &self.public_key),
            ("sourceVersion", &self.source_version),
        ] {
            if value.trim().is_empty() {
                return Err(invalid(format!("{name} is required and must not be empty")));
            }
        }
        for value in self.ip_addresses.iter().chain([
            &self.ssid,
            &self.passphrase,
            &self.device_identifier,
            &self.public_key,
            &self.source_version,
        ]) {
            if value.contains('\0') {
                return Err(invalid("NUL-terminated strings must not contain U+0000"));
            }
        }
        if self.ip_addresses.iter().any(|s| s.trim().is_empty()) {
            return Err(invalid(
                "Every wireless IP address must be non-blank and must not contain U+0000",
            ));
        }
        Ok(())
    }
}

/// Bring-up budget can be extended only while the lifecycle owner proves liveness.
pub struct WirelessControlDeadline {
    started: Instant,
    timeout: Option<Duration>,
    extended: bool,
}
impl WirelessControlDeadline {
    pub fn new(timeout: Option<Duration>, now: Instant) -> Self {
        Self {
            started: now,
            timeout,
            extended: false,
        }
    }
    pub fn remaining(&mut self, now: Instant, keep_alive: bool) -> Duration {
        self.remaining_with(now, || keep_alive)
    }
    pub fn remaining_with(&mut self, now: Instant, keep_alive: impl FnOnce() -> bool) -> Duration {
        let poll = Duration::from_secs(300);
        if self.extended {
            return if keep_alive() { poll } else { Duration::ZERO };
        }
        let Some(timeout) = self.timeout else {
            return poll;
        };
        let left = timeout.saturating_sub(now.saturating_duration_since(self.started));
        if left.is_zero() && keep_alive() {
            self.extended = true;
            return poll;
        }
        left.min(poll)
    }
    fn require_remaining(&mut self, now: Instant, keep_alive: bool) -> Result<Duration> {
        self.require_remaining_with(now, || keep_alive)
    }
    fn require_remaining_with(
        &mut self,
        now: Instant,
        keep_alive: impl FnOnce() -> bool,
    ) -> Result<Duration> {
        let remaining = self.remaining_with(now, keep_alive);
        if remaining.is_zero() {
            Err(invalid("Timed out during wireless iAP2 control bring-up"))
        } else {
            Ok(remaining)
        }
    }
}
pub(crate) fn monotonic_nanos() -> u64 {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
        .min(u64::MAX as u128) as u64
        + 1
}
pub fn car_play_availability_diagnostic(frame: &Iap2Frame) -> String {
    let decode = || -> Result<String> {
        Iap2CarPlayMessages::availability(frame)?;
        let body = Iap2BodyReader::of(frame)?;
        let mut values = Vec::new();
        for id in 0..3 {
            values.push(
                body.optional_group(id)?
                    .map(|g| g.optional_u8(0))
                    .transpose()?
                    .flatten()
                    .map(|v| (v == 1).to_string())
                    .unwrap_or_else(|| "unknown".into()),
            );
        }
        Ok(format!(
            "iap2 availability wired={} wireless={} themeAssets={}",
            values[0], values[1], values[2]
        ))
    };
    decode().unwrap_or_else(|_| {
        "iap2 availability decode=failed failureClass=Iap2ProtocolException".into()
    })
}

/// Emits the lifecycle event only after the complete StartSession write succeeds.
pub fn send_start_session(
    endpoint: &Iap2WirelessCarPlayEndpoint,
    send: impl FnOnce(Iap2Frame) -> Result<()>,
    on_sent: impl FnOnce(Iap2StartSessionSent),
) -> Result<()> {
    send(Iap2WirelessControlClient::car_play_start_session(endpoint)?)?;
    on_sent(Iap2StartSessionSent {
        sent_at_nanos: monotonic_nanos(),
    });
    Ok(())
}
