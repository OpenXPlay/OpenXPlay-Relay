// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Pure observation accounting. Wall time labels the journal; durations use source monotonic time.
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    time::Instant,
};
struct Attempt {
    number: u64,
    start: Instant,
    previous: Instant,
    active: Option<Instant>,
    terminal: bool,
    status: String,
    summarized: bool,
    marks: BTreeMap<String, Instant>,
}
impl Attempt {
    fn new(number: u64, at: Instant) -> Self {
        Self {
            number,
            start: at,
            previous: at,
            active: None,
            terminal: false,
            status: String::new(),
            summarized: false,
            marks: BTreeMap::new(),
        }
    }
    fn line(&mut self, domain: &str, event: &str, detail: Value, at: Instant) -> Value {
        let line = json!({"event":"timeline", "domain":domain, "mark":event, "attempt":self.number,
            "t":millis(self.start, at), "dt":millis(self.previous, at), "detail":detail});
        self.previous = self.previous.max(at);
        line
    }
}
fn millis(from: Instant, to: Instant) -> u64 {
    to.saturating_duration_since(from).as_millis() as u64
}
struct Browser {
    id: String,
    info: Value,
    rendered: bool,
    attempt: Attempt,
}
pub struct Timeline {
    process_start: Instant,
    carlink_generation: Option<u64>,
    attempts: BTreeMap<&'static str, Attempt>,
    browsers: VecDeque<Browser>,
    transitions: BTreeMap<String, String>,
}
impl Default for Timeline {
    fn default() -> Self {
        Self {
            process_start: Instant::now(),
            carlink_generation: None,
            attempts: BTreeMap::new(),
            browsers: VecDeque::new(),
            transitions: BTreeMap::new(),
        }
    }
}
impl Timeline {
    pub fn observe(&mut self, event: &str, fields: &Value, at: Instant) -> Vec<Value> {
        let carlink = matches!(event, "link" | "state" | "clink-summary");
        let mut new_generation = false;
        if let Some(generation) = fields["generation"].as_u64().filter(|_| carlink) {
            if self
                .carlink_generation
                .is_some_and(|current| generation < current)
            {
                return vec![];
            }
            new_generation = self.carlink_generation != Some(generation);
            self.carlink_generation = Some(generation);
        }
        let (domain, mark, starts, active, ended) = match event {
            "stage" | "iap2-stage" => {
                let stage = fields["stage"].as_str().unwrap_or("");
                (
                    "phone",
                    stage,
                    stage == "DiscoveringMfi",
                    false,
                    matches!(stage, "Failed" | "Stopped" | "ControlEnded"),
                )
            }
            "airplay-session-active" => ("phone", "session_active", false, true, false),
            "airplay-session-ended" | "bootstrap-ended" => {
                ("phone", "session_ended", false, false, true)
            }
            "airplay-milestone" => (
                "phone",
                fields["name"].as_str().unwrap_or("milestone"),
                false,
                fields["name"] == "first_video_received",
                false,
            ),
            "retry" | "startup-failed" => ("phone", event, false, false, true),
            "link" => {
                let state = fields["state"].as_str().unwrap_or("");
                (
                    "carlink",
                    state,
                    state == "scanning",
                    false,
                    state == "lost",
                )
            }
            "clink-summary" => ("carlink", "summary", false, false, false),
            "state" => {
                let state = fields["state"].as_str().unwrap_or("");
                (
                    "carlink",
                    state,
                    false,
                    state.eq_ignore_ascii_case("playing"),
                    state.eq_ignore_ascii_case("ended"),
                )
            }
            "diagnostic" => {
                let message = fields["message"].as_str().unwrap_or("");
                let key = message.split_whitespace().next().unwrap_or("");
                if !["hotspot", "wifi", "bluetooth", "network", "rfkill"]
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
                {
                    return vec![];
                }
                if self.transitions.get(key).is_some_and(|v| v == message) {
                    return vec![];
                }
                if self.transitions.len() >= 256 {
                    self.transitions.clear();
                }
                self.transitions.insert(key.into(), message.into());
                ("network", key, false, false, false)
            }
            "network-observed" => {
                let key = fields.to_string();
                if self.transitions.get("network-observed") == Some(&key) {
                    return vec![];
                }
                self.transitions.insert("network-observed".into(), key);
                ("network", "radio_transition", false, false, false)
            }
            _ => return vec![],
        };
        if mark.is_empty() {
            return vec![];
        }
        let previous = self.attempts.get(domain);
        let status = format!("{mark} {fields}");
        if previous.is_some_and(|a| a.status == status) {
            return vec![];
        }
        if previous.is_none_or(|a| {
            starts || new_generation || a.terminal && !ended && event != "clink-summary"
        }) {
            let number = previous.map_or(1, |a| a.number + 1);
            self.attempts.insert(
                domain,
                Attempt::new(
                    number,
                    if domain == "network" {
                        self.process_start
                    } else {
                        at
                    },
                ),
            );
        }
        let attempt = self.attempts.get_mut(domain).unwrap();
        attempt.status = status;
        attempt.marks.entry(mark.into()).or_insert(at);
        let mut lines = vec![attempt.line(domain, mark, fields.clone(), at)];
        if active && !attempt.summarized {
            attempt.active = Some(at);
            attempt.summarized = true;
            let mut summary = json!({"total":millis(attempt.start, at)});
            for (name, start, end) in [
                ("mfi", "DiscoveringMfi", "MfiReady"),
                ("hotspot", "StartingHotspot", "HotspotReady"),
                ("bluetooth", "ConnectingBluetooth", "RunningWireless"),
                ("wireless", "RunningWireless", "WirelessActive"),
                ("ble_exchange", "connecting", "joined"),
                ("negotiate", "Negotiating", "Playing"),
            ] {
                if let (Some(start), Some(end)) = (attempt.marks.get(start), attempt.marks.get(end))
                {
                    summary[name] = json!(millis(*start, *end));
                }
            }
            lines.push(attempt.line(domain, "connected", summary, at));
        }
        if ended && !attempt.terminal {
            attempt.terminal = true;
            lines.push(attempt.line(domain, "ended", json!({"played":attempt.active.map_or(0, |start| millis(start, at)), "reason":fields}), at));
        }
        lines
    }
    pub fn browser(&mut self, event: &str, fields: &Value, at: Instant) -> Vec<Value> {
        if matches!(event, "receiver-stopped" | "receiver-restarting") {
            return self
                .browsers
                .drain(..)
                .map(|mut client| {
                    client.attempt.line(
                        "browser",
                        "output_stopped",
                        json!({"client":client.id}),
                        at,
                    )
                })
                .collect();
        }
        if !matches!(
            event,
            "receiver-info" | "stats" | "relay-close" | "receiver-disconnected" | "fullscreen-exit"
        ) {
            return vec![];
        }
        let limit = if event == "receiver-info" {
            65536
        } else {
            4096
        };
        if serde_json::to_vec(fields).map_or(true, |bytes| bytes.len() > limit) {
            return vec![];
        }
        let Some(id) = fields["client"].as_str().filter(|id| id.len() <= 64) else {
            return vec![];
        };
        let mut lines = Vec::new();
        let index = self
            .browsers
            .iter()
            .position(|client| client.id == id)
            .unwrap_or_else(|| {
                if self.browsers.len() == 8 {
                    self.browsers.pop_front();
                }
                let mut attempt = Attempt::new(1, at);
                lines.push(attempt.line(
                    "browser",
                    "client_observed",
                    json!({"client":id, "source":event, "address":"unavailable"}),
                    at,
                ));
                self.browsers.push_back(Browser {
                    id: id.into(),
                    info: Value::Null,
                    rendered: false,
                    attempt,
                });
                self.browsers.len() - 1
            });
        let client = &mut self.browsers[index];
        let mark = match event {
            "receiver-info" => {
                // Retain dimensions/decoder capability, never arbitrary page text or URLs.
                let info = &fields["info"];
                let ua = info["userAgent"].as_str().unwrap_or("");
                let family = [
                    ("Edg/", "Edge"),
                    ("Chrome/", "Chrome"),
                    ("Firefox/", "Firefox"),
                    ("Safari/", "Safari"),
                ]
                .into_iter()
                .find(|(needle, _)| ua.contains(needle))
                .map_or("unknown", |(_, family)| family);
                let detail = json!({"client":id, "ua":family, "screen":info["screen"], "viewport":info["viewport"], "videoDecoder":info["videoDecoder"], "videoDecode":info["videoDecode"]});
                if !client.info.is_null()
                    && (client.info["screen"] != detail["screen"]
                        || client.info["viewport"] != detail["viewport"])
                {
                    lines.push(client.attempt.line(
                        "browser",
                        "display_changed",
                        detail.clone(),
                        at,
                    ));
                }
                if client.info != detail {
                    client.info = detail.clone();
                    lines.push(client.attempt.line("browser", "receiver_info", detail, at));
                }
                None
            }
            "stats"
                if !client.rendered
                    && (fields["rendered"].as_u64().unwrap_or(0) > 0
                        || fields["renderedFps"].as_f64().unwrap_or(0.0) > 0.0) =>
            {
                client.rendered = true;
                Some("first_render_observed")
            }
            "relay-close" | "receiver-disconnected" => Some("client_disconnected"),
            "fullscreen-exit" => Some("display_changed"),
            _ => None,
        };
        if let Some(mark) = mark {
            lines.push(client.attempt.line(
                "browser",
                mark,
                json!({"client":id, "source":event}),
                at,
            ));
        }
        if matches!(event, "relay-close" | "receiver-disconnected") {
            self.browsers.remove(index);
        }
        lines
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn attempts_use_monotonic_deltas_summarize_played_time_and_restart_numbers() {
        let mut timeline = Timeline::default();
        let start = Instant::now();
        assert!(timeline
            .observe("stage", &json!({"message":"not a phase"}), start)
            .is_empty());
        let stage = json!({"stage":"DiscoveringMfi"});
        assert_eq!(timeline.observe("stage", &stage, start)[0]["attempt"], 1);
        assert!(timeline.observe("stage", &stage, start).is_empty());
        timeline.observe(
            "airplay-session-active",
            &json!({}),
            start + Duration::from_millis(500),
        );
        let ended = timeline.observe(
            "airplay-session-ended",
            &json!({}),
            start + Duration::from_millis(2000),
        );
        assert_eq!(ended[0]["dt"], 1500);
        assert_eq!(ended[1]["detail"]["played"], 1500);
        assert_eq!(
            timeline.observe("stage", &stage, start + Duration::from_millis(3000))[0]["attempt"],
            2
        );
    }
    #[test]
    fn carlink_generations_reject_late_events_and_keep_summary_identifiers() {
        let mut timeline = Timeline::default();
        let now = Instant::now();
        timeline.observe("link", &json!({"generation":1,"state":"scanning"}), now);
        let next = timeline.observe("link", &json!({"generation":2,"state":"scanning"}), now);
        assert_eq!(next[0]["attempt"], 2);
        assert!(timeline
            .observe("state", &json!({"generation":1,"state":"Ended"}), now)
            .is_empty());
        let summary = timeline.observe("clink-summary", &json!({"generation":2,"fields":{"vendor":"Test","frequency":"5180","ssid":"TechnicalWifi","hevc":"on","uibc":"on"}}), now);
        assert_eq!(summary[0]["attempt"], 2);
        assert_eq!(summary[0]["detail"]["fields"]["frequency"], "5180");
    }
    #[test]
    fn browser_first_render_deduplication_disconnect_and_radio_transitions() {
        let mut timeline = Timeline::default();
        let now = Instant::now();
        let stats = json!({"client":"1", "rendered":1});
        assert_eq!(timeline.browser("stats", &stats, now).len(), 2);
        assert!(timeline.browser("stats", &stats, now).is_empty());
        let info = json!({"client":"1","info":{"userAgent":"Chrome/test","screen":{"width":800,"height":480}}});
        timeline.browser("receiver-info", &info, now);
        let changed = timeline.browser(
            "receiver-info",
            &json!({"client":"1","info":{"screen":{"width":1024,"height":600}}}),
            now,
        );
        assert_eq!(changed[0]["mark"], "display_changed");
        assert_eq!(
            timeline.browser("relay-close", &stats, now)[0]["mark"],
            "client_disconnected"
        );
        let radio = json!({"wifi":"up", "ssid":"TechWifi"});
        assert_eq!(
            timeline.observe("network-observed", &radio, now)[0]["mark"],
            "radio_transition"
        );
        assert!(timeline.observe("network-observed", &radio, now).is_empty());
    }
}
