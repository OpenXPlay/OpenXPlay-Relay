// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Private, short-lived journal exports and explicit, single-attempt uploads.
pub(crate) mod http;
pub mod jobs;
pub mod journal;
pub mod redactor;
pub mod timeline;
mod traces;
pub mod upload;
mod zip;

use self::http::Http;
use redactor::Redactor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

#[cfg(target_os = "linux")]
const MAX_JOURNAL: usize = 32 * 1024 * 1024;
const RETENTION: u64 = 3_600_000;
pub const UNITS: &[&str] = &["openxplay-relay.service"];
#[derive(Clone, Copy, Debug, Deserialize)]
pub enum Range {
    #[serde(rename = "10m")]
    TenMinutes,
    #[serde(rename = "30m")]
    ThirtyMinutes,
    #[serde(rename = "24h")]
    Day,
}
impl Range {
    fn seconds(self) -> u64 {
        match self {
            Self::TenMinutes => 600,
            Self::ThirtyMinutes => 1800,
            Self::Day => 86400,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub id: String,
    pub bytes: u64,
    pub sha256: String,
    pub created_millis: u64,
    pub first_millis: Option<u64>,
    pub last_millis: Option<u64>,
    pub empty: bool,
    pub truncated: bool,
}
pub struct Download {
    pub file: File,
    pub bytes: u64,
    pub name: String,
}
pub struct JournalSlice {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}
pub trait Journal: Send + Sync {
    fn read(&self, since: u64, until: u64) -> io::Result<JournalSlice>;
}
pub struct SystemJournal;
impl Journal for SystemJournal {
    fn read(&self, since: u64, until: u64) -> io::Result<JournalSlice> {
        #[cfg(target_os = "linux")]
        {
            let mut command = std::process::Command::new("journalctl");
            for unit in UNITS {
                command.args(["-u", unit]);
            }
            command.args([format!("--since=@{since}"), format!("--until=@{until}")]);
            command.args(["-o", "short-iso-precise", "--no-pager"]);
            bounded_command(command, MAX_JOURNAL, Duration::from_secs(20))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Journal export requires Linux",
            ))
        }
    }
}
#[cfg(any(target_os = "linux", test))]
fn bounded_command(
    mut command: std::process::Command,
    limit: usize,
    timeout: Duration,
) -> io::Result<JournalSlice> {
    use std::{process::Stdio, sync::mpsc, time::Instant};
    let mut child = command
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + timeout;
    let result = 'outer_result: {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(mut bytes)) if bytes.len() > limit => {
                bytes.truncate(limit);
                // Discard the cut line: it could end halfway through a private value.
                bytes.truncate(bytes.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1));
                break 'outer_result Ok(JournalSlice {
                    bytes,
                    truncated: true,
                });
            }
            Ok(Ok(bytes)) => loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        break 'outer_result status
                            .success()
                            .then_some(JournalSlice {
                                bytes,
                                truncated: false,
                            })
                            .ok_or_else(|| io::Error::other("journalctl failed"))
                    }
                    Err(e) => break 'outer_result Err(e),
                    _ if Instant::now() >= deadline => {
                        break 'outer_result Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "journalctl timeout",
                        ))
                    }
                    _ => std::thread::sleep(Duration::from_millis(5)),
                }
            },
            Ok(Err(e)) => break 'outer_result Err(e),
            Err(_) => {
                break 'outer_result Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "journalctl timeout",
                ))
            }
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    result
}

