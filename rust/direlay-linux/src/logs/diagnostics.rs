// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

//! What the shell waits for, which starts it refused, and the facts the stall watchdog reports.
//! Shell threads write here; the watchdog's sampler only uses `try_lock`, so it never waits for them.
use super::waits::{Record, WaitLog};
use super::watchdog::clocks;
use crate::platform::JsonEvents;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// How long the main loop may take for one pass before it counts as blocked.
pub const LOOP_BUDGET: Duration = Duration::from_secs(10);
/// How long one admin operation may run: activating a network alone may take 45 s.
pub const OPERATION_BUDGET: Duration = Duration::from_secs(120);
/// Waits of a start that has not reached the controller's session yet.
pub const START_WAITS: &[&str] = &["network", "provisioning", "carlink", "car_screen"];
/// Waits of a running controller.
pub const SESSION_WAITS: &[&str] = &["bluetooth", "iphone"];

/// What one watched thread is doing: its innermost operation (empty while idle), when that began,
/// when the thread last made progress and how long it may go without progress.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub operation: &'static str,
    pub since: u64,
    pub updated: u64,
    pub budget: u64,
    pub tid: u32,
    pub serial: u64,
}
#[derive(Default)]
pub struct Probe(Mutex<Progress>);
thread_local! {
    static OWNED: RefCell<Option<Arc<Probe>>> = const { RefCell::new(None) };
}
impl Probe {
    /// Makes this the calling thread's probe and enters `operation`. Dropping the guard returns to
    /// the outer operation, or to idle, and counts as progress.
    pub fn enter(self: &Arc<Self>, operation: &'static str, budget: Duration) -> ProgressGuard {
        OWNED.with(|owned| *owned.borrow_mut() = Some(self.clone()));
        self.push(operation, budget)
    }
    /// The thread is alive in its current operation.
    pub fn beat(&self) {
        self.0.lock().unwrap().updated = clocks().mono;
    }
    /// The current progress, unless the owner holds the lock right now.
    pub fn sample(&self) -> Option<Progress> {
        self.0.try_lock().ok().map(|p| p.clone())
    }
    fn push(self: &Arc<Self>, operation: &'static str, budget: Duration) -> ProgressGuard {
        let now = clocks().mono;
        let mut progress = self.0.lock().unwrap();
        let outer = progress.clone();
        *progress = Progress {
            operation,
            since: now,
            updated: now,
            budget: budget.as_millis() as u64,
            tid: tid(),
            serial: outer.serial.wrapping_add(1),
        };
        ProgressGuard {
            probe: self.clone(),
            outer,
        }
    }
}
/// Nests `operation` in the calling thread's probe; threads without one are not watched.
pub fn nested(operation: &'static str, budget: Duration) -> Option<ProgressGuard> {
    OWNED
        .with(|owned| owned.borrow().clone())
        .map(|probe| probe.push(operation, budget))
}
pub struct ProgressGuard {
    probe: Arc<Probe>,
    outer: Progress,
}
impl Drop for ProgressGuard {
    fn drop(&mut self) {
        let mut progress = self.probe.0.lock().unwrap();
        let serial = progress.serial.wrapping_add(1);
        *progress = Progress {
            updated: clocks().mono,
            serial,
            ..self.outer.clone()
        };
    }
}
/// The kernel task ID on Linux, for `/proc/self/task`; elsewhere a per-thread number.
pub fn tid() -> u32 {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::syscall(libc::SYS_gettid) as u32 }
    }
    #[cfg(not(target_os = "linux"))]
    {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        thread_local!(static ID: u32 = NEXT.fetch_add(1, Ordering::Relaxed));
        ID.with(|id| *id)
    }
}

/// Small, current facts for the heartbeat; radio facts belong to the radio-conflict work.
#[derive(Clone, Debug, Default)]
struct Facts {
    session: Option<String>,
    generation: Option<u64>,
    start: Option<String>,
    carlink: Option<String>,
    airplay: Option<String>,
}

