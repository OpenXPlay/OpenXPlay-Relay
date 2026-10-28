// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use std::{
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct CommandResult {
    pub exit_code: i32,
    pub output: String,
}
impl CommandResult {
    pub fn checked(self, operation: &str) -> Result<String> {
        if self.exit_code != 0 {
            Err(std::io::Error::other(format!(
                "{operation} failed (exit {})",
                self.exit_code
            )))
        } else {
            Ok(self.output)
        }
    }
}
pub trait CommandRunner: Send + Sync {
    fn run(&self, arguments: &[String], timeout: Duration) -> Result<CommandResult>;
}
#[derive(Default)]
pub struct ProcessCommandRunner;
impl ProcessCommandRunner {
    pub fn new() -> Self {
        Self
    }
}
impl CommandRunner for ProcessCommandRunner {
    fn run(&self, args: &[String], timeout: Duration) -> Result<CommandResult> {
        if args.is_empty() || timeout.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Empty command or timeout",
            ));
        }
        let mut child = Command::new(&args[0])
            .args(&args[1..])
            // C.UTF-8 keeps tool messages stable but preserves non-ASCII SSIDs (C prints them as "?").
            .env("LC_ALL", "C.UTF-8")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        for mut pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let out = bytes.clone();
            let done = done_tx.clone();
            spawn_worker(
                "command-output",
                Box::new(move || {
                    let mut buf = [0; 4096];
                    while let Ok(n) = pipe.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        let mut o = out.lock().unwrap();
                        let keep = n.min((1024 * 1024usize).saturating_sub(o.len()));
                        o.extend_from_slice(&buf[..keep]);
                    }
                    let _ = done.send(());
                }),
            )?;
        }
        let start = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait()? {
                break s;
            }
            if start.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("{} timed out", args[0]),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        // Let pipe readers consume the final output without waiting on inherited descriptors.
        let _ = done_rx.recv_timeout(Duration::from_secs(1));
        let _ = done_rx.recv_timeout(Duration::from_secs(1));
        let output = String::from_utf8_lossy(&bytes.lock().unwrap()).into_owned();
        Ok(CommandResult {
            exit_code: status.code().unwrap_or(-1),
            output,
        })
    }
}
type StartupListener = Arc<dyn Fn(&str) + Send + Sync>;
pub struct JsonEvents {
    failure_secrets: Mutex<Vec<String>>,
    startup_diagnostics: std::sync::atomic::AtomicBool,
    output: Arc<dyn Fn(String) + Send + Sync>,
    gate: Mutex<()>,
    log_timeline: Mutex<crate::logs::timeline::Timeline>,
    follow_timeline: Mutex<Option<(Instant, Instant, bool)>>,
    timing: crate::boot_timing::BootTiming,
    pub(crate) facts: Arc<Mutex<crate::path_facts::RuntimePathFacts>>,
    startup: Mutex<Option<StartupListener>>,
}
impl JsonEvents {
    pub fn new(output: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        Self {
            failure_secrets: Mutex::default(),
            startup_diagnostics: std::sync::atomic::AtomicBool::new(true),
            output,
            gate: Mutex::new(()),
            log_timeline: Mutex::default(),
            follow_timeline: Mutex::new(None),
            timing: Default::default(),
            facts: Arc::default(),
            startup: Mutex::default(),
        }
    }
    /// Records a step towards a CarPlay session; the first ACTIVE session of a controller
    /// attempt logs the attempt's steps as one `boot-timing` event.
    pub fn milestone(&self, milestone: crate::boot_timing::Milestone) {
        if let Some(fields) = self.timing.mark(milestone) {
            self.emit("boot-timing", &fields);
        }
    }
    /// Records that the attempt's access point is up, reused or new.
    pub fn hotspot_milestone(&self, reused: bool) {
        self.timing.hotspot_ready(reused);
    }
    pub fn follow_over(&self, stage: &str) {
        let now = Instant::now();
        let mut timeline = self.follow_timeline.lock().unwrap();
        if stage == "link-joined" {
            if timeline.is_some() {
                return;
            }
            *timeline = Some((now, now, false));
        }
        let Some((joined, previous, started)) = timeline.as_mut() else {
            return;
        };
        if stage == "iphone-connected" && !*started {
            return;
        }
        if stage == "controller-start" {
            *started = true;
        }
        let fields = serde_json::json!({"stage":stage,
            "sinceJoinedMillis":now.duration_since(*joined).as_millis() as u64,
            "sincePreviousMillis":now.duration_since(*previous).as_millis() as u64});
        *previous = now;
        if stage == "iphone-connected" || stage == "link-left" {
            *timeline = None;
        }
        drop(timeline);
        self.emit("carlink-follow-over", &fields);
    }
    pub fn runtime_path_facts(&self) -> crate::path_facts::RuntimePathFacts {
        self.facts.lock().unwrap().clone()
    }
    pub(crate) fn startup_listener(&self, listener: StartupListener) {
        *self.startup.lock().unwrap() = Some(listener);
    }
    pub fn startup_milestone(&self, name: &str) {
        self.emit("airplay-milestone", &serde_json::json!({"name":name}));
    }
    /// W13 can apply the effective startup-diagnostics experiment at the next connection.
    pub fn set_startup_diagnostics(&self, enabled: bool) {
        self.startup_diagnostics
            .store(enabled, std::sync::atomic::Ordering::Release);
    }
    pub fn failure_secrets(&self, values: Vec<String>) {
        *self.failure_secrets.lock().unwrap() =
            values.into_iter().filter(|s| !s.is_empty()).collect();
    }
    pub fn iap2_frame(&self) {
        if !self
            .startup_diagnostics
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return;
        }
        let count = {
            let mut facts = self.facts.lock().unwrap();
            facts.iphone.runtime_iap2_frames = facts.iphone.runtime_iap2_frames.saturating_add(1);
            facts.iphone.runtime_iap2_frames
        };
        if count.is_power_of_two() {
            self.emit("runtime-iap2", &serde_json::json!({"frames":count}));
        }
    }
    pub fn filtered(&self, fields: &serde_json::Value) -> serde_json::Value {
        let mut fields = fields.clone();
        let secrets = self.failure_secrets.lock().unwrap();
        fn filter(value: &mut serde_json::Value, secrets: &[String]) {
            match value {
                serde_json::Value::String(text) => {
                    *text = crate::failure_text::redact(text, secrets)
                }
                serde_json::Value::Array(values) => {
                    values.iter_mut().for_each(|v| filter(v, secrets))
                }
                serde_json::Value::Object(values) => {
                    values.values_mut().for_each(|v| filter(v, secrets))
                }
                _ => {}
            }
        }
        filter(&mut fields, &secrets);
        drop(secrets);
        crate::logs::journal::value(&mut fields);
        fields
    }
    /// Observations that need accounting but must not put periodic stats in the journal.
    pub fn log_observation(&self, event: &str, fields: &serde_json::Value) {
        let at = Instant::now();
        let fields = self.filtered(fields);
        let mut timeline = self.log_timeline.lock().unwrap();
        let records = if event == "network-observed" {
            timeline.observe(event, &fields, at)
        } else {
            timeline.browser(event, &fields, at)
        };
        drop(timeline);
        if records.is_empty() {
            return;
        }
        let _gate = self.gate.lock().unwrap();
        for mut record in records {
            record["time"] = serde_json::json!(timestamp());
            (self.output)(record.to_string());
        }
    }
    pub fn emit(&self, event: &str, fields: &serde_json::Value) {
        // Raw vehicle data belongs to the opt-in history consumer, never to journald.
        if event == "clink-vehicle" {
            return;
        }
        let observed_at = Instant::now();
        let fields = self.filtered(fields);
        if event == "stage" && fields["stage"] == "WirelessActive" {
            self.follow_over("iphone-connected");
        }
        let reason =
            self.facts
                .lock()
                .unwrap()
                .observe(event, &fields, wall_clock().as_millis() as u64);
        if let Some(reason) = reason {
            let listener = self.startup.lock().unwrap().clone();
            if let Some(listener) = listener {
                listener(reason);
            }
        }
        let _g = self.gate.lock().unwrap();
        let mut v = serde_json::json!({"event":event,"time":timestamp()});
        if let Some(m) = fields.as_object() {
            for (k, x) in m {
                v[k] = x.clone();
            }
        }
        let timeline = self
            .log_timeline
            .lock()
            .unwrap()
            .observe(event, &fields, observed_at);
        (self.output)(v.to_string());
        for mut record in timeline {
            record["time"] = v["time"].clone();
            (self.output)(record.to_string());
        }
    }
    pub fn log(&self, message: &str) {
        self.emit("diagnostic", &serde_json::json!({"message":message}));
    }
    pub fn trace(&self, message: &str) {
        self.log(message.lines().next().unwrap_or(""));
    }
}
/// Time since the Unix epoch on the wall clock. The relay may run without an RTC or network time,
/// so the wall clock can be hours off and can step: it labels logs and display values only. Every
/// deadline, expiry and rate limit uses the monotonic clock instead.
pub fn wall_clock() -> Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}
fn timestamp() -> String {
    utc_timestamp(wall_clock())
}
/// `since_epoch` as an ISO 8601 UTC timestamp with milliseconds. The date is computed here rather
/// than by libc, so it needs no time zone and none of libc's time types.
fn utc_timestamp(since_epoch: Duration) -> String {
    let seconds = since_epoch.as_secs();
    let (days, time) = (seconds / 86_400, seconds % 86_400);
    // Count days from 0000-03-01 in 400-year eras of 146,097 days; with March as the first month
    // the leap day ends the year, so each month's start follows from the day of the year alone.
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = era * 400 + year_of_era + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        time / 3_600,
        time / 60 % 60,
        time % 60,
        since_epoch.subsec_millis()
    )
}
pub fn private_state_file(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    crate::admin::auth::private_file(path, bytes)
}
pub const WORKER_STACK_BYTES: usize = 256 * 1024;
pub fn spawn_worker(
    name: &str,
    run: Box<dyn FnOnce() + Send>,
) -> Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name(name.into())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(run)
}
pub fn command(runner: &dyn CommandRunner, args: &[&str], timeout: Duration) -> Result<String> {
    runner
        .run(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            timeout,
        )?
        .checked(args[0])
}

