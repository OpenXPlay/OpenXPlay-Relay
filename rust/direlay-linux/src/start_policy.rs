// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use std::time::{Duration, Instant};

pub const MAX_STARTUP_RETRIES: u8 = 5;
pub const STABLE_SESSION: Duration = Duration::from_secs(60);
pub const ARRIVAL_DELAY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct RetryBudget {
    failures: u8,
}
impl RetryBudget {
    pub fn failed(&mut self, code: &str, stable_video: bool) -> bool {
        if stable_video {
            self.failures = 0;
        }
        if matches!(
            code,
            "hotspot_configuration"
                | "bluetooth_unusable"
                | "wifi_reset_required"
                | "usb_trust_denied"
                | "iphone_locked"
                | "carlink_not_ready"
        ) {
            return false;
        }
        if code == "iphone_unplugged" {
            return true;
        }
        self.failures = self.failures.saturating_add(1);
        self.failures <= MAX_STARTUP_RETRIES
    }
}

#[derive(Clone, Copy, Default)]
pub struct RadioReadings {
    pub bluetooth: Option<bool>,
    pub wifi: Option<bool>,
    pub hotspot: Option<bool>,
    pub carlink: Option<bool>,
    pub iphone: Option<bool>,
}
#[derive(Default)]
pub struct RecoverOnce {
    previous: RadioReadings,
    recovered: bool,
}
impl RecoverOnce {
    pub fn reset(&mut self) {
        self.recovered = false;
    }
    pub fn observe(
        &mut self,
        failure: Option<&crate::path_facts::LinkFailure>,
        readings: RadioReadings,
        user_stopped: bool,
        automatic: bool,
    ) -> bool {
        let before = std::mem::replace(&mut self.previous, readings);
        let Some(failure) =
            failure.filter(|f| !f.retrying && !user_stopped && (automatic || !self.recovered))
        else {
            return false;
        };
        let restored = match failure.code.as_str() {
            "hotspot_not_ready" | "hotspot_configuration" => {
                before.hotspot == Some(false) && readings.hotspot == Some(true)
            }
            "bluetooth_off" | "bluetooth_unusable" => {
                before.bluetooth == Some(false) && readings.bluetooth == Some(true)
            }
            "wifi_off" => before.wifi == Some(false) && readings.wifi == Some(true),
            "unknown" | "rfcomm_timeout" | "iphone_unpaired" | "first_tcp_timeout" if automatic => {
                before.iphone == Some(false) && readings.iphone == Some(true)
            }
            "carlink_not_ready" if automatic => {
                before.carlink != Some(true) && readings.carlink == Some(true)
            }
            _ => false,
        };
        self.recovered |= restored;
        restored
    }
}