pub struct Service {
    state: PathBuf,
    journal: Arc<dyn Journal>,
    http: Arc<dyn Http>,
    gate: Mutex<()>,
    jobs: Mutex<std::collections::VecDeque<Arc<jobs::Transfer>>>,
    cleanup: Arc<(Mutex<bool>, Condvar)>,
}
impl Drop for Service {
    fn drop(&mut self) {
        for job in self.jobs.lock().unwrap().iter() {
            job.cancel();
        }
        let mut closed = self.cleanup.0.lock().unwrap();
        *closed = true;
        self.cleanup.1.notify_all();
    }
}
impl Service {
    pub fn new(state: PathBuf) -> Self {
        Self::with_sources(
            state,
            Arc::new(SystemJournal),
            Arc::new(http::Client::new(
                Duration::from_secs(180),
                64 * 1024,
                upload::USER_AGENT,
            )),
        )
    }
    pub fn with_sources(state: PathBuf, journal: Arc<dyn Journal>, http: Arc<dyn Http>) -> Self {
        let cleanup = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = cleanup.clone();
        let dir = state.join("log-exports");
        // Wake at the next expiry, including files left by a previous relay process.
        let _ = std::thread::Builder::new()
            .name("log-export-retention".into())
            .spawn(move || {
                let mut closed = signal.0.lock().unwrap();
                while !*closed {
                    let delay = prune(&dir, now_millis(), 2).unwrap_or(RETENTION).max(1);
                    closed = signal
                        .1
                        .wait_timeout(closed, Duration::from_millis(delay))
                        .unwrap()
                        .0;
                }
            });
        Self {
            state,
            journal,
            http,
            gate: Mutex::new(()),
            jobs: Mutex::default(),
            cleanup,
        }
    }
    pub fn export(
        &self,
        range: Range,
        info: &str,
        known: &[(String, &str)],
        timeline: &str,
    ) -> io::Result<Export> {
        let _gate = self.gate.lock().unwrap();
        let now = now_millis();
        let dir = self.state.join("log-exports");
        fs::create_dir_all(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        prune(&dir, now, 1)?;
        let mut redactor = Redactor::new(token_key(&self.state)?);
        for (value, kind) in known {
            redactor.remember(value, kind);
        }
        let journal = self
            .journal
            .read((now / 1000).saturating_sub(range.seconds()), now / 1000)?;
        let journal_text = String::from_utf8_lossy(&journal.bytes);
        // The journal predates the export: learn keyed values before filtering earlier bare uses.
        for line in journal_text.lines() {
            redactor.redact(line);
        }
        redactor.reset_stream();
        let mut log = redact_lines(&mut redactor, &journal_text);
        let log_limited = cap_lines(&mut log, 32 * 1024 * 1024);
        if journal.truncated || log_limited {
            log.push_str("[Journal output limited to 32 MiB; later lines omitted]\n");
        }
        let since = now.saturating_sub(range.seconds() * 1000);
        let mut timeline_text = journal_timeline(&log);
        if !timeline.is_empty() {
            let selected = timeline
                .lines()
                .filter(|line| {
                    line.split_whitespace()
                        .next()
                        .and_then(|v| v.parse::<u64>().ok())
                        .is_some_and(|at| at >= since && at <= now)
                })
                .collect::<Vec<_>>()
                .join("\n");
            timeline_text.push_str(&redact_lines(&mut redactor, &selected));
        }
        let timeline_limited = cap_lines(&mut timeline_text, 8 * 1024 * 1024);
        let traces = traces::collect(
            &self.state.join("carlink-trace"),
            since,
            now,
            (48 * 1024 * 1024_usize).saturating_sub(log.len() + timeline_text.len() + 64 * 1024),
            &mut redactor,
        )?;
        let times: Vec<_> = log.lines().filter_map(line_time).collect();
        let first_millis = times.iter().copied().chain(traces.first).min();
        let last_millis = times.iter().copied().chain(traces.last).max();
        let empty = times.is_empty()
            && traces.entries.is_empty()
            && journal_text
                .lines()
                .all(|line| line.trim().is_empty() || line.starts_with("-- "));
        let truncated = journal.truncated || log_limited || timeline_limited || traces.omitted > 0;
        let info = redact_lines(&mut redactor, &format!("OpenXPlay Linux {}\nExported: {now} ms since Unix epoch\nSelected range: {since}..{now} ms\nFirst line: {first_millis:?}\nLast line: {last_millis:?}\nEmpty range: {empty}\nJournal limited: {}\nTrace segments omitted (legacy or size cap): {}\n{}\n{info}\nLogs are stored in this relay's system journal; its retention is controlled by the system. Private values are redacted on export. Network names and addresses are retained. Tokens are installation-scoped. Complete overlapping trace segments may extend beyond the selected range.\n", env!("CARGO_PKG_VERSION"), truncated, traces.omitted, build_details()));
        let mut owned = vec![
            ("relay.log".to_owned(), log.into_bytes()),
            ("info.txt".into(), info.into_bytes()),
            ("timeline.txt".into(), timeline_text.into_bytes()),
        ];
        owned.extend(traces.entries);
        let entries: Vec<_> = owned
            .iter()
            .map(|(name, body)| (name.as_str(), body.as_slice()))
            .collect();
        let zip = zip::archive(&entries, now / 1000)?;
        if zip.len() > upload::MAX_FILE_BYTES {
            return Err(io::Error::other("Archive exceeds 50 MiB"));
        }
        let id = format!("{now}-{}", crate::admin::auth::random_hex(16)?);
        crate::platform::private_state_file(&dir.join(format!("{id}.zip")), &zip)?;
        prune(&dir, now, 2)?;
        let _retention_guard = self.cleanup.0.lock().unwrap();
        self.cleanup.1.notify_all();
        Ok(Export {
            id,
            bytes: zip.len() as u64,
            sha256: upload::sha256(&zip),
            created_millis: now,
            first_millis,
            last_millis,
            empty,
            truncated,
        })
    }
    pub fn download(&self, id: &str) -> io::Result<Download> {
        let _gate = self.gate.lock().unwrap();
        self.open_export(id, now_millis())
    }
    fn open_export(&self, id: &str, now: u64) -> io::Result<Download> {
        let time = export_time(id).ok_or(io::ErrorKind::NotFound)?;
        let dir = self.state.join("log-exports");
        prune(&dir, now, 2)?;
        if time > now || now - time >= RETENTION {
            return Err(io::ErrorKind::NotFound.into());
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(dir.join(format!("{id}.zip")))?;
        if !file.metadata()?.is_file() {
            return Err(io::ErrorKind::NotFound.into());
        }
        let bytes = file.metadata()?.len();
        Ok(Download {
            file,
            bytes,
            name: format!("openxplay-linux-logs-{time}.zip"),
        })
    }
    pub fn upload(&self, id: &str) -> Result<jobs::Status, upload::UploadError> {
        let _gate = self.gate.lock().unwrap();
        let mut jobs = self.jobs.lock().unwrap();
        if jobs.iter().any(|job| !job.worker_done()) {
            return Err(upload::UploadError::Rejected);
        }
        let download = self
            .open_export(id, now_millis())
            .map_err(|_| upload::UploadError::Rejected)?;
        if download.bytes > upload::MAX_FILE_BYTES as u64 {
            return Err(upload::UploadError::TooLarge);
        }
        let transfer = jobs::Transfer::new(
            crate::admin::auth::random_hex(16).map_err(|_| upload::UploadError::Rejected)?,
            id.into(),
            download.bytes,
        );
        let job = transfer.clone();
        let http = self.http.clone();
        let state = self.state.clone();
        std::thread::Builder::new()
            .name("log-upload".into())
            .spawn(move || {
                let result = (|| {
                    if job.cancelled() {
                        return Err(upload::UploadError::Rejected);
                    }
                    let mut bytes = Vec::new();
                    download
                        .file
                        .take(upload::MAX_FILE_BYTES as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| upload::UploadError::Rejected)?;
                    let identity =
                        upload::user_id(&state).map_err(|_| upload::UploadError::Rejected)?;
                    if job.cancelled() {
                        return Err(upload::UploadError::Rejected);
                    }
                    upload::upload(
                        &job,
                        http.as_ref(),
                        &bytes,
                        &download.name,
                        &identity,
                        env!("CARGO_PKG_VERSION"),
                    )
                })();
                if let Ok(receipt) = &result {
                    let _ = upload::save(&state, receipt);
                }
                job.finish(result);
            })
            .map_err(|_| upload::UploadError::Rejected)?;
        let status = transfer.status();
        if jobs.len() == 8 {
            jobs.pop_front();
        }
        jobs.push_back(transfer);
        Ok(status)
    }
    pub fn upload_status(&self, id: &str, cancel: bool) -> Option<jobs::Status> {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .find(|job| job.status().id == id)
            .map(|job| if cancel { job.cancel() } else { job.status() })
    }
    pub fn receipt(&self) -> io::Result<Option<upload::Receipt>> {
        upload::latest(&self.state, now_millis())
    }
}
pub fn now_millis() -> u64 {
    crate::platform::wall_clock()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
pub(crate) fn read_private(path: &Path, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Not a regular state file"));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::other("State file exceeds limit"));
    }
    Ok(Some(bytes))
}
fn token_key(state: &Path) -> io::Result<[u8; 32]> {
    let path = state.join("log-token.key");
    if let Some(bytes) = read_private(&path, 32)? {
        return bytes
            .try_into()
            .map_err(|_| io::Error::other("Invalid log token key"));
    }
    let mut bytes = [0; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    crate::platform::private_state_file(&path, &bytes)?;
    Ok(bytes)
}
fn export_time(id: &str) -> Option<u64> {
    let (time, random) = id.split_once('-')?;
    if time.len() > 20
        || time.is_empty()
        || !time.bytes().all(|b| b.is_ascii_digit())
        || random.len() != 32
        || !random
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return None;
    }
    time.parse().ok()
}
fn prune(dir: &Path, now: u64, keep: usize) -> io::Result<u64> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(RETENTION),
        Err(e) => return Err(e),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(time) = name.strip_suffix(".zip").and_then(export_time) else {
            continue;
        };
        if entry.file_type()?.is_file() {
            files.push((time, entry.path()));
        }
    }
    files.sort();
    let mut next = RETENTION;
    let excess = files.len().saturating_sub(keep);
    for (index, (time, path)) in files.into_iter().enumerate() {
        if time > now || now - time >= RETENTION || index < excess {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        } else {
            next = next.min(RETENTION - (now - time));
        }
    }
    Ok(next)
}
fn cap_lines(text: &mut String, limit: usize) -> bool {
    if text.len() <= limit {
        return false;
    }
    let end = text.as_bytes()[..limit]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |n| n + 1);
    text.truncate(end);
    true
}
fn redact_lines(redactor: &mut Redactor, text: &str) -> String {
    let mut result = String::new();
    for line in text.lines() {
        result.push_str(&redactor.redact(line));
        result.push('\n');
    }
    result
}
/// Runtime lane hook: append its bounded recent connection timeline here via RelayAdmin.
pub fn journal_timeline(redacted_journal: &str) -> String {
    let mut timeline = String::from(
        "Recent controller, iAP2, AirPlay and Carlink events from the relay journal\n",
    );
    for line in redacted_journal.lines() {
        let lower = line.to_ascii_lowercase();
        if [
            "stage",
            "controller",
            "iap2",
            "carlink",
            "clink",
            "startup",
            "session",
            "first_video",
            "pair_verified",
            "boot_timing",
            "timeline",
        ]
        .iter()
        .any(|key| lower.contains(key))
        {
            timeline.push_str(line);
            timeline.push('\n');
        }
    }
    timeline
}
fn line_time(line: &str) -> Option<u64> {
    let first = line.split_whitespace().next()?;
    let normalized = if first.len() >= 5
        && first.as_bytes()[first.len() - 5..]
            .first()
            .is_some_and(|b| matches!(b, b'+' | b'-'))
        && first.as_bytes()[first.len() - 4..]
            .iter()
            .all(u8::is_ascii_digit)
    {
        format!(
            "{}:{}",
            &first[..first.len() - 2],
            &first[first.len() - 2..]
        )
    } else {
        first.to_owned()
    };
    upload::rfc3339_millis(&normalized).or_else(|| {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| v["time"].as_str().and_then(upload::rfc3339_millis))
    })
}

