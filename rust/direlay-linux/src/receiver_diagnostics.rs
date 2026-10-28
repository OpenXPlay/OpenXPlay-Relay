// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! The latest `receiver-info` and a short window of per-second `stats` for each browser on the
//! car screen, kept in memory for the admin developer section. Every dimension is bounded.
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

pub const MAX_CLIENTS: usize = 8;
pub const MAX_SAMPLES: usize = 30;
/// Compatibility expiry for children that do not report socket termination.
pub const EXPIRY: Duration = Duration::from_secs(60);
const MAX_INFO_BYTES: usize = 64 * 1024;
const MAX_SAMPLE_BYTES: usize = 4 * 1024;
const MAX_CLIENT_ID: usize = 64;

struct Client {
    id: String,
    info: Option<Value>,
    stats: VecDeque<Value>,
    fullscreen_exits: VecDeque<Value>,
    seen: Instant,
}
#[derive(Default)]
struct DiagnosticsState {
    clients: Vec<Client>,
    screens: VecDeque<((u32, u32, u32), Value)>,
    exits: VecDeque<Value>,
    disconnects: VecDeque<Value>,
}
#[derive(Default)]
pub struct ReceiverDiagnostics {
    state: Mutex<DiagnosticsState>,
}
impl ReceiverDiagnostics {
    /// Takes oxp-relay's `receiver-info` and `stats` events and returns whether the event was
    /// one of them. Client ids belong to one oxp-relay process, so a stopped receiver clears live clients only.
    pub fn observe(&self, event: &Value, now: Instant) -> bool {
        let kind = event["type"].as_str().unwrap_or("");
        if matches!(kind, "receiver-stopped" | "receiver-restarting") {
            self.state.lock().unwrap().clients.clear();
            return false;
        }
        if kind == "receiver-disconnected" {
            let Some(id) = client_id(&event["client"]) else {
                return true;
            };
            let Some(code) = event["code"].as_u64().filter(|v| *v <= u16::MAX.into()) else {
                return true;
            };
            let Some(reason) = event["reason"]
                .as_str()
                .filter(|r| matches!(*r, "closed" | "timeout" | "error"))
            else {
                return true;
            };
            let mut state = self.state.lock().unwrap();
            let client = state
                .clients
                .iter()
                .position(|c| c.id == id)
                .map(|i| state.clients.remove(i));
            let report = client.map(
                |c| json!({"info":c.info,"stats":c.stats,"fullscreenExits":c.fullscreen_exits}),
            );
            if state.disconnects.len() == MAX_SAMPLES {
                state.disconnects.pop_front();
            }
            state.disconnects.push_back(json!({"time":crate::platform::wall_clock().as_millis() as u64,"client":id,"code":code,"reason":reason,"report":report}));
            return true;
        }
        if !matches!(kind, "receiver-info" | "stats" | "fullscreen-exit") {
            return false;
        }
        let Some(id) = client_id(&event["client"]) else {
            return true;
        };
        let (entry, limit) = if kind == "receiver-info" {
            (event["info"].clone(), MAX_INFO_BYTES)
        } else {
            let mut sample = event.clone();
            if let Some(fields) = sample.as_object_mut() {
                fields.remove("type");
                fields.remove("client");
            }
            (sample, MAX_SAMPLE_BYTES)
        };
        if !entry.is_object() || serde_json::to_vec(&entry).map_or(true, |b| b.len() > limit) {
            return true;
        }
        let mut state = self.state.lock().unwrap();
        if kind == "fullscreen-exit" {
            if let Some(client) = state.clients.iter_mut().find(|client| client.id == id) {
                if client.fullscreen_exits.len() == MAX_SAMPLES {
                    client.fullscreen_exits.pop_front();
                }
                client.fullscreen_exits.push_back(entry.clone());
            }
            if state.exits.len() == MAX_SAMPLES {
                state.exits.pop_front();
            }
            state.exits.push_back(json!({"time": crate::platform::wall_clock().as_millis() as u64, "client":id, "report":entry}));
            return true;
        }
        if kind == "receiver-info" {
            if let Some(key) = screen_key(&entry) {
                state.screens.retain(|(screen, _)| *screen != key);
                state.screens.push_back((key, entry.clone()));
                if state.screens.len() > 10 {
                    state.screens.pop_front();
                }
            }
        }
        let clients = &mut state.clients;
        clients.retain(|c| now.saturating_duration_since(c.seen) < EXPIRY);
        let index = match clients.iter().position(|c| c.id == id) {
            Some(index) => index,
            None => {
                if clients.len() >= MAX_CLIENTS {
                    // Make room by forgetting the browser that has been silent longest.
                    if let Some(oldest) = (0..clients.len()).min_by_key(|&i| clients[i].seen) {
                        clients.remove(oldest);
                    }
                }
                clients.push(Client {
                    id,
                    info: None,
                    stats: VecDeque::with_capacity(MAX_SAMPLES),
                    fullscreen_exits: VecDeque::with_capacity(MAX_SAMPLES),
                    seen: now,
                });
                clients.len() - 1
            }
        };
        let client = &mut clients[index];
        client.seen = now;
        if kind == "receiver-info" {
            client.info = Some(entry);
        } else {
            if client.stats.len() == MAX_SAMPLES {
                client.stats.pop_front();
            }
            client.stats.push_back(entry);
        }
        true
    }
    /// Screen keys use physical pixels and Android's 160 dpi reference density.
    pub fn latest_info(&self, width: u32, height: u32, density_dpi: u32) -> Option<Value> {
        self.state
            .lock()
            .unwrap()
            .screens
            .iter()
            .find(|(key, _)| *key == (width, height, density_dpi))
            .map(|(_, info)| info.clone())
    }
    /// Retained independently of live clients, including across receiver process restarts.
    pub fn history(&self) -> Value {
        let state = self.state.lock().unwrap();
        let screens: Vec<_> = state.screens.iter().map(|((width, height, density), info)|
            json!({"width":width,"height":height,"densityDpi":density,"info":info})).collect();
        json!({"screens":screens,"fullscreenExits":state.exits,"disconnects":state.disconnects})
    }
    pub fn page_count(&self, now: Instant) -> usize {
        self.state
            .lock()
            .unwrap()
            .clients
            .iter()
            .filter(|c| now.saturating_duration_since(c.seen) < EXPIRY)
            .count()
    }
    pub fn clear(&self) {
        *self.state.lock().unwrap() = DiagnosticsState::default();
    }
    /// `[{client, idleSeconds, info, stats}]` in arrival order, oldest sample first.
    pub fn snapshot(&self, now: Instant) -> Vec<Value> {
        let mut state = self.state.lock().unwrap();
        state
            .clients
            .retain(|c| now.saturating_duration_since(c.seen) < EXPIRY);
        state.clients
            .iter()
            .map(|c| {
                let idle = now.saturating_duration_since(c.seen).as_secs_f64();
                json!({"client": c.id, "idleSeconds": (idle * 10.0).round() / 10.0, "info": c.info, "stats": c.stats, "fullscreenExits":c.fullscreen_exits})
            })
            .collect()
    }
}
fn screen_key(info: &Value) -> Option<(u32, u32, u32)> {
    let ratio = info["devicePixelRatio"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)?;
    let dimension =
        |v: f64| (v.is_finite() && (1.0..=32768.0).contains(&v.round())).then(|| v.round() as u32);
    Some((
        dimension(info["screen"]["width"].as_f64()? * ratio)?,
        dimension(info["screen"]["height"].as_f64()? * ratio)?,
        dimension(ratio * 160.0)?,
    ))
}
fn client_id(value: &Value) -> Option<String> {
    let id = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!id.is_empty() && id.len() <= MAX_CLIENT_ID && !id.chars().any(char::is_control)).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stats(client: &str, fps: u32) -> Value {
        json!({"type":"stats","client":client,"screen":0,"renderedFps":fps,"wsBytesPerSecond":500000})
    }
    #[test]
    fn disconnect_removes_live_page_and_retains_bounded_reports_across_restarts() {
        let d = ReceiverDiagnostics::default();
        let now = Instant::now();
        d.observe(
            &json!({"type":"receiver-info","client":"browser-1","info":{"refreshHz":60}}),
            now,
        );
        d.observe(&stats("browser-1", 60), now);
        d.observe(&json!({"type":"receiver-disconnected","client":"browser-1","code":1011,"reason":"timeout","secret":"untrusted"}),now);
        assert_eq!(d.page_count(now), 0);
        assert!(d.snapshot(now).is_empty());
        let history = d.history();
        assert_eq!(
            history["disconnects"][0]["report"]["stats"][0]["renderedFps"],
            60
        );
        assert_eq!(history["disconnects"][0]["code"], 1011);
        assert!(!history.to_string().contains("untrusted"));
        d.observe(&json!({"type":"receiver-stopped"}), now);
        assert_eq!(d.history(), history);
        for n in 0..40 {
            d.observe(
                &json!({"type":"receiver-disconnected","client":n,"code":1000,"reason":"closed"}),
                now,
            );
        }
        assert_eq!(
            d.history()["disconnects"].as_array().unwrap().len(),
            MAX_SAMPLES
        );
        d.clear();
        assert!(d.history()["disconnects"].as_array().unwrap().is_empty());
    }
    #[test]
    fn keeps_latest_info_and_a_rolling_stats_window_per_browser() {
        let d = ReceiverDiagnostics::default();
        let t = Instant::now();
        assert!(d.observe(
            &json!({"type":"receiver-info","client":"7","info":{"userAgent":"A","refreshHz":60}}),
            t
        ));
        for i in 0..(MAX_SAMPLES as u32 + 5) {
            assert!(d.observe(&stats("7", i), t + Duration::from_millis(i as u64 * 100)));
        }
        assert!(d.observe(
            &json!({"type":"receiver-info","client":"7","info":{"userAgent":"B"}}),
            t + Duration::from_secs(4)
        ));
        let snapshot = d.snapshot(t + Duration::from_millis(4500));
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0]["client"], "7");
        assert_eq!(snapshot[0]["idleSeconds"], 0.5);
        assert_eq!(snapshot[0]["info"], json!({"userAgent":"B"}));
        let samples = snapshot[0]["stats"].as_array().unwrap();
        assert_eq!(samples.len(), MAX_SAMPLES);
        assert_eq!(samples[0]["renderedFps"], 5);
        assert_eq!(
            samples[MAX_SAMPLES - 1]["renderedFps"],
            MAX_SAMPLES as u32 + 4
        );
        assert!(samples[0].get("type").is_none() && samples[0].get("client").is_none());
    }
    #[test]
    fn fullscreen_exits_are_bounded_and_separate_from_stats() {
        let d = ReceiverDiagnostics::default();
        let t = Instant::now();
        for n in 0..40 {
            assert!(d.observe(&json!({"type":"fullscreen-exit","client":"1","hiddenMs":n,"autoRestore":"denied","display":{"widthPx":1000,"heightPx":700}}),t));
        }
        assert!(d.snapshot(t + EXPIRY).is_empty());
        assert_eq!(d.page_count(t), 0);
        let history = d.history();
        assert_eq!(
            history["fullscreenExits"].as_array().unwrap().len(),
            MAX_SAMPLES
        );
        assert_eq!(history["fullscreenExits"][0]["report"]["hiddenMs"], 10);
        assert_eq!(
            history["fullscreenExits"][29]["report"]["autoRestore"],
            "denied"
        );
        d.observe(&json!({"type":"receiver-restarting"}), t);
        assert_eq!(d.history(), history);
        d.observe(&stats("1", 60), t);
        assert!(d.snapshot(t)[0]["fullscreenExits"]
            .as_array()
            .unwrap()
            .is_empty());
        d.observe(
            &json!({"type":"fullscreen-exit","client":"1","autoRestore":"denied"}),
            t,
        );
        assert_eq!(
            d.snapshot(t)[0]["fullscreenExits"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        d.clear();
        assert!(d.history()["fullscreenExits"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn screen_info_survives_expiry_eviction_and_receiver_restart_with_ten_screen_bound() {
        let d = ReceiverDiagnostics::default();
        let t = Instant::now();
        for width in 100..112 {
            d.observe(&json!({"type":"receiver-info","client":width,"info":{"screen":{"width":width,"height":50},"devicePixelRatio":2,"userAgent":"old"}}), t);
        }
        assert!(d.latest_info(200, 100, 320).is_none());
        assert!(d.latest_info(204, 100, 320).is_some());
        d.observe(&json!({"type":"receiver-info","client":1,"info":{"screen":{"width":102,"height":50},"devicePixelRatio":2,"userAgent":"new"}}), t);
        assert!(d.snapshot(t + EXPIRY).is_empty());
        assert_eq!(d.page_count(t + EXPIRY), 0);
        d.observe(&json!({"type":"receiver-stopped"}), t + EXPIRY);
        assert_eq!(d.latest_info(204, 100, 320).unwrap()["userAgent"], "new");
        let history = d.history();
        assert_eq!(history["screens"].as_array().unwrap().len(), 10);
        assert!(history["screens"][0].get("client").is_none());
        // Malformed screen data must not replace retained valid records.
        for ratio in [0.0, -1.0, 1e12] {
            d.observe(&json!({"type":"receiver-info","client":1,"info":{"screen":{"width":10,"height":10},"devicePixelRatio":ratio}}), t);
        }
        assert_eq!(d.history(), history);
        d.clear();
        assert!(d.latest_info(204, 100, 320).is_none());
    }
    #[test]
    fn bounds_browsers_and_forgets_silent_ones() {
        let d = ReceiverDiagnostics::default();
        let t = Instant::now();
        for i in 0..(MAX_CLIENTS as u64 + 3) {
            d.observe(&stats(&i.to_string(), 60), t + Duration::from_secs(i));
        }
        d.observe(&stats("4", 59), t + Duration::from_secs(20));
        let now = t + Duration::from_secs(21);
        let ids: Vec<_> = d
            .snapshot(now)
            .iter()
            .map(|c| c["client"].as_str().unwrap().to_owned())
            .collect();
        // The three oldest of eleven browsers made room; a later report keeps its position.
        assert_eq!(ids, ["3", "4", "5", "6", "7", "8", "9", "10"]);
        assert_eq!(d.snapshot(t + Duration::from_secs(70)).len(), 1);
        assert!(d.snapshot(t + Duration::from_secs(81)).is_empty());
    }
    #[test]
    fn ignores_malformed_or_oversized_reports_and_clears_with_the_receiver() {
        let d = ReceiverDiagnostics::default();
        let t = Instant::now();
        assert!(d.observe(&json!({"type":"stats","renderedFps":60}), t));
        assert!(d.observe(&json!({"type":"stats","client":{"id":1}}), t));
        assert!(d.observe(&json!({"type":"stats","client":"x".repeat(65)}), t));
        assert!(d.observe(&json!({"type":"stats","client":"a\nb"}), t));
        assert!(d.observe(
            &json!({"type":"receiver-info","client":"1","info":"text"}),
            t
        ));
        assert!(d.observe(
            &json!({"type":"receiver-info","client":"1","info":{"pad":"x".repeat(MAX_INFO_BYTES)}}),
            t
        ));
        assert!(d.observe(
            &json!({"type":"stats","client":"1","pad":"x".repeat(MAX_SAMPLE_BYTES)}),
            t
        ));
        assert!(d.snapshot(t).is_empty());
        assert!(!d.observe(&json!({"type":"pairing-url","url":"x"}), t));
        assert!(d.observe(&stats("3", 60), t));
        assert_eq!(d.snapshot(t)[0]["client"], "3");
        assert!(d.observe(&json!({"type":"stats","client":12,"renderedFps":60}), t));
        assert_eq!(d.snapshot(t)[1]["client"], "12");
        assert!(d.snapshot(t)[1]["info"].is_null());
        assert!(!d.observe(&json!({"type":"receiver-restarting"}), t));
        assert!(d.snapshot(t).is_empty());
        d.observe(&stats("3", 60), t);
        assert!(!d.observe(&json!({"type":"receiver-stopped"}), t));
        assert!(d.snapshot(t).is_empty());
    }
}