/// The kernel boot ID makes Stop survive a daemon restart without carrying it into another boot.
pub struct AutoStartGate {
    file: std::path::PathBuf,
    boot: String,
    paused: bool,
}
impl AutoStartGate {
    pub fn load(state_dir: &std::path::Path, root: &std::path::Path) -> crate::Result<Self> {
        let identity_file = root.join("proc/sys/kernel/random/boot_id");
        let identity = std::fs::read_to_string(&identity_file).or_else(|e| {
            if root != std::path::Path::new("/") && e.kind() == std::io::ErrorKind::NotFound {
                std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            } else {
                Err(e)
            }
        });
        let boot = match identity {
            Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
            // Non-Linux hosts are used for tests and cannot run the Linux daemon.
            Err(e) if !cfg!(target_os = "linux") && e.kind() == std::io::ErrorKind::NotFound => {
                "non-linux".into()
            }
            Ok(_) => return Err(std::io::Error::other("Kernel boot identity is empty")),
            Err(e) => return Err(e),
        };
        let file = state_dir.join("auto-start-pause");
        let previous = match std::fs::read_to_string(&file) {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let paused = previous.trim() == boot;
        let mut gate = Self { file, boot, paused };
        if !previous.is_empty() && !paused {
            gate.set_paused(false)?;
        }
        Ok(gate)
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn set_paused(&mut self, paused: bool) -> crate::Result<()> {
        crate::platform::private_state_file(
            &self.file,
            if paused { self.boot.as_bytes() } else { b"" },
        )?;
        self.paused = paused;
        Ok(())
    }
}

#[derive(Default)]
pub struct Arrival {
    pub deadline: Option<Instant>,
}
impl Arrival {
    pub fn ready(&mut self, now: Instant, ready: bool) -> bool {
        if !ready {
            self.deadline = None;
            return false;
        }
        now >= *self.deadline.get_or_insert(now + ARRIVAL_DELAY)
    }
    pub fn cancel(&mut self) {
        self.deadline = None;
    }
}

pub type StartGate = std::sync::Arc<dyn Fn() -> crate::Result<bool> + Send + Sync>;

/// One start owns both budgets; polling and controller retries cannot extend them.
#[derive(Default)]
pub struct StartPolicy {
    carlink: Option<Instant>,
    screen: Option<Instant>,
    requested: bool,
    failed: bool,
    blocked: bool,
}
impl StartPolicy {
    pub fn request(&mut self, now: Instant, carlink: bool, screen_known: bool) {
        self.failed = false;
        self.blocked = false;
        self.requested = true;
        self.carlink = carlink.then_some(now + Duration::from_secs(60));
        self.screen = (!screen_known).then_some(now + Duration::from_secs(20));
    }
    pub fn requested(&self) -> bool {
        self.requested
    }
    pub fn waiting(&self) -> bool {
        self.carlink.is_some() || self.screen.is_some()
    }
    pub fn skip(&mut self) {
        self.failed = false;
        self.blocked = false;
        self.requested = true;
        self.carlink = None;
        self.screen = None;
    }
    pub fn fail(&mut self) {
        self.skip();
        self.failed = true;
        self.blocked = true;
    }
    pub fn take_failure(&mut self) -> bool {
        std::mem::take(&mut self.failed)
    }
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.carlink.or(self.screen)
    }
    pub fn ready(
        &mut self,
        now: Instant,
        carlink_enabled: bool,
        carlink_ready: bool,
        screen_reported: bool,
    ) -> crate::Result<bool> {
        if self.blocked {
            return Err(std::io::Error::other(
                "Carlink output did not become ready before CarPlay startup",
            ));
        }
        if let Some(until) = self.carlink {
            if !carlink_enabled {
                return Err(std::io::Error::other(
                    "Carlink output was disabled while waiting to start CarPlay",
                ));
            }
            if carlink_ready {
                self.carlink = None;
            } else if now >= until {
                return Err(std::io::Error::other(
                    "Carlink output did not become ready before CarPlay startup",
                ));
            } else {
                return Ok(false);
            }
        }
        if let Some(until) = self.screen {
            if !screen_reported && now < until {
                return Ok(false);
            }
            self.screen = None;
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gate_is_bounded_and_saved_screen_cannot_release_it() {
        let t = Instant::now();
        let mut p = StartPolicy::default();
        p.request(t, true, true);
        assert!(!p
            .ready(t + Duration::from_secs(59), true, false, true)
            .unwrap());
        assert!(p
            .ready(t + Duration::from_secs(60), true, false, true)
            .is_err());
        assert!(p.ready(t, false, false, true).is_err());
        assert!(p.ready(t, true, true, false).unwrap());
    }
    #[test]
    fn car_first_expires_without_extending_and_start_now_skips_both() {
        let t = Instant::now();
        let mut p = StartPolicy::default();
        p.request(t, false, false);
        assert!(!p
            .ready(t + Duration::from_secs(19), false, false, false)
            .unwrap());
        assert!(p
            .ready(t + Duration::from_secs(20), false, false, false)
            .unwrap());
        p.request(t, true, false);
        p.skip();
        assert!(p.ready(t, true, false, false).unwrap());
        p.request(t, false, false);
        assert!(p.ready(t, false, false, true).unwrap());
        p.cancel();
        assert!(!p.requested());
    }
}

/// Missing radio means off; unreadable rfkill readings remain unknown.
pub fn wifi_enabled(root: &std::path::Path, interface: &str) -> Option<bool> {
    let net = root.join("sys/class/net").join(interface);
    if !net.join("address").exists() {
        return Some(false);
    }
    let phy = net.join("phy80211");
    let mut known = false;
    for entry in std::fs::read_dir(phy).ok()?.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("rfkill") {
            continue;
        }
        let soft = std::fs::read_to_string(entry.path().join("soft")).ok()?;
        let hard = std::fs::read_to_string(entry.path().join("hard")).ok()?;
        if soft.trim() == "1" || hard.trim() == "1" {
            return Some(false);
        }
        if soft.trim() != "0" || hard.trim() != "0" {
            return None;
        }
        known = true;
    }
    known.then_some(true)
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use crate::path_facts::LinkFailure;

    #[test]
    fn retry_budget_requires_stable_video_and_never_retries_configuration() {
        let mut budget = RetryBudget::default();
        for _ in 0..5 {
            assert!(budget.failed("rfcomm_timeout", false));
        }
        assert!(!budget.failed("rfcomm_timeout", false));
        assert!(budget.failed("rfcomm_timeout", true));
        for code in [
            "hotspot_configuration",
            "bluetooth_unusable",
            "wifi_reset_required",
            "usb_trust_denied",
            "iphone_locked",
        ] {
            assert!(!RetryBudget::default().failed(code, true), "{code}");
        }
    }
    #[test]
    fn recovery_needs_a_matching_real_edge_once_per_start() {
        let mut recovery = RecoverOnce::default();
        let mut failure = LinkFailure {
            code: "bluetooth_off".into(),
            ..Default::default()
        };
        let off = RadioReadings {
            bluetooth: Some(false),
            ..Default::default()
        };
        let on = RadioReadings {
            bluetooth: Some(true),
            ..Default::default()
        };
        assert!(!recovery.observe(Some(&failure), on, false, false));
        assert!(!recovery.observe(Some(&failure), off, false, false));
        failure.retrying = true;
        assert!(!recovery.observe(Some(&failure), on, false, false));
        failure.retrying = false;
        recovery.observe(Some(&failure), off, false, false);
        assert!(!recovery.observe(Some(&failure), on, true, false));
        recovery.observe(Some(&failure), off, false, false);
        assert!(recovery.observe(Some(&failure), on, false, false));
        recovery.observe(Some(&failure), off, false, false);
        assert!(!recovery.observe(Some(&failure), on, false, false));
        recovery.reset();
        recovery.observe(Some(&failure), off, false, false);
        assert!(recovery.observe(Some(&failure), on, false, false));
    }
    #[test]
    fn automatic_carlink_rescue_requires_readiness_and_can_repeat_but_stop_wins() {
        let mut recovery = RecoverOnce::default();
        let failure = LinkFailure {
            code: "carlink_not_ready".into(),
            ..Default::default()
        };
        let off = RadioReadings {
            carlink: Some(false),
            ..Default::default()
        };
        let on = RadioReadings {
            carlink: Some(true),
            ..Default::default()
        };
        for _ in 0..2 {
            assert!(!recovery.observe(Some(&failure), off, false, true));
            assert!(recovery.observe(Some(&failure), on, false, true));
            assert!(!recovery.observe(Some(&failure), on, false, true));
        }
        recovery.observe(Some(&failure), off, false, true);
        assert!(!recovery.observe(Some(&failure), on, true, true));
        recovery.reset();
        recovery.observe(Some(&failure), off, false, false);
        assert!(!recovery.observe(Some(&failure), on, false, false));
    }
    #[test]
    fn automatic_iphone_rescue_uses_a_real_connected_edge() {
        let mut recovery = RecoverOnce::default();
        let failure = LinkFailure {
            code: "rfcomm_timeout".into(),
            ..Default::default()
        };
        let off = RadioReadings {
            iphone: Some(false),
            ..Default::default()
        };
        let on = RadioReadings {
            iphone: Some(true),
            ..Default::default()
        };
        assert!(!recovery.observe(Some(&failure), on, false, true));
        recovery.observe(Some(&failure), off, false, true);
        assert!(recovery.observe(Some(&failure), on, false, true));
        assert!(!recovery.observe(Some(&failure), on, false, true));
    }
    #[test]
    fn failed_gate_stays_closed_after_the_failure_notification_is_consumed() {
        let mut policy = StartPolicy::default();
        policy.fail();
        assert!(policy.take_failure());
        assert!(!policy.take_failure());
        assert!(policy.ready(Instant::now(), true, true, true).is_err());
        policy.request(Instant::now(), true, true);
        assert!(policy.ready(Instant::now(), true, true, true).unwrap());
    }
    #[test]
    fn arrival_is_five_seconds_cancellable_and_restarts_after_lost_readiness() {
        let t = Instant::now();
        let mut arrival = Arrival::default();
        assert!(!arrival.ready(t, true));
        assert!(!arrival.ready(t + Duration::from_millis(4999), true));
        assert!(arrival.ready(t + ARRIVAL_DELAY, true));
        assert!(!arrival.ready(t + ARRIVAL_DELAY, false));
        assert!(arrival.deadline.is_none());
        assert!(!arrival.ready(t + Duration::from_secs(10), true));
        arrival.cancel();
        assert!(arrival.deadline.is_none());
    }
    #[test]
    fn pause_survives_reconstruction_and_expires_only_on_a_different_boot() {
        let dir = std::env::temp_dir().join(format!(
            "w11-gate-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let boot = dir.join("proc/sys/kernel/random");
        std::fs::create_dir_all(&boot).unwrap();
        std::fs::write(boot.join("boot_id"), "boot-one\n").unwrap();
        let mut gate = AutoStartGate::load(&dir, &dir).unwrap();
        assert!(!gate.paused());
        gate.set_paused(true).unwrap();
        assert!(AutoStartGate::load(&dir, &dir).unwrap().paused());
        std::fs::write(boot.join("boot_id"), "boot-two\n").unwrap();
        assert!(!AutoStartGate::load(&dir, &dir).unwrap().paused());
        assert_eq!(
            std::fs::read_to_string(dir.join("auto-start-pause")).unwrap(),
            ""
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
