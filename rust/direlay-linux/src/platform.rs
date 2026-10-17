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
pub struct JsonEvents {
    output: Arc<dyn Fn(String) + Send + Sync>,
    gate: Mutex<()>,
    follow_timeline: Mutex<Option<(Instant, Instant, bool)>>,
    timing: crate::boot_timing::BootTiming,
}
impl JsonEvents {
    pub fn new(output: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        Self {
            output,
            gate: Mutex::new(()),
            follow_timeline: Mutex::new(None),
            timing: Default::default(),
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
    pub fn emit(&self, event: &str, fields: &serde_json::Value) {
        if event == "stage" && fields["stage"] == "WirelessActive" {
            self.follow_over("iphone-connected");
        }
        let _g = self.gate.lock().unwrap();
        let mut v = serde_json::json!({"event":event,"time":timestamp()});
        if let Some(m) = fields.as_object() {
            for (k, x) in m {
                v[k] = x.clone();
            }
        }
        (self.output)(v.to_string());
    }
    pub fn log(&self, message: &str) {
        self.emit("diagnostic", &serde_json::json!({"message":message}));
    }
    pub fn trace(&self, message: &str) {
        self.log(message.lines().next().unwrap_or(""));
    }
}
fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let t = now.as_secs() as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    unsafe { libc::gmtime_r(&t, &mut tm) };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        now.subsec_millis()
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