fn build_details() -> String {
    let os = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let os = os
        .lines()
        .filter(|line| {
            ["PRETTY_NAME=", "VERSION_ID=", "ID="]
                .iter()
                .any(|key| line.starts_with(key))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    format!(
        "OS: {}\nArchitecture: {}\nKernel: {}\n{os}\nBuild: {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        kernel.trim(),
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    struct JournalFake(Mutex<Vec<(u64, u64)>>);
    impl Journal for JournalFake {
        fn read(&self, since: u64, until: u64) -> io::Result<JournalSlice> {
            self.0.lock().unwrap().push((since, until));
            Ok(JournalSlice {bytes:b"bare PrivateWifi topsecret\ncontroller stage=connecting ssid=PrivateWifi password=topsecret\ncarlink state=playing public=8.8.8.8:7000\n".to_vec(),truncated:false})
        }
    }
    fn temp() -> PathBuf {
        std::env::temp_dir().join(format!(
            "relay-logs-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ))
    }
    #[test]
    fn export_assembly_retention_permissions_trace_and_install_identity() {
        let dir = temp();
        let journal = Arc::new(JournalFake(Mutex::default()));
        let service = Service::with_sources(
            dir.clone(),
            journal.clone(),
            Arc::new(http::Client::new(Duration::from_secs(1), 65536, "test")),
        );
        crate::platform::private_state_file(
            &dir.join("carlink-trace/trace-00000001-0-00000001.jsonl"),
            format!("{{\"type\":\"phase\",\"atMillis\":{},\"codec\":\"h264\",\"service\":\"sensor\",\"payload\":\"private-gps\"}}\n", now_millis()).as_bytes(),
        )
        .unwrap();
        let export = service
            .export(
                Range::TenMinutes,
                "model=Test",
                &[],
                &format!("{} runtime stage=ready", now_millis()),
            )
            .unwrap();
        let mut file = service.download(&export.id).unwrap().file;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(upload::sha256(&bytes), export.sha256);
        assert_eq!(bytes.len() as u64, export.bytes);
        let entries = zip::unzip(&bytes);
        assert_eq!(
            entries.iter().map(|e| e.0.as_str()).collect::<Vec<_>>(),
            [
                "relay.log",
                "info.txt",
                "timeline.txt",
                "carlink-trace/trace-00000001-0-00000001.jsonl"
            ]
        );
        let text = entries
            .iter()
            .map(|(_, b)| String::from_utf8_lossy(b))
            .collect::<Vec<_>>()
            .join("\n");
        for private in ["topsecret", "private-gps"] {
            assert!(!text.contains(private));
        }
        assert!(text.contains("runtime stage=ready"));
        assert!(text.contains("h264"));
        assert!(text.contains("PrivateWifi"));
        assert!(text.contains("8.8.8.8"));
        let times = journal.0.lock().unwrap();
        assert_eq!(times[0].1 - times[0].0, 600);
        drop(times);
        let identity = upload::user_id(&dir).unwrap();
        assert_eq!(upload::user_id(&dir).unwrap(), identity);
        for path in [
            dir.join("log-upload-user-id"),
            dir.join("log-token.key"),
            dir.join(format!("log-exports/{}.zip", export.id)),
        ] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(
            fs::metadata(dir.join("log-exports"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        service.export(Range::Day, "", &[], "").unwrap();
        service.export(Range::ThirtyMinutes, "", &[], "").unwrap();
        assert_eq!(fs::read_dir(dir.join("log-exports")).unwrap().count(), 2);
        assert!(service.download("../settings").is_err());
        prune(&dir.join("log-exports"), now_millis() + RETENTION, 2).unwrap();
        assert_eq!(fs::read_dir(dir.join("log-exports")).unwrap().count(), 0);
        drop(service);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn coverage_accepts_journald_timezone_offsets_and_json_clock() {
        assert_eq!(line_time("+中x"), None);
        assert_eq!(
            line_time("2026-10-27T12:00:00.123456+0800 host service: message"),
            upload::rfc3339_millis("2026-10-27T04:00:00.123Z")
        );
        assert_eq!(
            line_time(r#"{"time":"2026-10-27T12:00:00.123Z","event":"stage"}"#),
            upload::rfc3339_millis("2026-10-27T12:00:00.123Z")
        );
    }
    #[test]
    fn journal_command_caps_output_and_times_out() {
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "printf 'one\ntwo\nthree\n'"]);
        let slice = bounded_command(command, 9, Duration::from_secs(1)).unwrap();
        assert_eq!(slice.bytes, b"one\ntwo\n");
        assert!(slice.truncated);
        let mut command = std::process::Command::new("sh");
        command.args(["-c", "exec sleep 2"]);
        assert_eq!(
            bounded_command(command, 10, Duration::from_millis(20))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;
    use crate::public_name::http::{Failure, Request, Response};
    use std::sync::mpsc;
    struct Empty;
    impl Journal for Empty {
        fn read(&self, _: u64, _: u64) -> io::Result<JournalSlice> {
            Ok(JournalSlice {
                bytes: b"-- No entries --\n".to_vec(),
                truncated: false,
            })
        }
    }
    struct Controlled {
        ready: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        receipt: Mutex<Value>,
    }
    impl Http for Controlled {
        fn send(
            &self,
            _: Request,
            transfer: &Arc<jobs::Transfer>,
            _: std::ops::Range<usize>,
        ) -> Result<Response, Failure> {
            let total = transfer.status().total;
            for (sent, awaiting) in [(0, false), (total, false), (total, true)] {
                transfer.progress(sent, awaiting);
                self.ready.send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
            }
            Ok(Response {
                status: 201,
                body: serde_json::to_vec(&*self.receipt.lock().unwrap()).unwrap(),
                ..Default::default()
            })
        }
    }
    #[test]
    fn background_job_keeps_zip_and_late_receipt_on_cancel_without_repeating_upload() {
        let dir = std::env::temp_dir().join(format!(
            "log-job-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let (ready, wait) = mpsc::channel();
        let (next, release) = mpsc::channel();
        let http = Arc::new(Controlled {
            ready,
            release: Mutex::new(release),
            receipt: Mutex::new(Value::Null),
        });
        let service = Service::with_sources(dir.clone(), Arc::new(Empty), http.clone());
        let export = service.export(Range::TenMinutes, "", &[], "").unwrap();
        assert!(export.empty);
        assert_eq!(export.first_millis, None);
        assert_eq!(export.last_millis, None);
        let identity = upload::user_id(&dir).unwrap();
        *http.receipt.lock().unwrap() = serde_json::json!({"id":"a".repeat(32),"user_id":identity,"bytes":export.bytes,"sha256":export.sha256,
            "tracking_url":format!("{}/r/{}",upload::ORIGIN,"a".repeat(32)), "qr_code_url":format!("{}/qr/{}.png",upload::ORIGIN,"a".repeat(32)),"expires_at":"2099-01-01T00:00:00Z"});
        let job = service.upload(&export.id).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            service.upload_status(&job.id, false).unwrap().phase,
            "Sending"
        );
        assert!(service.upload(&export.id).is_err());
        next.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        let sending = service.upload_status(&job.id, false).unwrap();
        assert_eq!(sending.sent, export.bytes);
        assert!(sending.receipt.is_none());
        assert_eq!(sending.state, "running");
        next.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            service.upload_status(&job.id, false).unwrap().phase,
            "AwaitingReceipt"
        );
        assert_eq!(
            service.upload_status(&job.id, true).unwrap().state,
            "cancelled"
        );
        let retained = service.download(&export.id).unwrap();
        assert_eq!(retained.bytes, export.bytes);
        next.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while service
            .upload_status(&job.id, false)
            .unwrap()
            .receipt
            .is_none()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(service.receipt().unwrap().is_some());
        assert_eq!(
            service.upload_status(&job.id, false).unwrap().state,
            "cancelled"
        );
        assert!(service.upload_status("invalid", true).is_none());
        drop(service);
        fs::remove_dir_all(dir).unwrap();
    }
}
