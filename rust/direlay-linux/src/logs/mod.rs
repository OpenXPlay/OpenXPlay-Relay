// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Private, short-lived journal exports and explicit, single-attempt uploads.
pub(crate) mod http;
pub mod redactor;
pub mod upload;
mod zip;

use crate::public_name::http::Http;
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
    cleanup: Arc<(Mutex<bool>, Condvar)>,
}
impl Drop for Service {
    fn drop(&mut self) {
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
        if journal.truncated {
            log.push_str("[Journal output limited to 32 MiB; later lines omitted]\n");
        }
        let mut timeline_text = journal_timeline(&log);
        if !timeline.is_empty() {
            timeline_text.push_str(&redact_lines(&mut redactor, timeline));
        }
        let info = redact_lines(&mut redactor,&format!("OpenXPlay Linux {}\nExported: {now} ms since Unix epoch\nRange: {} seconds\n{info}\nPrivate values use install-scoped HMAC-SHA256 tokens; matching tokens identify matching values without revealing them.\n",env!("CARGO_PKG_VERSION"),range.seconds()));
        let summary = latest_trace_summary(&self.state.join("carlink-trace"), &mut redactor)?;
        let mut entries = vec![
            ("relay.log", log.as_bytes()),
            ("info.txt", info.as_bytes()),
            ("timeline.txt", timeline_text.as_bytes()),
        ];
        if let Some(summary) = &summary {
            entries.push(("carlink-summary.txt", summary.as_bytes()));
        }
        let zip = zip::archive(&entries, now / 1000)?;
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
    pub fn upload(&self, id: &str) -> Result<upload::Receipt, upload::UploadError> {
        let _gate = self.gate.lock().unwrap();
        let download = self
            .open_export(id, now_millis())
            .map_err(|_| upload::UploadError::Rejected)?;
        if download.bytes > upload::MAX_FILE_BYTES as u64 {
            return Err(upload::UploadError::TooLarge);
        }
        let mut bytes = Vec::new();
        download
            .file
            .take(upload::MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| upload::UploadError::Rejected)?;
        let identity = upload::user_id(&self.state).map_err(|_| upload::UploadError::Rejected)?;
        let receipt = upload::upload(
            self.http.as_ref(),
            &bytes,
            &download.name,
            &identity,
            env!("CARGO_PKG_VERSION"),
        )?;
        // Like Android, a validated receipt is useful even if the state disk has filled up.
        let _ = upload::save(&self.state, &receipt);
        Ok(receipt)
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
fn latest_trace_summary(dir: &Path, redactor: &mut Redactor) -> io::Result<Option<String>> {
    let entries = crate::admin::server::trace_files::listing(dir)?;
    let Some(entry) = entries.iter().rev().find(|v| v["kind"] == "trace") else {
        return Ok(None);
    };
    let name = entry["name"].as_str().unwrap();
    let mut file = crate::admin::server::trace_files::open(dir, name)?;
    use std::io::{Seek, SeekFrom};
    let length = crate::admin::server::trace_files::snapshot_length(&mut file, true)?;
    let start = length.saturating_sub(256 * 1024);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(length - start).read_to_end(&mut bytes)?;
    let bytes = if start > 0 {
        bytes.splitn(2, |b| *b == b'\n').nth(1).unwrap_or(&[])
    } else {
        &bytes
    };
    let mut summary = format!("Latest Carlink trace: {name}\nSummary of complete records in the last 256 KiB; payloads excluded.\n");
    // An allowlist prevents traces' raw sensor/control payloads from entering exports.
    for line in String::from_utf8_lossy(bytes).lines() {
        if let Ok(Value::Object(value)) = serde_json::from_str::<Value>(line) {
            let safe: serde_json::Map<String, Value> = value
                .into_iter()
                .filter(|(key, value)| {
                    [
                        "type",
                        "event",
                        "time",
                        "timestamp",
                        "atNanos",
                        "session",
                        "stage",
                        "direction",
                        "millis",
                        "elapsed",
                        "phase",
                        "state",
                        "code",
                        "reason",
                        "codec",
                        "width",
                        "height",
                        "joinMethod",
                        "model",
                        "modelId",
                        "duration",
                    ]
                    .contains(&key.as_str())
                        && !value.is_object()
                        && !value.is_array()
                })
                .collect();
            if !safe.is_empty() {
                summary.push_str(&redactor.redact(&Value::Object(safe).to_string()));
                summary.push('\n');
            }
        }
    }
    Ok(Some(summary))
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
            b"{\"type\":\"phase\",\"codec\":\"h264\",\"payload\":\"private-gps\"}\n",
        )
        .unwrap();
        let export = service
            .export(Range::TenMinutes, "model=Test", &[], "runtime stage=ready")
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
                "carlink-summary.txt"
            ]
        );
        let text = entries
            .iter()
            .map(|(_, b)| String::from_utf8_lossy(b))
            .collect::<Vec<_>>()
            .join("\n");
        for private in ["PrivateWifi", "topsecret", "8.8.8.8", "private-gps"] {
            assert!(!text.contains(private));
        }
        assert!(text.contains("runtime stage=ready"));
        assert!(text.contains("h264"));
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
