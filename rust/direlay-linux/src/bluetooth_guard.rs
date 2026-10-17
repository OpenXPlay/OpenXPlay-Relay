// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Optional device-support contract: a Bluetooth guard service (UFI003 `bt-guard.sh`) writes
//! its state atomically to a small JSON file. Without the file there is no guard status.
pub use crate::admin::AdminBluetoothGuard as BluetoothGuard;
use std::io::Read;

pub const DEFAULT_STATUS_PATH: &str = "/run/openxplay/bluetooth-guard.json";
const MAX_BYTES: u64 = 4096;

/// A missing, oversized or malformed file, or an unknown state, yields `None`.
pub fn read(path: &std::path::Path) -> Option<BluetoothGuard> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    let mut guard: BluetoothGuard = serde_json::from_slice(&bytes).ok()?;
    if !matches!(guard.state.as_str(), "ok" | "recovering" | "failed") {
        return None;
    }
    let bound = |value: &mut Option<String>, limit: usize| {
        if let Some(text) = value {
            *text = text
                .chars()
                .filter(|c| !c.is_control())
                .take(limit)
                .collect();
        }
    };
    bound(&mut guard.last_recovery, 40);
    bound(&mut guard.last_error, 200);
    bound(&mut guard.address, 17);
    Some(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(name: &str, content: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oxp-bt-guard-{}-{name}",
            crate::admin::auth::random_hex(6).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bluetooth-guard.json");
        std::fs::write(&path, content).unwrap();
        path
    }
    #[test]
    fn reads_the_guard_contract_and_tolerates_missing_optional_fields() {
        let path = file("full", br#"{"state":"recovering","attempts":2,"lastRecovery":"2026-10-16T08:00:00Z","lastError":"hci0 has no address","address":null,"future":1}"#);
        assert_eq!(
            read(&path),
            Some(BluetoothGuard {
                state: "recovering".into(),
                attempts: 2,
                last_recovery: Some("2026-10-16T08:00:00Z".into()),
                last_error: Some("hci0 has no address".into()),
                address: None,
            })
        );
        let json = serde_json::to_value(read(&path).unwrap()).unwrap();
        assert_eq!(json["lastRecovery"], "2026-10-16T08:00:00Z");
        assert!(json["address"].is_null());
        let path = file("short", br#"{"state":"ok"}"#);
        assert_eq!(read(&path).unwrap().attempts, 0);
    }
    #[test]
    fn missing_malformed_unknown_or_oversized_files_are_absent() {
        assert_eq!(
            read(std::path::Path::new("/nonexistent/bluetooth-guard.json")),
            None
        );
        assert_eq!(read(&file("bad", b"{\"state\":")), None);
        assert_eq!(
            read(&file("state", br#"{"state":"exploded","attempts":1}"#)),
            None
        );
        assert_eq!(
            read(&file("type", br#"{"state":"ok","attempts":"many"}"#)),
            None
        );
        let mut big = br#"{"state":"ok","lastError":""#.to_vec();
        big.extend(vec![b'x'; 5000]);
        big.extend(br#""}"#);
        assert_eq!(read(&file("big", &big)), None);
        let long = format!(
            r#"{{"state":"failed","attempts":3,"lastError":"{}\u0007"}}"#,
            "e".repeat(300)
        );
        let guard = read(&file("long", long.as_bytes())).unwrap();
        assert_eq!(guard.last_error.unwrap(), "e".repeat(200));
    }
}