#[cfg(test)]
mod follow_timeline_tests {
    use super::*;
    #[test]
    fn follow_over_logs_all_stages_and_monotonic_durations_once() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let copy = lines.clone();
        let events = JsonEvents::new(Arc::new(move |line| copy.lock().unwrap().push(line)));
        events.follow_over("link-joined");
        events.follow_over("link-joined");
        // An old controller's late proof cannot finish the new timeline.
        events.emit("stage", &serde_json::json!({"stage":"WirelessActive"}));
        events.follow_over("controller-stop");
        events.follow_over("controller-start");
        events.emit("stage", &serde_json::json!({"stage":"WirelessActive"}));
        events.emit("stage", &serde_json::json!({"stage":"WirelessActive"}));
        let entries: Vec<serde_json::Value> = lines
            .lock()
            .unwrap()
            .iter()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|v| v["event"] == "carlink-follow-over")
            .collect();
        assert_eq!(
            entries
                .iter()
                .map(|v| v["stage"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "link-joined",
                "controller-stop",
                "controller-start",
                "iphone-connected"
            ]
        );
        let mut previous = 0;
        for entry in entries {
            let elapsed = entry["sinceJoinedMillis"].as_u64().unwrap();
            assert!(elapsed >= previous);
            assert!(entry["sincePreviousMillis"].as_u64().unwrap() <= elapsed);
            previous = elapsed;
        }
    }
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;
    #[test]
    fn utc_timestamps_match_known_dates() {
        for (seconds, millis, expected) in [
            (0, 0, "1970-01-01T00:00:00.000Z"),
            (68_169_600, 0, "1972-02-29T00:00:00.000Z"),
            (951_782_400, 0, "2000-02-29T00:00:00.000Z"),
            (951_868_799, 999, "2000-02-29T23:59:59.999Z"),
            (1_792_207_872, 345, "2026-10-17T03:31:12.345Z"),
            (4_107_456_000, 0, "2100-02-28T00:00:00.000Z"),
            (4_107_542_400, 0, "2100-03-01T00:00:00.000Z"),
            (253_402_300_799, 999, "9999-12-31T23:59:59.999Z"),
        ] {
            let since_epoch = Duration::from_secs(seconds) + Duration::from_millis(millis);
            assert_eq!(utc_timestamp(since_epoch), expected);
        }
    }
    #[test]
    fn utc_dates_follow_the_gregorian_calendar_day_by_day() {
        let (mut year, mut month, mut day) = (1970, 1, 1);
        // Through 2400, so the 100- and 400-year leap rules both apply.
        for days in 0..(431 * 365 + 105) {
            let expected = format!("{year:04}-{month:02}-{day:02}T23:59:59.000Z");
            assert_eq!(
                utc_timestamp(Duration::from_secs(days * 86_400 + 86_399)),
                expected
            );
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let length = match month {
                2 if leap => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            day += 1;
            if day > length {
                (day, month) = (1, month + 1);
                if month > 12 {
                    (month, year) = (1, year + 1);
                }
            }
        }
        assert_eq!((year, month, day), (2401, 1, 1));
    }
}

