// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Plain runtime observations for the admin presenter. No I/O or JSON assembly at read time.
use crate::{carplay_policy, display::Size};
use std::collections::VecDeque;

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
pub struct RuntimePathFacts {
    pub iphone: IphoneFacts,
    pub carlink: CarlinkFacts,
    pub web: WebFacts,
    pub pages: usize,
    pub screen: Option<Size>,
    pub saved_screen: Option<Size>,
    pub built_in_screen: Option<Size>,
    pub session: Option<u64>,
    pub picture_on_car: bool,
    pub chips: Vec<String>,
    pub timeline: VecDeque<String>,
    pub carlink_summary: Option<String>,
    phone_trail: Vec<String>,
    carlink_started: Option<u64>,
    carlink_summarized: bool,
}
fn bounded(lines: &mut Vec<String>, line: String) {
    if lines.last() == Some(&line) {
        return;
    }
    if lines.len() == 16 {
        lines.remove(0);
    }
    lines.push(line);
}
impl RuntimePathFacts {
    fn step(&mut self, step: &str, now: u64) {
        if self.iphone.step != step {
            self.iphone.step = step.into();
            self.iphone.since_millis = Some(now);
        }
    }
    pub fn failure(&mut self, message: &str, retrying: bool, now: u64) {
        let code = carplay_policy::classify_failure(message);
        let step = match code {
            "hotspot_not_ready" | "hotspot_configuration" | "wifi_off" => "STARTING",
            "bluetooth_off" | "bluetooth_unusable" | "iphone_unpaired" | "rfcomm_timeout" => {
                "BLUETOOTH"
            }
            "first_tcp_timeout" => "JOINING",
            _ => &self.iphone.step,
        }
        .to_owned();
        let text = carplay_policy::failure_text(code);
        bounded(&mut self.phone_trail, format!("{now} failed {code}"));
        self.iphone.failure = Some(LinkFailure {
            step: step.clone(),
            code: code.into(),
            text: text.into(),
            at_millis: now,
            retrying,
            trail: self.phone_trail.clone(),
        });
        self.step(&step, now);
    }
    /// Returns the startup reason to publish, when this event changes the startup state.
    pub fn observe(
        &mut self,
        kind: &str,
        fields: &serde_json::Value,
        now: u64,
    ) -> Option<&'static str> {
        let mut reason = None;
        if kind == "stage" || kind == "iap2-stage" {
            let stage = fields["stage"].as_str().unwrap_or("");
            if stage == "DiscoveringMfi" {
                self.phone_trail.clear();
                self.picture_on_car = false;
                self.iphone.session_since_millis = None;
            }
            if let Some((step, r)) = carplay_policy::startup_stage(stage) {
                // A late handoff callback must not move an active stream backwards.
                if !(matches!(step, "JOINING" | "WIFI_SENT" | "AUTH")
                    && matches!(self.iphone.step.as_str(), "SESSION" | "VIDEO"))
                {
                    self.step(step, now);
                    bounded(&mut self.phone_trail, format!("{now} {stage}"));
                    reason = Some(r);
                }
            }
            if stage == "Stopped" {
                if let Some(failure) = &mut self.iphone.failure {
                    failure.retrying = false;
                }
                self.iphone.session_since_millis = None;
                self.picture_on_car = false;
            }
            if let Some(message) = fields["reason"]
                .as_str()
                .filter(|_| fields["retryMillis"].is_number())
            {
                self.failure(message, true, now);
            }
        } else if kind == "retry" {
            self.failure(fields["reason"].as_str().unwrap_or(""), true, now);
            reason = Some("failed");
        } else if kind == "airplay-session-active" {
            self.step("SESSION", now);
            self.iphone.failure = None;
            self.iphone.session_since_millis.get_or_insert(now);
            reason = Some("waiting-media");
        } else if kind == "airplay-milestone" {
            let name = fields["name"].as_str().unwrap_or("");
            bounded(&mut self.phone_trail, format!("{now} {name}"));
            self.timeline(format!("{now} phone {name}"));
            if name == "first_video_received" {
                self.step("VIDEO", now);
                self.iphone.failure = None;
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
        let kind = event["type"].as_str().unwrap_or("");
        if kind == "receiver-starting" {
            self.carlink_summarized = false;
            self.carlink.car_name = None;
            self.carlink.model_id = None;
            self.carlink.join_method = None;
            self.carlink.live_frames = 0;
            self.carlink.screen = None;
            self.carlink.picture = None;
            self.carlink.phase_timings.clear();
            self.carlink.trail.clear();
            self.carlink_started = Some(now);
        }
        let phase = match kind {
            "link" => event["state"].as_str().map(|s| match s {
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
                bounded(&mut self.carlink.trail, line.clone());
                self.timeline(line);
            }
            if phase == "SCANNING" || phase == "PLAYING" {
                self.carlink.code = None;
                self.carlink.error = None;
                self.carlink.retrying = false;
            }
        }
        if kind == "pin-request" {
            self.carlink.pin_pending = true;
        }
        if matches!(kind, "receiver-stopped" | "pin-timeout") || self.carlink.phase == "PLAYING" {
            self.carlink.pin_pending = false;
        }
        if kind == "clink-pin" && event["result"] == "incorrect" {
            self.carlink.code = Some("pin_incorrect".into());
        }
        if matches!(kind, "error" | "pin-timeout" | "carlink-backoff")
            || (kind == "link" && event["state"] == "lost")
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
            self.carlink.error = Some("Carlink connection ended".into());
            self.carlink.retrying = kind == "carlink-backoff";
            bounded(
                &mut self.carlink.trail,
                format!("{now} {}", self.carlink.code.as_deref().unwrap()),
            );
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
        if kind == "clink-view-size" || kind == "media-stats" {
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