pub struct Diagnostics {
    enabled: AtomicBool,
    watching: AtomicBool,
    dropped: AtomicU64,
    pub main: Arc<Probe>,
    pub admin: Arc<Probe>,
    pub(super) waits: Mutex<WaitLog>,
    facts: Mutex<Facts>,
    process: Mutex<Value>,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            watching: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            main: Arc::default(),
            admin: Arc::default(),
            waits: Mutex::default(),
            facts: Mutex::default(),
            process: Mutex::new(json!({"status":"pending"})),
        }
    }
}
impl Diagnostics {
    fn send(events: &JsonEvents, records: Vec<Record>) {
        for (event, fields) in records {
            events.emit(event, &fields);
        }
    }
    /// The effective `stallWatchdog` choice; the sampler applies a change within a second.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
    pub(super) fn set_watching(&self, watching: bool) {
        self.watching.store(watching, Ordering::Release);
    }
    pub(super) fn count_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub(super) fn set_process(&self, process: Value) {
        *self.process.lock().unwrap() = process;
    }
    /// Due reminders, unless a shell thread holds the waits right now.
    pub(super) fn reminders(&self, now: u64) -> Vec<Record> {
        self.waits
            .try_lock()
            .map(|mut waits| waits.tick(now))
            .unwrap_or_default()
    }
    /// The heartbeat's facts, unless a shell thread holds them right now.
    pub(super) fn heartbeat_facts(&self, now: u64) -> Option<Value> {
        let facts = self.facts.try_lock().ok()?.clone();
        let waits = self.waits.try_lock().ok()?.compact(now);
        Some(
            json!({"session":facts.session,"generation":facts.generation,"start":facts.start,
            "carlink":facts.carlink,"airplay":facts.airplay,"waits":waits}),
        )
    }

    pub fn begin(&self, events: &JsonEvents, name: &str, detail: Value) {
        let records = self
            .waits
            .lock()
            .unwrap()
            .begin(clocks().mono, name, detail);
        Self::send(events, records);
    }
    pub fn end(&self, events: &JsonEvents, name: &str, outcome: &str) {
        let records = self.waits.lock().unwrap().end(clocks().mono, name, outcome);
        Self::send(events, records);
    }
    pub fn end_each(&self, events: &JsonEvents, names: &[&str], outcome: &str) {
        let records = self
            .waits
            .lock()
            .unwrap()
            .end_each(clocks().mono, names, outcome);
        Self::send(events, records);
    }
    pub fn end_all(&self, events: &JsonEvents, outcome: &str) {
        let records = self.waits.lock().unwrap().end_all(clocks().mono, outcome);
        Self::send(events, records);
    }
    /// A requested start waits for its network: `reason` is one of the Linux start reasons.
    pub fn hold(&self, events: &JsonEvents, reason: &'static str, detail: Value) {
        let mut fields = json!({"reason":reason});
        if let Value::Object(detail) = detail {
            fields.as_object_mut().unwrap().extend(detail);
        }
        self.facts.lock().unwrap().start = Some(format!("waiting:{reason}"));
        self.begin(events, "network", fields);
    }
    /// A start that does not go ahead; the same refusal is logged once until a start goes ahead.
    pub fn refuse(&self, events: &JsonEvents, reason: &'static str, detail: Value) {
        let records = self
            .waits
            .lock()
            .unwrap()
            .refused(clocks().mono, reason, detail);
        if !records.is_empty() {
            self.facts.lock().unwrap().start = Some(format!("blocked:{reason}"));
        }
        Self::send(events, records);
    }
    /// The controller starts: the network wait is over and later refusals are news again.
    pub fn admitted(&self, events: &JsonEvents) {
        self.waits.lock().unwrap().admitted();
        self.facts.lock().unwrap().start = Some("admitted".into());
        self.end(events, "network", "ready");
    }
    /// A new Start, Restart or Stop request: the same refusal as before is news again.
    pub fn new_request(&self) {
        self.waits.lock().unwrap().admitted();
    }
    /// The start is over before its session: Stop, a new request, a failure or shutdown.
    pub fn start_ended(&self, events: &JsonEvents, outcome: &str) {
        self.facts.lock().unwrap().start = None;
        self.end_each(events, START_WAITS, outcome);
    }
    /// One look at the start gate: the Carlink and car-screen waits end as their budgets resolve.
    pub fn start_gate(
        &self,
        events: &JsonEvents,
        result: &std::io::Result<bool>,
        carlink_ready: bool,
        screen_reported: bool,
    ) {
        match result {
            Ok(true) => {
                self.end(events, "carlink", "ready");
                let screen = if screen_reported {
                    "reported"
                } else {
                    "timeout"
                };
                self.end(events, "car_screen", screen);
            }
            Ok(false) if carlink_ready => self.end(events, "carlink", "ready"),
            Ok(false) => {}
            Err(error) => {
                let carlink = if error.to_string().contains("did not become ready") {
                    "timeout"
                } else {
                    "failed"
                };
                self.end(events, "carlink", carlink);
                self.end(events, "car_screen", "failed");
            }
        }
    }
    /// For the export's `info.txt`, one fact per line.
    pub fn export_details(&self) -> String {
        let now = clocks().mono;
        let process = self.process.lock().unwrap().clone();
        let facts = self.facts.lock().unwrap().clone();
        let waits = self.waits.lock().unwrap().summary(now);
        format!(
            "Process: {}\nPrevious process: {}\nProcess exit: {}\nStall watchdog: {}\nSession: {}\nWaits: {}\n",
            process["current"],
            process["previous"],
            process["previousExit"],
            json!({"enabled":self.enabled(),"sampling":self.watching.load(Ordering::Acquire),"droppedDiagnostics":self.dropped()}),
            json!({"session":facts.session,"generation":facts.generation,"start":facts.start,"carlink":facts.carlink,"airplay":facts.airplay}),
            waits,
        )
    }
}

