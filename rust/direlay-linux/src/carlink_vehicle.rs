// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Private vehicle snapshots. Raw sensor events must never enter the journal or log exports.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub kind: String,
    pub fields: BTreeMap<String, Value>,
    pub received_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: u64,
    pub time: u64,
    pub reports: Vec<Report>,
}
pub struct Recorder {
    file: PathBuf,
    enabled: bool,
    requested_enabled: bool,
    sample: bool,
    sample_tick: u64,
    sample_next: Option<Instant>,
    latest: BTreeMap<String, Report>,
    history: Vec<Snapshot>,
    next: Option<Instant>,
    storage_failed: bool,
    sensor_active: bool,
    malformed: u64,
}
impl Recorder {
    pub fn new(root: &std::path::Path, enabled: bool) -> Self {
        let file = root.join("vehicle-history.json");
        let read = crate::logs::read_private(&file, 1024 * 1024).and_then(|b| {
            let history: Vec<Snapshot> = b
                .map(|b| serde_json::from_slice(&b))
                .transpose()?
                .unwrap_or_default();
            if history.len() > 30
                || history
                    .iter()
                    .any(|s| s.reports.len() > 8 || s.reports.iter().any(|r| r.fields.len() > 16))
            {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            Ok(history)
        });
        Self {
            file,
            enabled,
            requested_enabled: enabled,
            sample: false,
            sample_tick: 0,
            sample_next: None,
            latest: BTreeMap::new(),
            storage_failed: read.is_err(),
            history: read.unwrap_or_default(),
            next: None,
            sensor_active: false,
            malformed: 0,
        }
    }
    pub fn enable(&mut self, value: bool) {
        self.requested_enabled = value;
        let effective = value || self.sample;
        if self.enabled != effective {
            self.enabled = effective;
            self.latest.clear();
            self.next = None;
            self.sensor_active = false;
        }
    }
    /// Session-only preview follows Android's reproducible raw samples and never writes history.
    pub fn sample(&mut self, value: bool, now: Instant, wall: u64) {
        if self.sample == value {
            return;
        }
        self.sample = value;
        self.sample_tick = 0;
        self.sample_next = None;
        self.latest.clear();
        self.next = None;
        self.sensor_active = false;
        self.enable(self.requested_enabled);
        self.tick(now, wall);
    }
    fn sample_reports(&mut self, now: Instant, wall: u64) {
        if self.sample_next.is_some_and(|next| now < next) {
            return;
        }
        let tick = self.sample_tick;
        let speed = 40 + tick % 40;
        let night = tick % 20 >= 10;
        let timestamp = tick.to_string();
        for data in [
            json!({"type":"Gear","gear":"D","speed":speed}),
            json!({"type":"Gps","height":12,"latitude":31230400,"longitude":121473700,"antennaState":1,"pdop":2,"speed":speed,"heading":90,"satellitesUsed":8,"satellitesVisible":12,"timestamp":timestamp}),
            json!({"type":"Lights","lowBeam":night,"highBeam":false,"positionLamp":true,"reverseLamp":false,"brakeLamp":tick.is_multiple_of(8)}),
            json!({"type":"DayNight","mode":if night {"NIGHT"} else {"DAY"}}),
            json!({"type":"Fuel","max":100,"current":65}),
            json!({"type":"Gyroscope","gyroscopeType":"THREE_AXIS","x":0.1,"y":(tick%4) as f64*0.25,"z":-0.1,"timestamp":timestamp}),
            json!({"type":"Acceleration","x":0.0,"y":0.2,"z":9.8,"timestamp":timestamp}),
            json!({"type":"LightSensor","maxLux":1000.0,"minLux":0.0,"currentLux":if night {12.0} else {650.0}}),
        ] {
            let kind = data["type"].as_str().unwrap().to_owned();
            let fields = data
                .as_object()
                .unwrap()
                .iter()
                .filter(|(key, _)| key.as_str() != "type")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            self.latest.insert(
                kind.clone(),
                Report {
                    kind,
                    fields,
                    received_at: wall,
                },
            );
        }
        self.sample_tick = self.sample_tick.saturating_add(1);
        self.sample_next = Some(now + Duration::from_secs(1));
    }
    /// Return true for every vehicle event, including ignored data while disabled.
    pub fn observe(&mut self, event: &Value, now: Instant) -> bool {
        if self.sample
            && matches!(
                event["type"].as_str(),
                Some("clink-vehicle" | "clink-vehicle-state")
            )
        {
            return true;
        }
        match event["type"].as_str() {
            Some("clink-vehicle-state") => {
                self.sensor_active = self.enabled && event["sensorActive"] == true;
                self.malformed = event["malformedFrames"].as_u64().unwrap_or(0);
            }
            Some("clink-vehicle") => {
                if !self.enabled {
                    return true;
                }
                let Some(kind) = event["data"]["type"].as_str().filter(|k| {
                    [
                        "Gps",
                        "Lights",
                        "Gyroscope",
                        "Acceleration",
                        "Fuel",
                        "Gear",
                        "LightSensor",
                        "DayNight",
                    ]
                    .contains(k)
                }) else {
                    return true;
                };
                let Some(data) = event["data"].as_object() else {
                    return true;
                };
                let fields = data
                    .iter()
                    .filter(|(k, v)| {
                        k.as_str() != "type"
                            && (v.is_number()
                                || v.is_boolean()
                                || v.is_null()
                                || v.as_str().is_some_and(|s| s.len() <= 128))
                    })
                    .take(16)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                self.latest.insert(
                    kind.into(),
                    Report {
                        kind: kind.into(),
                        fields,
                        received_at: event["atMillis"].as_u64().unwrap_or(0),
                    },
                );
                self.next.get_or_insert(now + Duration::from_secs(60));
            }
            _ => return false,
        }
        true
    }
    fn write(&self, history: &[Snapshot]) -> std::io::Result<()> {
        crate::platform::private_state_file(&self.file, &serde_json::to_vec(history)?)
    }
    pub fn tick(&mut self, now: Instant, wall: u64) {
        if self.sample {
            self.sample_reports(now, wall);
            return;
        }
        if !self.enabled || self.latest.is_empty() || self.next.is_none_or(|due| now < due) {
            return;
        }
        let id = wall.max(self.history.first().map_or(0, |s| s.id.saturating_add(1)));
        let mut next = vec![Snapshot {
            id,
            time: wall,
            reports: self.latest.values().cloned().collect(),
        }];
        next.extend(self.history.iter().take(29).cloned());
        self.storage_failed = self.write(&next).is_err();
        if !self.storage_failed {
            self.history = next;
        }
        self.next = Some(now + Duration::from_secs(60));
    }
    pub fn clear(&mut self) -> std::io::Result<()> {
        self.write(&[])?;
        self.history.clear();
        self.storage_failed = false;
        Ok(())
    }
    pub fn view(&self) -> Value {
        json!({"sample":self.sample,"enabled":self.enabled,"sensorActive":self.sensor_active,"malformedFrames":self.malformed,"reports":self.latest.values().collect::<Vec<_>>(),"history":self.history,"storageFailed":self.storage_failed})
    }
    pub fn status(&self) -> Value {
        json!({"sample":self.sample,"enabled":self.enabled,"state":if !self.enabled { "off" } else if self.latest.is_empty() { "waiting" } else { "data" },"sensorActive":self.sensor_active,"storageFailed":self.storage_failed})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn developer_samples_reset_and_never_persist_or_mix_with_real_data() {
        let root = std::env::temp_dir().join(format!(
            "vehicle-sample-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut recorder = Recorder::new(&root, false);
        let now = Instant::now();
        recorder.sample(true, now, 1000);
        assert_eq!(recorder.latest.len(), 8);
        assert_eq!(recorder.latest["Gear"].fields["speed"], 40);
        recorder.observe(
            &json!({"type":"clink-vehicle","data":{"type":"Gear","speed":999}}),
            now,
        );
        recorder.tick(now + Duration::from_secs(120), 2000);
        assert_eq!(recorder.latest["Gear"].fields["speed"], 41);
        assert!(!recorder.file.exists());
        recorder.sample(false, now, 3000);
        assert!(!recorder.enabled);
        assert!(recorder.latest.is_empty());
        recorder.sample(true, now, 3000);
        assert_eq!(recorder.latest["Gear"].fields["speed"], 40);
        assert!(!Recorder::new(&root, false).sample);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn opt_in_monotonic_cadence_bounded_private_history_and_clear() {
        let root = std::env::temp_dir().join(format!(
            "vehicle-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut r = Recorder::new(&root, false);
        let now = Instant::now();
        let event = json!({"type":"clink-vehicle","atMillis":1,"data":{"type":"Gps","latitude":12,"timestamp":"18446744073709551615"}});
        assert!(r.observe(&event, now));
        assert!(r.latest.is_empty());
        r.enable(true);
        r.observe(&event, now);
        r.tick(now + Duration::from_secs(59), 1000);
        assert!(r.history.is_empty());
        for i in 1..=31 {
            r.tick(now + Duration::from_secs(i * 60), 1000);
        }
        assert_eq!(r.history.len(), 30);
        assert_eq!(r.history[0].id, 1030);
        assert_eq!(
            r.history[0].reports[0].fields["timestamp"],
            "18446744073709551615"
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&r.file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        r.enable(false);
        assert!(r.latest.is_empty());
        let mut r = Recorder::new(&root, false);
        assert_eq!(r.history.len(), 30);
        r.clear().unwrap();
        assert!(Recorder::new(&root, true).history.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
