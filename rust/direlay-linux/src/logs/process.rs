// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-30

//! This run's identity, its heartbeat file and what the previous run left behind. A stale
//! heartbeat alone never explains an exit; only systemd's journal lines for that invocation do.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

pub const HEARTBEAT: &str = "stall-heartbeat.json";
const MAX_HEARTBEAT: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub run: String,
    pub boot_id: Option<String>,
    pub pid: u32,
    pub start_ticks: Option<u64>,
    pub invocation_id: Option<String>,
}
impl Identity {
    pub fn current() -> Self {
        Self {
            run: crate::admin::auth::random_hex(16)
                .unwrap_or_else(|_| format!("{}-{}", std::process::id(), super::now_millis())),
            boot_id: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| boot_id(s)),
            pid: std::process::id(),
            start_ticks: pid_start(std::process::id()),
            // systemd sets it for each start of the unit.
            invocation_id: std::env::var("INVOCATION_ID")
                .ok()
                .filter(|s| invocation(s)),
        }
    }
}
pub fn pid_start(pid: u32) -> Option<u64> {
    stat_start(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}
fn stat_start(stat: &str) -> Option<u64> {
    // comm may contain spaces and parentheses; field 22 follows the final ')'.
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    pub version: u8,
    pub identity: Identity,
    pub wall_millis: u64,
    pub monotonic_millis: u64,
    pub boottime_millis: u64,
    /// The stall watchdog sampled; otherwise the file was only written at start, switch-off and exit.
    pub watching: bool,
    pub clean_exit: bool,
    pub state: Value,
}
pub fn read(path: &Path) -> io::Result<Heartbeat> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_HEARTBEAT {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_HEARTBEAT + 1).read_to_end(&mut bytes)?;
    let heartbeat: Heartbeat = serde_json::from_slice(&bytes)?;
    if heartbeat.version != 1 || heartbeat.identity.run.is_empty() || heartbeat.identity.pid == 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(heartbeat)
}
/// Atomically replaced, owner-only.
pub fn write(path: &Path, heartbeat: &Heartbeat) -> io::Result<()> {
    crate::platform::private_state_file(path, &serde_json::to_vec(heartbeat)?)
}

pub trait ExitHistory {
    /// systemd's journal lines about how `previous` ended, if it ran as an invocation of the unit.
    fn previous_exit(&self, previous: &Identity) -> Option<Value>;
}
pub struct JournalHistory;
impl ExitHistory for JournalHistory {
    fn previous_exit(&self, previous: &Identity) -> Option<Value> {
        #[cfg(target_os = "linux")]
        {
            use std::{process::Command, time::Duration};
            let id = previous
                .invocation_id
                .as_deref()
                .filter(|s| invocation(s))?;
            let boot = previous.boot_id.as_deref().filter(|s| boot_id(s))?;
            // systemd's own lines (PID 1) for that invocation only: few, and never the relay's.
            let mut command = Command::new("journalctl");
            command
                .args(["--no-pager", "-o", "json", "-n", "32"])
                .args([
                    format!("_BOOT_ID={}", boot.replace('-', "")),
                    "_PID=1".into(),
                    format!("UNIT={}", super::UNITS[0]),
                    format!("INVOCATION_ID={id}"),
                ]);
            let slice = super::bounded_command(command, 128 * 1024, Duration::from_secs(2)).ok()?;
            attributed_exit(previous, &slice.bytes)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = previous;
            None
        }
    }
}
fn invocation(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn boot_id(id: &str) -> bool {
    id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}
/// systemd's message IDs for an out-of-memory kill (kernel and systemd-oomd) and a clean stop.
const OUT_OF_MEMORY: &[&str] = &[
    "fe6faa94e7774663a0da52717891d8ef",
    "d989611b15e44c9dbf31e3c81256e4ed",
];
const UNIT_SUCCESS: &str = "7ad2d189f7e94e70a38c781354912448";
/// Structured evidence from rows of that boot, unit and invocation only; never message text.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn attributed_exit(previous: &Identity, bytes: &[u8]) -> Option<Value> {
    let id = previous
        .invocation_id
        .as_deref()
        .filter(|s| invocation(s))?;
    let boot = previous.boot_id.as_deref()?.replace('-', "");
    let mut exit = json!({"pid":previous.pid,"invocationId":id,"source":"journal"});
    let mut found = false;
    for row in String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| {
            row["_BOOT_ID"].as_str() == Some(boot.as_str())
                && row["INVOCATION_ID"] == id
                && row["UNIT"] == super::UNITS[0]
        })
    {
        let message = row["MESSAGE_ID"].as_str().unwrap_or("");
        if row.get("EXIT_CODE").is_some() {
            exit["exitCode"] = row["EXIT_CODE"].clone();
            exit["exitStatus"] = row["EXIT_STATUS"].clone();
        } else if row.get("UNIT_RESULT").is_some() {
            exit["result"] = row["UNIT_RESULT"].clone();
        } else if message == UNIT_SUCCESS {
            exit["result"] = "success".into();
        } else if OUT_OF_MEMORY.contains(&message) {
            exit["oomKill"] = true.into();
        } else {
            continue;
        }
        found = true;
        exit["journalRealtime"] = row["__REALTIME_TIMESTAMP"].clone();
    }
    found.then_some(exit)
}