/// Follows the events that tell what the controller waits for. Called by [`JsonEvents::emit`]
/// before it takes its own locks; the wait lines this logs come back here and are ignored.
pub fn observe(events: &JsonEvents, event: &str, fields: &Value) {
    let d = &events.diagnostics;
    match event {
        "stage" => {
            let stage = fields["stage"].as_str().unwrap_or("");
            let generation = fields["generation"].as_u64();
            {
                let mut facts = d.facts.lock().unwrap();
                facts.session = Some(stage.into());
                if generation.is_some() {
                    facts.generation = generation;
                }
            }
            match stage {
                "WaitingForBluetooth" => {
                    d.begin(events, "bluetooth", json!({"generation":generation}))
                }
                // The stage of a failed attempt carries the retry delay; selecting does not.
                "WaitingForPairedIphone" if fields.get("retryMillis").is_some() => {
                    let unreachable = fields["unreachable"] == true;
                    d.begin(
                        events,
                        "iphone",
                        json!({"generation":generation,"unreachable":unreachable}),
                    );
                    if selection_problem(fields["reason"].as_str().unwrap_or("")) {
                        d.refuse(events, "iphone_selection", json!({"generation":generation}));
                    }
                }
                "Failed" => d.end_each(events, SESSION_WAITS, "failed"),
                "Stopped" => d.end_each(events, SESSION_WAITS, "stopped"),
                _ => {}
            }
        }
        "startup-failed" => {
            d.end_each(events, SESSION_WAITS, "failed");
            d.end_each(events, &["carlink", "car_screen"], "failed");
        }
        "airplay-session-active" | "airplay-session-ended" => {
            d.facts.lock().unwrap().airplay = Some(event["airplay-session-".len()..].into());
        }
        "state" | "link" => {
            if let Some(state) = fields["state"].as_str() {
                d.facts.lock().unwrap().carlink = Some(format!("{event}:{state}"));
            }
        }
        _ => {}
    }
}
/// The bonded-iPhone choice itself is the problem, not the iPhone's absence.
fn selection_problem(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    reason.contains("no unambiguous bonded iphone")
        || reason.contains("more than one bonded iphone")
        || crate::carplay_policy::classify_failure(&reason) == "iphone_unpaired"
}

/// The Linux reason a start that failed before its controller ran did not go ahead.
pub fn start_failure_reason(error: &std::io::Error) -> &'static str {
    if error.kind() == std::io::ErrorKind::InvalidInput {
        return "invalid_settings";
    }
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    match crate::carplay_policy::classify_failure(&message) {
        "bluetooth_off" | "bluetooth_unusable" | "wifi_off" | "wifi_reset_required" => {
            "radio_unavailable"
        }
        "hotspot_configuration" => "invalid_settings",
        "hotspot_not_ready" => "provisioning",
        "iphone_unpaired" => "iphone_selection",
        _ if lower.contains("mfi") || lower.contains("identity") => "identity",
        _ if selection_problem(&message) => "iphone_selection",
        _ => "error",
    }
}

