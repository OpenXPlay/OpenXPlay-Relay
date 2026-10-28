// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Android-compatible v1 envelopes; the shell applies plans through its normal settings editor.
use crate::{admin::Values, settings::Settings};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fmt};

pub const MAX_BYTES: usize = 64 * 1024;
pub const USER_FORMAT: &str = "openxplay-user-config";
pub const PRIVACY_FORMAT: &str = "openxplay-privacy-config";
pub type Errors = BTreeMap<String, String>;
// The only nested wire-key table. Targets absent from settings_values are reported as ignored.
// W13 can expose a stored choice by adding its matching target to Settings; no codec branch is needed.
const NESTED_KEYS: &[(&str, &str, &str)] = &[
    ("car", "brand", "carBrand"),
    ("car", "brandText", "carBrandText"),
    ("car", "model", "carModel"),
    ("car", "modelText", "carModelText"),
    ("car", "year", "carYear"),
    ("car", "yearText", "carYearText"),
    ("developer", "active", "developerActive"),
    ("developer", "carlinkTrace", "developerCarlinkTrace"),
    (
        "developer",
        "carlinkHevcEnabled",
        "developerCarlinkHevcEnabled",
    ),
    ("developer", "carlinkHevc", "developerCarlinkHevc"),
    (
        "developer",
        "carlinkPictureSizeEnabled",
        "developerCarlinkPictureSizeEnabled",
    ),
    (
        "developer",
        "carlinkPictureSize",
        "developerCarlinkPictureSize",
    ),
    (
        "developer",
        "physicalWidthEnabled",
        "developerPhysicalWidthEnabled",
    ),
    (
        "developer",
        "physicalWidthOverrideMm",
        "developerPhysicalWidthOverrideMm",
    ),
    ("developer", "localPreview", "developerLocalPreview"),
    ("developer", "playCarPlayAudio", "developerPlayCarPlayAudio"),
    (
        "developer",
        "microphoneSourceEnabled",
        "developerMicrophoneSourceEnabled",
    ),
    ("developer", "microphoneSource", "developerMicrophoneSource"),
    ("developer", "webPortEnabled", "developerWebPortEnabled"),
    ("developer", "webPort", "developerWebPort"),
    (
        "developer",
        "carlinkDeviceModelEnabled",
        "developerCarlinkDeviceModelEnabled",
    ),
    (
        "developer",
        "carlinkDeviceModel",
        "developerCarlinkDeviceModel",
    ),
    ("experimental", "active", "experimentalActive"),
    (
        "experimental",
        "remotePlaybackProof",
        "experimentalRemotePlaybackProof",
    ),
    (
        "experimental",
        "cancelRecoveredRetries",
        "experimentalCancelRecoveredRetries",
    ),
    ("experimental", "startupStatus", "experimentalStartupStatus"),
    (
        "experimental",
        "startupDiagnostics",
        "experimentalStartupDiagnostics",
    ),
    (
        "experimental",
        "networkObservationBackoff",
        "experimentalNetworkObservationBackoff",
    ),
    (
        "experimental",
        "screenSessionIsolation",
        "experimentalScreenSessionIsolation",
    ),
    (
        "experimental",
        "screenReconnectReset",
        "experimentalScreenReconnectReset",
    ),
    (
        "experimental",
        "screenDeliverySerialization",
        "experimentalScreenDeliverySerialization",
    ),
    (
        "experimental",
        "sendBluetoothAddress",
        "experimentalSendBluetoothAddress",
    ),
    (
        "experimental",
        "bluetoothAddress",
        "experimentalBluetoothAddress",
    ),
];