/// The start report: what the previous run's heartbeat says, and the attributed exit, if any.
pub fn previous_report(
    heartbeat: io::Result<Heartbeat>,
    current: &Identity,
    clock: super::watchdog::Clocks,
    live_start: impl FnOnce(u32) -> Option<u64>,
    history: &dyn ExitHistory,
) -> (Value, Option<Value>) {
    let previous = match heartbeat {
        Ok(previous) => previous,
        Err(e) => {
            let status = if e.kind() == io::ErrorKind::NotFound {
                "missing"
            } else {
                "corrupt"
            };
            return (json!({"status":status,"exit":"unknown"}), None);
        }
    };
    let same_boot = previous
        .identity
        .boot_id
        .as_ref()
        .zip(current.boot_id.as_ref())
        .map(|(a, b)| a == b);
    let pid_state = match same_boot {
        Some(true) => match (
            previous.identity.start_ticks,
            live_start(previous.identity.pid),
        ) {
            (Some(a), Some(b)) if a == b => "still_running",
            (_, Some(_)) => "reused",
            (_, None) => "gone",
        },
        Some(false) => "previous_boot",
        None => "unknown",
    };
    // CLOCK_BOOTTIME keeps counting through suspend and never steps; the wall clock may.
    let elapsed =
        (same_boot == Some(true)).then(|| clock.boot.saturating_sub(previous.boottime_millis));
    let wall_clock_changed = elapsed.map(|elapsed| {
        clock.boot < previous.boottime_millis
            || (clock.wall as i128 - previous.wall_millis as i128 - elapsed as i128).abs() > 5_000
    });
    let exit = (pid_state != "still_running"
        && previous.identity.invocation_id != current.invocation_id)
        .then(|| history.previous_exit(&previous.identity))
        .flatten();
    let report = json!({"status":"available","identity":previous.identity,"cleanExit":previous.clean_exit,
        "watching":previous.watching,"lastSeenWallMillis":previous.wall_millis,"lastSeenAgeMillis":elapsed,
        "sameBoot":same_boot,"pidState":pid_state,"wallClockChanged":wall_clock_changed,
        "exit":if exit.is_some() {"attributed"} else {"unknown"},"state":previous.state});
    (report, exit)
}

