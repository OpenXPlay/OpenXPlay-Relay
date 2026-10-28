// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Source: admin-web/src/{api,schema}.ts (Linux-only extension).
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub type Values = Map<String, Value>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminStatus {
    #[serde(default)]
    pub path_input: Value,
    #[serde(default)]
    pub admin_features: Vec<String>,
    #[serde(default)]
    pub host_addresses: Vec<AdminHostAddress>,
    #[serde(default)]
    pub hotspot: Option<AdminHotspot>,
    #[serde(default)]
    pub wifi_control: bool,
    #[serde(default)]
    pub receiver_port: u16,
    #[serde(default)]
    pub admin_port: u16,
    #[serde(default)]
    pub pairing_available: bool,
    /// When the pairing window closes, on the relay's wall clock: for display only, since that
    /// clock may be wrong.
    #[serde(default)]
    pub pairing_deadline: Option<u64>,
    /// Seconds left at response time, so browsers with a wrong clock still count down correctly.
    #[serde(default)]
    pub pairing_remaining: Option<u64>,
    #[serde(default)]
    pub pairing_cancelable: bool,
    /// Bonded Bluetooth devices; names appear only in this authenticated response, never in logs.
    #[serde(default)]
    pub paired_devices: Vec<AdminPairedDevice>,
    #[serde(default)]
    pub video: Value,
    /// Per-browser receiver diagnostics, only for `GET /status?diagnostics=1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receivers: Option<Vec<Value>>,
    /// Bounded screen information and fullscreen exits retained after live clients expire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_history: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bluetooth_guard: Option<AdminBluetoothGuard>,
    /// Bluetooth Secure Connections as the relay last switched it (off only while pairing);
    /// absent before the first switch and after a failed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bluetooth_secure_connections: Option<bool>,
    pub platform: String,
    pub name: String,
    pub version: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_profile: Option<AdminHostProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_capabilities: Option<crate::host_capabilities::HostCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_hotspot: Option<AdminHotspotStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wireless: Option<AdminWireless>,
    pub admin_url: String,
    #[serde(default)]
    pub admin_urls: Vec<String>,
    pub receiver_urls: Vec<String>,
    pub state: String,
    pub running: bool,
    pub auto_start_paused: bool,
    pub iphone: AdminIphone,
    pub network: AdminNetwork,
    pub output: String,
    pub carlink: Option<AdminCarlink>,
    pub notices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_provisioning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_client: Option<WifiClient>,
    /// Trusted HTTPS with a free domain name; absent where the relay has no such feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_name: Option<crate::public_name::PublicNameStatus>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminHotspotStatus {
    pub state: String,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminWireless {
    pub preset: crate::device_preset::PresetSelection,
    #[serde(flatten)]
    pub summary: crate::wifi_capabilities::WifiSummary,
    pub hotspot: crate::device_preset::HotspotResolution,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminHotspot {
    pub has_password: bool,
    pub ssid: String,
    pub password: String,
    pub ip: Option<String>,
    pub active: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminHostAddress {
    pub interface: String,
    pub ipv4: String,
    pub developer_only: bool,
}
impl AdminHostAddress {
    pub fn new(interface: String, ipv4: String) -> Self {
        let developer_only = ["usb", "rndis", "veth", "docker", "tun", "wg"]
            .iter()
            .any(|prefix| interface.starts_with(prefix));
        Self {
            interface,
            ipv4,
            developer_only,
        }
    }
}
/// Device-support state written by a Bluetooth guard service (UFI003 `bt-guard.sh`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminBluetoothGuard {
    /// `ok`, `recovering` or `failed`.
    pub state: String,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub last_recovery: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AdminPairedDevice {
    pub name: String,
    pub connected: bool,
    pub iphone: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminIphone {
    pub name: Option<String>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminNetwork {
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub planned: Option<crate::network_roles::NetworkStatus>,
    #[serde(default)]
    pub following_carlink: bool,
    pub mode: String,
    pub wifi: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AdminCarlink {
    pub code: Option<String>,
    pub retrying: bool,
    pub trail: Vec<String>,
    pub state: String,
    pub detail: String,
    pub last_lost_reason: Option<String>,
    pub last_error: Option<String>,
    /// When the band backoff ends, on the relay's wall clock (`nextAttempt` of the
    /// `carlink-backoff` event): for display only, since that clock may be wrong.
    pub next_attempt: Option<u64>,
    pub head_unit: Option<CarlinkHeadUnit>,
    pub network: Option<CarlinkNetwork>,
    pub trusted: bool,
    pub view_size: Option<CarlinkViewSize>,
    pub stats: Option<Value>,
    pub pin_incorrect: bool,
    #[serde(skip)]
    pub attempt_requested_pin: bool,
    pub pin_requested: bool,
    /// When the PIN request lapses, in seconds on the monotonic clock of `apply_event`'s `now`.
    #[serde(skip)]
    pub pin_expiry: Option<u64>,
    /// `pin_expiry` on the relay's wall clock, set by [`Self::at`] for display only, since that
    /// clock may be wrong.
    pub pin_deadline: Option<u64>,
    /// Seconds left at response time, so browsers with a wrong clock still count down correctly.
    pub pin_remaining: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarlinkViewSize {
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CarlinkHeadUnit {
    pub address: String,
    pub name: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CarlinkNetwork {
    pub ssid: String,
    pub interface: String,
}
impl AdminCarlink {
    pub fn clear_pin(&mut self) {
        self.pin_requested = false;
        self.pin_expiry = None;
        self.pin_deadline = None;
        self.pin_remaining = None;
    }
    /// This status as a response at `now` on the monotonic clock and `wall_now` on the wall
    /// clock, both in seconds: a lapsed PIN request reads as timed out, a pending one carries the
    /// seconds left.
    pub fn at(mut self, now: u64, wall_now: u64) -> Self {
        match self.pin_expiry {
            Some(expiry) if expiry > now => {
                self.pin_remaining = Some(expiry - now);
                self.pin_deadline = Some(wall_now.saturating_add(expiry - now));
            }
            Some(_) => {
                self.clear_pin();
                self.detail = "carlink.pinTimeout".into();
            }
            None => {
                self.pin_deadline = None;
                self.pin_remaining = None;
            }
        }
        self
    }
    /// Applies a receiver event at `now`, in seconds on the monotonic clock.
    pub fn apply_event(&mut self, event: &Value, now: u64) {
        // Teardown events from the failed child must not hide the retry deadline.
        if self.next_attempt.is_some()
            && (matches!(
                event["type"].as_str(),
                Some("error" | "receiver-error" | "state")
            ) || (event["type"] == "link" && event["state"] == "lost"))
        {
            return;
        }
        match event["type"].as_str().unwrap_or("") {
            "receiver-starting" => {
                self.view_size = None;
                self.stats = None;
            }
            "clink-view-size" => {
                if let Ok(size) = serde_json::from_value::<CarlinkViewSize>(event.clone()) {
                    if size.width > 0 && size.height > 0 {
                        self.view_size = Some(size);
                    }
                }
            }
            "media-stats" => {
                self.stats = Some(event.clone());
            }
            "clink-pin" if event["result"] == "incorrect" => {
                self.pin_incorrect = true;
            }
            "link" => {
                if event["state"] != "lost" {
                    self.next_attempt = None;
                }
                self.state = event["state"].as_str().unwrap_or("unknown").into();
                self.detail = if self.last_error.as_deref() == Some("carlink.pinStale")
                    && self.state == "lost"
                {
                    "carlink.pinStale".into()
                } else {
                    event["reason"].as_str().unwrap_or("").into()
                };
                if self.state == "connecting" {
                    self.attempt_requested_pin = false;
                }
                if self.state == "lost" {
                    self.last_lost_reason = event["reason"].as_str().map(str::to_owned);
                    if self.last_error.as_deref() != Some("carlink.pinStale") {
                        self.last_error = self.last_lost_reason.clone();
                    }
                    self.attempt_requested_pin = false;
                } else if self.state == "joined" {
                    self.last_lost_reason = None;
                    if self
                        .last_error
                        .as_deref()
                        .is_some_and(|reason| reason.starts_with("head_unit_band_unsupported"))
                    {
                        self.last_error = None;
                    }
                }
                if let Ok(head) = serde_json::from_value(event["headUnit"].clone()) {
                    self.head_unit = Some(head);
                }
                if let Ok(network) = serde_json::from_value(event["network"].clone()) {
                    self.network = Some(network);
                }
                if ["scanning", "lost"].contains(&self.state.as_str()) {
                    self.network = None;
                    self.clear_pin();
                    if self.state == "scanning" {
                        self.head_unit = None;
                        self.attempt_requested_pin = false;
                        if self.last_error.as_deref() == Some("carlink.pinStale") {
                            self.last_error = None;
                        }
                    }
                }
            }
            "carlink-paused" => {
                self.state = "paused".into();
                self.detail = "carlink.manualHint".into();
                self.next_attempt = None;
                self.clear_pin();
            }
            "carlink-unpaired" => {
                self.state = "unpaired".into();
                // Preserve the band failure while an unpaired relay is idle.
                if self.next_attempt.is_none() {
                    self.detail = if self.last_error.as_deref() == Some("carlink.pinStale") {
                        "carlink.pinStale".into()
                    } else {
                        "carlink.unpairedHint".into()
                    };
                }
                self.attempt_requested_pin = false;
                self.clear_pin();
            }
            "pin-request" => {
                self.attempt_requested_pin = true;
                self.pin_requested = true;
                self.pin_expiry = Some(now.saturating_add(120));
                self.detail.clear();
            }
            "pin-timeout" => {
                self.attempt_requested_pin = false;
                self.clear_pin();
                self.detail = "carlink.pinTimeout".into();
            }
            "state" => {
                if let Some(state) = event["state"].as_str() {
                    self.state = state.to_ascii_lowercase();
                }
                if matches!(
                    self.state.as_str(),
                    "ended" | "disconnected" | "playing" | "active" | "connected"
                ) {
                    self.attempt_requested_pin = false;
                }
                if !matches!(
                    self.state.as_str(),
                    "connecting" | "pairing" | "authenticating"
                ) {
                    self.clear_pin();
                }
            }
            "error" | "receiver-error" => {
                self.state = "error".into();
                self.detail = event["message"].as_str().unwrap_or("").into();
                if self.attempt_requested_pin
                    && matches!(
                        self.detail.as_str(),
                        "auth_internal_error" | "auth_rejected"
                    )
                {
                    self.detail = "carlink.pinStale".into();
                }
                self.last_error = Some(self.detail.clone());
                self.attempt_requested_pin = false;
                self.clear_pin();
            }
            "carlink-backoff" => {
                self.state = "waiting".into();
                self.detail = event["reason"].as_str().unwrap_or("").into();
                self.last_error = Some(self.detail.clone());
                self.next_attempt = event["nextAttempt"].as_u64();
                self.clear_pin();
            }
            "receiver-restarting" => {
                *self = Self {
                    state: "waiting".into(),
                    last_lost_reason: self.last_lost_reason.take(),
                    next_attempt: self.next_attempt,
                    detail: if self.next_attempt.is_some() {
                        self.detail.clone()
                    } else if self.last_error.as_deref() == Some("carlink.pinStale") {
                        "carlink.pinStale".into()
                    } else {
                        String::new()
                    },
                    last_error: self.last_error.take(),
                    ..Self::default()
                };
            }
            _ => {}
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPhone {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub connected: bool,
    pub likely_iphone: bool,
    pub selected: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminSettings {
    #[serde(default, rename = "installedPlugins")]
    pub installed_plugins: Vec<crate::plugins::InstalledPlugin>,
    pub schema: Vec<Section>,
    pub values: Values,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    pub title_key: String,
    pub fields: Vec<Field>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub label_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<FieldOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_when: Option<VisibleWhen>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldOption {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<BTreeMap<String, String>>,
    pub value: Value,
    pub label_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub illustration: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VisibleWhen {
    pub key: String,
    pub equals: Vec<Value>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WifiClient {
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub signal: Option<u8>,
    pub setup_hotspot: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
    pub known: bool,
}
pub enum AdminUpdate {
    AppliedNow,
    NextStart,
    Errors(BTreeMap<String, String>),
}
pub type AdminResult = Result<(), String>;
/// The shell implements this seam; the HTTP server never accesses receiver internals.
pub trait RelayAdmin: Send + Sync + 'static {
    fn logs_export(&self, _range: &str) -> Result<Value, String> {
        Err("error.request".into())
    }
    fn logs_download(&self, _id: &str) -> std::io::Result<(std::fs::File, u64, String)> {
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn logs_upload(&self, _id: &str) -> Result<Value, String> {
        Err("rejected".into())
    }
    /// Poll/cancel a single explicit background upload; cancellation does not remove its ZIP.
    fn logs_upload_status(&self, _id: &str, _cancel: bool) -> Option<Value> {
        None
    }
    fn logs_receipt(&self) -> std::io::Result<Option<Value>> {
        Ok(None)
    }
    /// Runtime lane hook for a bounded snapshot of its recent connection timeline.
    fn recent_connection_timeline_for_log_export(&self) -> String {
        String::new()
    }
    fn config_export(&self, _privacy: bool) -> Result<Value, BTreeMap<String, String>> {
        Err(BTreeMap::from([("file".into(), "unsupported".into())]))
    }
    fn config_import(
        &self,
        _bytes: &[u8],
        _dry_run: bool,
    ) -> Result<Value, BTreeMap<String, String>> {
        Err(BTreeMap::from([("file".into(), "unsupported".into())]))
    }
    fn update_check(&self, _channel: &str) -> Result<Value, &'static str> {
        Err("offline")
    }
    /// Linux recorder support; absent on other platforms.
    fn carlink_trace(&self) -> Option<(bool, bool, Value)> {
        None
    }
    fn carlink_trace_control(&self, _clear: bool, _action: &str) -> AdminResult {
        Err("error.carlinkTraceOff".into())
    }

    /// Called on the request worker after the response has been flushed.
    fn response_sent(&self) {}
    /// Discard deferred actions if delivery of the response fails.
    fn response_failed(&self) {}

    fn network_roles_preview(&self, _values: Values) -> Option<crate::network_roles::SaveResult> {
        None
    }

    fn network_roles_update(&self, _values: Values) -> Option<crate::network_roles::SaveResult> {
        None
    }

    fn status(&self) -> AdminStatus;
    fn settings(&self) -> AdminSettings;
    fn update(&self, values: Values) -> AdminUpdate;
    fn action(&self, name: &str) -> AdminResult;
    fn public_name_import(
        &self,
        _content: &str,
    ) -> Result<(String, String), crate::public_name::service::ImportError> {
        Err(crate::public_name::service::ImportError::Storage)
    }
    fn carlink_connect(&self, _pin: Option<&str>) -> AdminResult {
        Err("error.action".into())
    }
    fn carlink_pin(&self, _pin: &str) -> AdminResult {
        Err("error.action".into())
    }
    fn phones(&self) -> Vec<AdminPhone>;
    fn hotspot_update(&self, _ssid: &str, _password: &str, _band: Option<&str>) -> AdminResult {
        Err("error.action".into())
    }
    /// Latest receiver-info and recent stats per browser, as `[{client, idleSeconds, info, stats}]`.
    fn receivers(&self) -> Vec<Value> {
        Vec::new()
    }
    fn receiver_history(&self) -> Value {
        serde_json::json!({"screens":[],"fullscreenExits":[]})
    }
    fn wifi_status(&self) -> WifiClient;
    fn wifi_networks(&self) -> Result<Vec<WifiNetwork>, String>;
    /// Queue the join so the HTTP response can leave before the setup AP goes down.
    fn wifi_join(&self, ssid: &str, password: Option<&str>) -> AdminResult;
}

pub fn validate_hotspot_credentials(
    ssid: &str,
    password: &str,
) -> std::result::Result<(), &'static str> {
    if !(1..=32).contains(&ssid.len()) {
        return Err("error.hotspotSsid");
    }
    if !(8..=63).contains(&password.len()) || !password.bytes().all(|b| (32..=126).contains(&b)) {
        return Err("error.hotspotPassword");
    }
    Ok(())
}

#[cfg(test)]
mod backoff_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn retry_reason_and_deadline_survive_teardown_and_clear_on_new_attempt() {
        let mut status = AdminCarlink::default();
        status.apply_event(&json!({"type":"carlink-backoff","reason":"head_unit_band_unsupported:5180","nextAttempt":160}), 100);
        for event in [
            json!({"type":"error","message":"closed"}),
            json!({"type":"link","state":"lost","reason":"closed"}),
            json!({"type":"state","state":"ended"}),
            json!({"type":"receiver-stopped"}),
            json!({"type":"receiver-restarting"}),
            json!({"type":"carlink-unpaired"}),
        ] {
            status.apply_event(&event, 101);
            assert_eq!(status.detail, "head_unit_band_unsupported:5180");
            assert_eq!(serde_json::to_value(&status).unwrap()["nextAttempt"], 160);
        }
        status.apply_event(&json!({"type":"link","state":"scanning"}), 160);
        assert_eq!(status.next_attempt, None);
        status.apply_event(&json!({"type":"link","state":"joined"}), 161);
        assert_eq!(status.next_attempt, None);
        assert!(status.detail.is_empty());
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminHostProfile {
    pub model: Option<String>,
    pub profile_path: std::path::PathBuf,
    pub profile_loaded: bool,
    pub profile_error: Option<String>,
    pub bluetooth_adapter: Option<AdminBluetoothAdapter>,
    pub wifi_interface: String,
    pub wifi_present: bool,
    pub secure_connections: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminBluetoothAdapter {
    pub name: String,
    pub address: String,
}
