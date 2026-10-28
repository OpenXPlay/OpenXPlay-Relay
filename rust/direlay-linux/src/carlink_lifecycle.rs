// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Generation fencing and retries for the stdin/stdout Carlink control contract.
use serde_json::Value;
use std::time::{Duration, Instant};

pub(super) struct Lifecycle {
    generation: u64,
    request: u64,
    pending: bool,
    retry: Option<Instant>,
    pending_until: Option<Instant>,
    failures: u32,
    auto: bool,
    cancelled: bool,
    pub played: bool,
}
impl Lifecycle {
    pub fn new(auto: bool) -> Self {
        Self {
            generation: 1,
            request: 0,
            pending: false,
            retry: None,
            pending_until: None,
            failures: 0,
            auto,
            cancelled: false,
            played: false,
        }
    }
    pub fn begin_child(&mut self, picture_override: bool) {
        self.generation = 1;
        self.pending = picture_override;
        self.pending_until = None;
        self.retry = None;
        self.played = false;
    }
    pub fn replacement(&mut self) -> String {
        self.generation += 1;
        self.request += 1;
        self.pending = true;
        self.pending_until = Some(Instant::now() + Duration::from_secs(5));
        self.retry = None;
        self.cancelled = false;
        format!("relay-{}", self.request)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn accept(&self, event: &Value) -> bool {
        if event["type"] == "clink-restart" {
            return event["request"] == format!("relay-{}", self.request);
        }
        if let Some(generation) = event["generation"].as_u64() {
            return !(self.pending && self.pending_until.is_none())
                && generation == self.generation;
        }
        // Legacy listener records have no generation. Drop them until the matching restart
        // acknowledgement drains older stdout; the child fences its own retired callbacks.
        !self.pending
            || !matches!(
                event["type"].as_str(),
                Some(
                    "state"
                        | "error"
                        | "diagnostic"
                        | "clink-view-size"
                        | "media-stats"
                        | "clink-pin"
                        | "pin-timeout"
                )
            )
    }
    pub fn observe(&mut self, event: &Value, now: Instant) {
        if event["type"] == "clink-restart" {
            self.pending = false;
            if let Some(generation) = event["generation"].as_u64() {
                self.generation = generation;
            }
            if event.get("error").is_some() {
                self.control_failed(now);
            }
        }
        if event["type"] == "clink-pairing" && event["result"] == "cancelled" {
            self.cancel();
        }
        if event["type"] == "state"
            && event["state"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("playing"))
        {
            self.failures = 0;
            self.retry = None;
            self.played = true;
        }
        // A failed discovery attempt is diagnostic. Only terminal loss starts our retry.
        if super::carlink_attempt_failed(event)
            && self.retry.is_none()
            && !self.pending
            && self.auto
            && !self.cancelled
        {
            self.control_failed(now);
        }
    }
    pub fn control_failed(&mut self, now: Instant) {
        self.pending = false;
        if self.auto && !self.cancelled {
            let seconds = (1u64 << self.failures.min(4)).min(15);
            self.failures = self.failures.saturating_add(1);
            self.retry = Some(now + Duration::from_secs(seconds));
        }
    }
    pub fn retry_now(&mut self) {
        self.retry = Some(Instant::now());
    }
    pub fn defer(&mut self, until: Instant) {
        if !self.cancelled {
            self.retry = Some(until);
        }
    }
    pub fn due(&mut self, now: Instant) -> bool {
        if self.pending && self.pending_until.is_some_and(|t| now >= t) {
            self.control_failed(now);
        }
        self.retry.is_some_and(|t| now >= t)
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.retry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn retries_cap_reset_on_current_playing_and_diagnostic_failures_do_not_retry() {
        let mut life = Lifecycle::new(true);
        let mut now = Instant::now();
        life.observe(
            &json!({"type":"link","state":"failed","code":"wifi_join"}),
            now,
        );
        assert!(life.retry.is_none());
        for seconds in [1, 2, 4, 8, 15, 15] {
            life.observe(&json!({"type":"state","state":"Ended"}), now);
            assert_eq!(life.retry, Some(now + Duration::from_secs(seconds)));
            let request = life.replacement();
            life.observe(&json!({"type":"clink-restart","request":request}), now);
            now += Duration::from_secs(seconds);
        }
        assert!(!life.accept(&json!({"type":"state","state":"Playing","generation":1})));
        life.observe(
            &json!({"type":"state","state":"Playing","generation":life.generation}),
            now,
        );
        life.observe(&json!({"type":"state","state":"Ended"}), now);
        assert_eq!(life.retry, Some(now + Duration::from_secs(1)));
    }
    #[test]
    fn pending_replacement_and_cancel_fence_old_events() {
        let mut life = Lifecycle::new(true);
        life.begin_child(true);
        assert!(!life.accept(&json!({"type":"state","state":"Playing","generation":1})));
        let request = life.replacement();
        assert!(!life.accept(&json!({"type":"media-stats"})));
        assert!(!life.accept(&json!({"type":"state","generation":1})));
        assert!(life.accept(&json!({"type":"state","generation":2})));
        assert!(!life.accept(&json!({"type":"clink-restart","request":"stale"})));
        life.observe(
            &json!({"type":"clink-restart","request":request}),
            Instant::now(),
        );
        life.cancel();
        life.observe(&json!({"type":"state","state":"Ended"}), Instant::now());
        assert!(life.retry.is_none());
    }
}
