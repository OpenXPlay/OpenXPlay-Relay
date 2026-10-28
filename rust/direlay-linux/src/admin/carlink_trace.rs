// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-22

use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::Mutex,
    time::UNIX_EPOCH,
};

#[derive(Default)]
pub struct TraceStatus {
    status: Mutex<Value>,
    error: Mutex<Option<Value>>,
}
impl TraceStatus {
    /// Status never reaches the timeline. Errors remain eligible for normal logging.
    pub fn observe(&self, event: &Value) -> bool {
        if event["type"] == "receiver-starting" || event["type"] == "receiver-stopped" {
            self.reset();
        }
        if event["type"] != "clink-trace" {
            return false;
        }
        if event.get("error").is_some() {
            let mut last = self.error.lock().unwrap();
            if last.as_ref() == Some(event) {
                return true;
            }
            *last = Some(event.clone());
            return false;
        }
        *self.status.lock().unwrap() = event.clone();
        true
    }
    pub fn get(&self) -> Value {
        self.status.lock().unwrap().clone()
    }
    pub fn reset(&self) {
        *self.status.lock().unwrap() = Value::Null;
        *self.error.lock().unwrap() = None;
    }
}

pub fn kind(name: &str) -> Option<&'static str> {
    let (body, kind) = if let Some(body) = name
        .strip_prefix("trace-")
        .and_then(|n| n.strip_suffix(".jsonl"))
    {
        (body, "trace")
    } else if let Some(body) = name
        .strip_prefix("media-")
        .and_then(|n| n.strip_suffix(".ts.part"))
    {
        (body, "media-partial")
    } else {
        (name.strip_prefix("media-")?.strip_suffix(".ts")?, "media")
    };
    let fields: Vec<_> = body.split('-').collect();
    if fields.len() != 3
        || fields[0].len() != 8
        || fields[if kind == "trace" { 2 } else { 1 }].len() != 8
        || fields
            .iter()
            .any(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(kind)
}

pub fn listing(dir: &Path) -> io::Result<Vec<Value>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(kind) = kind(&name) else {
            continue;
        };
        if !entry.file_type()?.is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|t| t.as_secs());
        files.push(
            json!({"name": name, "kind": kind, "size": metadata.len(), "modified": modified}),
        );
    }
    files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(files)
}

/// Do not follow symlinks or open special files, even if their names look like recordings.
pub fn open(dir: &Path, name: &str) -> io::Result<File> {
    if !matches!(kind(name), Some("trace" | "media")) {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(dir.join(name))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    Ok(file)
}

/// Snapshot complete records with at most 64 KiB of backward reads per file.
pub fn snapshot_length(file: &mut File, trace: bool) -> io::Result<u64> {
    let length = file.metadata()?.len();
    if !trace {
        return Ok(length);
    }
    let floor = length.saturating_sub(64 * 1024);
    let mut end = length;
    let mut buffer = [0; 4096];
    let trimmed = loop {
        if end == floor {
            // No boundary in the window: discard the trailing bytes we inspected.
            break floor;
        }
        let start = end.saturating_sub(buffer.len() as u64).max(floor);
        let count = (end - start) as usize;
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer[..count])?;
        if let Some(index) = buffer[..count].iter().rposition(|b| *b == b'\n') {
            break start + index as u64 + 1;
        }
        end = start;
    };
    file.seek(SeekFrom::Start(0))?;
    Ok(trimmed)
}