#[cfg(test)]
mod runtime_diagnostic_tests {
    use super::*;
    #[test]
    fn liveness_is_counted_and_logged_only_at_powers_of_two() {
        let output = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let saved = output.clone();
        let events = JsonEvents::new(Arc::new(move |line| {
            saved
                .lock()
                .unwrap()
                .push(serde_json::from_str(&line).unwrap())
        }));
        events.failure_secrets(vec!["unlabelled-secret".into()]);
        events.emit("stage", &serde_json::json!({"stage":"DiscoveringMfi"}));
        for _ in 0..10 {
            events.iap2_frame();
        }
        assert_eq!(events.runtime_path_facts().iphone.runtime_iap2_frames, 10);
        let counts: Vec<_> = output
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e["event"] == "runtime-iap2")
            .map(|e| e["frames"].as_u64().unwrap())
            .collect();
        assert_eq!(counts, [1, 2, 4, 8]);
        events.emit(
            "startup-failed",
            &serde_json::json!({"reason":"error=17 unlabelled-secret password='secret phrase'"}),
        );
        let failure = events.runtime_path_facts().iphone.failure.unwrap();
        assert!(failure.text.contains("error=17"));
        assert!(!failure.text.contains("secret"));
        assert!(failure
            .trail
            .iter()
            .any(|line| line.contains("runtime_iap2 frames=8")));
        assert!(!serde_json::to_string(&*output.lock().unwrap())
            .unwrap()
            .contains("secret"));
        events.emit("stage", &serde_json::json!({"stage":"DiscoveringMfi"}));
        assert_eq!(events.runtime_path_facts().iphone.runtime_iap2_frames, 0);
        events.set_startup_diagnostics(false);
        events.iap2_frame();
        assert_eq!(events.runtime_path_facts().iphone.runtime_iap2_frames, 0);
    }
}
