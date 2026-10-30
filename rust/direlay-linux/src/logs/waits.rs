// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

//! Named waits: one line on entry, reminders after 30 s with doubling gaps of at most 10 min, a
//! line as soon as the detail changes and one at the end. Accounting only, on the monotonic clock:
//! callers keep every retry and deadline.
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, VecDeque};

pub type Record = (&'static str, Value);

/// Android's `nextStallReport`: the first reminder at 30 s, then doubling gaps of at most 10 min.
pub fn next_report(waited: u64) -> u64 {
    waited
        .saturating_mul(2)
        .min(waited.saturating_add(600_000))
        .max(30_000)
}

const RECENT: usize = 32;

struct Wait {
    since: u64,
    next: u64,
    detail: Map<String, Value>,
}

#[derive(Default)]
pub struct WaitLog {
    open: BTreeMap<String, Wait>,
    recent: VecDeque<Value>,
    refused: Option<Value>,
    last_refused: Option<(u64, Value)>,
}
impl WaitLog {
    /// Opens `name`. For an open wait a changed detail is logged at once; an unchanged one is not.
    pub fn begin(&mut self, now: u64, name: &str, detail: Value) -> Vec<Record> {
        let detail = object(detail);
        if let Some(wait) = self.open.get_mut(name) {
            if wait.detail == detail {
                return vec![];
            }
            wait.detail = detail;
            let waited = now.saturating_sub(wait.since);
            if waited >= wait.next {
                wait.next = next_report(waited);
            }
            return vec![("carplay_waiting", line(name, wait, Some(waited)))];
        }
        let wait = Wait {
            since: now,
            next: 30_000,
            detail,
        };
        let record = ("carplay_wait", line(name, &wait, None));
        self.open.insert(name.into(), wait);
        vec![record]
    }
    pub fn open(&self, name: &str) -> bool {
        self.open.contains_key(name)
    }
    /// The reminders that are due; the owner calls this about once a second.
    pub fn tick(&mut self, now: u64) -> Vec<Record> {
        self.open
            .iter_mut()
            .filter_map(|(name, wait)| {
                let waited = now.saturating_sub(wait.since);
                if waited < wait.next {
                    return None;
                }
                wait.next = next_report(waited);
                Some(("carplay_waiting", line(name, wait, Some(waited))))
            })
            .collect()
    }
    pub fn end(&mut self, now: u64, name: &str, outcome: &str) -> Vec<Record> {
        let Some(wait) = self.open.remove(name) else {
            return vec![];
        };
        let mut line = line(name, &wait, Some(now.saturating_sub(wait.since)));
        line["outcome"] = outcome.into();
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back(line.clone());
        vec![("carplay_wait_end", line)]
    }
    pub fn end_each(&mut self, now: u64, names: &[&str], outcome: &str) -> Vec<Record> {
        names
            .iter()
            .flat_map(|name| self.end(now, name, outcome))
            .collect()
    }
    pub fn end_all(&mut self, now: u64, outcome: &str) -> Vec<Record> {
        let names: Vec<_> = self.open.keys().cloned().collect();
        names
            .iter()
            .flat_map(|name| self.end(now, name, outcome))
            .collect()
    }
    /// A start that does not go ahead: one line per distinct refusal until the next admitted start,
    /// as Android logs `carplay_start_blocked` as a transition.
    pub fn refused(&mut self, now: u64, reason: &str, detail: Value) -> Vec<Record> {
        let mut fields = Map::from_iter([("reason".to_string(), Value::from(reason))]);
        fields.extend(object(detail));
        let fields = Value::Object(fields);
        if self.refused.as_ref() == Some(&fields) {
            return vec![];
        }
        self.refused = Some(fields.clone());
        self.last_refused = Some((now, fields.clone()));
        vec![("carplay_start_blocked", fields)]
    }
    /// A start went ahead, so the same refusal later is news again.
    pub fn admitted(&mut self) {
        self.refused = None;
    }
    /// Open waits with their age in seconds, as Android's heartbeat lists them: `network:12s,iphone:300s`.
    pub fn compact(&self, now: u64) -> String {
        self.open
            .iter()
            .map(|(name, wait)| format!("{name}:{}s", now.saturating_sub(wait.since) / 1000))
            .collect::<Vec<_>>()
            .join(",")
    }
    /// For the export: open waits, the last ended ones and the last refused start.
    pub fn summary(&self, now: u64) -> Value {
        json!({
            "open":self.open.iter().map(|(name, wait)| line(name, wait, Some(now.saturating_sub(wait.since)))).collect::<Vec<_>>(),
            "recent":self.recent,
            "lastBlocked":self.last_refused.as_ref().map(|(at, fields)| {
                let mut fields = fields.clone();
                fields["agoMillis"] = now.saturating_sub(*at).into();
                fields
            }),
        })
    }
}
/// `for`, the detail fields and, after entry, `waitedMillis`; a detail never overrides those.
fn line(name: &str, wait: &Wait, waited: Option<u64>) -> Value {
    let mut line = wait.detail.clone();
    line.insert("for".into(), name.into());
    match waited {
        Some(waited) => line.insert("waitedMillis".into(), waited.into()),
        None => line.remove("waitedMillis"),
    };
    line.remove("outcome");
    Value::Object(line)
}
fn object(detail: Value) -> Map<String, Value> {
    match detail {
        Value::Object(fields) => fields,
        Value::Null => Map::new(),
        other => Map::from_iter([("detail".to_string(), other)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn names(records: &[Record]) -> Vec<&'static str> {
        records.iter().map(|r| r.0).collect()
    }
    #[test]
    fn reminders_start_at_30_seconds_double_and_cap_at_ten_minutes() {
        assert_eq!(
            [0, 29_999, 30_000, 480_000, 960_000].map(next_report),
            [30_000, 59_998, 60_000, 960_000, 1_560_000]
        );
        let mut waits = WaitLog::default();
        let begun = waits.begin(0, "network", json!({"reason":"radio_unavailable"}));
        assert_eq!(names(&begun), ["carplay_wait"]);
        assert_eq!(
            begun[0].1,
            json!({"for":"network","reason":"radio_unavailable"})
        );
        let mut at = Vec::new();
        for second in 0..=3_000 {
            for (event, line) in waits.tick(second * 1000) {
                assert_eq!(event, "carplay_waiting");
                at.push(line["waitedMillis"].as_u64().unwrap() / 1000);
            }
        }
        assert_eq!(at, [30, 60, 120, 240, 480, 960, 1560, 2160, 2760]);
    }
    #[test]
    fn repeated_begin_is_quiet_and_changed_detail_logs_at_once() {
        let mut waits = WaitLog::default();
        waits.begin(0, "network", json!({"reason":"radio_unavailable"}));
        assert!(waits
            .begin(5_000, "network", json!({"reason":"radio_unavailable"}))
            .is_empty());
        let changed = waits.begin(10_000, "network", json!({"reason":"shared_network"}));
        assert_eq!(names(&changed), ["carplay_waiting"]);
        assert_eq!(changed[0].1["waitedMillis"], 10_000);
        assert_eq!(changed[0].1["reason"], "shared_network");
        // The change does not move the reminder that is not yet due.
        assert_eq!(names(&waits.tick(30_000)), ["carplay_waiting"]);
        // A change when a reminder is due counts as that reminder.
        waits.begin(60_000, "network", json!({"reason":"follow_over"}));
        assert!(waits.tick(60_000).is_empty());
        assert_eq!(names(&waits.tick(120_000)), ["carplay_waiting"]);
    }
    #[test]
    fn end_is_logged_once_with_outcome_and_kept_in_bounded_history() {
        let mut waits = WaitLog::default();
        assert!(waits.end(0, "carlink", "ready").is_empty());
        waits.begin(
            1_000,
            "carlink",
            json!({"maxMillis":60_000,"outcome":"ignored"}),
        );
        assert!(waits.open("carlink"));
        let end = waits.end(61_000, "carlink", "timeout");
        assert_eq!(
            end[0],
            (
                "carplay_wait_end",
                json!({"for":"carlink","maxMillis":60_000,"waitedMillis":60_000,"outcome":"timeout"})
            )
        );
        assert!(!waits.open("carlink"));
        assert!(waits.end(61_000, "carlink", "timeout").is_empty());
        assert!(waits.tick(600_000).is_empty());
        for i in 0..40 {
            waits.begin(i, "iphone", json!({"generation":i}));
            let outcome = ["ready", "start_now", "stopped", "failed", "shutdown"][i as usize % 5];
            assert_eq!(waits.end_all(i + 1, outcome).len(), 1);
        }
        let summary = waits.summary(50);
        assert_eq!(summary["open"], json!([]));
        assert_eq!(summary["recent"].as_array().unwrap().len(), RECENT);
        assert_eq!(summary["recent"][RECENT - 1]["outcome"], "shutdown");
    }
    #[test]
    fn end_each_closes_only_the_named_waits_and_compact_lists_ages() {
        let mut waits = WaitLog::default();
        waits.begin(0, "carlink", json!({}));
        waits.begin(0, "car_screen", json!({}));
        waits.begin(0, "iphone", json!({}));
        assert_eq!(
            waits.compact(12_500),
            "car_screen:12s,carlink:12s,iphone:12s"
        );
        let ended = waits.end_each(20_000, &["carlink", "car_screen", "network"], "stopped");
        assert_eq!(names(&ended), ["carplay_wait_end", "carplay_wait_end"]);
        assert_eq!(waits.compact(20_000), "iphone:20s");
    }
    #[test]
    fn refused_starts_log_once_until_changed_or_admitted() {
        let mut waits = WaitLog::default();
        let busy = json!({"action":"start"});
        let first = waits.refused(1_000, "busy", busy.clone());
        assert_eq!(
            first,
            [(
                "carplay_start_blocked",
                json!({"reason":"busy","action":"start"})
            )]
        );
        assert!(waits.refused(2_000, "busy", busy.clone()).is_empty());
        assert_eq!(waits.refused(3_000, "identity", json!({})).len(), 1);
        assert_eq!(waits.refused(4_000, "busy", busy.clone()).len(), 1);
        waits.admitted();
        assert_eq!(waits.refused(5_000, "busy", busy).len(), 1);
        assert_eq!(waits.summary(6_500)["lastBlocked"]["agoMillis"], 1_500);
    }
}