pub fn clear(dir: &Path) -> io::Result<()> {
    for file in listing(dir)? {
        match fs::remove_file(dir.join(file["name"].as_str().unwrap())) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub fn tail(dir: &Path, filter: Option<&str>, limit: usize) -> io::Result<Vec<String>> {
    let mut budget = 128 * 1024 * 1024_u64;
    let limit = limit.min(5000);
    let mut lines = Vec::new();
    for entry in listing(dir)?
        .into_iter()
        .rev()
        .filter(|e| e["kind"] == "trace")
    {
        if budget == 0 || lines.len() >= limit {
            break;
        }
        let mut file = match open(dir, entry["name"].as_str().unwrap()) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let length = file.metadata()?.len();
        let count = length.min(budget);
        let start = length - count;
        file.seek(SeekFrom::Start(start))?;
        let mut data = Vec::with_capacity(count as usize);
        file.take(count).read_to_end(&mut data)?;
        budget -= count;
        // A cut at the beginning of the window is not a complete JSONL record.
        let data = if start > 0 {
            data.splitn(2, |b| *b == b'\n').nth(1).unwrap_or(&[])
        } else {
            &data
        };
        for line in data.split(|b| *b == b'\n').rev() {
            if lines.len() >= limit {
                break;
            }
            let Ok(raw) = std::str::from_utf8(line) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<Value>(raw) else {
                continue;
            };
            if value.is_object() && filter.is_none_or(|kind| value["type"] == kind) {
                lines.push(raw.to_owned());
            }
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "relay-trace-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn snapshot_trims_partial_lines_with_a_bounded_scan() {
        let dir = temp();
        let name = "trace-00000001-0-00000001.jsonl";
        for (data, expected) in [
            (Vec::new(), 0),
            (b"partial".to_vec(), 0),
            (b"{}\npartial".to_vec(), 3),
            (b"{}\n".to_vec(), 3),
            ([b"{}\n".as_slice(), &vec![b'x'; 8192]].concat(), 3),
            (vec![b'x'; 65537], 1),
        ] {
            fs::write(dir.join(name), &data).unwrap();
            let mut file = open(&dir, name).unwrap();
            assert_eq!(snapshot_length(&mut file, true).unwrap(), expected);
            assert_eq!(file.stream_position().unwrap(), 0);
            assert_eq!(
                snapshot_length(&mut file, false).unwrap(),
                data.len() as u64
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn names_reject_paths_partials_and_unrelated_files() {
        for name in [
            "../trace-00000001-0-00000001.jsonl",
            "trace-1-0-00000001.jsonl",
            "trace-00000001-x-00000001.jsonl",
            "other-00000001-0-00000001.jsonl",
            "media-00000001-00000001-0.ts/extra",
        ] {
            assert_eq!(kind(name), None, "{name}");
        }
        assert_eq!(kind("trace-00000001-0-00000001.jsonl"), Some("trace"));
        assert_eq!(
            kind("media-00000001-00000001-0.ts.part"),
            Some("media-partial")
        );
    }
    #[test]
    fn tail_orders_filters_limits_and_keeps_5000_across_segments() {
        let dir = temp();
        let old = "trace-00000001-9999999-00000001.jsonl";
        let new = "trace-00000002-0-00000001.jsonl";
        fs::write(dir.join(old), "{\"type\":\"Error\",\"n\":0}\n").unwrap();
        fs::write(dir.join(new), "{\"type\":\"Link\",\"n\":1}\n{\"type\":\"Error\",\"n\":2}\ninvalid\n{\"type\":\"Media\",\"n\":3}\n").unwrap();
        let all = tail(&dir, None, 500).unwrap();
        assert_eq!(all.len(), 4);
        assert!(all[0].contains("3"));
        assert!(all[3].contains("0"));
        assert_eq!(tail(&dir, Some("Link"), 500).unwrap().len(), 1);
        assert_eq!(tail(&dir, Some("Error"), 1).unwrap(), [all[1].clone()]);
        assert!(tail(&dir, None, 0).unwrap().is_empty());
        let end = "\n{\"type\":\"Media\"}\n";
        fs::write(dir.join(new), format!("{}{}", "x".repeat(1024 * 1024), end)).unwrap();
        assert_eq!(
            tail(&dir, None, 500).unwrap(),
            ["{\"type\":\"Media\"}", "{\"type\":\"Error\",\"n\":0}"]
        );
        let records = (0..5001)
            .map(|n| format!("{{\"type\":\"Link\",\"n\":{n}}}\n"))
            .collect::<String>();
        fs::write(dir.join(new), records).unwrap();
        let records = tail(&dir, None, 6000).unwrap();
        assert_eq!(records.len(), 5000);
        assert_eq!(
            serde_json::from_str::<Value>(&records[0]).unwrap()["n"],
            5000
        );
        assert_eq!(
            serde_json::from_str::<Value>(&records[4999]).unwrap()["n"],
            1
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn listing_and_clear_include_partials_but_never_follow_symlinks() {
        let dir = temp();
        let trace = "trace-00000001-0-00000001.jsonl";
        let partial = "media-00000001-00000001-0.ts.part";
        fs::write(dir.join(trace), b"one").unwrap();
        fs::write(dir.join(partial), b"partial").unwrap();
        fs::write(dir.join("unrelated"), b"keep").unwrap();
        std::os::unix::fs::symlink(
            dir.join("unrelated"),
            dir.join("media-00000001-00000001-0.ts"),
        )
        .unwrap();
        assert_eq!(listing(&dir).unwrap().len(), 2);
        assert!(open(&dir, partial).is_err());
        assert!(open(&dir, "media-00000001-00000001-0.ts").is_err());
        clear(&dir).unwrap();
        assert!(dir.join("unrelated").exists());
        assert!(listing(&dir).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn status_events_stay_out_of_log_and_reset_between_children() {
        let status = TraceStatus::default();
        assert!(status.observe(&json!({"type":"clink-trace","lines":42})));
        assert_eq!(status.get()["lines"], 42);
        assert!(!status.observe(&json!({"type":"stats"})));
        assert!(!status.observe(&json!({"type":"clink-trace","error":"trace_off"})));
        assert!(status.observe(&json!({"type":"clink-trace","error":"trace_off"})));
        assert_eq!(status.get()["lines"], 42);
        status.observe(&json!({"type":"receiver-starting"}));
        assert!(status.get().is_null());
    }
}
