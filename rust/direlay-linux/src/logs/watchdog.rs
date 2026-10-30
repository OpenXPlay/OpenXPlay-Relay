// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

//! The stall watchdog. Its own thread samples once a second, independent of the main loop, the
//! admin worker and the controller, and reads their state with `try_lock` only. Only its delivery
//! thread calls `JsonEvents::emit`, through a bounded queue: a blocked journal never blocks the
//! sampler, and lines that do not fit are counted. Diagnostics only: it never stops anything.
use super::{
    diagnostics::{Diagnostics, Progress},
    process::{self, Heartbeat, Identity},
    waits::{next_report, Record},
};
use crate::platform::JsonEvents;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clocks {
    pub wall: u64,
    pub mono: u64,
    /// CLOCK_BOOTTIME: like CLOCK_MONOTONIC, but it keeps counting while the system is suspended.
    pub boot: u64,
}
pub fn clocks() -> Clocks {
    #[cfg(target_os = "linux")]
    fn clock(id: libc::clockid_t) -> u64 {
        let mut t = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(id, &mut t) } != 0 {
            return 0;
        }
        (t.tv_sec as u64)
            .saturating_mul(1000)
            .saturating_add(t.tv_nsec as u64 / 1_000_000)
    }
    #[cfg(target_os = "linux")]
    let (mono, boot) = (clock(libc::CLOCK_MONOTONIC), clock(libc::CLOCK_BOOTTIME));
    #[cfg(not(target_os = "linux"))]
    let (mono, boot) = {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let n = START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis() as u64;
        (n, n)
    };
    Clocks {
        wall: super::now_millis(),
        mono,
        boot,
    }
}

/// A gap this long in the sampler's own one-second rhythm means the process did not run.
const PAUSE: u64 = 5_000;
/// After a pause the watched threads get this long to catch up before they count as blocked.
const GRACE: u64 = 5_000;
const HEARTBEAT_FILE: u64 = 10_000;
const HEARTBEAT_LINE: u64 = 30_000;

