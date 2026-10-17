// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-17

//! When the steps from power-on to a CarPlay session happened, in milliseconds on the monotonic
//! clock, which counts from boot like journald's monotonic timestamps. The first time a session of
//! a controller attempt becomes ACTIVE, the relay logs one `boot-timing` event with the steps of
//! that attempt, so a power-on measurement needs only that line.
use std::sync::Mutex;

/// A step on the way to a CarPlay session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Milestone {
    /// The relay process started.
    RelayStart,
    /// A controller attempt began; the later steps belong to it.
    AttemptStart,
    /// The attempt found the Bluetooth adapter usable.
    AdapterReady,
    /// The attempt's access point is up; see [`BootTiming::hotspot_ready`].
    HotspotReady,
    /// The last Bluetooth connection to the iPhone started.
    BluetoothConnectStart,
    /// The iPhone accepted the identification and the MFi authentication.
    Authenticated,
    /// The access point's credentials went to the iPhone (its first Wi-Fi configuration).
    WifiInformationSent,
    /// The iPhone's station was first seen on the access point.
    IphoneJoined,
    /// The AirPlay session began (CONNECTING).
    SessionConnecting,
    /// The session became ACTIVE: its first video format reached the outputs.
    SessionActive,
}
/// The steps of one attempt in event order, with their field names.
const STEPS: [(Milestone, &str); 9] = [
    (Milestone::AttemptStart, "attemptStartMs"),
    (Milestone::AdapterReady, "adapterReadyMs"),
    (Milestone::HotspotReady, "hotspotReadyMs"),
    (Milestone::BluetoothConnectStart, "btConnectStartMs"),
    (Milestone::Authenticated, "authDoneMs"),
    (Milestone::WifiInformationSent, "wifiInfoSentMs"),
    (Milestone::IphoneJoined, "iphoneJoinedMs"),
    (Milestone::SessionConnecting, "sessionConnectingMs"),
    (Milestone::SessionActive, "sessionActiveMs"),
];
#[derive(Default)]
struct State {
    relay_start: Option<u64>,
    attempts: u64,
    steps: [Option<u64>; STEPS.len()],
    hotspot_reused: Option<bool>,
    reported: bool,
}
/// Records the milestones; [`crate::JsonEvents::milestone`] logs the event.
#[derive(Default)]
pub struct BootTiming {
    state: Mutex<State>,
}
impl BootTiming {
    /// Records `milestone` now. Returns the `boot-timing` fields when it is the first
    /// [`Milestone::SessionActive`] of the attempt.
    pub fn mark(&self, milestone: Milestone) -> Option<serde_json::Value> {
        self.mark_at(milestone, monotonic_millis())
    }
    /// Records the attempt's access point, reused from an earlier attempt or controller or not.
    pub fn hotspot_ready(&self, reused: bool) {
        self.mark(Milestone::HotspotReady);
        self.state.lock().unwrap().hotspot_reused = Some(reused);
    }
    fn mark_at(&self, milestone: Milestone, now: u64) -> Option<serde_json::Value> {
        let mut s = self.state.lock().unwrap();
        let step = |m| STEPS.iter().position(|(s, _)| *s == m);
        match milestone {
            Milestone::RelayStart => {
                s.relay_start.get_or_insert(now);
                return None;
            }
            Milestone::AttemptStart => {
                s.attempts += 1;
                s.steps = Default::default();
                s.hotspot_reused = None;
                s.reported = false;
            }
            _ => {}
        }
        let index = step(milestone)?;
        // The credentials and the iPhone's arrival count when they first happened; the other
        // steps when they last happened, i.e. on the way to this session.
        let first_only = matches!(
            milestone,
            Milestone::WifiInformationSent | Milestone::IphoneJoined
        );
        if !(first_only && s.steps[index].is_some()) {
            s.steps[index] = Some(now);
        }
        if milestone != Milestone::SessionActive || std::mem::replace(&mut s.reported, true) {
            return None;
        }
        let mut fields = serde_json::json!({
            "relayStartMs": s.relay_start,
            "attempt": s.attempts,
            "hotspotReused": s.hotspot_reused,
        });
        for ((_, name), value) in STEPS.iter().zip(s.steps) {
            fields[*name] = value.into();
        }
        Some(fields)
    }
}
/// Milliseconds on the system's monotonic clock, which starts at boot.
pub fn monotonic_millis() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    now.tv_sec as u64 * 1000 + now.tv_nsec as u64 / 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_first_active_session_of_an_attempt_reports_its_steps() {
        let timing = BootTiming::default();
        assert_eq!(timing.mark_at(Milestone::RelayStart, 7_600), None);
        assert_eq!(timing.mark_at(Milestone::RelayStart, 9_000), None);
        timing.mark_at(Milestone::AttemptStart, 8_000);
        timing.mark_at(Milestone::BluetoothConnectStart, 8_100);
        // A failed attempt's steps are forgotten.
        timing.mark_at(Milestone::AttemptStart, 9_500);
        for (milestone, at) in [
            (Milestone::AdapterReady, 9_510),
            (Milestone::BluetoothConnectStart, 9_520),
            (Milestone::BluetoothConnectStart, 9_600),
            (Milestone::Authenticated, 11_000),
            (Milestone::HotspotReady, 11_100),
            (Milestone::WifiInformationSent, 11_120),
            (Milestone::WifiInformationSent, 11_900),
            (Milestone::SessionConnecting, 13_000),
        ] {
            assert_eq!(timing.mark_at(milestone, at), None);
        }
        timing.state.lock().unwrap().hotspot_reused = Some(false);
        let fields = timing.mark_at(Milestone::SessionActive, 15_000).unwrap();
        assert_eq!(
            fields,
            serde_json::json!({
                "relayStartMs": 7_600,
                "attempt": 2,
                "attemptStartMs": 9_500,
                "adapterReadyMs": 9_510,
                "hotspotReadyMs": 11_100,
                "hotspotReused": false,
                "btConnectStartMs": 9_600,
                "authDoneMs": 11_000,
                "wifiInfoSentMs": 11_120,
                "iphoneJoinedMs": null,
                "sessionConnectingMs": 13_000,
                "sessionActiveMs": 15_000,
            })
        );
        // Later sessions of the same attempt are not reported again; the next attempt's are.
        assert_eq!(timing.mark_at(Milestone::SessionActive, 16_000), None);
        timing.mark_at(Milestone::AttemptStart, 20_000);
        let next = timing.mark_at(Milestone::SessionActive, 21_000).unwrap();
        assert_eq!(next["attempt"], 3);
        assert_eq!(next["btConnectStartMs"], serde_json::Value::Null);
        assert!(monotonic_millis() > 0);
    }
}
