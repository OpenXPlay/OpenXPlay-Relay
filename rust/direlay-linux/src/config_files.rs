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
fn error(key: &str, code: &str) -> Errors {
    BTreeMap::from([(key.into(), code.into())])
}
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub applied: Vec<String>,
    pub ignored: Vec<String>,
    pub restart_needed: bool,
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
pub fn export_user(settings: &Settings, display: &Value, now: u64) -> Result<Value, Errors> {
    let mut value = envelope(USER_FORMAT, now);
    let mut values = settings_values(settings, display);
    for (linux, android) in [
        ("carlinkName", "carlinkDeviceName"),
        ("carlinkModel", "carlinkDeviceModel"),
        ("autoStart", "autoStartOnBoot"),
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
    let car: Values = [
        ("carBrand", "brand"),
        ("carBrandText", "brandText"),
        ("carModel", "model"),
        ("carModelText", "modelText"),
        ("carYear", "year"),
    ]
    .into_iter()
    .filter_map(|(linux, android)| values.get(linux).cloned().map(|v| (android.into(), v)))
    .collect();
    if !car.is_empty() {
        values.insert("car".into(), Value::Object(car));
    }
    let mut experimental: Values = [
        ("experimentalActive", "active"),
        ("experimentalSendBluetoothAddress", "sendBluetoothAddress"),
        ("experimentalBluetoothAddress", "bluetoothAddress"),
    ]
    .into_iter()
    .filter_map(|(linux, android)| values.get(linux).cloned().map(|v| (android.into(), v)))
    .collect();
    if !experimental.is_empty() {
        experimental.insert("version".into(), json!(2));
        values.insert("experimental".into(), Value::Object(experimental));
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
    values.insert("carlinkTrace".into(), json!(settings.carlink_trace == "on"));
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
pub fn plan_with_privacy(
    bytes: &[u8],
    current: &Values,
    public: Option<&crate::public_name::config::Config>,
    now: u64,
) -> Result<Plan, Errors> {
    if bytes.len() > MAX_BYTES {
        return Err(error("file", "too-large"));
    }
    let Strict(value) =
        serde_json::from_slice::<Strict>(bytes).map_err(|_| error("file", "not-json"))?;
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
            let mut ignored = Vec::new();
            for (key, value) in settings {
                if key == "output" && settings.contains_key("outputs") {
                    continue;
                }
                import_key(&mut plan.values, &mut ignored, current, key, value, key)?;
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
            plan.report.restart_needed = plan.values.keys().any(|key| restart_key(key));
        }
        Some(PRIVACY_FORMAT) => {
            let privacy = crate::public_name::privacy::decode(&value, public, now)?;
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
fn restart_key(key: &str) -> bool {
    !matches!(
        key,
        "teslaBrowser"
            | "teslaVirtualAddress"
            | "teslaPublicAddress"
            | "teslaMirrorAddress"
            | "teslaAndroidAddress"
            | "autoStart"
            | "videoStats"
            | "carBrand"
            | "carBrandText"
            | "carModel"
            | "carModelText"
            | "carYear"
            | "experimentalActive"
            | "experimentalBluetoothAddress"
            | "experimentalSendBluetoothAddress"
    )
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
        "autoStartOnBoot" => "autoStart",
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
        "car" | "experimental" => {
            let object = value.as_object().ok_or_else(|| error(source, "invalid"))?;
            for (leaf, value) in object {
                let target = match (key, leaf.as_str()) {
                    ("car", "brand") => "carBrand",
                    ("car", "brandText") => "carBrandText",
                    ("car", "model") => "carModel",
                    ("car", "modelText") => "carModelText",
                    ("car", "year") => "carYear",
                    ("experimental", "active") => "experimentalActive",
                    ("experimental", "bluetoothAddress") => "experimentalBluetoothAddress",
                    ("experimental", "sendBluetoothAddress") => "experimentalSendBluetoothAddress",
                    ("experimental", "version") if value == 2 => continue,
                    _ => {
                        ignored.push(format!("{source}.{leaf}"));
                        continue;
                    }
                };
                import_key(
                    values,
                    ignored,
                    current,
                    target,
                    value,
                    &format!("{source}.{leaf}"),
                )?;
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
        let p = plan(&serde_json::to_vec(&file).unwrap(), &current()).unwrap();
        assert_eq!(p.values["output"], "both");
        assert_eq!(p.values["autoStart"], false);
        assert_eq!(p.values["carlinkName"], "Car display");
        assert_eq!(p.values["carlinkTrace"], "on");
        assert_eq!(p.values["screenWidth"], 125);
        assert_eq!(p.report.ignored, ["localPreview", "remoteToken"]);
        assert!(p.report.restart_needed);
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
            assert!(p.public_name.is_none());
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