/// Bounded `/proc` evidence for a blocked thread: its scheduler state and kernel wait channel.
/// Rust cannot sample another thread's stack safely, so there is no symbolic stack.
pub fn task(tid: u32) -> Value {
    let base = format!("/proc/self/task/{tid}");
    let bounded = |name: &str| -> Option<String> {
        let mut text = String::new();
        std::fs::File::open(format!("{base}/{name}"))
            .ok()?
            .take(4096)
            .read_to_string(&mut text)
            .ok()?;
        Some(text.trim().into())
    };
    let state = bounded("stat")
        .and_then(|s| Some(s.rsplit_once(')')?.1.split_whitespace().next()?.to_string()));
    json!({"state":state,"wchan":bounded("wchan").filter(|w| !w.is_empty() && w != "0")})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    const BOOT: &str = "11111111-1111-1111-1111-111111111111";
    pub fn identity() -> Identity {
        Identity {
            run: "run-one".into(),
            boot_id: Some(BOOT.into()),
            pid: 12,
            start_ticks: Some(100),
            invocation_id: Some("a".repeat(32)),
        }
    }
    fn heartbeat() -> Heartbeat {
        Heartbeat {
            version: 1,
            identity: identity(),
            wall_millis: 1000,
            monotonic_millis: 500,
            boottime_millis: 500,
            watching: true,
            clean_exit: false,
            state: json!({"session":"WaitingForPairedIphone","waits":"iphone:300s"}),
        }
    }
    fn clock(wall: u64, boot: u64) -> super::super::watchdog::Clocks {
        super::super::watchdog::Clocks {
            wall,
            mono: boot,
            boot,
        }
    }
    struct History(Option<Value>, std::cell::Cell<u32>);
    impl ExitHistory for History {
        fn previous_exit(&self, previous: &Identity) -> Option<Value> {
            assert_eq!(previous.run, "run-one");
            self.1.set(self.1.get() + 1);
            self.0.clone()
        }
    }
    fn none() -> History {
        History(None, Default::default())
    }
    fn dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{name}-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn heartbeat_round_trip_is_private_and_damage_reads_as_corrupt() {
        let dir = dir("w-heartbeat");
        let path = dir.join(HEARTBEAT);
        assert_eq!(read(&path).unwrap_err().kind(), io::ErrorKind::NotFound);
        let mut h = heartbeat();
        write(&path, &h).unwrap();
        assert_eq!(read(&path).unwrap().identity, h.identity);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        h.clean_exit = true;
        write(&path, &h).unwrap();
        assert!(read(&path).unwrap().clean_exit);
        let oversized = format!("{{\"pad\":\"{}\"}}", "x".repeat(MAX_HEARTBEAT as usize));
        for bad in ["", "{", "{}", "[]", oversized.as_str()] {
            std::fs::write(&path, bad).unwrap();
            assert!(read(&path).is_err(), "{bad:.8}");
            let (report, _) =
                previous_report(read(&path), &identity(), clock(0, 0), |_| None, &none());
            assert_eq!(report["status"], "corrupt");
        }
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), &path).unwrap();
        assert!(read(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn missing_clean_and_unclean_previous_runs_are_explicit() {
        let mut current = identity();
        current.run = "run-two".into();
        current.invocation_id = Some("b".repeat(32));
        let history = none();
        let missing = previous_report(
            Err(io::ErrorKind::NotFound.into()),
            &current,
            clock(0, 0),
            |_| None,
            &history,
        );
        assert_eq!(
            missing,
            (json!({"status":"missing","exit":"unknown"}), None)
        );
        for clean in [false, true] {
            let mut h = heartbeat();
            h.clean_exit = clean;
            let (report, exit) =
                previous_report(Ok(h), &current, clock(10_500, 10_000), |_| None, &history);
            assert_eq!(report["cleanExit"], clean);
            assert_eq!(report["pidState"], "gone");
            assert_eq!(report["lastSeenAgeMillis"], 9_500);
            assert_eq!(report["wallClockChanged"], false);
            assert_eq!(report["state"]["waits"], "iphone:300s");
            assert_eq!(report["exit"], "unknown");
            assert!(exit.is_none());
        }
        assert_eq!(history.1.get(), 2);
    }
    #[test]
    fn pid_reuse_reboot_live_process_and_clock_steps_are_told_apart() {
        let mut current = identity();
        current.invocation_id = Some("b".repeat(32));
        let history = none();
        let (report, _) = previous_report(
            Ok(heartbeat()),
            &current,
            clock(100_000, 1000),
            |pid| (pid == 12).then_some(200),
            &history,
        );
        assert_eq!(report["pidState"], "reused");
        assert_eq!(report["wallClockChanged"], true);
        // The same process still runs (it has not exited yet): no exit is looked up for it.
        let (report, _) = previous_report(
            Ok(heartbeat()),
            &current,
            clock(1500, 1000),
            |_| Some(100),
            &history,
        );
        assert_eq!(report["pidState"], "still_running");
        assert_eq!(history.1.get(), 1);
        current.boot_id = Some("22222222-2222-2222-2222-222222222222".into());
        let (report, _) = previous_report(
            Ok(heartbeat()),
            &current,
            clock(0, 0),
            |_| panic!("a PID of another boot means nothing"),
            &history,
        );
        assert_eq!(report["pidState"], "previous_boot");
        assert!(report["lastSeenAgeMillis"].is_null());
        assert!(report["wallClockChanged"].is_null());
    }
    #[test]
    fn exit_is_attributed_only_to_that_invocation_and_never_to_the_current_one() {
        let attributed = History(Some(json!({"result":"oom-kill"})), Default::default());
        let mut current = identity();
        current.run = "run-two".into();
        current.invocation_id = Some("b".repeat(32));
        let (report, exit) = previous_report(
            Ok(heartbeat()),
            &current,
            clock(0, 0),
            |_| None,
            &attributed,
        );
        assert_eq!(report["exit"], "attributed");
        assert_eq!(exit.unwrap()["result"], "oom-kill");
        // Without systemd (no invocation ID on either side) nothing can be attributed.
        let mut previous = heartbeat();
        previous.identity.invocation_id = None;
        current.invocation_id = None;
        let (_, exit) = previous_report(Ok(previous), &current, clock(0, 0), |_| None, &attributed);
        assert!(exit.is_none());
        assert_eq!(attributed.1.get(), 1);
    }
    #[test]
    fn journal_rows_of_that_boot_unit_and_invocation_merge_into_one_exit() {
        let previous = identity();
        let unit = super::super::UNITS[0];
        let row = |fields: Value| {
            let mut row = json!({"_BOOT_ID":BOOT.replace('-', ""),"UNIT":unit,"INVOCATION_ID":"a".repeat(32),
                "_PID":"1","__REALTIME_TIMESTAMP":"1700000000000000","MESSAGE":"text is never copied"});
            row.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            row.to_string()
        };
        let rows = [
            row(json!({"MESSAGE_ID":"39f53479d3a045ac8e11786248231fbf"})),
            row(json!({"MESSAGE_ID":"fe6faa94e7774663a0da52717891d8ef"})),
            row(json!({"MESSAGE_ID":"98e322203f7a4ed290d09fe03c09fe15","EXIT_CODE":"killed","EXIT_STATUS":"9"})),
            row(json!({"MESSAGE_ID":"d9b373ed55a64feb8242e02dbe79a49c","UNIT_RESULT":"oom-kill","__REALTIME_TIMESTAMP":"1700000000000009"})),
            "not json".into(),
        ]
        .join("\n");
        let exit = attributed_exit(&previous, rows.as_bytes()).unwrap();
        assert_eq!(
            exit,
            json!({"pid":12,"invocationId":"a".repeat(32),"source":"journal","exitCode":"killed",
                "exitStatus":"9","result":"oom-kill","oomKill":true,"journalRealtime":"1700000000000009"})
        );
        assert!(!exit.to_string().contains("never copied"));
        let clean = row(json!({"MESSAGE_ID":UNIT_SUCCESS}));
        assert_eq!(
            attributed_exit(&previous, clean.as_bytes()).unwrap()["result"],
            "success"
        );
        for (key, other) in [
            ("INVOCATION_ID", "b".repeat(32)),
            ("_BOOT_ID", "2".repeat(32)),
            ("UNIT", "other.service".into()),
        ] {
            let mut row: Value = serde_json::from_str(&clean).unwrap();
            row[key] = other.into();
            assert!(attributed_exit(&previous, row.to_string().as_bytes()).is_none());
        }
        assert!(attributed_exit(&previous, b"").is_none());
    }
    #[test]
    fn proc_stat_handles_parentheses_in_comm_and_tasks_are_bounded() {
        let stat = format!("12 (a ) b) S {} 12345 0", vec!["0"; 18].join(" "));
        assert_eq!(stat_start(&stat), Some(12345));
        assert_eq!(stat_start("damaged"), None);
        assert_eq!(task(u32::MAX), json!({"state":null,"wchan":null}));
        #[cfg(target_os = "linux")]
        assert!(task(super::super::diagnostics::tid())["state"].is_string());
    }
}
