// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Last boundary before stdout/stderr enter journald. No private log recorder.
use super::redactor::{kind, Redactor};
use serde_json::Value;
use std::sync::{Mutex, OnceLock};
fn filter() -> &'static Mutex<Redactor> {
    static FILTER: OnceLock<Mutex<Redactor>> = OnceLock::new();
    FILTER.get_or_init(|| Mutex::new(Redactor::for_journal()))
}
pub fn line(text: &str) -> String {
    super::redact_lines(&mut filter().lock().unwrap(), text)
        .trim_end_matches('\n')
        .into()
}
pub fn value(value: &mut Value) {
    fn visit(value: &mut Value, redactor: &mut Redactor) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if kind(key).is_some_and(|kind| matches!(kind, "secret" | "material")) {
                        *value = Value::String("[secret]".into());
                    } else {
                        visit(value, redactor);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, redactor);
                }
            }
            Value::String(text) => {
                *text = super::redact_lines(redactor, text)
                    .trim_end_matches('\n')
                    .into()
            }
            _ => {}
        }
    }
    fn learn(value: &Value, redactor: &mut Redactor) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    if kind(key).is_some_and(|kind| matches!(kind, "secret" | "material")) {
                        if let Some(text) = value.as_str() {
                            redactor.remember(text, "secret");
                        }
                    } else {
                        learn(value, redactor);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    learn(value, redactor);
                }
            }
            _ => {}
        }
    }
    let mut redactor = filter().lock().unwrap();
    learn(value, &mut redactor);
    visit(value, &mut redactor);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_sink_filters_before_emitting_timeline_and_excludes_raw_vehicle_events() {
        use std::sync::Arc;
        let lines = Arc::new(Mutex::new(Vec::new()));
        let saved = lines.clone();
        let events = crate::platform::JsonEvents::new(Arc::new(move |line| {
            saved.lock().unwrap().push(line)
        }));
        events.emit("stage", &serde_json::json!({"stage":"DiscoveringMfi", "message":"bare cross-field-private", "token":"cross-field-private", "network":{"ssid":"TechnicalWifi", "address":"AA:BB:CC:DD:EE:FF"}}));
        let count = lines.lock().unwrap().len();
        assert_eq!(count, 2);
        events.emit(
            "clink-vehicle",
            &serde_json::json!({"data":{"latitude":123.456789}}),
        );
        assert_eq!(lines.lock().unwrap().len(), count);
        let log = lines.lock().unwrap().join("\n");
        assert!(!log.contains("cross-field-private"));
        assert!(log.contains("TechnicalWifi"));
        assert!(log.contains("AA:BB:CC:DD:EE:FF"));
    }
    #[test]
    fn journal_boundary_removes_nested_credentials_material_hex_and_multiline_pem() {
        let mut event = serde_json::json!({"password":"wifi-private-123", "nested":{"remoteMfiToken":"mfi-private-123", "certificate":[1,2,3], "psk":1234}, "message":"Authorization: Bearer service-private-123\nprivateKey=key-private-123", "ssid":"TechnicalWifi", "address":"AA:BB:CC:DD:EE:FF"});
        value(&mut event);
        let text = event.to_string();
        for private in [
            "wifi-private-123",
            "mfi-private-123",
            "service-private-123",
            "key-private-123",
            "1234",
            "[1,2,3]",
        ] {
            assert!(!text.contains(private), "{text}");
        }
        assert!(text.contains("TechnicalWifi"));
        assert!(text.contains("AA:BB:CC:DD:EE:FF"));
        assert!(!line(
            "-----BEGIN PRIVATE KEY-----\nbase64-private-material\n-----END PRIVATE KEY-----"
        )
        .contains("base64-private-material"));
    }
}
