// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

use crate::{
    boot_timing::{boot_id, monotonic_millis},
    Result, WirelessHotspotInfo, WirelessHotspotManager,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// How long the relay keeps its AP down before bringing up the next one. An iPhone that was
/// associated with the old AP retries that association when the AP returns within seconds, and
/// the UFI003's wcn36xx then rejects it (`STA ... not found`) until the AP stays down a while.
pub const AP_DOWN_GAP: Duration = Duration::from_secs(15);
/// State-directory file holding the last AP teardown, so the gap also holds across a restart.
pub const AP_DOWN_RECORD: &str = "hotspot-down";

/// The relay owns the AP across controller instances and attempts. A released AP stays up only
/// when the relay asked to keep it for a session restart or the controller keeps it for a retry.
#[derive(Default)]
pub struct HotspotLifetime {
    current: Mutex<Option<Arc<HotspotLease>>>,
    /// Serializes `create` without holding `current` across the slow nmcli calls.
    creating: Mutex<()>,
    /// Set while a Carlink link owns the radio; every lease shares it.
    held: Arc<AtomicBool>,
    /// When the last AP went down; every lease records its teardown here.
    downtime: Arc<Downtime>,
    /// The APs a previous run left behind were looked for.
    stale_checked: Mutex<bool>,
}
impl HotspotLifetime {
    /// Keeps a new AP down until `gap` has passed since the previous one went down. With a
    /// `record` file the last teardown survives relay restarts: a teardown less than `gap` ago,
    /// in the same boot, delays the first AP of this process by the rest of the gap.
    pub fn with_down_gap(gap: Duration, record: Option<PathBuf>) -> Self {
        let remaining = record
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map_or(Duration::ZERO, |saved| {
                remaining_gap(&saved, &boot_id(), monotonic_millis(), gap)
            });
        Self {
            downtime: Arc::new(Downtime {
                gap,
                record,
                until: Mutex::new((!remaining.is_zero()).then(|| Instant::now() + remaining)),
            }),
            ..Default::default()
        }
    }
    /// How long the current lease must wait before it brings a new AP up: zero while an AP is up
    /// (a reused lease starts at once), otherwise the rest of the gap since the last teardown.
    pub fn down_remaining(&self) -> Duration {
        let up = self.current.lock().unwrap().as_ref().is_some_and(|lease| {
            let state = lease.state.lock().unwrap();
            state.ready.is_some() && !state.closed
        });
        if up {
            Duration::ZERO
        } else {
            self.downtime.remaining()
        }
    }
    /// Runs `cleanup` once per relay process, before its first AP: it removes the APs a previous
    /// run left behind when it was killed before its teardown (so it wrote no teardown record),
    /// and reports whether one of them was up. That AP goes down only now, so the gap before the
    /// next AP starts now and is recorded like any teardown. `None` once the check has run.
    pub fn remove_stale_once(
        &self,
        cleanup: impl FnOnce() -> Result<bool>,
    ) -> Option<Result<bool>> {
        let mut checked = self.stale_checked.lock().unwrap();
        if std::mem::replace(&mut *checked, true) {
            return None;
        }
        let result = cleanup();
        if matches!(result, Ok(true)) {
            self.downtime.went_down();
        }
        Some(result)
    }
    /// While a Carlink link owns the radio no attempt may create or start an AP: bringing one up
    /// would take the interface from the link. An AP that is already up is left to its owner.
    pub fn hold_for_carlink(&self, held: bool) {
        self.held.store(held, Ordering::Release);
    }
    pub fn create(
        &self,
        factory: impl FnOnce() -> Arc<dyn WirelessHotspotManager>,
    ) -> Result<Arc<dyn WirelessHotspotManager>> {
        let _creating = self.creating.lock().unwrap();
        check_held(&self.held)?;
        let previous = self.current.lock().unwrap().clone();
        let mut ready = None;
        if let Some(previous) = &previous {
            let retained = previous.state.lock().unwrap().retained();
            if retained && previous.inner.validate_ready().is_ok() {
                ready = previous.state.lock().unwrap().ready.clone();
            } else {
                // On failure `current` keeps the previous lease, so a later close retries it.
                previous.force_close()?;
            }
        }
        let reused = previous.as_ref().filter(|_| ready.is_some());
        let lease = Arc::new(HotspotLease {
            inner: reused.map_or_else(factory, |previous| previous.inner.clone()),
            reused: ready.is_some(),
            held: self.held.clone(),
            downtime: self.downtime.clone(),
            state: Mutex::new(LeaseState {
                ready,
                ..Default::default()
            }),
        });
        let mut current = self.current.lock().unwrap();
        let unchanged = match (current.as_ref(), previous.as_ref()) {
            (Some(now), Some(before)) => Arc::ptr_eq(now, before),
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            // `close` ran while the previous AP was being checked: the relay is stopping.
            return Err(std::io::Error::other("Hotspot lease cancelled"));
        }
        if let Some(previous) = reused {
            // The new lease owns the AP now; stale cleanup of the old one must not close it.
            previous.state.lock().unwrap().closed = true;
        }
        *current = Some(lease.clone());
        Ok(lease)
    }
    /// Keeps the AP for the next controller once the current one releases it.
    pub fn keep_for_restart(&self) {
        if let Some(lease) = self.current.lock().unwrap().as_ref() {
            let mut state = lease.state.lock().unwrap();
            if state.ready.is_some() && !state.failed && (!state.released || state.retained()) {
                state.keep = true;
            }
        }
    }
    /// Keeps the AP while the controller retries a failed attempt. An AP whose start or
    /// validation failed is never kept.
    pub fn keep_for_retry(&self) {
        if let Some(lease) = self.current.lock().unwrap().as_ref() {
            let mut state = lease.state.lock().unwrap();
            if !state.released && state.ready.is_some() && !state.failed {
                state.retry = true;
            }
        }
    }
    /// Withdraws a retry retention when the controller stops for good: the AP closes now, or
    /// when its attempt releases it, unless the relay kept it for a restart.
    pub fn cancel_retry(&self) -> Result<()> {
        let lease = self.current.lock().unwrap().clone();
        if let Some(lease) = lease {
            let close = {
                let mut state = lease.state.lock().unwrap();
                let only_retry = state.retry && !state.keep;
                state.retry = false;
                only_retry && state.released
            };
            if close {
                lease.force_close()?;
            }
        }
        Ok(())
    }
    /// The AP lease can be ready before the controller publishes its network snapshot.
    pub fn ready_info(&self) -> Option<WirelessHotspotInfo> {
        let current = self.current.lock().unwrap();
        let state = current.as_ref()?.state.lock().unwrap();
        (!state.closed).then(|| state.ready.clone()).flatten()
    }
    pub fn reused(&self) -> bool {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|lease| lease.reused)
    }
    pub fn close(&self) -> Result<()> {
        let lease = self.current.lock().unwrap().take();
        if let Some(lease) = lease {
            if let Err(e) = lease.force_close() {
                // Keep the lease so a later close or create retries the shutdown.
                self.current.lock().unwrap().get_or_insert(lease);
                return Err(e);
            }
        }
        Ok(())
    }
}
fn check_held(held: &AtomicBool) -> Result<()> {
    if held.load(Ordering::Acquire) {
        return Err(std::io::Error::other("Hotspot held for Carlink"));
    }
    Ok(())
}
/// When the last AP went down.
#[derive(Default)]
struct Downtime {
    gap: Duration,
    record: Option<PathBuf>,
    /// The earliest time the next AP may come up.
    until: Mutex<Option<Instant>>,
}
impl Downtime {
    fn went_down(&self) {
        if self.gap.is_zero() {
            return;
        }
        *self.until.lock().unwrap() = Some(Instant::now() + self.gap);
        if let Some(path) = &self.record {
            // Best effort: without the record a restart only skips its startup wait.
            let _ = crate::platform::private_state_file(
                path,
                format!("{} {}\n", monotonic_millis(), boot_id()).as_bytes(),
            );
        }
    }
    fn remaining(&self) -> Duration {
        self.until.lock().unwrap().map_or(Duration::ZERO, |until| {
            until.saturating_duration_since(Instant::now())
        })
    }
}
/// The rest of `gap` after a teardown saved as `<monotonic ms> <boot id>`. A record from another
/// boot, from the future or unreadable leaves nothing to wait; the result never exceeds `gap`.
fn remaining_gap(saved: &str, boot: &str, now_millis: u64, gap: Duration) -> Duration {
    let mut fields = saved.split_whitespace();
    let Some(Ok(down)) = fields.next().map(str::parse::<u64>) else {
        return Duration::ZERO;
    };
    if fields.next().unwrap_or_default() != boot || fields.next().is_some() || now_millis < down {
        return Duration::ZERO;
    }
    gap.saturating_sub(Duration::from_millis(now_millis - down))
}
#[derive(Default)]
struct LeaseState {
    ready: Option<WirelessHotspotInfo>,
    /// The relay keeps the AP for a session restart.
    keep: bool,
    /// The controller keeps the AP across a retry.
    retry: bool,
    released: bool,
    failed: bool,
    /// This lease no longer owns an open AP: it was closed or handed to a newer lease.
    closed: bool,
}
impl LeaseState {
    fn retained(&self) -> bool {
        self.released
            && (self.keep || self.retry)
            && !self.failed
            && !self.closed
            && self.ready.is_some()
    }
}
struct HotspotLease {
    inner: Arc<dyn WirelessHotspotManager>,
    state: Mutex<LeaseState>,
    reused: bool,
    held: Arc<AtomicBool>,
    downtime: Arc<Downtime>,
}
impl HotspotLease {
    fn force_close(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Ok(());
        }
        state.keep = false;
        state.retry = false;
        state.released = true;
        let up = state.ready.is_some();
        drop(state);
        self.close_inner(up)
    }
    /// A failed close leaves `closed` unset so the next close attempt retries it. Closing an AP
    /// that was up starts the gap before the next one.
    fn close_inner(&self, up: bool) -> Result<()> {
        self.inner.close()?;
        self.state.lock().unwrap().closed = true;
        if up {
            self.downtime.went_down();
        }
        Ok(())
    }
    fn checked<T>(&self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            let close_retained = {
                let mut state = self.state.lock().unwrap();
                let retained = state.retained();
                state.failed = true;
                retained
            };
            // Validation may have been in flight when controller cleanup retained the AP.
            if close_retained {
                let _ = self.force_close();
            }
        }
        result
    }
}
impl WirelessHotspotManager for HotspotLease {
    fn start(&self, timeout: Duration) -> Result<WirelessHotspotInfo> {
        {
            let state = self.state.lock().unwrap();
            if state.released {
                return Err(std::io::Error::other("Hotspot lease cancelled"));
            }
            if let Some(info) = &state.ready {
                return Ok(info.clone());
            }
        }
        // Carlink may have taken the radio after this lease was created.
        check_held(&self.held)?;
        let info = self.checked(self.inner.start(timeout))?;
        let mut state = self.state.lock().unwrap();
        if state.released {
            drop(state);
            self.close_inner(true)?;
            return Err(std::io::Error::other("Hotspot lease cancelled"));
        }
        state.ready = Some(info.clone());
        Ok(info)
    }
    fn validate_ready(&self) -> Result<()> {
        self.checked(self.inner.validate_ready())
    }
    fn on_car_play_confirmed(&self) -> Result<()> {
        self.checked(self.inner.on_car_play_confirmed())
    }
    fn close(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.released {
            return Ok(());
        }
        state.released = true;
        if state.retained() {
            return Ok(());
        }
        state.keep = false;
        state.retry = false;
        let up = state.ready.is_some();
        drop(state);
        self.close_inner(up)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    pub(crate) struct Fake {
        pub(crate) starts: AtomicUsize,
        pub(crate) closes: AtomicUsize,
        pub(crate) failed: AtomicBool,
        close_error: AtomicBool,
        /// While set, `validate_ready` blocks; `validating` reports that it was entered.
        gate: Mutex<bool>,
        gate_changed: std::sync::Condvar,
        validating: AtomicBool,
    }
    impl Fake {
        fn block_validation(&self, blocked: bool) {
            *self.gate.lock().unwrap() = blocked;
            self.gate_changed.notify_all();
        }
    }
    impl WirelessHotspotManager for Fake {
        fn start(&self, _: Duration) -> Result<WirelessHotspotInfo> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(WirelessHotspotInfo {
                ssid: "test".into(),
                passphrase: "password".into(),
                channel: 6,
                security: carplay_iap2::Iap2WirelessSecurity::WpaWpa2,
                host_address: Some("10.42.0.1".parse().unwrap()),
                bssid: None,
                access_point_bssid: None,
                interface_name: Some("wlan0".into()),
            })
        }
        fn validate_ready(&self) -> Result<()> {
            self.validating.store(true, Ordering::SeqCst);
            let mut blocked = self.gate.lock().unwrap();
            while *blocked {
                blocked = self.gate_changed.wait(blocked).unwrap();
            }
            drop(blocked);
            if self.failed.load(Ordering::SeqCst) {
                Err(std::io::Error::other("AP failed"))
            } else {
                Ok(())
            }
        }
        fn on_car_play_confirmed(&self) -> Result<()> {
            Ok(())
        }
        fn close(&self) -> Result<()> {
            if self.close_error.load(Ordering::SeqCst) {
                return Err(std::io::Error::other("nmcli failed"));
            }
            self.closes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    #[test]
    fn retained_hotspot_skips_second_start_and_hard_stop_closes_it() {
        let lifetime = HotspotLifetime::default();
        let fake = Arc::new(Fake::default());
        let first = lifetime.create(|| fake.clone()).unwrap();
        first.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_restart();
        first.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 0);
        let second = lifetime.create(|| panic!("must reuse")).unwrap();
        second.start(Duration::from_secs(1)).unwrap();
        assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
        // Stale cleanup cannot close the new controller's AP.
        first.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 0);
        lifetime.close().unwrap();
        second.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn failed_validation_recreates_retained_hotspot() {
        let lifetime = HotspotLifetime::default();
        let old = Arc::new(Fake::default());
        let first = lifetime.create(|| old.clone()).unwrap();
        first.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_restart();
        first.close().unwrap();
        old.failed.store(true, Ordering::SeqCst);
        let new = Arc::new(Fake::default());
        let second = lifetime.create(|| new.clone()).unwrap();
        second.start(Duration::from_secs(1)).unwrap();
        assert!(!lifetime.reused());
        assert_eq!(old.closes.load(Ordering::SeqCst), 1);
        assert_eq!(new.starts.load(Ordering::SeqCst), 1);
        lifetime.close().unwrap();
    }
    #[test]
    fn validation_failure_after_cleanup_closes_retained_ap() {
        let lifetime = HotspotLifetime::default();
        let fake = Arc::new(Fake::default());
        let lease = lifetime.create(|| fake.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_restart();
        lease.close().unwrap();
        fake.failed.store(true, Ordering::SeqCst);
        assert!(lease.validate_ready().is_err());
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        lifetime.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn hotspot_failure_overrides_armed_restart() {
        let lifetime = HotspotLifetime::default();
        let fake = Arc::new(Fake::default());
        let lease = lifetime.create(|| fake.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_restart();
        fake.failed.store(true, Ordering::SeqCst);
        assert!(lease.validate_ready().is_err());
        lease.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        lifetime.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn retry_keeps_ap_until_the_controller_stops_for_good() {
        let lifetime = HotspotLifetime::default();
        let fake = Arc::new(Fake::default());
        let first = lifetime.create(|| fake.clone()).unwrap();
        first.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_retry();
        first.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 0);
        // The next attempt reuses the AP; a failure before its start keeps it retained.
        let second = lifetime.create(|| panic!("must reuse")).unwrap();
        assert!(lifetime.reused());
        second.start(Duration::from_secs(1)).unwrap();
        assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
        lifetime.keep_for_retry();
        second.close().unwrap();
        lifetime.keep_for_retry();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 0);
        // A hard stop withdraws the retry retention.
        lifetime.cancel_retry().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        lifetime.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn restart_request_wins_over_retry_cancellation() {
        let lifetime = HotspotLifetime::default();
        let fake = Arc::new(Fake::default());
        let first = lifetime.create(|| fake.clone()).unwrap();
        first.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_retry();
        first.close().unwrap();
        // A soft restart arrives while the controller waits to retry.
        lifetime.keep_for_restart();
        lifetime.cancel_retry().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 0);
        let second = lifetime.create(|| panic!("must reuse")).unwrap();
        assert!(lifetime.reused());
        // Cancelling before the attempt releases its lease closes it on release.
        second.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_retry();
        lifetime.cancel_retry().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 0);
        second.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn failed_hotspot_is_not_kept_for_retry() {
        let lifetime = HotspotLifetime::default();
        let fake = Arc::new(Fake::default());
        let lease = lifetime.create(|| fake.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        fake.failed.store(true, Ordering::SeqCst);
        assert!(lease.validate_ready().is_err());
        lifetime.keep_for_retry();
        lease.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        let new = Arc::new(Fake::default());
        let next = lifetime.create(|| new.clone()).unwrap();
        next.start(Duration::from_secs(1)).unwrap();
        assert!(!lifetime.reused());
        assert_eq!(new.starts.load(Ordering::SeqCst), 1);
        lifetime.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn failed_close_keeps_the_previous_lease_for_a_later_attempt() {
        let lifetime = HotspotLifetime::default();
        let fake = Arc::new(Fake::default());
        let lease = lifetime.create(|| fake.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        fake.close_error.store(true, Ordering::SeqCst);
        assert!(lease.close().is_err());
        assert!(lifetime
            .create(|| panic!("previous AP is still up"))
            .is_err());
        assert!(lifetime.close().is_err());
        fake.close_error.store(false, Ordering::SeqCst);
        let new = Arc::new(Fake::default());
        let next = lifetime.create(|| new.clone()).unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        next.start(Duration::from_secs(1)).unwrap();
        lifetime.close().unwrap();
        assert_eq!(new.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn carlink_hold_blocks_new_aps_until_released() {
        let lifetime = HotspotLifetime::default();
        let early = lifetime.create(|| Arc::new(Fake::default())).unwrap();
        lifetime.hold_for_carlink(true);
        assert!(lifetime
            .create(|| panic!("no AP while Carlink holds the radio"))
            .is_err());
        // A lease created before the hold cannot bring its AP up either.
        assert_eq!(
            early.start(Duration::from_secs(1)).unwrap_err().to_string(),
            "Hotspot held for Carlink"
        );
        lifetime.hold_for_carlink(false);
        let fake = Arc::new(Fake::default());
        let lease = lifetime.create(|| fake.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        assert_eq!(fake.starts.load(Ordering::SeqCst), 1);
        lifetime.close().unwrap();
    }
    fn retained(fake: &Arc<Fake>) -> Arc<HotspotLifetime> {
        let lifetime = Arc::new(HotspotLifetime::default());
        let lease = lifetime.create(|| fake.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        lifetime.keep_for_restart();
        lease.close().unwrap();
        fake.block_validation(true);
        lifetime
    }
    fn create_in_background(
        lifetime: &Arc<HotspotLifetime>,
        fake: &Arc<Fake>,
    ) -> std::thread::JoinHandle<bool> {
        let (copy, factory) = (lifetime.clone(), Arc::new(Fake::default()));
        let worker = std::thread::spawn(move || copy.create(|| factory).is_ok());
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !fake.validating.load(Ordering::SeqCst) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        worker
    }
    #[test]
    fn create_does_not_block_the_relay_while_validating() {
        let fake = Arc::new(Fake::default());
        let lifetime = retained(&fake);
        let worker = create_in_background(&lifetime, &fake);
        let (tx, rx) = std::sync::mpsc::channel();
        let copy = lifetime.clone();
        std::thread::spawn(move || {
            copy.keep_for_restart();
            tx.send(copy.reused()).unwrap();
        });
        assert!(!rx.recv_timeout(Duration::from_secs(2)).unwrap());
        fake.block_validation(false);
        assert!(worker.join().unwrap());
        assert!(lifetime.reused());
        lifetime.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn close_while_validating_cancels_the_new_lease() {
        let fake = Arc::new(Fake::default());
        let lifetime = retained(&fake);
        let worker = create_in_background(&lifetime, &fake);
        lifetime.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        fake.block_validation(false);
        assert!(!worker.join().unwrap());
        assert!(!lifetime.reused());
        lifetime.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
    }
    fn within_gap(remaining: Duration, gap: Duration) -> bool {
        remaining > gap - Duration::from_secs(1) && remaining <= gap
    }
    #[test]
    fn closing_an_ap_that_was_up_starts_the_gap_and_reuse_skips_it() {
        let gap = Duration::from_secs(10);
        let lifetime = HotspotLifetime::with_down_gap(gap, None);
        assert_eq!(lifetime.down_remaining(), Duration::ZERO);
        // A lease whose AP never came up leaves nothing to wait for.
        let unused = lifetime.create(|| Arc::new(Fake::default())).unwrap();
        unused.close().unwrap();
        assert_eq!(lifetime.down_remaining(), Duration::ZERO);
        let fake = Arc::new(Fake::default());
        let first = lifetime.create(|| fake.clone()).unwrap();
        first.start(Duration::from_secs(1)).unwrap();
        assert_eq!(lifetime.down_remaining(), Duration::ZERO);
        // An AP kept for a retry or a restart stays up: the next lease starts at once.
        lifetime.keep_for_retry();
        first.close().unwrap();
        assert_eq!(lifetime.down_remaining(), Duration::ZERO);
        let second = lifetime.create(|| panic!("must reuse")).unwrap();
        assert_eq!(lifetime.down_remaining(), Duration::ZERO);
        second.start(Duration::from_secs(1)).unwrap();
        // Released without retention, the AP goes down and the next one waits for the gap.
        second.close().unwrap();
        assert_eq!(fake.closes.load(Ordering::SeqCst), 1);
        assert!(within_gap(lifetime.down_remaining(), gap));
        let next = lifetime.create(|| Arc::new(Fake::default())).unwrap();
        assert!(within_gap(lifetime.down_remaining(), gap));
        drop(next);
        lifetime.close().unwrap();
        // Without a gap nothing is recorded.
        let plain = HotspotLifetime::default();
        let lease = plain.create(|| Arc::new(Fake::default())).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        plain.close().unwrap();
        assert_eq!(plain.down_remaining(), Duration::ZERO);
    }
    #[test]
    fn a_restart_waits_out_the_gap_of_the_previous_run() {
        let gap = Duration::from_secs(10);
        let dir = std::env::temp_dir().join(format!(
            "hotspot-down-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let record = dir.join(AP_DOWN_RECORD);
        // No record yet: the first run starts its AP at once.
        let first = HotspotLifetime::with_down_gap(gap, Some(record.clone()));
        assert_eq!(first.down_remaining(), Duration::ZERO);
        let lease = first.create(|| Arc::new(Fake::default())).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        // Relay shutdown closes the AP and records when.
        first.close().unwrap();
        drop(first);
        let second = HotspotLifetime::with_down_gap(gap, Some(record.clone()));
        assert!(within_gap(second.down_remaining(), gap));
        // A teardown longer ago than the gap, or from another boot, leaves nothing to wait for.
        let now = monotonic_millis();
        std::fs::write(
            &record,
            format!("{} {}\n", now.saturating_sub(20_000), boot_id()),
        )
        .unwrap();
        assert_eq!(
            HotspotLifetime::with_down_gap(gap, Some(record.clone())).down_remaining(),
            Duration::ZERO
        );
        std::fs::write(&record, format!("{now} another-boot\n")).unwrap();
        assert_eq!(
            HotspotLifetime::with_down_gap(gap, Some(record.clone())).down_remaining(),
            Duration::ZERO
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn a_stale_ap_left_up_by_a_killed_run_starts_the_gap_once() {
        let gap = Duration::from_secs(10);
        let dir = std::env::temp_dir().join(format!(
            "hotspot-stale-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let record = dir.join(AP_DOWN_RECORD);
        // The killed run wrote no teardown record: nothing to wait for yet.
        let lifetime = HotspotLifetime::with_down_gap(gap, Some(record.clone()));
        assert_eq!(lifetime.down_remaining(), Duration::ZERO);
        // Its AP was still up and is removed now: the gap starts now, also for a restart.
        assert!(matches!(
            lifetime.remove_stale_once(|| Ok(true)),
            Some(Ok(true))
        ));
        assert!(within_gap(lifetime.down_remaining(), gap));
        assert!(within_gap(
            HotspotLifetime::with_down_gap(gap, Some(record.clone())).down_remaining(),
            gap
        ));
        // The check runs once per process.
        assert!(lifetime
            .remove_stale_once(|| panic!("checked only once"))
            .is_none());
        // Only removed profiles, or a failed check, leave nothing to wait for.
        for result in [Ok(false), Err(std::io::Error::other("nmcli failed"))] {
            let fresh = HotspotLifetime::with_down_gap(gap, None);
            assert!(fresh.remove_stale_once(|| result).is_some());
            assert_eq!(fresh.down_remaining(), Duration::ZERO);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn saved_teardown_yields_at_most_the_gap() {
        let gap = Duration::from_millis(15_000);
        assert_eq!(
            remaining_gap("1000 boot\n", "boot", 4_000, gap),
            Duration::from_millis(12_000)
        );
        assert_eq!(remaining_gap("1000", "", 1_000, gap), gap);
        assert_eq!(
            remaining_gap("1000 boot", "boot", 16_000, gap),
            Duration::ZERO
        );
        assert_eq!(
            remaining_gap("1000 boot", "boot", 99_000, gap),
            Duration::ZERO
        );
        // Another boot, a teardown in the future and unreadable records wait for nothing.
        assert_eq!(
            remaining_gap("1000 old", "boot", 2_000, gap),
            Duration::ZERO
        );
        assert_eq!(remaining_gap("1000", "boot", 2_000, gap), Duration::ZERO);
        assert_eq!(
            remaining_gap("5000 boot", "boot", 2_000, gap),
            Duration::ZERO
        );
        for saved in ["", "boot 1000", "1000 boot extra", "-1 boot"] {
            assert_eq!(remaining_gap(saved, "boot", 2_000, gap), Duration::ZERO);
        }
    }
}