struct Blocked {
    operation: &'static str,
    serial: u64,
    updated: u64,
    next: u64,
}
/// Turns one-second samples into findings, like Android's `ProcessStallDetector`.
#[derive(Default)]
pub struct Detector {
    last: Option<Clocks>,
    grace_until: u64,
    blocked: BTreeMap<&'static str, Blocked>,
}
impl Detector {
    pub fn observe(
        &mut self,
        now: Clocks,
        probes: &[(&'static str, &Progress)],
        task: impl Fn(u32) -> Value,
    ) -> Vec<Record> {
        let mut records = Vec::new();
        if let Some(last) = self
            .last
            .filter(|last| now.mono >= last.mono && now.boot >= last.boot)
        {
            let gap = now.boot - last.boot;
            if gap > PAUSE {
                // Suspend stops CLOCK_MONOTONIC but not CLOCK_BOOTTIME; a stopped or starved
                // process misses both. Evidence, not proof of a freezer.
                let monotonic = now.mono - last.mono;
                let cause = if monotonic.saturating_mul(2) < gap {
                    "suspend"
                } else {
                    "stopped_or_starved"
                };
                records.push((
                    "process_paused",
                    json!({"gapMillis":gap,"monotonicGapMillis":monotonic,"cause":cause}),
                ));
                self.grace_until = now.mono.saturating_add(GRACE);
            }
        }
        self.last = Some(now);
        for (target, progress) in probes {
            let age = now.mono.saturating_sub(progress.updated);
            let stale = !progress.operation.is_empty() && age > progress.budget;
            if self
                .blocked
                .get(target)
                .is_some_and(|b| !stale || b.serial != progress.serial)
            {
                let b = self.blocked.remove(target).unwrap();
                records.push((
                    "runtime_unblocked",
                    json!({"target":target,"operation":b.operation,"afterMillis":now.mono.saturating_sub(b.updated)}),
                ));
            }
            if !stale || now.mono < self.grace_until {
                continue;
            }
            let b = self.blocked.entry(target).or_insert(Blocked {
                operation: progress.operation,
                serial: progress.serial,
                updated: progress.updated,
                next: 0,
            });
            if age < b.next {
                continue;
            }
            b.next = next_report(age);
            records.push((
                "runtime_blocked",
                json!({"target":target,"operation":progress.operation,"forMillis":age,
                    "budgetMillis":progress.budget,"thread":progress.tid,"task":task(progress.tid)}),
            ));
        }
        records
    }
}

struct Delivery {
    sender: Option<SyncSender<Record>>,
    diagnostics: Arc<Diagnostics>,
    reported: u64,
    done: Receiver<()>,
}
impl Delivery {
    fn new(events: Arc<JsonEvents>) -> std::io::Result<Self> {
        let (sender, queue) = mpsc::sync_channel::<Record>(64);
        let (finished, done) = mpsc::channel();
        let diagnostics = events.diagnostics.clone();
        std::thread::Builder::new()
            .name("diagnostic-log".into())
            .spawn(move || {
                while let Ok((event, fields)) = queue.recv() {
                    events.emit(event, &fields);
                }
                let _ = finished.send(());
            })?;
        Ok(Self {
            sender: Some(sender),
            diagnostics,
            reported: 0,
            done,
        })
    }
    fn send(&mut self, record: Record) {
        let Some(sender) = &self.sender else {
            return;
        };
        let dropped = self.diagnostics.dropped();
        if dropped != self.reported
            && sender
                .try_send((
                    "diagnostics_dropped",
                    json!({"total":dropped,"sinceLastReport":dropped - self.reported}),
                ))
                .is_ok()
        {
            self.reported = dropped;
        }
        if sender.try_send(record).is_err() {
            self.diagnostics.count_dropped();
        }
    }
    /// Lets queued lines out before the process exits, without waiting long for a blocked journal.
    fn finish(mut self, timeout: Duration) {
        drop(self.sender.take());
        let _ = self.done.recv_timeout(timeout);
    }
}

struct Sampler {
    diagnostics: Arc<Diagnostics>,
    identity: Identity,
    path: PathBuf,
    detector: Detector,
    enabled: Option<bool>,
    next_file: u64,
    next_line: u64,
    main: Progress,
    admin: Progress,
    facts: Value,
}
impl Sampler {
    fn new(diagnostics: Arc<Diagnostics>, identity: Identity, path: PathBuf) -> Self {
        Self {
            diagnostics,
            identity,
            path,
            detector: Detector::default(),
            enabled: None,
            next_file: 0,
            next_line: 0,
            main: Progress::default(),
            admin: Progress::default(),
            facts: json!({}),
        }
    }
    /// Cached facts and probes: a value the shell holds right now is replaced next time.
    fn state(&mut self, now: Clocks) -> Value {
        if let Some(facts) = self.diagnostics.heartbeat_facts(now.mono) {
            self.facts = facts;
        }
        let age =
            |p: &Progress| (!p.operation.is_empty()).then(|| now.mono.saturating_sub(p.updated));
        let mut state = self.facts.clone();
        state["run"] = self.identity.run.clone().into();
        state["pid"] = self.identity.pid.into();
        state["mainOperation"] = self.main.operation.into();
        state["tickAgeMillis"] = age(&self.main).into();
        state["operation"] = self.admin.operation.into();
        state["operationAgeMillis"] = age(&self.admin).into();
        state["droppedDiagnostics"] = self.diagnostics.dropped().into();
        state
    }
    fn write(&mut self, now: Clocks, clean_exit: bool, send: &mut impl FnMut(Record)) {
        let heartbeat = Heartbeat {
            version: 1,
            identity: self.identity.clone(),
            wall_millis: now.wall,
            monotonic_millis: now.mono,
            boottime_millis: now.boot,
            watching: self.enabled == Some(true),
            clean_exit,
            state: self.state(now),
        };
        if let Err(error) = process::write(&self.path, &heartbeat) {
            send((
                "heartbeat_error",
                json!({"kind":error.kind().to_string(),"cleanExit":clean_exit}),
            ));
        }
    }
    fn sample(&mut self, now: Clocks, send: &mut impl FnMut(Record), task: impl Fn(u32) -> Value) {
        // Named waits are always on; only the findings below follow the switch.
        for record in self.diagnostics.reminders(now.mono) {
            send(record);
        }
        let enabled = self.diagnostics.enabled();
        if self.enabled != Some(enabled) {
            let first = self.enabled.is_none();
            self.enabled = Some(enabled);
            // A fresh detector, so time spent off never reads as a pause.
            self.detector = Detector::default();
            self.diagnostics.set_watching(enabled);
            // The file says at once what changed.
            self.next_file = now.mono;
            self.next_line = now.mono;
            if !enabled && !first {
                self.write(now, false, send);
            }
            let state = if enabled { "on" } else { "off" };
            send(("stall_watchdog", json!({"state":state})));
        }
        if !enabled {
            return;
        }
        if let Some(progress) = self.diagnostics.main.sample() {
            self.main = progress;
        }
        if let Some(progress) = self.diagnostics.admin.sample() {
            self.admin = progress;
        }
        for record in self.detector.observe(
            now,
            &[("main-loop", &self.main), ("admin-worker", &self.admin)],
            task,
        ) {
            send(record);
        }
        if now.mono >= self.next_file {
            self.next_file = now.mono.saturating_add(HEARTBEAT_FILE);
            self.write(now, false, send);
        }
        if now.mono >= self.next_line {
            self.next_line = now.mono.saturating_add(HEARTBEAT_LINE);
            send(("heartbeat", self.state(now)));
        }
    }
}

pub struct Watchdog {
    stop: Arc<AtomicBool>,
    clean: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Watchdog {
    /// Reports how the previous run ended and starts sampling; `enabled` is the effective
    /// `stallWatchdog` choice until the settings change it through [`Diagnostics::set_enabled`].
    pub fn start(state: &Path, events: Arc<JsonEvents>, enabled: bool) -> std::io::Result<Self> {
        let diagnostics = events.diagnostics.clone();
        diagnostics.set_enabled(enabled);
        let mut delivery = Delivery::new(events)?;
        let path = state.join(process::HEARTBEAT);
        let stop = Arc::new(AtomicBool::new(false));
        let clean = Arc::new(AtomicBool::new(false));
        let (stopping, clean_exit) = (stop.clone(), clean.clone());
        let worker = std::thread::Builder::new()
            .name("stall-watchdog".into())
            .spawn(move || {
                let identity = Identity::current();
                // Always, whatever the switch: the file this run is about to replace.
                let (previous, exit) = process::previous_report(
                    process::read(&path),
                    &identity,
                    clocks(),
                    process::pid_start,
                    &process::JournalHistory,
                );
                diagnostics.set_process(
                    json!({"current":identity,"previous":previous,"previousExit":exit}),
                );
                delivery.send(("previous_process", previous));
                if let Some(exit) = exit {
                    delivery.send(("process_exit", exit));
                }
                let mut sampler = Sampler::new(diagnostics, identity, path);
                sampler.write(clocks(), false, &mut |r| delivery.send(r));
                while !stopping.load(Ordering::Acquire) {
                    sampler.sample(clocks(), &mut |r| delivery.send(r), process::task);
                    std::thread::park_timeout(Duration::from_secs(1));
                }
                let clean = clean_exit.load(Ordering::Acquire);
                sampler.write(clocks(), clean, &mut |r| delivery.send(r));
                sampler.diagnostics.set_watching(false);
                delivery.finish(Duration::from_secs(1));
            })?;
        Ok(Self {
            stop,
            clean,
            worker: Some(worker),
        })
    }
    /// The next heartbeat write, at drop, marks this run as ended cleanly.
    pub fn clean_exit(&self) {
        self.clean.store(true, Ordering::Release);
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    fn clock(t: u64) -> Clocks {
        Clocks {
            wall: t,
            mono: t,
            boot: t,
        }
    }
    fn busy(operation: &'static str, updated: u64, budget: u64, serial: u64) -> Progress {
        Progress {
            operation,
            since: updated,
            updated,
            budget,
            tid: 12,
            serial,
        }
    }
    fn names(records: &[Record]) -> Vec<&'static str> {
        records.iter().map(|r| r.0).collect()
    }
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{name}-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn suspend_and_stopped_process_differ_and_threads_get_grace_after_a_pause() {
        for (mono, cause) in [(1_000, "suspend"), (60_000, "stopped_or_starved")] {
            let mut detector = Detector::default();
            let stale = busy("tick", 0, 10_000, 1);
            assert!(detector
                .observe(clock(0), &[("main-loop", &stale)], |_| json!({}))
                .is_empty());
            let after = Clocks {
                wall: 60_000,
                mono,
                boot: 60_000,
            };
            let records = detector.observe(after, &[("main-loop", &stale)], |_| panic!("grace"));
            assert_eq!(names(&records), ["process_paused"]);
            assert_eq!(records[0].1["cause"], cause);
            assert_eq!(records[0].1["gapMillis"], 60_000);
            let later = Clocks {
                wall: 64_000,
                mono: mono + 4_000,
                boot: 64_000,
            };
            if mono > 10_000 {
                assert!(detector
                    .observe(later, &[("main-loop", &stale)], |_| panic!("grace"))
                    .is_empty());
            }
        }
        // A one-second rhythm is no pause, and a clock that went back is ignored.
        let mut detector = Detector::default();
        for t in [0, 1_000, 5_000, 4_000, 9_000] {
            assert!(detector.observe(clock(t), &[], |_| json!({})).is_empty());
        }
    }
    #[test]
    fn blocked_threads_report_with_growing_gaps_then_unblock_and_idle_is_never_blocked() {
        let mut detector = Detector::default();
        let mut records = Vec::new();
        for second in 0..=400 {
            let t = second * 1000;
            let main = busy("tick", t, 10_000, 1);
            // The admin worker sits in one operation from 10 s to 350 s, then goes idle.
            let admin = if (10..=350).contains(&second) {
                busy("join_wifi", 10_000, 120_000, 7)
            } else {
                Progress::default()
            };
            records.extend(detector.observe(
                clock(t),
                &[("main-loop", &main), ("admin-worker", &admin)],
                |tid| json!({"tid":tid,"state":"D","wchan":"fake_wait"}),
            ));
        }
        assert_eq!(
            names(&records),
            ["runtime_blocked", "runtime_blocked", "runtime_unblocked"]
        );
        assert_eq!(records[0].1["target"], "admin-worker");
        assert_eq!(records[0].1["operation"], "join_wifi");
        assert_eq!(records[0].1["forMillis"], 121_000);
        assert_eq!(records[0].1["budgetMillis"], 120_000);
        assert_eq!(records[0].1["task"]["wchan"], "fake_wait");
        assert_eq!(records[1].1["forMillis"], 242_000);
        assert_eq!(records[2].1["afterMillis"], 341_000);
    }
    #[test]
    fn a_new_operation_or_a_beat_ends_the_block() {
        let mut detector = Detector::default();
        let stuck = busy("tick", 0, 10_000, 1);
        let blocked = detector.observe(clock(11_000), &[("main-loop", &stuck)], |_| json!({}));
        assert_eq!(names(&blocked), ["runtime_blocked"]);
        assert!(detector
            .observe(clock(12_000), &[("main-loop", &stuck)], |_| json!({}))
            .is_empty());
        let next = busy("startup", 12_500, 120_000, 2);
        let records = detector.observe(clock(13_000), &[("main-loop", &next)], |_| json!({}));
        assert_eq!(names(&records), ["runtime_unblocked"]);
        assert_eq!(records[0].1["operation"], "tick");
        // The old operation is back and stuck again: blocked anew, until the thread beats.
        let mut again = Vec::new();
        for second in 14..=25 {
            again.extend(
                detector.observe(
                    clock(second * 1000),
                    &[("main-loop", &stuck)],
                    |_| json!({}),
                ),
            );
        }
        assert_eq!(names(&again), ["runtime_blocked"]);
        let beat = Progress {
            updated: 26_000,
            ..stuck
        };
        assert_eq!(
            names(&detector.observe(clock(26_000), &[("main-loop", &beat)], |_| json!({}))),
            ["runtime_unblocked"]
        );
    }
    #[test]
    fn heartbeat_file_every_10_s_line_every_30_s_and_waits_tick_while_off() {
        let dir = temp("w-cadence");
        let (events, _) = recorder();
        events
            .diagnostics
            .waits
            .lock()
            .unwrap()
            .begin(0, "iphone", json!({"generation":1}));
        let mut sampler = Sampler::new(
            events.diagnostics.clone(),
            Identity::current(),
            dir.join(process::HEARTBEAT),
        );
        let mut records = Vec::new();
        for second in 0..=60 {
            sampler.sample(
                clock(second * 1000),
                &mut |r| records.push(r),
                |_| json!({}),
            );
        }
        // Off: no heartbeat, but the wait still gets its 30 s and 60 s reminders.
        assert_eq!(
            names(&records),
            ["stall_watchdog", "carplay_waiting", "carplay_waiting"]
        );
        assert!(!sampler.path.exists());
        events.diagnostics.set_enabled(true);
        records.clear();
        let mut writes = 0;
        let mut modified = None;
        for second in 61..=121 {
            sampler.sample(
                clock(second * 1000),
                &mut |r| records.push(r),
                |_| json!({}),
            );
            let now = std::fs::read(&sampler.path).ok();
            if now != modified {
                writes += 1;
                modified = now;
            }
        }
        assert_eq!(writes, 7);
        let lines: Vec<_> = records.iter().filter(|r| r.0 == "heartbeat").collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].1["waits"].as_str().unwrap(), "iphone:61s");
        assert!(lines[0].1["run"].is_string());
        let saved = process::read(&sampler.path).unwrap();
        assert!(saved.watching && !saved.clean_exit);
        assert_eq!(saved.state["waits"], "iphone:121s");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn switching_off_retires_the_heartbeat_and_on_starts_a_fresh_detector() {
        let dir = temp("w-toggle");
        let diagnostics = Arc::new(Diagnostics::default());
        let mut sampler = Sampler::new(
            diagnostics.clone(),
            Identity::current(),
            dir.join(process::HEARTBEAT),
        );
        let mut records = Vec::new();
        diagnostics.set_enabled(true);
        sampler.write(clock(0), false, &mut |r| records.push(r));
        sampler.sample(clock(0), &mut |r| records.push(r), |_| json!({}));
        assert!(process::read(&sampler.path).unwrap().watching);
        diagnostics.set_enabled(false);
        sampler.sample(clock(1_000), &mut |r| records.push(r), |_| json!({}));
        let retired = process::read(&sampler.path).unwrap();
        assert!(!retired.watching && retired.wall_millis == 1_000);
        // An hour off, then on again: no pause is reported for the time spent off.
        diagnostics.set_enabled(true);
        sampler.sample(clock(3_600_000), &mut |r| records.push(r), |_| json!({}));
        assert!(process::read(&sampler.path).unwrap().watching);
        assert!(!records.iter().any(|r| r.0 == "process_paused"));
        let states: Vec<_> = records
            .iter()
            .filter(|r| r.0 == "stall_watchdog")
            .map(|r| r.1["state"].as_str().unwrap())
            .collect();
        assert_eq!(states, ["on", "off", "on"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
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
    fn wait_for(lines: &Mutex<Vec<Value>>, event: &str) -> Value {
        for _ in 0..200 {
            if let Some(line) = lines.lock().unwrap().iter().find(|l| l["event"] == event) {
                return line.clone();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no {event} line");
    }
    #[test]
    fn every_start_reports_the_previous_run_and_a_clean_stop_is_marked_even_when_off() {
        let dir = temp("w-runs");
        let path = dir.join(process::HEARTBEAT);
        let (events, lines) = recorder();
        let first = Watchdog::start(&dir, events, false).unwrap();
        assert_eq!(wait_for(&lines, "previous_process")["status"], "missing");
        wait_for(&lines, "stall_watchdog");
        assert!(!process::read(&path).unwrap().clean_exit);
        // An unclean end: the run is gone without a clean-exit write.
        drop(first);
        let unclean = process::read(&path).unwrap();
        assert!(!unclean.clean_exit && !unclean.watching);
        let (events, lines) = recorder();
        let second = Watchdog::start(&dir, events.clone(), false).unwrap();
        let previous = wait_for(&lines, "previous_process");
        assert_eq!(previous["status"], "available");
        assert_eq!(previous["cleanExit"], false);
        assert_eq!(previous["identity"]["run"], unclean.identity.run);
        assert_eq!(
            events.diagnostics.export_details().lines().nth(1).unwrap(),
            format!("Previous process: {}", {
                let mut p = previous.clone();
                p.as_object_mut().unwrap().remove("event");
                p.as_object_mut().unwrap().remove("time");
                p
            })
        );
        second.clean_exit();
        drop(second);
        assert!(process::read(&path).unwrap().clean_exit);
        let (events, lines) = recorder();
        let third = Watchdog::start(&dir, events, true).unwrap();
        assert_eq!(wait_for(&lines, "previous_process")["cleanExit"], true);
        assert_eq!(wait_for(&lines, "stall_watchdog")["state"], "on");
        wait_for(&lines, "heartbeat");
        drop(third);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn a_blocked_journal_blocks_neither_delivery_nor_sampler_and_drops_are_counted() {
        let (entered, ready) = mpsc::channel();
        let (release, wait) = mpsc::channel::<()>();
        let wait = Mutex::new(wait);
        let events = Arc::new(JsonEvents::new(Arc::new(move |_| {
            let _ = entered.send(());
            let _ = wait.lock().unwrap().recv();
        })));
        let diagnostics = events.diagnostics.clone();
        let mut delivery = Delivery::new(events).unwrap();
        delivery.send(("heartbeat", json!({})));
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..1000 {
            delivery.send(("heartbeat", json!({})));
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(diagnostics.dropped() >= 1000 - 64);
        let started = std::time::Instant::now();
        delivery.finish(Duration::from_millis(200));
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(release);
    }
    #[test]
    fn systemd_never_restarts_the_relay_on_a_missed_heartbeat() {
        // The stall watchdog is diagnostics only: no sd_notify watchdog for systemd to act on.
        let unit = include_str!("../../../packaging/openxplay-relay.service");
        assert!(!unit.contains("WatchdogSec"));
    }
}