fn error(key: &str, code: &str) -> Errors {
    BTreeMap::from([(key.into(), code.into())])
}
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub applied: Vec<String>,
    pub ignored: Vec<String>,
    pub restart_needed: bool,
    pub applied_now: Vec<String>,
    pub next_start: Vec<String>,
}
pub struct Plan {
    pub report: ImportReport,
    pub values: Values,
    pub privacy: Option<crate::public_name::privacy::Import>,
}
fn protected(key: &str) -> bool {
    matches!(
        key,
        "networkRoles"
            | "wifiCountry"
            | "adminHotspotPassword"
            | "carlinkPin"
            | "carBluetoothAudioDefaulted"
    ) || key.starts_with("publicName")
        || normalized_secret(key)
}
fn normalized_secret(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token") || lower.contains("privatekey") || lower.contains("certificate")
}
pub fn settings_values(settings: &Settings, display: &Value) -> Values {
    let mut values = serde_json::to_value(settings)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    values.retain(|key, _| !settings.unknown.contains_key(key) && !protected(key));
    for key in ["resolution", "screenWidth", "videoCodec"] {
        if let Some(value) = display.get(key) {
            values.insert(key.into(), value.clone());
        }
    }
    values
}
fn envelope(format: &str, now: u64) -> Value {
    let seconds = (now / 1000).try_into().unwrap_or(0);
    let mut utc = std::mem::MaybeUninit::<libc::tm>::uninit();
    let exported = if unsafe { libc::gmtime_r(&seconds, utc.as_mut_ptr()) }.is_null() {
        "1970-01-01T00:00:00Z".into()
    } else {
        let utc = unsafe { utc.assume_init() };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            utc.tm_year + 1900,
            utc.tm_mon + 1,
            utc.tm_mday,
            utc.tm_hour,
            utc.tm_min,
            utc.tm_sec,
            now % 1000
        )
    };
    json!({"format":format,"version":1,"platform":"linux","appVersion":env!("CARGO_PKG_VERSION"),"exportedAt":exported})
}
/// Linux names are retained for shared settings. Android aliases are accepted on import.
#[cfg(test)]
pub fn export_user(settings: &Settings, display: &Value, now: u64) -> Result<Value, Errors> {
    export_user_values(settings_values(settings, display), now)
}
pub fn export_user_values(mut values: Values, now: u64) -> Result<Value, Errors> {
    let mut value = envelope(USER_FORMAT, now);
    for (linux, android) in [
        ("carlinkName", "carlinkDeviceName"),
        ("carlinkModel", "carlinkDeviceModel"),
        ("bootStart", "autoStartOnBoot"),
        ("autoStart", "autoStartWhenReady"),
        ("carPlayViewAreas", "viewAreas"),
        ("resolution", "carplayResolution"),
    ] {
        if let Some(value) = values.get(linux).cloned() {
            values.insert(android.into(), value);
        }
    }
    if let Some(width) = values.get("screenWidth").and_then(Value::as_u64) {
        values.insert("carplayScreenWidth".into(), json!(format!("mm{width}")));
    }
    for group in ["car", "developer", "experimental"] {
        let mut nested: Values = NESTED_KEYS
            .iter()
            .filter(|(g, _, _)| *g == group)
            .filter_map(|(_, leaf, target)| {
                values.get(*target).cloned().map(|v| ((*leaf).into(), v))
            })
            .collect();
        if !nested.is_empty() {
            if group == "experimental" {
                nested.insert("version".into(), json!(2));
            }
            values.insert(group.into(), Value::Object(nested));
        }
    }
    // Shared enum keys use Android's published wire spelling; import maps back to settings.json.
    for (key, linux, android) in [
        ("carlinkHevc", "auto", "stream-only"),
        ("hotspotBand", "2.4", "ghz-2-4"),
        ("hotspotBand", "5", "ghz-5"),
        ("videoStats", "5", "seconds-5"),
        ("videoStats", "15", "seconds-15"),
    ] {
        if values.get(key).and_then(Value::as_str) == Some(linux) {
            values.insert(key.into(), json!(android));
        }
    }
    if let Some(trace) = values.get("carlinkTrace").and_then(Value::as_str) {
        values.insert("carlinkTrace".into(), json!(trace == "on"));
    }
    value["settings"] = Value::Object(values);
    bounded(value)
}
pub fn export_privacy(sections: Value, now: u64) -> Result<Value, Errors> {
    let mut value = envelope(PRIVACY_FORMAT, now);
    if let Some(sections) = sections.as_object() {
        value.as_object_mut().unwrap().extend(sections.clone());
    }
    bounded(value)
}
fn bounded(value: Value) -> Result<Value, Errors> {
    if serde_json::to_vec_pretty(&value).unwrap().len() > MAX_BYTES {
        Err(error("file", "too-large"))
    } else {
        Ok(value)
    }
}
#[cfg(test)]
pub fn plan(bytes: &[u8], current: &Values) -> Result<Plan, Errors> {
    plan_with_privacy(
        bytes,
        current,
        None,
        crate::platform::wall_clock().as_secs(),
    )
}
#[cfg(test)]
pub fn plan_with_privacy(
    bytes: &[u8],
    current: &Values,
    public: Option<&crate::public_name::config::Config>,
    now: u64,
) -> Result<Plan, Errors> {
    let value = read_envelope(bytes)?;
    plan_envelope(&value, current, public, now)
}
pub fn read_envelope(bytes: &[u8]) -> Result<Value, Errors> {
    if bytes.len() > MAX_BYTES {
        return Err(error("file", "too-large"));
    }
    let Strict(value) =
        serde_json::from_slice::<Strict>(bytes).map_err(|_| error("file", "not-json"))?;
    if !value.is_object() || !valid_depth(&value, 0) {
        return Err(error("file", "not-json"));
    }
    Ok(value)
}
fn valid_depth(value: &Value, depth: usize) -> bool {
    depth < 64
        && match value {
            Value::Object(values) => values.values().all(|value| valid_depth(value, depth + 1)),
            Value::Array(values) => values.iter().all(|value| valid_depth(value, depth + 1)),
            _ => true,
        }
}
pub fn plan_envelope(
    value: &Value,
    current: &Values,
    public: Option<&crate::public_name::config::Config>,
    now: u64,
) -> Result<Plan, Errors> {
    let object = value.as_object().ok_or_else(|| error("file", "not-json"))?;
    if object.get("version").and_then(Value::as_f64) != Some(1.0) {
        return Err(error("version", "bad-version"));
    }
    let mut plan = Plan {
        report: ImportReport::default(),
        values: Values::new(),
        privacy: None,
    };
    match object.get("format").and_then(Value::as_str) {
        Some(USER_FORMAT) => {
            let settings = object
                .get("settings")
                .and_then(Value::as_object)
                .ok_or_else(|| error("settings", "invalid"))?;
            let mut ignored: Vec<String> = object
                .keys()
                .filter(|key| {
                    ![
                        "format",
                        "version",
                        "platform",
                        "appVersion",
                        "exportedAt",
                        "settings",
                        "linux",
                    ]
                    .contains(&key.as_str())
                })
                .cloned()
                .collect();
            let mut errors = Errors::new();
            for (key, value) in settings {
                if key == "output" && settings.contains_key("outputs") {
                    continue;
                }
                if let Err(error) =
                    import_key(&mut plan.values, &mut ignored, current, key, value, key)
                {
                    errors.extend(error);
                }
            }
            if !errors.is_empty() {
                return Err(errors);
            }
            if let Some(developer) = settings.get("developer").and_then(Value::as_object) {
                let fps = developer
                    .get("fps")
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite() && v.fract() == 0.0 && *v >= 0.0)
                    .map(|v| v as u64);
                if !settings.contains_key("frameRate")
                    && developer.get("fpsEnabled") == Some(&json!(true))
                    && fps.is_some_and(|v| [30, 35, 40, 45, 50, 55, 60].contains(&v))
                {
                    plan.values
                        .insert("frameRate".into(), json!(fps.unwrap().to_string()));
                } else {
                    for key in ["fpsEnabled", "fps"] {
                        if developer.contains_key(key) {
                            ignored.push(format!("developer.{key}"));
                        }
                    }
                }
            }
            // Optional Linux-only settings do not collide with Android's catalogue.
            if let Some(linux) = object.get("linux") {
                for (key, value) in linux.as_object().ok_or_else(|| error("linux", "invalid"))? {
                    import_key(
                        &mut plan.values,
                        &mut ignored,
                        current,
                        key,
                        value,
                        &format!("linux.{key}"),
                    )?;
                }
            }
            plan.values
                .retain(|key, value| current.get(key) != Some(value));
            plan.report.applied = plan.values.keys().cloned().collect();
            plan.report.ignored = ignored;
        }
        Some(PRIVACY_FORMAT) => {
            let privacy = crate::public_name::privacy::decode(value, public, now)?;
            plan.report.applied = privacy.sections();
            plan.privacy = Some(privacy);
            for key in object.keys() {
                if ![
                    "format",
                    "version",
                    "platform",
                    "appVersion",
                    "exportedAt",
                    "publicName",
                    "providers",
                    "publicNameCertificate",
                    "publicNameAccount",
                ]
                .contains(&key.as_str())
                {
                    plan.report.ignored.push(key.clone());
                }
            }
        }
        _ => return Err(error("format", "invalid")),
    }
    plan.report.applied.sort();
    plan.report.ignored.sort();
    plan.report.ignored.dedup();
    Ok(plan)
}
/// Hotspot credentials live outside settings.json. Security becomes a normal setting when W13 provides it.
pub fn take_hotspot(
    values: &mut Values,
    current: &Values,
    native_security: bool,
) -> Result<Option<(String, String)>, Errors> {
    let ssid = values.remove("hotspotSsid");
    let password = values.remove("hotspotPassphrase");
    if !native_security
        && values
            .remove("hotspotSecurity")
            .is_some_and(|security| security != "wpa2")
    {
        return Err(error("hotspotSecurity", "unsupported"));
    }
    if ssid.is_none() && password.is_none() {
        return Ok(None);
    }
    let ssid = ssid
        .as_ref()
        .or_else(|| current.get("hotspotSsid"))
        .and_then(Value::as_str)
        .ok_or_else(|| error("hotspotSsid", "invalid"))?;
    let password = password
        .as_ref()
        .or_else(|| current.get("hotspotPassphrase"))
        .and_then(Value::as_str)
        .ok_or_else(|| error("hotspotPassphrase", "invalid"))?;
    crate::linux_hotspot::validate_hotspot_credentials(ssid, password)
        .map_err(|code| error("hotspotPassphrase", code))?;
    Ok(Some((ssid.into(), password.into())))
}
fn import_key(
    values: &mut Values,
    ignored: &mut Vec<String>,
    current: &Values,
    key: &str,
    value: &Value,
    source: &str,
) -> Result<(), Errors> {
    let mut value = value.clone();
    let target = match key {
        "carlinkDeviceName" => "carlinkName",
        "carlinkDeviceModel" => "carlinkModel",
        "autoStartOnBoot" => "bootStart",
        "autoStartWhenReady" => "autoStart",
        "viewAreas" => "carPlayViewAreas",
        "carplayResolution" => "resolution",
        "carplayScreenWidth" => "screenWidth",
        "outputs" => {
            let array = value.as_array().ok_or_else(|| error(source, "invalid"))?;
            let mut web = false;
            let mut carlink = false;
            let mut seen = std::collections::BTreeSet::new();
            for (index, item) in array.iter().enumerate() {
                let item = item.as_str().ok_or_else(|| error(source, "invalid"))?;
                if !seen.insert(item) {
                    return Err(error(source, "invalid"));
                }
                match item {
                    "web" => web = true,
                    "carlink" => carlink = true,
                    _ => ignored.push(format!("{source}[{index}]")),
                }
            }
            value = match (web, carlink) {
                (true, true) => json!("both"),
                (true, false) => json!("web"),
                (false, true) => json!("carlink"),
                _ => return Err(error(source, "invalid")),
            };
            "output"
        }
        "car" | "developer" | "experimental" => {
            let object = value.as_object().ok_or_else(|| error(source, "invalid"))?;
            let mut errors = Errors::new();
            for (leaf, value) in object {
                if key == "experimental" && leaf == "version" {
                    if value != 2 {
                        return Err(error(&format!("{source}.{leaf}"), "invalid"));
                    }
                    continue;
                }
                if key == "developer" && matches!(leaf.as_str(), "fpsEnabled" | "fps") {
                    continue;
                }
                let Some((_, _, target)) =
                    NESTED_KEYS.iter().find(|(g, l, _)| *g == key && *l == leaf)
                else {
                    ignored.push(format!("{source}.{leaf}"));
                    continue;
                };
                if let Err(error) = import_key(
                    values,
                    ignored,
                    current,
                    target,
                    value,
                    &format!("{source}.{leaf}"),
                ) {
                    errors.extend(error);
                }
            }
            if !errors.is_empty() {
                return Err(errors);
            }
            // Android's unversioned experimental envelope predates the master switch.
            if key == "experimental" && !object.contains_key("version") {
                values.insert("experimentalActive".into(), json!(true));
                if let Some(send) = values.get_mut("experimentalSendBluetoothAddress") {
                    *send = json!(object.get("active") == Some(&json!(true)) && *send == true);
                }
            }
            return Ok(());
        }
        _ => key,
    };
    if protected(target) || !current.contains_key(target) {
        ignored.push(source.into());
        return Ok(());
    }
    match target {
        "carlinkTrace" if value.is_boolean() => {
            value = json!(if value == true { "on" } else { "off" })
        }
        "carlinkHevc" => match value.as_str() {
            Some("stream-only") => value = json!("auto"),
            Some("signal") => {
                ignored.push(source.into());
                return Ok(());
            }
            _ => {}
        },
        "hotspotChannel" => {
            let channel = value
                .as_f64()
                .filter(|v| v.is_finite() && v.fract() == 0.0)
                .ok_or_else(|| error(source, "invalid"))?;
            if channel != 0.0
                && !(1.0..=14.0).contains(&channel)
                && !(32.0..=177.0).contains(&channel)
            {
                ignored.push(source.into());
                return Ok(());
            }
            value = json!(channel as u8);
        }
        "hotspotBand" => match value.as_str() {
            Some("ghz-2-4") => value = json!("2.4"),
            Some("ghz-5") => value = json!("5"),
            _ => {}
        },
        "videoStats" => match value.as_str() {
            Some("seconds-5") => value = json!("5"),
            Some("seconds-15") => value = json!("15"),
            _ => {}
        },
        "screenWidth" if value.is_string() => {
            if let Some(width) = value
                .as_str()
                .and_then(|v| v.strip_prefix("mm"))
                .and_then(|v| v.parse::<u32>().ok())
            {
                value = json!(width);
            }
        }
        // Android's null means its default port; Linux has an explicit port.
        "receiverPort" if value.is_null() => value = json!(8443),
        _ => {}
    }
    if let Some(prior) = current.get(target) {
        let same_type = match prior {
            Value::Bool(_) => value.is_boolean(),
            Value::String(_) => value.is_string(),
            Value::Number(_) => value.is_number(),
            Value::Array(_) => value.is_array(),
            Value::Object(_) => value.is_object(),
            Value::Null => true,
        };
        if !same_type {
            return Err(error(source, "invalid"));
        }
    }
    let valid = match target {
        "hotspotBand" => value
            .as_str()
            .is_some_and(|v| ["auto", "2.4", "5"].contains(&v)),
        "videoStats" => value
            .as_str()
            .is_some_and(|v| ["off", "5", "15"].contains(&v)),
        "output" => value
            .as_str()
            .is_some_and(|v| ["web", "carlink", "both"].contains(&v)),
        "carlinkHevc" => value
            .as_str()
            .is_some_and(|v| ["off", "auto", "signal"].contains(&v)),
        "carlinkPictureSize" => value
            .as_str()
            .is_some_and(|v| ["window", "mode"].contains(&v)),
        "iphone" => {
            value.is_null()
                || value
                    .as_str()
                    .is_some_and(crate::linux_bluetooth::valid_mac)
        }
        "experimentalBluetoothAddress" => value
            .as_str()
            .is_some_and(|v| v.is_empty() || crate::carlink_policy::bluetooth_address(v).is_some()),
        "carBrand" | "carBrandText" | "carModel" | "carModelText" | "carYear" | "carYearText" => {
            value
                .as_str()
                .is_some_and(|v| v.chars().count() <= 128 && !v.chars().any(char::is_control))
        }
        "frameRate" => value
            .as_str()
            .and_then(crate::carplay_policy::frame_rate_setting)
            .is_some(),
        "videoCodec" => value
            .as_str()
            .is_some_and(|v| ["auto", "h264", "h265"].contains(&v)),
        "resolution" => value
            .as_str()
            .is_some_and(|v| ["auto", "native", "1080p", "720p", "screen"].contains(&v)),
        "receiverPort" => value
            .as_u64()
            .is_some_and(|v| (1..=65535).contains(&v) && v != 6975),
        "networkMode" => value
            .as_str()
            .is_some_and(|v| ["device-hotspot", "shared-wifi"].contains(&v)),
        "carlinkName" | "carlinkModel" => value.as_str().is_some_and(|v| {
            !v.trim().is_empty() && v.len() <= 128 && !v.chars().any(char::is_control)
        }),
        "hotspotSsid" => value
            .as_str()
            .is_some_and(|v| !v.trim().is_empty() && v.len() <= 32 && !v.contains('\0')),
        "hotspotPassphrase" => value.as_str().is_some_and(|v| {
            (v.is_empty() || (8..=63).contains(&v.chars().count())) && !v.contains('\0')
        }),
        "hotspotSecurity" => value
            .as_str()
            .is_some_and(|v| ["open", "wpa2", "wpa3-transition", "wpa3"].contains(&v)),
        _ => true,
    };
    if !valid {
        return Err(error(source, "invalid"));
    }
    if values.get(target).is_some_and(|prior| prior != &value) {
        return Err(error(source, "conflict"));
    }
    values.insert(target.into(), value);
    Ok(())
}
/// Duplicate keys are rejected at every depth, like Android's StrictJson.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "strict JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Strict(Value::Number(n)))
                    .ok_or_else(|| E::custom("number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut values = Vec::new();
                while let Some(Strict(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Strict(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut values = Values::new();
                while let Some((key, Strict(v))) = a.next_entry::<String, Strict>()? {
                    if values.insert(key, v).is_some() {
                        return Err(de::Error::custom("duplicate key"));
                    }
                }
                Ok(Strict(Value::Object(values)))
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hotspot_custom_year_and_future_nested_keys_use_one_round_trip_table() {
        let mut current = current();
        for (key, value) in [
            ("hotspotSsid", json!("Relay")),
            ("hotspotPassphrase", json!("openxplay")),
            ("hotspotSecurity", json!("wpa2")),
            ("experimentalScreenSessionIsolation", json!(true)),
            ("experimentalRemotePlaybackProof", json!(true)),
            ("developerActive", json!(false)),
            ("developerCarlinkHevc", json!("off")),
        ] {
            current.insert(key.into(), value);
        }
        let file = json!({"format":USER_FORMAT,"version":1,"settings":{
            "hotspotSsid":"中继", "hotspotPassphrase":"password123", "hotspotSecurity":"wpa3", "car":{"yearText":"2027 custom"},
            "experimental":{"version":2,"screenSessionIsolation":false,"remotePlaybackProof":false},
            "developer":{"active":true,"carlinkHevc":"signal"}
        }});
        let p = plan(&serde_json::to_vec(&file).unwrap(), &current).unwrap();
        assert!(p.report.ignored.is_empty());
        current.extend(p.values);
        let exported = export_user_values(current, 0).unwrap();
        for key in ["hotspotSsid", "hotspotPassphrase", "hotspotSecurity"] {
            assert_eq!(exported["settings"][key], file["settings"][key]);
        }
        assert_eq!(exported["settings"]["car"]["yearText"], "2027 custom");
        assert_eq!(
            exported["settings"]["experimental"]["screenSessionIsolation"],
            false
        );
        assert_eq!(exported["settings"]["developer"]["carlinkHevc"], "signal");
    }
    #[test]
    fn linux_https_port_and_android_integral_channels_keep_their_platform_rules() {
        let file = json!({"format":USER_FORMAT,"version":1,"settings":{"receiverPort":443,"hotspotChannel":18}});
        let p = plan(&serde_json::to_vec(&file).unwrap(), &current()).unwrap();
        assert_eq!(p.values["receiverPort"], 443);
        assert_eq!(p.report.ignored, ["hotspotChannel"]);
        assert!(crate::relay_admin::update_settings(&Settings::default(), p.values).is_ok());
    }
    #[test]
    fn legacy_fps_migrates_only_without_an_explicit_frame_rate() {
        for (developer, frame, expected, ignored) in [
            (json!({"fpsEnabled":true,"fps":35}), None, Some("35"), false),
            (
                json!({"fpsEnabled":true,"fps":35}),
                Some("60"),
                Some("60"),
                true,
            ),
            (json!({"fpsEnabled":false,"fps":35}), None, None, true),
            (json!({"fpsEnabled":true,"fps":36}), None, None, true),
            (json!({"fpsEnabled":"true","fps":35}), None, None, true),
        ] {
            let mut file = json!({"format":USER_FORMAT,"version":1,"settings":{"developer":developer,"fps":55}});
            if let Some(frame) = frame {
                file["settings"]["frameRate"] = json!(frame);
            }
            let p = plan(&serde_json::to_vec(&file).unwrap(), &current()).unwrap();
            assert_eq!(p.values.get("frameRate").and_then(Value::as_str), expected);
            assert_eq!(p.report.ignored.contains(&"developer.fps".into()), ignored);
            assert!(p.report.ignored.contains(&"fps".into()));
        }
    }
    #[test]
    fn bad_values_keep_their_keys_and_malformed_utf8_is_rejected() {
        let file = json!({"format":USER_FORMAT,"version":1,"settings":{"frameRate":"31", "videoStats":false}});
        assert_eq!(
            plan(&serde_json::to_vec(&file).unwrap(), &current())
                .err()
                .unwrap(),
            BTreeMap::from([
                ("frameRate".into(), "invalid".into()),
                ("videoStats".into(), "invalid".into())
            ])
        );
        assert_eq!(
            plan(b"{\"invalid\":\"\xff\"}", &current()).err().unwrap()["file"],
            "not-json"
        );
    }
    #[test]
    fn android_depth_limit_and_escaped_duplicate_keys_are_strict() {
        let mut nested = json!(0);
        for _ in 0..62 {
            nested = json!([nested]);
        }
        assert!(read_envelope(&serde_json::to_vec(&json!({"nested":nested})).unwrap()).is_ok());
        assert!(read_envelope(&serde_json::to_vec(&json!({"nested":[nested]})).unwrap()).is_err());
        assert!(read_envelope(br#"{"version":1,"ver\u0073ion":2}"#).is_err());
    }
    #[test]
    fn startup_preferences_round_trip_independently_with_android_names() {
        let settings = Settings {
            auto_start: false,
            boot_start: true,
            ..Default::default()
        };
        let envelope = export_user(&settings, &json!({}), 0).unwrap();
        assert_eq!(envelope["settings"]["autoStartOnBoot"], true);
        assert_eq!(envelope["settings"]["autoStartWhenReady"], false);
        let imported = plan(&serde_json::to_vec(&json!({"format":USER_FORMAT,"version":1,"platform":"android","settings":{"autoStartOnBoot":false,"autoStartWhenReady":true}})).unwrap(), &settings_values(&settings, &json!({}))).unwrap();
        assert_eq!(imported.values["bootStart"], false);
        assert_eq!(imported.values["autoStart"], true);
        assert!(!imported.report.restart_needed);
    }

    fn current() -> Values {
        settings_values(
            &Settings::default(),
            &json!({"resolution":"auto","screenWidth":250,"videoCodec":"auto"}),
        )
    }
    #[test]
    fn tesla_android_keys_import_export_and_apply_without_a_restart() {
        for mode in [
            "off",
            "virtual-name",
            "public-address",
            "teslamirror",
            "tesla-android",
        ] {
            let input = json!({"format":USER_FORMAT,"version":1,"platform":"android","settings":{
                "teslaBrowser":mode,"teslaVirtualAddress":"169.254.88.2","teslaPublicAddress":"169.254.88.3",
                "teslaMirrorAddress":"100.88.0.2","teslaAndroidAddress":"3.3.3.2"}});
            let plan = plan(&serde_json::to_vec(&input).unwrap(), &current()).unwrap();
            assert!(!plan.report.restart_needed);
            assert!(plan.report.ignored.is_empty());
            let next =
                crate::relay_admin::update_settings(&Settings::default(), plan.values).unwrap();
            let output = export_user(&next, &json!({}), 0).unwrap();
            for (key, value) in input["settings"].as_object().unwrap() {
                assert_eq!(&output["settings"][key], value);
            }
        }
    }
    #[test]
    fn linux_round_trip_has_no_identity_or_remote_secrets() {
        let settings = Settings {
            carlink_pin: Some("1234".into()),
            admin_hotspot_password: Some("secret-pass".into()),
            unknown: BTreeMap::from([("remoteToken".into(), json!("never-export"))]),
            ..Default::default()
        };
        let value = export_user(
            &settings,
            &json!({"resolution":"auto","screenWidth":250,"videoCodec":"auto"}),
            1_792_713_600_000,
        )
        .unwrap();
        assert_eq!(value["platform"], "linux");
        assert_eq!(value["exportedAt"], "2026-10-23T00:00:00.000Z");
        let bytes = serde_json::to_vec(&value).unwrap();
        for secret in ["1234", "secret-pass", "never-export"] {
            assert!(!String::from_utf8_lossy(&bytes).contains(secret));
        }
        assert!(plan(&bytes, &current()).unwrap().values.is_empty());
    }
    #[test]
    fn android_file_aliases_and_unknowns_use_linux_validation() {
        let file = json!({"format":USER_FORMAT,"version":1,"platform":"android","settings":{"outputs":["web","carlink"],"output":"web","autoStartOnBoot":false,"carlinkDeviceName":"Car display","carlinkTrace":true,"carlinkHevc":"stream-only","hotspotBand":"ghz-5","videoStats":"seconds-15","carplayScreenWidth":"mm125","carplayResolution":"native","sharedWifiNetworks":[{"ssid":"MyWiFi","password":"password123"}],"localPreview":true,"remoteToken":"ignored"}});
        let mut current = current();
        current.insert("bootStart".into(), json!(true));
        let p = plan(&serde_json::to_vec(&file).unwrap(), &current).unwrap();
        assert_eq!(p.values["output"], "both");
        assert_eq!(p.values["bootStart"], false);
        assert_eq!(p.values["carlinkName"], "Car display");
        assert_eq!(p.values["carlinkTrace"], "on");
        assert_eq!(p.values["screenWidth"], 125);
        assert_eq!(p.report.ignored, ["localPreview", "remoteToken"]);
        assert!(!p.report.restart_needed);
        let mut settings = p.values.clone();
        for key in ["resolution", "screenWidth", "videoCodec"] {
            settings.remove(key);
        }
        assert!(crate::relay_admin::update_settings(&Settings::default(), settings).is_ok());
    }
    #[test]
    fn android_screen_experiments_are_ignored_without_changing_linux_settings() {
        for enabled in [true, false] {
            let current = current();
            let file = json!({"format":USER_FORMAT,"version":1,"platform":"android","settings":{
                "experimental":{"version":2,"screenSessionIsolation":enabled,
                    "screenReconnectReset":enabled,"screenDeliverySerialization":enabled}
            }});
            let p = plan(&serde_json::to_vec(&file).unwrap(), &current).unwrap();
            assert_eq!(
                p.report.ignored,
                [
                    "experimental.screenDeliverySerialization",
                    "experimental.screenReconnectReset",
                    "experimental.screenSessionIsolation",
                ]
            );
            assert!(p.report.applied.is_empty());
            assert!(!p.report.restart_needed);
            assert!(p.values.is_empty());
            assert!(p.privacy.is_none());
            let mut after = current.clone();
            after.extend(p.values);
            assert_eq!(after, current);
        }
    }
    #[test]
    fn privacy_setup_round_trip_and_android_only_material_is_reported() {
        let public = json!({"version":1,"enabled":false,"provider":"dynv6","name":"test.dynv6.net","token":"secret-token","agreeLetsEncryptTerms":true});
        let mut file = export_privacy(json!({"publicName":public}), 1000).unwrap();
        file["importedCertificate"] = json!({});
        file["mfiRemoteToken"] = json!("ignored");
        let p = plan(&serde_json::to_vec(&file).unwrap(), &current()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&p.privacy.unwrap().config.unwrap().to_json()).unwrap(),
            public
        );
        assert_eq!(p.report.applied, ["publicName"]);
        assert_eq!(p.report.ignored, ["importedCertificate", "mfiRemoteToken"]);
        assert!(!p.report.restart_needed);
    }
    #[test]
    fn invalid_json_duplicate_keys_size_version_and_conflicts_are_rejected() {
        for text in [
            "{} trailing",
            "{\"version\":1,\"version\":2}",
            "{\"a\":{\"b\":1,\"b\":2}}",
        ] {
            assert!(plan(text.as_bytes(), &current()).is_err());
        }
        assert_eq!(
            plan(&vec![b' '; MAX_BYTES + 1], &current()).err().unwrap()["file"],
            "too-large"
        );
        let value = json!({"format":USER_FORMAT,"version":2,"settings":{}});
        assert!(plan(&serde_json::to_vec(&value).unwrap(), &current()).is_err());
        let value = json!({"format":USER_FORMAT,"version":1,"settings":{"carlinkName":"a","carlinkDeviceName":"b"}});
        assert!(plan(&serde_json::to_vec(&value).unwrap(), &current()).is_err());
    }
}
