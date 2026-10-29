// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Plain runtime observations for the admin presenter. No I/O or JSON assembly at read time.
use crate::{carplay_policy, display::Size};
use std::{collections::VecDeque, time::Instant};

/// First wall timestamp plus monotonic offsets; keep the first line and most recent facts.
#[derive(Clone, Debug, Default)]
pub struct StepTrail {
    lines: Vec<String>,
    start: Option<Instant>,
    omitted: usize,
}
impl StepTrail {
    pub fn record(&mut self, line: &str, wall: u64, now: Instant) {
        let prefix = if let Some(start) = self.start {
            format!(
                "+{:.3}s ",
                now.saturating_duration_since(start).as_secs_f64()
            )
        } else {
            self.start = Some(now);
            format!(
                "{:02}:{:02}:{:02}.{:03} ",
                wall / 3_600_000 % 24,
                wall / 60_000 % 60,
                wall / 1000 % 60,
                wall % 1000
            )
        };
        self.lines
            .push(format!("{prefix}{line}").chars().take(200).collect());
        if self.lines.len() > 80 {
            if self.omitted == 0 {
                self.lines.remove(1);
                self.lines.remove(1);
                self.lines.insert(1, String::new());
                self.omitted = 2;
            } else {
                self.lines.remove(2);
                self.omitted += 1;
            }
            self.lines[1] = format!("{} earlier lines omitted", self.omitted);
        }
    }
    pub fn snapshot(&self) -> Vec<String> {
        self.lines.clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct LinkFailure {
    pub step: String,
    pub code: String,
    pub text: String,
    pub at_millis: u64,
    pub retrying: bool,
    pub trail: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct IphoneFacts {
    pub step: String,
    pub since_millis: Option<u64>,
    pub detail: Option<String>,
    pub failure: Option<LinkFailure>,
    pub session_since_millis: Option<u64>,
    pub car_wait_until_millis: Option<u64>,
    pub armed: bool,
    pub runtime_iap2_frames: u64,
}
impl Default for IphoneFacts {
    fn default() -> Self {
        Self {
            step: "IDLE".into(),
            since_millis: None,
            detail: None,
            failure: None,
            session_since_millis: None,
            car_wait_until_millis: None,
            armed: false,
            runtime_iap2_frames: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct CarlinkFacts {
    pub phase: String,
    pub since_millis: Option<u64>,
    pub code: Option<String>,
    pub error: Option<String>,
    pub retrying: bool,
    pub trail: Vec<String>,
    pub picture: Option<String>,
    pub screen: Option<Size>,
    pub car_name: Option<String>,
    pub model_id: Option<String>,
    pub join_method: Option<String>,
    pub trusted: bool,
    pub pin_pending: bool,
    pub live_frames: u64,
    pub phase_timings: Vec<(String, u64)>,
    pub failure: Option<LinkFailure>,
}
impl Default for CarlinkFacts {
    fn default() -> Self {
        Self {
            phase: "OFF".into(),
            since_millis: None,
            code: None,
            error: None,
            retrying: false,
            trail: vec![],
            picture: None,
            screen: None,
            car_name: None,
            model_id: None,
            join_method: None,
            trusted: false,
            pin_pending: false,
            live_frames: 0,
            phase_timings: vec![],
            failure: None,
        }
    }
}
#[derive(Clone, Debug)]
pub struct WebFacts {
    pub phase: String,
    pub port: u16,
    pub code: Option<String>,
    pub since_millis: Option<u64>,
}
impl Default for WebFacts {
    fn default() -> Self {
        Self {
            phase: "OFF".into(),
            port: 0,
            code: None,
            since_millis: None,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct TeslaFacts {
    pub url: Option<String>,
    pub failure: Option<String>,
    pub since_millis: Option<u64>,
    pub activation: Option<crate::tesla_address::Activation>,
}
/// WP3 publishes `usb-presence` on discovery, re-enumeration and unplug:
/// `{ "present": true, "product": "iPhone" }` or `{ "present": false }`.
/// Presence is independent of attempts and is not cleared by reset_phone.
#[derive(Clone, Debug, Default)]
pub struct UsbPresence {
    pub present: bool,
    pub product: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct RuntimePathFacts {
    pub iphone: IphoneFacts,
    pub usb: UsbPresence,
    pub automatic: bool,
    pub boot_start: bool,
    pub auto_start_paused: bool,
    pub arrival_until_millis: Option<u64>,
    pub arrival_remaining_millis: Option<u64>,
    pub carlink: CarlinkFacts,
    pub web: WebFacts,
    pub tesla: TeslaFacts,
    pub pages: usize,
    pub screen: Option<Size>,
    pub saved_screen: Option<Size>,
    pub built_in_screen: Option<Size>,
    pub session: Option<u64>,
    pub picture_on_car: bool,
    pub chips: Vec<String>,
    pub timeline: VecDeque<String>,
    pub carlink_summary: Option<String>,
    phone_trail: StepTrail,
    carlink_trail: StepTrail,
    video_since: Option<Instant>,
    carlink_started: Option<u64>,
    carlink_summarized: bool,
}
impl RuntimePathFacts {
    fn step(&mut self, step: &str, now: u64) {
        if self.iphone.step != step {
            self.iphone.step = step.into();
            self.iphone.since_millis = Some(now);
        }
    }
    pub fn reset_phone(&mut self) {
        self.iphone = IphoneFacts::default();
        self.phone_trail = StepTrail::default();
        self.video_since = None;
        self.picture_on_car = false;
    }
    pub fn failure(&mut self, message: &str, retrying: bool, now: u64) {
        self.failure_at(message, retrying, now, Instant::now());
    }
    pub(crate) fn failure_at(
        &mut self,
        message: &str,
        retrying: bool,
        now: u64,
        monotonic: Instant,
    ) {
        let code = carplay_policy::classify_failure(message);
        let step = match code {
            "hotspot_not_ready"
            | "hotspot_configuration"
            | "wifi_off"
            | "wifi_reset_required"
            | "carlink_not_ready" => "STARTING",
            "bluetooth_off" | "bluetooth_unusable" | "iphone_unpaired" | "rfcomm_timeout" => {
                "BLUETOOTH"
            }
            "first_tcp_timeout" => "JOINING",
            _ => &self.iphone.step,
        }
        .to_owned();
        let text = crate::failure_text::redact(message, &[]);
        self.phone_trail.record(
            &format!("status=Failed reason={code} retrying={retrying}"),
            now,
            monotonic,
        );
        self.video_since = None;
        self.picture_on_car = false;
        self.iphone.session_since_millis = None;
        self.iphone.failure = Some(LinkFailure {
            step: step.clone(),
            code: code.into(),
            text,
            at_millis: now,
            retrying,
            trail: self.phone_trail.snapshot(),
        });
        self.step(&step, now);
        self.iphone.detail = None;
    }
    /// Playback state remains available when optional milestone diagnostics are disabled.
    pub fn video_received(&mut self, now: u64, monotonic: Instant) {
        self.video_since.get_or_insert(monotonic);
        self.step("VIDEO", now);
        self.iphone.failure = None;
    }
    pub fn stable_video(&self, now: Instant) -> bool {
        self.video_since.is_some_and(|since| {
            now.saturating_duration_since(since) >= crate::start_policy::STABLE_SESSION
        })
    }
    pub fn phone_trail(&self) -> Vec<String> {
        self.phone_trail.snapshot()
    }
    /// Returns the startup reason to publish, when this event changes the startup state.
    pub fn observe(
        &mut self,
        kind: &str,
        fields: &serde_json::Value,
        now: u64,
    ) -> Option<&'static str> {
        self.observe_at(kind, fields, now, Instant::now())
    }
    pub(crate) fn observe_at(
        &mut self,
        kind: &str,
        fields: &serde_json::Value,
        now: u64,
        monotonic: Instant,
    ) -> Option<&'static str> {
        let mut reason = None;
        if kind == "usb-presence" {
            self.usb.present = fields["present"].as_bool().unwrap_or(false);
            self.usb.product = fields["product"]
                .as_str()
                .filter(|_| self.usb.present)
                .map(str::to_owned);
        }
        if kind == "stage" || kind == "iap2-stage" {
            let stage = fields["stage"].as_str().unwrap_or("");
            let waiting = stage == "WaitingForPairedIphone" && fields["unreachable"] == true;
            if waiting {
                self.iphone.failure = None;
                self.iphone.detail = None;
                self.video_since = None;
                self.iphone.session_since_millis = None;
                self.picture_on_car = false;
                let message =
                    crate::failure_text::redact(fields["reason"].as_str().unwrap_or(""), &[]);
                self.phone_trail
                    .record(&format!("waiting_for_iphone {message}"), now, monotonic);
            }
            if stage == "DiscoveringMfi" {
                self.phone_trail = StepTrail::default();
                self.iphone.runtime_iap2_frames = 0;
                self.video_since = None;
                self.picture_on_car = false;
                self.iphone.session_since_millis = None;
            }
            if let Some((step, r)) = carplay_policy::startup_stage(stage)
                .filter(|_| !matches!(stage, "Failed" | "Stopped") || self.iphone.failure.is_none())
            {
                // A late handoff callback must not move an active stream backwards.
                if !(matches!(
                    step,
                    "JOINING" | "WIFI_SENT" | "AUTH" | "USB_DETECT" | "USB_TRUST" | "USB_CONTROL"
                ) && matches!(self.iphone.step.as_str(), "SESSION" | "VIDEO"))
                {
                    self.step(step, now);
                    self.phone_trail
                        .record(&format!("status={stage}"), now, monotonic);
                    reason = Some(r);
                }
            }
            if stage == "Stopped" {
                if let Some(failure) = &mut self.iphone.failure {
                    failure.retrying = false;
                }
                self.iphone.session_since_millis = None;
                self.video_since = None;
                self.picture_on_car = false;
            }
            if let Some(message) = fields["reason"]
                .as_str()
                .filter(|_| !waiting && fields["retryMillis"].is_number())
            {
                if !self.iphone.failure.as_ref().is_some_and(|f| {
                    f.retrying && f.text == crate::failure_text::redact(message, &[])
                }) {
                    self.failure_at(message, true, now, monotonic);
                }
            }
        } else if kind == "retry" || kind == "startup-failed" {
            self.failure_at(
                fields["reason"].as_str().unwrap_or(""),
                kind == "retry",
                now,
                monotonic,
            );
            reason = Some("failed");
        } else if kind == "airplay-session-active" {
            self.phone_trail.record("session_active", now, monotonic);
            self.step("SESSION", now);
            self.iphone.failure = None;
            self.iphone.session_since_millis.get_or_insert(now);
            reason = Some("waiting-media");
        } else if kind == "airplay-milestone" {
            let name = fields["name"].as_str().unwrap_or("");
            self.phone_trail
                .record(&format!("milestone={name}"), now, monotonic);
            self.timeline(format!("{now} phone {name}"));
            if name == "first_video_received" {
                self.video_received(now, monotonic);
            }
        }
        if kind == "runtime-iap2" {
            self.phone_trail.record(
                &format!("runtime_iap2 frames={}", fields["frames"]),
                now,
                monotonic,
            );
        }
        if kind == "diagnostic" {
            let message = fields["message"].as_str().unwrap_or("");
            let hex_run = message
                .split(|c: char| !c.is_ascii_hexdigit())
                .any(|s| s.len() >= 16);
            if message.len() <= 160 && message.bytes().all(|b| (32..=126).contains(&b)) && !hex_run
            {
                self.phone_trail
                    .record(&format!("progress {message}"), now, monotonic);
            }
        }
        if let Some(r) = reason {
            let line = format!("{now} phone {} startup/{r}", self.iphone.step);
            self.timeline(line);
        }
        reason
    }
    pub fn timeline(&mut self, line: String) {
        if self.timeline.len() == 160 {
            self.timeline.pop_front();
        }
        self.timeline.push_back(line);
    }
    pub fn carlink_event(&mut self, event: &serde_json::Value, now: u64) -> Option<String> {
        self.carlink_event_at(event, now, Instant::now())
    }
    pub(crate) fn carlink_event_at(
        &mut self,
        event: &serde_json::Value,
        now: u64,
        monotonic: Instant,
    ) -> Option<String> {
        let kind = event["type"].as_str().unwrap_or("");
        if kind == "receiver-starting" || kind == "carlink-starting" {
            self.carlink_summarized = false;
            self.carlink.car_name = None;
            self.carlink.model_id = None;
            self.carlink.join_method = None;
            self.carlink.live_frames = 0;
            self.carlink.screen = None;
            self.carlink.picture = None;
            self.carlink.phase_timings.clear();
            self.carlink_trail = StepTrail::default();
            self.carlink.trail.clear();
            self.carlink_started = Some(now);
        }
        let phase = match kind {
            "link" if event["state"] != "failed" => event["state"].as_str().map(|s| match s {
                "scanning" => "SCANNING",
                "connecting" => "EXCHANGING",
                "joined" => "AUTHENTICATING",
                "lost" => "ENDED",
                _ => "SCANNING",
            }),
            "state" => event["state"]
                .as_str()
                .map(|s| match s.to_ascii_lowercase().as_str() {
                    "playing" | "active" => "PLAYING",
                    "ended" | "disconnected" => "ENDED",
                    "negotiating" => "NEGOTIATING",
                    _ => "AUTHENTICATING",
                }),
            "carlink-starting" => Some("SCANNING"),
            "pin-request" => Some("PIN"),
            "carlink-backoff" | "error" | "pin-timeout" => Some("ENDED"),
            "receiver-stopped" if self.carlink.phase != "OFF" => Some("ENDED"),
            "carlink-paused" => Some("ENDED"),
            "carlink-unpaired" => Some("ENDED"),
            _ => None,
        };
        if let Some(phase) = phase {
            if self.carlink.phase != phase {
                self.carlink.phase = phase.into();
                self.carlink.since_millis = Some(now);
                self.carlink.phase_timings.push((
                    phase.into(),
                    now.saturating_sub(self.carlink_started.unwrap_or(now)),
                ));
                self.carlink.phase_timings.truncate(32);
                let line = format!("{now} carlink {phase}");
                self.carlink_trail
                    .record(&format!("phase={phase}"), now, monotonic);
                self.carlink.trail = self.carlink_trail.snapshot();
                self.timeline(line);
            }
            if phase == "PLAYING" {
                self.carlink.failure = None;
                self.carlink.code = None;
                self.carlink.error = None;
                self.carlink.retrying = false;
            }
        }
        if kind == "pin-request" {
            self.carlink.pin_pending = true;
        }
        if matches!(kind, "receiver-stopped" | "pin-timeout" | "clink-pairing")
            || self.carlink.phase == "PLAYING"
        {
            self.carlink.pin_pending = false;
        }
        if kind == "clink-pin" && event["result"] == "incorrect" {
            self.carlink.code = Some("pin_incorrect".into());
        }
        if kind == "clink-summary" {
            let mut parts = Vec::new();
            for key in [
                "vendor",
                "product",
                "name",
                "ssid",
                "frequency",
                "mode",
                "view",
                "window",
                "stream",
                "codec",
                "hevc",
                "audio",
                "uibc",
            ] {
                if let Some(value) = event["fields"][key].as_str() {
                    let safe = crate::failure_text::redact(value, &[])
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(512)
                        .collect::<String>();
                    parts.push(format!("{key}={safe:?}"));
                }
            }
            if self.carlink.phase == "PLAYING" {
                if let Some(size) = event["fields"]["stream"]
                    .as_str()
                    .filter(|s| *s != "unknown")
                {
                    self.carlink.picture = Some(format!(
                        "{} {}",
                        size,
                        event["fields"]["codec"].as_str().unwrap_or("")
                    ));
                }
            }
            let summary = format!("carlink summary {}", parts.join(" "));
            for part in &parts {
                self.carlink_trail.record(part, now, monotonic);
            }
            self.carlink.trail = self.carlink_trail.snapshot();
            if let Some(failure) = self.carlink.failure.as_mut() {
                failure.trail = self.carlink.trail.clone();
            }
            self.carlink_summary = Some(summary.clone());
            self.carlink_summarized = true;
            self.timeline(format!("{now} {summary}"));
            return Some(summary);
        }
        if kind == "link" && event["state"] == "failed" {
            let code = event["code"].as_str().unwrap_or("link_failed");
            let mut detail = Vec::new();
            for key in [
                "stage",
                "code",
                "elapsedMs",
                "totalElapsedMs",
                "attempt",
                "osCode",
                "operation",
                "status",
            ] {
                if !event[key].is_null() {
                    detail.push(format!("{key}={}", event[key]));
                }
            }
            let text = crate::failure_text::redact(&detail.join(" "), &[]);
            self.carlink.code = Some(code.into());
            self.carlink.error = Some(text.clone());
            self.carlink_trail.record(&text, now, monotonic);
            self.carlink.trail = self.carlink_trail.snapshot();
            self.carlink.failure = Some(LinkFailure {
                step: self.carlink.phase.clone(),
                code: code.into(),
                text,
                at_millis: now,
                retrying: true,
                trail: self.carlink.trail.clone(),
            });
            self.carlink.retrying = true;
        }
        let retained_backoff = ((kind == "carlink-backoff")
            || (kind == "link" && event["state"] == "lost"))
            && self.carlink.failure.is_some();
        if retained_backoff {
            self.carlink.retrying = true;
            self.carlink.failure.as_mut().unwrap().retrying = true;
        }
        if !retained_backoff
            && (matches!(kind, "error" | "pin-timeout" | "carlink-backoff")
                || (kind == "link" && event["state"] == "lost"))
        {
            let message = event["reason"]
                .as_str()
                .or_else(|| event["message"].as_str())
                .unwrap_or(if kind == "pin-timeout" {
                    "pin timeout"
                } else {
                    "connection failed"
                });
            self.carlink.code = Some(crate::carlink_policy::failure_code(message).into());
            self.carlink.error = Some(crate::failure_text::redact(message, &[]));
            self.carlink.retrying = kind == "carlink-backoff";
            self.carlink_trail.record(
                &format!("failed {}", self.carlink.code.as_deref().unwrap()),
                now,
                monotonic,
            );
            self.carlink.trail = self.carlink_trail.snapshot();
            self.carlink.failure = Some(LinkFailure {
                step: self.carlink.phase.clone(),
                code: self.carlink.code.clone().unwrap(),
                text: self.carlink.error.clone().unwrap(),
                at_millis: now,
                retrying: self.carlink.retrying,
                trail: self.carlink.trail.clone(),
            });
        }
        if let Some(name) = event["headUnit"]["name"].as_str() {
            self.carlink.car_name =
                Some(name.chars().filter(|c| !c.is_control()).take(128).collect());
        }
        let model = event["modelId"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| event["modelId"].as_u64().map(|n| n.to_string()));
        if let Some(id) = model {
            self.carlink.model_id = Some(id.chars().filter(|c| !c.is_control()).take(64).collect());
        }
        if kind == "link" && event["state"] == "joined" {
            self.carlink.join_method = Some(
                if event["network"]["ssid"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("DIRECT-"))
                {
                    "Wi-Fi Direct"
                } else {
                    "Wi-Fi"
                }
                .into(),
            );
        }
        if kind == "media-stats" && self.carlink.phase == "PLAYING" {
            let size = event["width"]
                .as_u64()
                .zip(event["height"].as_u64())
                .filter(|(w, h)| (1..=32768).contains(w) && (1..=32768).contains(h))
                .map(|(w, h)| Size {
                    width: w as u32,
                    height: h as u32,
                });
            if let Some(s) = size {
                self.carlink.screen = Some(s);
                self.carlink.picture = Some(
                    format!(
                        "{}×{} {}",
                        s.width,
                        s.height,
                        match event["codec"].as_str() {
                            Some("H264") => "H.264",
                            Some("H265") => "H.265",
                            _ => "",
                        }
                    )
                    .trim()
                    .into(),
                );
            }
        }
        let mut first = false;
        if kind == "media-stats" {
            let count = event["liveFrames"].as_u64().unwrap_or(0);
            first = count > 0 && self.carlink.live_frames == 0;
            self.carlink.live_frames = count;
        }
        let ended = self.carlink.phase == "ENDED"
            && (self.carlink.car_name.is_some()
                || self.carlink.model_id.is_some()
                || self.carlink.picture.is_some());
        if !self.carlink_summarized && (first || ended) {
            self.carlink_summarized = true;
            let summary = format!(
                "carlink car name={:?} model={:?} join={:?} picture={:?} timings={:?} code={:?}",
                self.carlink.car_name,
                self.carlink.model_id,
                self.carlink.join_method,
                self.carlink.picture,
                self.carlink.phase_timings,
                self.carlink.code
            );
            self.carlink_summary = Some(summary.clone());
            self.timeline(format!("{now} {summary}"));
            return Some(summary);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_link_failure_and_full_summary_survive_in_path_trails() {
        let mut f = RuntimePathFacts::default();
        f.carlink_event(&serde_json::json!({"type":"link","state":"connecting"}), 1);
        f.carlink_event(&serde_json::json!({"type":"link","state":"failed","stage":"wifi_join","code":"system_tool_failed","elapsedMs":70,"totalElapsedMs":3000,"attempt":2,"operation":"nmcli","status":7}),2);
        assert_eq!(f.carlink.phase, "EXCHANGING");
        assert_eq!(f.carlink.code.as_deref(), Some("system_tool_failed"));
        f.carlink_event(
            &serde_json::json!({"type":"link","state":"lost","reason":"connection failed"}),
            3,
        );
        assert_eq!(f.carlink.code.as_deref(), Some("system_tool_failed"));
        let summary=f.carlink_event(&serde_json::json!({"type":"clink-summary","fields":{"vendor":"Vendor","product":"Car","frequency":"5180","hevc":"supported","audio":"true","uibc":"HIDC"}}),4).unwrap();
        for value in [
            "vendor=",
            "product=",
            "frequency=",
            "hevc=",
            "audio=",
            "uibc=",
        ] {
            assert!(summary.contains(value));
            assert!(f
                .carlink
                .failure
                .as_ref()
                .unwrap()
                .trail
                .iter()
                .any(|s| s.contains(value)));
        }
    }
    #[test]
    fn trail_keeps_first_and_latest_monotonic_offsets_under_clock_changes() {
        let mut trail = StepTrail::default();
        let t = Instant::now();
        trail.record("first", 3600123, t);
        for i in 1..=100 {
            trail.record(
                &format!("line {i}"),
                0,
                t + std::time::Duration::from_millis(i * 10),
            );
        }
        let lines = trail.snapshot();
        assert_eq!(lines.len(), 80);
        assert!(lines[0].ends_with("00:00.123 first"));
        assert_eq!(lines[1], "22 earlier lines omitted");
        assert_eq!(lines.last().unwrap(), "+1.000s line 100");
    }
    #[test]
    fn phone_and_carlink_failures_freeze_details_until_success() {
        let mut facts = RuntimePathFacts::default();
        facts.observe("stage", &serde_json::json!({"stage":"RunningWireless"}), 1);
        facts.observe(
            "startup-failed",
            &serde_json::json!({"reason":"RFCOMM timeout error=17 password=secret"}),
            2,
        );
        let failure = facts.iphone.failure.clone().unwrap();
        facts.observe("stage", &serde_json::json!({"stage":"DiscoveringMfi"}), 3);
        assert_eq!(facts.iphone.failure.as_ref().unwrap().trail, failure.trail);
        assert!(failure.text.contains("error=17"));
        assert!(!failure.text.contains("secret"));
        facts.carlink_event(
            &serde_json::json!({"type":"error","message":"AUTH rejected token=secret"}),
            4,
        );
        let failure = facts.carlink.failure.clone().unwrap();
        facts.carlink_event(
            &serde_json::json!({"type":"carlink-backoff","reason":"retrying"}),
            5,
        );
        assert_eq!(
            facts.carlink.failure.as_ref().unwrap().at_millis,
            failure.at_millis
        );
        assert_eq!(facts.carlink.error, Some(failure.text.clone()));
        facts.carlink_event(&serde_json::json!({"type":"receiver-starting"}), 5);
        facts.carlink_event(&serde_json::json!({"type":"link","state":"scanning"}), 6);
        assert_eq!(facts.carlink.failure.as_ref().unwrap().trail, failure.trail);
        assert_eq!(facts.carlink.error, Some(failure.text));
        facts.carlink_event(&serde_json::json!({"type":"state","state":"playing"}), 7);
        assert!(facts.carlink.failure.is_none());
        assert!(facts.carlink.error.is_none());
    }
    #[test]
    fn only_sixty_seconds_of_video_restores_the_retry_budget() {
        let mut facts = RuntimePathFacts::default();
        let now = Instant::now();
        facts.observe("airplay-session-active", &serde_json::json!({}), 0);
        assert!(!facts.stable_video(now + std::time::Duration::from_secs(60)));
        facts.video_since = Some(now);
        assert!(!facts.stable_video(now + std::time::Duration::from_secs(59)));
        assert!(facts.stable_video(now + std::time::Duration::from_secs(60)));
        facts.failure("ended", true, 1);
        assert!(!facts.stable_video(now + std::time::Duration::from_secs(61)));
    }
    #[test]
    fn stages_keep_since_failure_trail_and_late_handoff_order() {
        let mut f = RuntimePathFacts::default();
        for now in [10, 20] {
            assert_eq!(
                f.observe(
                    "stage",
                    &serde_json::json!({"stage":"RunningWireless"}),
                    now
                ),
                Some("authenticating")
            );
        }
        assert_eq!(f.iphone.since_millis, Some(10));
        f.observe(
            "retry",
            &serde_json::json!({"reason":"RFCOMM timeout password=secret"}),
            30,
        );
        let error = f.iphone.failure.as_ref().unwrap();
        assert_eq!(error.code, "rfcomm_timeout");
        assert!(error.retrying);
        assert!(!error.text.contains("secret"));
        f.observe("airplay-session-active", &serde_json::json!({}), 40);
        f.observe("stage", &serde_json::json!({"stage":"WirelessActive"}), 50);
        assert_eq!(f.iphone.step, "SESSION");
        assert!(f.iphone.failure.is_none());
    }
    #[test]
    fn carlink_scan_is_not_failure_and_summary_requires_live_frames() {
        let mut f = RuntimePathFacts::default();
        f.carlink_event(&serde_json::json!({"type":"link","state":"scanning"}), 1);
        assert_eq!(f.carlink.phase, "SCANNING");
        assert!(f.carlink.code.is_none());
        assert!(f
            .carlink_event(&serde_json::json!({"type":"media-stats","liveFrames":0}), 2)
            .is_none());
        let e = serde_json::json!({"type":"media-stats","liveFrames":1,"width":1280,"height":720,"codec":"H264"});
        assert!(f.carlink_event(&e, 3).is_some());
        assert!(f.carlink_event(&e, 4).is_none());
        f.carlink_event(&serde_json::json!({"type":"receiver-starting"}), 5);
        f.carlink_event(
            &serde_json::json!({"type":"link","state":"connecting","headUnit":{"name":"Car"}}),
            6,
        );
        assert!(f
            .carlink_event(
                &serde_json::json!({"type":"error","message":"auth_rejected"}),
                7
            )
            .is_some());
        assert_eq!(f.carlink.code.as_deref(), Some("auth_rejected"));
    }
}

#[cfg(test)]
mod usb_tests {
    use super::*;
    #[test]
    fn usb_presence_survives_attempt_reset_and_unplug_clears_product() {
        let mut facts = RuntimePathFacts::default();
        facts.observe(
            "usb-presence",
            &serde_json::json!({"present":true,"product":"iPhone 16"}),
            1,
        );
        facts.reset_phone();
        assert!(facts.usb.present);
        assert_eq!(facts.usb.product.as_deref(), Some("iPhone 16"));
        for stage in [
            carplay_policy::WAITING_FOR_IPHONE_USB,
            carplay_policy::PAIRING,
            carplay_policy::START_SESSION_SENT,
        ] {
            facts.observe("stage", &serde_json::json!({"stage":stage}), 2);
            assert_eq!(
                facts.iphone.step,
                carplay_policy::startup_stage(stage).unwrap().0
            );
        }
        facts.observe("airplay-session-active", &serde_json::json!({}), 3);
        facts.observe(
            "stage",
            &serde_json::json!({"stage":carplay_policy::START_SESSION_SENT}),
            4,
        );
        assert_eq!(facts.iphone.step, "SESSION");
        facts.observe(
            "usb-presence",
            &serde_json::json!({"present":false,"product":"stale"}),
            5,
        );
        assert!(!facts.usb.present);
        assert_eq!(facts.usb.product, None);
    }
}
