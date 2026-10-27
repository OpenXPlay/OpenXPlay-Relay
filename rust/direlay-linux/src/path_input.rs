// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! The Linux observations translated into the shared Android path contract.
use crate::{admin::AdminStatus, display::Size, path_facts::RuntimePathFacts, settings::Settings};
use serde_json::{json, Value};

fn screen(size: Option<Size>) -> Option<String> {
    size.map(|s| format!("{}×{}", s.width, s.height))
}

/// Keep the legacy Carlink diagnostic fields and the presenter input on the same snapshot.
pub fn update(
    facts: &RuntimePathFacts,
    settings: &Settings,
    status: &mut AdminStatus,
    now: u64,
    identity_ready: bool,
    bluetooth_on: bool,
) {
    if let Some(carlink) = &mut status.carlink {
        carlink.code = facts.carlink.code.clone();
        carlink.retrying = facts.carlink.retrying;
        carlink.trail = facts.carlink.trail.clone();
    }
    status.path_input = build(facts, settings, status, now, identity_ready, bluetooth_on);
}

/// All arguments are snapshots. This adapter never probes the host or infers playback from a session.
pub fn build(
    facts: &RuntimePathFacts,
    settings: &Settings,
    status: &AdminStatus,
    now: u64,
    identity_ready: bool,
    bluetooth_on: bool,
) -> Value {
    let shared = settings.network_mode == "shared-wifi";
    let web = settings.output != "carlink";
    let carlink = settings.output != "web";
    let outputs: Vec<_> = [(web, "BROWSER"), (carlink, "CARLINK")]
        .into_iter()
        .filter_map(|(enabled, name)| enabled.then_some(name))
        .collect();
    let pick = match status.iphone.reason.as_str() {
        "Selected" | "Connected" | "OnlyCandidate" => "PICKED",
        "SelectedUnpaired" => "UNPAIRED",
        "MultipleConnected" | "MultipleCandidates" => "MULTIPLE",
        "OnlyBonded" => "NOT_IPHONE",
        _ => "NONE",
    };
    let hotspot = status.hotspot.as_ref();
    let phone = &facts.iphone;
    let clink = &facts.carlink;
    let failure = phone.failure.as_ref().map(|f| {
        json!({
            "step": f.step, "code": f.code, "text": f.text, "atMillis": f.at_millis,
            "retrying": f.retrying, "trail": f.trail,
        })
    });
    let address = hotspot
        .and_then(|h| h.ip.as_deref())
        .or_else(|| status.wifi_client.as_ref().and_then(|w| w.ip.as_deref()))
        .or_else(|| {
            status
                .host_addresses
                .iter()
                .find(|a| !a.developer_only)
                .map(|a| a.ipv4.as_str())
        });
    let trusted_name = status
        .public_name
        .as_ref()
        .and_then(|p| p.trusted_name.as_deref());
    let waiting = phone.car_wait_until_millis.is_some_and(|t| t > now);
    let output_off = json!({"phase":"OFF", "name":null, "code":null, "sinceMillis":null});
    json!({
        "platform":"linux", "nowMillis":now, "version":status.version,
        "setup":{"network":if shared {"SHARED_WIFI"} else {"PHONE_HOTSPOT"},
            "outputs":outputs, "summary":"", "valid":true},
        "relay":{"identity":if identity_ready {"READY"} else {"MISSING"}, "identityLabel":null,
            "permissionsGranted":0, "permissionsTotal":0, "vpnConsentNeeded":false,
            "keepAlive":"NOT_NEEDED", "bluetoothOn":bluetooth_on, "bluetoothName":status.name,
            "wifiOn":status.host_profile.as_ref().map(|p| p.wifi_present), "wifiSsid":status.network.wifi,
            "hotspotOn":hotspot.is_some_and(|h| h.active), "hotspotSsid":hotspot.map(|h| &h.ssid),
            "hotspotBand":settings.hotspot_band, "sharedWifiSsid":if shared {status.network.wifi.as_deref()} else {None},
            "overlayNeeded":false},
        "car":{"address":address, "web":{"phase":if web {&facts.web.phase} else {"OFF"},
            "port":if facts.web.port > 0 {facts.web.port} else {settings.receiver_port},
            "code":facts.web.code, "trustedName":trusted_name, "sinceMillis":facts.web.since_millis},
            "pageUrl":if web {status.receiver_urls.first()} else {None}, "pages":facts.pages,
            "screen":screen(facts.screen.or(facts.carlink.screen)),
            "savedScreen":screen(facts.saved_screen.or(facts.built_in_screen)),
            "carlink":{"phase":if carlink {if clink.pin_pending {"PIN"} else {&clink.phase}} else {"OFF"},
                "sinceMillis":clink.since_millis, "code":clink.code, "error":clink.error,
                "retrying":clink.retrying, "carName":clink.car_name, "joinMethod":clink.join_method,
                "trusted":clink.trusted, "pinPending":clink.pin_pending, "picture":clink.picture, "trail":clink.trail},
            "dlna":output_off, "miracast":output_off, "tesla":output_off},
        "iphone":{"pick":pick, "name":status.iphone.name, "usb":"UNKNOWN",
            "started":status.running || waiting, "armed":false, "carWaitUntilMillis":phone.car_wait_until_millis,
            "step":phone.step, "stepSinceMillis":phone.since_millis, "stepDetail":phone.detail, "failure":failure},
        "live":{"pictureOnCar":facts.picture_on_car, "sessionSinceMillis":phone.session_since_millis, "chips":facts.chips},
    })
}