/// A wait around one blocking step, ended when the guard drops: `failed` unless finished.
pub struct WaitGuard<'a> {
    events: &'a JsonEvents,
    name: &'static str,
    outcome: &'static str,
}
impl<'a> WaitGuard<'a> {
    pub fn new(events: &'a JsonEvents, name: &'static str) -> Self {
        events.diagnostics.begin(events, name, json!({}));
        Self {
            events,
            name,
            outcome: "failed",
        }
    }
    pub fn finish(&mut self, outcome: &'static str) {
        self.outcome = outcome;
    }
}
impl Drop for WaitGuard<'_> {
    fn drop(&mut self) {
        self.events
            .diagnostics
            .end(self.events, self.name, self.outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recorder() -> (Arc<JsonEvents>, Arc<Mutex<Vec<Value>>>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let copy = lines.clone();
        let events = Arc::new(JsonEvents::new(Arc::new(move |line: String| {
            copy.lock()
                .unwrap()
                .push(serde_json::from_str(&line).unwrap())
        })));
        (events, lines)
    }
    fn wait_lines(lines: &Mutex<Vec<Value>>) -> Vec<String> {
        lines
            .lock()
            .unwrap()
            .iter()
            .filter(|l| {
                l["event"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("carplay_"))
            })
            .map(|l| {
                format!(
                    "{} {} {}",
                    l["event"].as_str().unwrap(),
                    l["for"].as_str().or(l["reason"].as_str()).unwrap_or(""),
                    l["outcome"].as_str().unwrap_or("")
                )
                .trim_end()
                .to_string()
            })
            .collect()
    }
    #[test]
    fn probes_nest_per_thread_and_count_returns_as_progress() {
        let probe = Arc::new(Probe::default());
        assert!(nested("orphan", OPERATION_BUDGET).is_none());
        let outer = probe.enter("tick", LOOP_BUDGET);
        let first = probe.sample().unwrap();
        assert_eq!((first.operation, first.budget), ("tick", 10_000));
        {
            let _inner = nested("startup", OPERATION_BUDGET).unwrap();
            let inner = probe.sample().unwrap();
            assert_eq!((inner.operation, inner.budget), ("startup", 120_000));
            assert_ne!(inner.serial, first.serial);
            // Another thread has no probe of its own, so it cannot disturb this one.
            std::thread::spawn(|| assert!(nested("other", OPERATION_BUDGET).is_none()))
                .join()
                .unwrap();
        }
        let back = probe.sample().unwrap();
        assert_eq!((back.operation, back.since), ("tick", first.since));
        assert!(back.serial > first.serial + 1);
        drop(outer);
        assert_eq!(probe.sample().unwrap().operation, "");
        OWNED.with(|owned| owned.borrow_mut().take());
    }
    #[test]
    fn controller_stages_open_and_end_session_waits_and_selection_is_refused_once() {
        let (events, lines) = recorder();
        let stage = |fields: Value| events.emit("stage", &fields);
        stage(json!({"stage":"WaitingForBluetooth","generation":1,"retryMillis":1000}));
        stage(json!({"stage":"WaitingForBluetooth","generation":1,"retryMillis":2000}));
        events.diagnostics.end(&events, "bluetooth", "ready");
        stage(
            json!({"stage":"WaitingForPairedIphone","reason":"Selecting bonded iPhone","generation":1}),
        );
        assert!(!events.diagnostics.waits.lock().unwrap().open("iphone"));
        for retry in [10_000, 20_000, 30_000, 30_000] {
            stage(
                json!({"stage":"WaitingForPairedIphone","generation":1,"unreachable":true,
                "reason":"Connection refused (os error 111)","retryMillis":retry}),
            );
        }
        let ambiguous = json!({"stage":"WaitingForPairedIphone","generation":1,"unreachable":false,
            "reason":"More than one bonded iPhone candidate; configure --iphone ADDR","retryMillis":10_000});
        stage(ambiguous.clone());
        stage(ambiguous);
        stage(json!({"stage":"Stopped"}));
        assert_eq!(
            wait_lines(&lines),
            [
                "carplay_wait bluetooth",
                "carplay_wait_end bluetooth ready",
                "carplay_wait iphone",
                "carplay_waiting iphone",
                "carplay_start_blocked iphone_selection",
                "carplay_wait_end iphone stopped",
            ]
        );
        let facts = events.diagnostics.heartbeat_facts(clocks().mono).unwrap();
        assert_eq!(
            (facts["session"].as_str(), facts["generation"].as_u64()),
            (Some("Stopped"), Some(1))
        );
    }
    #[test]
    fn hold_admit_refuse_and_stop_follow_one_start() {
        let (events, lines) = recorder();
        let d = &events.diagnostics;
        d.hold(&events, "radio_unavailable", json!({"wifi":"absent"}));
        d.hold(&events, "radio_unavailable", json!({"wifi":"absent"}));
        d.hold(&events, "shared_network", json!({}));
        d.begin(&events, "carlink", json!({"maxMillis":60_000}));
        d.admitted(&events);
        d.refuse(&events, "busy", json!({"action":"restart"}));
        d.refuse(&events, "busy", json!({"action":"restart"}));
        d.start_ended(&events, "stopped");
        // A failure that repeats on every tick stays one line until a new request.
        d.refuse(&events, "busy", json!({"action":"restart"}));
        d.new_request();
        d.refuse(&events, "busy", json!({"action":"restart"}));
        assert_eq!(
            wait_lines(&lines),
            [
                "carplay_wait network",
                "carplay_waiting network",
                "carplay_wait carlink",
                "carplay_wait_end network ready",
                "carplay_start_blocked busy",
                "carplay_wait_end carlink stopped",
                "carplay_start_blocked busy",
            ]
        );
        let held = lines.lock().unwrap()[0].clone();
        assert_eq!(
            (held["reason"].as_str(), held["wifi"].as_str()),
            (Some("radio_unavailable"), Some("absent"))
        );
    }
    #[test]
    fn start_gate_results_end_carlink_and_screen_waits() {
        let (events, lines) = recorder();
        let d = &events.diagnostics;
        let open = || {
            d.begin(&events, "carlink", json!({"maxMillis":60_000}));
            d.begin(&events, "car_screen", json!({"maxMillis":20_000}));
        };
        open();
        d.start_gate(&events, &Ok(false), false, false);
        d.start_gate(&events, &Ok(false), true, false);
        d.start_gate(&events, &Ok(true), true, false);
        open();
        d.start_gate(&events, &Ok(true), true, true);
        open();
        let timeout =
            std::io::Error::other("Carlink output did not become ready before CarPlay startup");
        d.start_gate(&events, &Err(timeout), false, false);
        open();
        d.start_gate(
            &events,
            &Err(std::io::Error::other(
                "Carlink output was disabled while waiting to start CarPlay",
            )),
            false,
            false,
        );
        let ends: Vec<_> = wait_lines(&lines)
            .into_iter()
            .filter(|l| l.starts_with("carplay_wait_end"))
            .collect();
        assert_eq!(
            ends,
            [
                "carplay_wait_end carlink ready",
                "carplay_wait_end car_screen timeout",
                "carplay_wait_end carlink ready",
                "carplay_wait_end car_screen reported",
                "carplay_wait_end carlink timeout",
                "carplay_wait_end car_screen failed",
                "carplay_wait_end carlink failed",
                "carplay_wait_end car_screen failed",
            ]
        );
    }
    #[test]
    fn failures_map_to_linux_reasons() {
        let other = |m: &str| std::io::Error::other(m.to_string());
        assert_eq!(
            start_failure_reason(&std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid Wi-Fi channel"
            )),
            "invalid_settings"
        );
        for (message, reason) in [
            ("Bluetooth adapter is powered off", "radio_unavailable"),
            ("Turn on Wi-Fi", "radio_unavailable"),
            ("Hotspot configuration rejected", "invalid_settings"),
            ("The network is not ready", "provisioning"),
            (
                "The configured iPhone is no longer paired",
                "iphone_selection",
            ),
            (
                "No unambiguous bonded iPhone found; pair one iPhone",
                "iphone_selection",
            ),
            ("Invalid saved AirPlay identity", "identity"),
            ("MFi certificate missing", "identity"),
            ("Broken pipe", "error"),
        ] {
            assert_eq!(start_failure_reason(&other(message)), reason, "{message}");
        }
    }
    #[test]
    fn guard_ends_failed_unless_finished_and_export_lists_every_fact() {
        let (events, lines) = recorder();
        drop(WaitGuard::new(&events, "provisioning"));
        WaitGuard::new(&events, "restart").finish("done");
        assert_eq!(
            wait_lines(&lines),
            [
                "carplay_wait provisioning",
                "carplay_wait_end provisioning failed",
                "carplay_wait restart",
                "carplay_wait_end restart done",
            ]
        );
        events.emit("state", &json!({"state":"Streaming"}));
        events.emit("airplay-session-active", &json!({}));
        events.diagnostics.set_enabled(true);
        let details = events.diagnostics.export_details();
        for line in [
            "Process: ",
            "Previous process: ",
            "Process exit: ",
            "Stall watchdog: {\"droppedDiagnostics\":0,\"enabled\":true",
            "Session: {\"airplay\":\"active\",\"carlink\":\"state:Streaming\"",
            "Waits: {\"lastBlocked\":null,\"open\":[]",
        ] {
            assert!(details.contains(line), "{line} in {details}");
        }
        assert_eq!(details.lines().count(), 6);
    }
}
