// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Complete newline-terminated segments whose wall-clock coverage overlaps the selection.
use super::{redactor::Redactor, upload::rfc3339_millis};
use serde_json::Value;
use std::{
    io::{self, Read},
    path::Path,
};

pub(super) struct Traces {
    pub entries: Vec<(String, Vec<u8>)>,
    pub first: Option<u64>,
    pub last: Option<u64>,
    pub omitted: usize,
}
pub(super) fn collect(
    dir: &Path,
    since: u64,
    until: u64,
    mut budget: usize,
    redactor: &mut Redactor,
) -> io::Result<Traces> {
    let mut result = Traces {
        entries: Vec::new(),
        first: None,
        last: None,
        omitted: 0,
    };
    if std::fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Ok(result);
    }
    for entry in crate::admin::server::trace_files::listing(dir)?
        .into_iter()
        .rev()
        .filter(|e| e["kind"] == "trace")
    {
        let name = entry["name"].as_str().unwrap();
        let mut file = match crate::admin::server::trace_files::open(dir, name) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let length = crate::admin::server::trace_files::snapshot_length(&mut file, true)?;
        if length == 0 {
            continue;
        }
        if length > 48 * 1024 * 1024 {
            result.omitted += 1;
            continue;
        }
        let mut bytes = Vec::new();
        file.take(length).read_to_end(&mut bytes)?;
        let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else {
            continue;
        };
        let mut records = Vec::new();
        let mut first = u64::MAX;
        let mut last = 0;
        let mut legacy = false;
        for line in bytes[..end].split(|b| *b == b'\n') {
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                legacy = true;
                break;
            };
            let at = value["atMillis"]
                .as_u64()
                .or_else(|| value["timestamp"].as_str().and_then(rfc3339_millis));
            let Some(at) = at else {
                legacy = true;
                break;
            };
            first = first.min(at);
            last = last.max(at);
            records.push(value);
        }
        if legacy {
            result.omitted += 1;
            continue;
        }
        if first > until || last < since {
            continue;
        }
        redactor.reset_stream();
        let mut cleaned = Vec::new();
        for mut record in records {
            redact_json(&mut record, redactor, false);
            serde_json::to_writer(&mut cleaned, &record)?;
            cleaned.push(b'\n');
            if cleaned.len() > budget {
                break;
            }
        }
        if cleaned.len() > budget {
            result.omitted += 1;
            continue;
        }
        budget -= cleaned.len();
        result.first = Some(result.first.unwrap_or(first).min(first));
        result.last = Some(result.last.unwrap_or(last).max(last));
        result
            .entries
            .push((format!("carlink-trace/{name}"), cleaned));
    }
    result.entries.reverse();
    Ok(result)
}
fn redact_json(value: &mut Value, redactor: &mut Redactor, sensor: bool) {
    match value {
        Value::Object(fields) => {
            let sensor = sensor || fields.get("service").is_some_and(|v| v == "sensor");
            for (key, value) in fields.iter_mut() {
                if sensor && matches!(key.as_str(), "preview" | "payload" | "body") {
                    *value = Value::String("[hidden]".into());
                } else if let Some(kind) = super::redactor::kind(key) {
                    *value = Value::String(
                        redactor.token(value.as_str().unwrap_or(&value.to_string()), kind),
                    );
                } else {
                    redact_json(value, redactor, sensor);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_json(value, redactor, sensor);
            }
        }
        Value::String(text) => {
            *text = super::redact_lines(redactor, text)
                .trim_end_matches('\n')
                .into();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::private_state_file;
    #[test]
    fn exports_whole_overlapping_segments_beyond_256k_but_not_legacy_old_or_partial_records() {
        let dir = std::env::temp_dir().join(format!(
            "range-traces-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let name = "trace-00000001-0-00000001.jsonl";
        let mut data = Vec::new();
        for (at, count) in [(99, 1), (100, 6000), (201, 1)] {
            for _ in 0..count {
                data.extend(format!("{{\"type\":\"Rtsp\",\"atMillis\":{at},\"headers\":{{\"Authorization\":\"Bearer private-bearer\"}},\"body\":\"wfd_video_formats: 00 00\\npassword: private-psk\"}}\n").bytes());
            }
        }
        data.extend(b"{\"atMillis\":150,\"partial\":true");
        private_state_file(&dir.join(name), &data).unwrap();
        private_state_file(
            &dir.join("trace-00000001-0-00000002.jsonl"),
            b"{\"atMillis\":50,\"type\":\"old\"}\n",
        )
        .unwrap();
        private_state_file(
            &dir.join("trace-00000001-0-00000003.jsonl"),
            b"{\"type\":\"legacy\",\"atNanos\":123}\n",
        )
        .unwrap();
        let mut redactor = Redactor::new([42; 32]);
        let result = collect(&dir, 100, 200, 2 * 1024 * 1024, &mut redactor).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.first, Some(99));
        assert_eq!(result.last, Some(201));
        let body = String::from_utf8_lossy(&result.entries[0].1);
        assert_eq!(body.lines().count(), 6002);
        assert!(body.len() > 256 * 1024);
        assert!(body.contains("wfd_video_formats"));
        for private in ["private-bearer", "private-psk", "partial", "legacy"] {
            assert!(!body.contains(private));
        }
        for line in body.lines() {
            assert!(serde_json::from_str::<Value>(line).is_ok());
        }
        let capped = collect(&dir, 100, 200, 100, &mut redactor).unwrap();
        assert!(capped.entries.is_empty());
        assert_eq!(capped.omitted, 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
