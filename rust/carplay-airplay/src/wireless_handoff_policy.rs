// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-30
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

//! Host policies for a wireless attempt's handoff: releasing the Bluetooth bootstrap after the
//! iPhone asked for it, and giving an iPhone that left before RECORD time to come back. Both are
//! off by default. The host feeds its sessions' events, polls with its own clock and runs the
//! returned actions outside its locks.

use std::time::{Duration, Instant};

/// How long after the iPhone's handoff request the bootstrap may be released.
pub const BLUETOOTH_RELEASE_GRACE: Duration = Duration::from_secs(5);
/// The least time an iPhone whose session ended before RECORD gets to reconnect.
pub const IPHONE_RECONNECT_GRACE: Duration = Duration::from_secs(25);

/// The host's choices for one attempt; the library default keeps both off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WirelessHandoffOptions {
    /// Close the Bluetooth bootstrap when neither tunnel iAP2 nor video followed the request.
    pub release_bluetooth_on_request: bool,
    /// Keep the receiver up for an iPhone whose session ended before RECORD.
    pub await_iphone_reconnect: bool,
}

/// What the release decision reads when the grace after the handoff request ends.
#[derive(Clone, Copy, Debug)]
pub struct BootstrapReleaseGuard {
    pub closed: bool,
    pub wireless: bool,
    pub same_generation: bool,
    pub same_session: bool,
    pub failure_reported: bool,
    pub handoff_requested: bool,
    pub tunnel_ready: bool,
    pub wireless_active: bool,
    pub frame_rendered: bool,
    pub bootstrap_released: bool,
}
impl BootstrapReleaseGuard {
    /// The first condition that keeps the bootstrap, or `None` when it may be released.
    pub fn blocked_reason(self) -> Option<&'static str> {
        [
            (self.closed, "closed"),
            (!self.wireless, "not-wireless"),
            (!self.same_generation, "stale-generation"),
            (!self.same_session, "stale-session"),
            (self.failure_reported, "failure"),
            (!self.handoff_requested, "no-request"),
            (self.tunnel_ready, "tunnel-ready"),
            (self.wireless_active, "wireless-active"),
            (self.frame_rendered, "video-rendered"),
            (self.bootstrap_released, "already-released"),
        ]
        .into_iter()
        .find_map(|(blocked, reason)| blocked.then_some(reason))
    }
    pub fn should_release(self) -> bool {
        self.blocked_reason().is_none()
    }
}

/// After the release, the bootstrap's error is its expected end while the attempt's session is active.
pub fn tolerates_bootstrap_error(
    bootstrap_released: bool,
    same_generation: bool,
    session_active: bool,
) -> bool {
    bootstrap_released && same_generation && session_active
}
/// A pending reconnect wait ends once a newer session is active.
pub fn should_cancel_restart(
    enabled: bool,
    activations_at_schedule: u64,
    activations: u64,
    session_active: bool,
) -> bool {
    enabled && activations != activations_at_schedule && session_active
}
/// The least wait after a session ended: only a wireless one that never became active waits.
pub fn minimum_reconnect_delay(enabled: bool, wireless: bool, was_active: bool) -> Duration {
    if enabled && wireless && !was_active {
        IPHONE_RECONNECT_GRACE
    } else {
        Duration::ZERO
    }
}
/// The minimum extends a shorter wait and never shortens a longer one.
pub fn reconnect_delay(computed: Duration, minimum: Duration) -> Duration {
    computed.max(minimum)
}

/// What the attempt's AirPlay sessions report, each with the session's identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WirelessSessionEvent {
    /// RECORD made the session the attempt's active one.
    Active(u64),
    Ended(u64),
    /// The session's control connection failed; its end follows.
    Failed(u64),
    /// A frame of the session's main screen reached a display.
    Rendered(u64),
    /// The session asked the accessory to give up Bluetooth (`disableBluetooth`).
    HandoffRequested(u64),
    /// Pairing or MFi authentication of the session failed; waiting cannot change it.
    AuthenticationFailed(u64),
}

/// What the host does and logs; [`HandoffAction::Release`] is the only one that touches a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffAction {
    ReleaseScheduled {
        session: u64,
    },
    /// Close the attempt's Bluetooth bootstrap; AirPlay, the access point and Bonjour stay.
    Release {
        session: u64,
    },
    ReleaseSkipped {
        session: u64,
        reason: &'static str,
    },
    /// The session's control failure ends only that session.
    FailureKept {
        session: u64,
    },
    ReconnectWait {
        session: u64,
        wait: Duration,
    },
    ReconnectCancelled {
        session: u64,
    },
    ReconnectEnded {
        reason: &'static str,
    },
}

/// What happens when the handoff deadline passes without tunnel iAP2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffDeadline {
    /// A reconnect wait still runs; the deadline moves to its end.
    Wait(Instant),
    /// The bootstrap was released and the active session shows video: it goes on without it.
    Fallback,
    Fail,
}

#[derive(Clone, Copy)]
struct Reconnect {
    session: u64,
    deadline: Instant,
    activations: u64,
}

/// One attempt's handoff state. The host keeps one per attempt and feeds it only that attempt's
/// events, so generation and closure checks stay with the host.
pub struct WirelessHandoffPolicy {
    options: WirelessHandoffOptions,
    active: Option<u64>,
    activations: u64,
    rendered: bool,
    requested: bool,
    release_due: Option<(u64, Instant)>,
    released: bool,
    reconnect: Option<Reconnect>,
    reconnect_used: bool,
    rejected: Vec<u64>,
}
impl WirelessHandoffPolicy {
    pub fn new(options: WirelessHandoffOptions) -> Self {
        Self {
            options,
            active: None,
            activations: 0,
            rendered: false,
            requested: false,
            release_due: None,
            released: false,
            reconnect: None,
            reconnect_used: false,
            rejected: Vec::new(),
        }
    }
    pub fn options(&self) -> WirelessHandoffOptions {
        self.options
    }
    pub fn bootstrap_released(&self) -> bool {
        self.released
    }
    /// Whether a bootstrap error is the expected end of the released bootstrap.
    pub fn tolerates_bootstrap_error(&self) -> bool {
        tolerates_bootstrap_error(self.released, true, self.active.is_some())
    }
    /// While a reconnect wait runs, the host holds its startup deadlines.
    pub fn startup_deferred(&self, now: Instant) -> bool {
        self.reconnect.is_some_and(|r| now < r.deadline)
    }
    /// The next instant [`Self::poll`] has something to do.
    pub fn next_due(&self) -> Option<Instant> {
        let release = self.release_due.map(|(_, due)| due);
        let reconnect = self.reconnect.map(|r| r.deadline);
        release.into_iter().chain(reconnect).min()
    }
    pub fn observe(&mut self, event: WirelessSessionEvent, now: Instant) -> Vec<HandoffAction> {
        use HandoffAction as A;
        use WirelessSessionEvent as E;
        match event {
            E::Active(id) => {
                if self.active != Some(id) {
                    self.rendered = false;
                }
                self.active = Some(id);
                self.activations += 1;
                match self.reconnect {
                    Some(r)
                        if should_cancel_restart(
                            self.options.await_iphone_reconnect,
                            r.activations,
                            self.activations,
                            true,
                        ) =>
                    {
                        self.reconnect = None;
                        vec![A::ReconnectCancelled { session: r.session }]
                    }
                    _ => vec![],
                }
            }
            E::Ended(id) => {
                if self.active == Some(id) {
                    // The host fails the attempt for an active session's end, as without the option.
                    self.active = None;
                    self.rendered = false;
                    return vec![];
                }
                if self.active.is_some() || self.rejected.contains(&id) || self.reconnect_used {
                    return vec![];
                }
                let wait = reconnect_delay(
                    Duration::ZERO,
                    minimum_reconnect_delay(self.options.await_iphone_reconnect, true, false),
                );
                if wait.is_zero() {
                    return vec![];
                }
                self.reconnect_used = true;
                self.reconnect = Some(Reconnect {
                    session: id,
                    deadline: now + wait,
                    activations: self.activations,
                });
                vec![A::ReconnectWait { session: id, wait }]
            }
            E::Failed(id) => {
                if self.options.await_iphone_reconnect
                    && self.active != Some(id)
                    && !self.rejected.contains(&id)
                {
                    vec![A::FailureKept { session: id }]
                } else {
                    vec![]
                }
            }
            E::Rendered(id) => {
                if self.active == Some(id) {
                    self.rendered = true;
                }
                vec![]
            }
            E::HandoffRequested(id) => {
                // Only the attempt's first request arms the release, so a repeat never moves it.
                if std::mem::replace(&mut self.requested, true)
                    || !self.options.release_bluetooth_on_request
                {
                    return vec![];
                }
                self.release_due = Some((id, now + BLUETOOTH_RELEASE_GRACE));
                vec![A::ReleaseScheduled { session: id }]
            }
            E::AuthenticationFailed(id) => {
                self.rejected.push(id);
                self.reconnect_used = true;
                match self.reconnect.take() {
                    Some(_) => vec![A::ReconnectEnded {
                        reason: "authentication",
                    }],
                    None => vec![],
                }
            }
        }
    }
    /// Runs what is due. `tunnel_ready` and `handoff_complete` are the attempt's own facts.
    pub fn poll(
        &mut self,
        now: Instant,
        tunnel_ready: bool,
        handoff_complete: bool,
    ) -> Vec<HandoffAction> {
        let mut actions = Vec::new();
        if let Some((session, due)) = self.release_due {
            if now >= due {
                self.release_due = None;
                let guard = BootstrapReleaseGuard {
                    closed: false,
                    wireless: true,
                    same_generation: true,
                    same_session: self.active == Some(session),
                    failure_reported: false,
                    handoff_requested: self.requested,
                    tunnel_ready,
                    wireless_active: handoff_complete,
                    frame_rendered: self.rendered,
                    bootstrap_released: self.released,
                };
                actions.push(match guard.blocked_reason() {
                    Some(reason) => HandoffAction::ReleaseSkipped { session, reason },
                    None => {
                        self.released = true;
                        HandoffAction::Release { session }
                    }
                });
            }
        }
        if self.reconnect.is_some_and(|r| now >= r.deadline) {
            self.reconnect = None;
            actions.push(HandoffAction::ReconnectEnded { reason: "expired" });
        }
        actions
    }
    /// The outcome once the host's handoff deadline passed without tunnel iAP2.
    pub fn deadline(&self, now: Instant) -> HandoffDeadline {
        match self.reconnect {
            Some(r) if now < r.deadline => HandoffDeadline::Wait(r.deadline),
            _ if self.released && self.active.is_some() && self.rendered => {
                HandoffDeadline::Fallback
            }
            _ => HandoffDeadline::Fail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use HandoffAction as A;
    use WirelessSessionEvent as E;

    fn policy(release: bool, reconnect: bool) -> WirelessHandoffPolicy {
        WirelessHandoffPolicy::new(WirelessHandoffOptions {
            release_bluetooth_on_request: release,
            await_iphone_reconnect: reconnect,
        })
    }
    fn guard() -> BootstrapReleaseGuard {
        BootstrapReleaseGuard {
            closed: false,
            wireless: true,
            same_generation: true,
            same_session: true,
            failure_reported: false,
            handoff_requested: true,
            tunnel_ready: false,
            wireless_active: false,
            frame_rendered: false,
            bootstrap_released: false,
        }
    }
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }
    /// An attempt whose session 1 is active and asked for the handoff at `t`.
    fn requested(release: bool, t: Instant) -> WirelessHandoffPolicy {
        let mut p = policy(release, true);
        p.observe(E::Active(1), t);
        p.observe(E::HandoffRequested(1), t);
        p
    }

    #[test]
    fn library_defaults_keep_both_experiments_off() {
        assert_eq!(
            WirelessHandoffOptions::default(),
            WirelessHandoffOptions {
                release_bluetooth_on_request: false,
                await_iphone_reconnect: false,
            }
        );
        let t = Instant::now();
        let mut p = WirelessHandoffPolicy::new(Default::default());
        p.observe(E::Active(1), t);
        assert!(p.observe(E::HandoffRequested(1), t).is_empty());
        assert!(p.observe(E::Failed(2), t).is_empty());
        assert!(p.observe(E::Ended(2), t).is_empty());
        assert!(p.poll(t + Duration::from_secs(60), false, false).is_empty());
        assert_eq!(p.next_due(), None);
    }
    #[test]
    fn each_condition_blocks_the_release_on_its_own() {
        assert!(guard().should_release());
        type Block = fn(&mut BootstrapReleaseGuard);
        let cases: [(Block, &str); 10] = [
            (|g| g.closed = true, "closed"),
            (|g| g.wireless = false, "not-wireless"),
            (|g| g.same_generation = false, "stale-generation"),
            (|g| g.same_session = false, "stale-session"),
            (|g| g.failure_reported = true, "failure"),
            (|g| g.handoff_requested = false, "no-request"),
            (|g| g.tunnel_ready = true, "tunnel-ready"),
            (|g| g.wireless_active = true, "wireless-active"),
            (|g| g.frame_rendered = true, "video-rendered"),
            (|g| g.bootstrap_released = true, "already-released"),
        ];
        for (block, reason) in cases {
            let mut g = guard();
            block(&mut g);
            assert!(!g.should_release(), "{reason}");
            assert_eq!(g.blocked_reason(), Some(reason));
        }
    }
    #[test]
    fn only_the_current_released_attempt_with_an_active_session_tolerates_bootstrap_errors() {
        for released in [false, true] {
            for current in [false, true] {
                for active in [false, true] {
                    assert_eq!(
                        tolerates_bootstrap_error(released, current, active),
                        released && current && active
                    );
                }
            }
        }
    }
    #[test]
    fn reconnect_cancels_only_for_a_newer_active_session_and_waits_only_before_record() {
        for enabled in [false, true] {
            for newer in [false, true] {
                for active in [false, true] {
                    assert_eq!(
                        should_cancel_restart(enabled, 3, if newer { 4 } else { 3 }, active),
                        enabled && newer && active
                    );
                }
            }
            for wireless in [false, true] {
                for was_active in [false, true] {
                    assert_eq!(
                        minimum_reconnect_delay(enabled, wireless, was_active),
                        if enabled && wireless && !was_active {
                            IPHONE_RECONNECT_GRACE
                        } else {
                            Duration::ZERO
                        }
                    );
                }
            }
        }
        for (computed, expected) in [(2, 25), (15, 25), (25, 25), (30, 30)] {
            assert_eq!(
                reconnect_delay(Duration::from_secs(computed), IPHONE_RECONNECT_GRACE),
                Duration::from_secs(expected)
            );
        }
        for delay in [0, 2, 15, 30] {
            let delay = Duration::from_secs(delay);
            assert_eq!(reconnect_delay(delay, Duration::ZERO), delay);
        }
    }
    #[test]
    fn release_comes_five_seconds_after_the_first_request_and_only_once() {
        let t = Instant::now();
        let mut p = requested(true, t);
        assert_eq!(p.next_due(), Some(t + BLUETOOTH_RELEASE_GRACE));
        assert!(p.poll(t + ms(4_999), false, false).is_empty());
        // A repeated request neither re-arms nor moves the release.
        assert!(p.observe(E::HandoffRequested(1), t + ms(4_999)).is_empty());
        assert_eq!(
            p.poll(t + ms(5_000), false, false),
            [A::Release { session: 1 }]
        );
        assert!(p.bootstrap_released());
        assert!(p.poll(t + Duration::from_secs(60), false, false).is_empty());
        assert!(p.observe(E::HandoffRequested(1), t + ms(6_000)).is_empty());
        assert_eq!(p.next_due(), None);
    }
    #[test]
    fn tunnel_video_handoff_and_a_stale_session_keep_the_bootstrap() {
        let t = Instant::now();
        let due = t + BLUETOOTH_RELEASE_GRACE;
        let skipped = |reason| [A::ReleaseSkipped { session: 1, reason }];
        assert_eq!(
            requested(true, t).poll(due, true, false),
            skipped("tunnel-ready")
        );
        assert_eq!(
            requested(true, t).poll(due, false, true),
            skipped("wireless-active")
        );
        let mut p = requested(true, t);
        p.observe(E::Rendered(1), t);
        assert_eq!(p.poll(due, false, false), skipped("video-rendered"));
        let mut p = requested(true, t);
        p.observe(E::Ended(1), t);
        assert_eq!(p.poll(due, false, false), skipped("stale-session"));
        let mut p = requested(true, t);
        p.observe(E::Active(2), t);
        assert_eq!(p.poll(due, false, false), skipped("stale-session"));
        // A request from a session that is not yet active needs it to be active at the release.
        let mut p = policy(true, false);
        p.observe(E::HandoffRequested(1), t);
        assert_eq!(p.poll(due, false, false), skipped("stale-session"));
        let mut p = policy(true, false);
        p.observe(E::HandoffRequested(1), t);
        p.observe(E::Active(1), t + ms(100));
        assert_eq!(p.poll(due, false, false), [A::Release { session: 1 }]);
    }
    #[test]
    fn without_the_option_a_request_schedules_nothing() {
        let t = Instant::now();
        let mut p = requested(false, t);
        assert!(p.poll(t + Duration::from_secs(60), false, false).is_empty());
        assert!(!p.bootstrap_released());
        assert!(!p.tolerates_bootstrap_error());
    }
    #[test]
    fn only_a_released_attempt_whose_active_session_shows_video_falls_back() {
        let t = Instant::now();
        let late = t + Duration::from_secs(50);
        let mut p = requested(true, t);
        p.poll(t + BLUETOOTH_RELEASE_GRACE, false, false);
        assert!(p.tolerates_bootstrap_error());
        assert_eq!(p.deadline(late), HandoffDeadline::Fail);
        p.observe(E::Rendered(2), late);
        assert_eq!(p.deadline(late), HandoffDeadline::Fail);
        p.observe(E::Rendered(1), late);
        assert_eq!(p.deadline(late), HandoffDeadline::Fallback);
        p.observe(E::Ended(1), late);
        assert!(!p.tolerates_bootstrap_error());
        assert_eq!(p.deadline(late), HandoffDeadline::Fail);
        // Video without a release changes nothing.
        let mut p = requested(false, t);
        p.observe(E::Rendered(1), t);
        assert_eq!(p.deadline(late), HandoffDeadline::Fail);
    }
    #[test]
    fn a_session_that_ends_before_record_gets_one_fixed_wait() {
        let t = Instant::now();
        let mut p = policy(false, true);
        assert_eq!(
            p.observe(E::Ended(1), t),
            [A::ReconnectWait {
                session: 1,
                wait: IPHONE_RECONNECT_GRACE
            }]
        );
        assert!(p.startup_deferred(t + ms(24_999)));
        // Later sessions that also leave before RECORD never extend it.
        for id in 2..8 {
            assert!(p
                .observe(E::Ended(id), t + Duration::from_secs(id))
                .is_empty());
        }
        assert_eq!(p.next_due(), Some(t + IPHONE_RECONNECT_GRACE));
        assert_eq!(
            p.deadline(t + ms(24_999)),
            HandoffDeadline::Wait(t + IPHONE_RECONNECT_GRACE)
        );
        assert!(p.poll(t + ms(24_999), false, false).is_empty());
        assert_eq!(
            p.poll(t + IPHONE_RECONNECT_GRACE, false, false),
            [A::ReconnectEnded { reason: "expired" }]
        );
        assert!(!p.startup_deferred(t + IPHONE_RECONNECT_GRACE));
        assert!(p
            .observe(E::Ended(9), t + Duration::from_secs(30))
            .is_empty());
        assert_eq!(
            p.deadline(t + Duration::from_secs(30)),
            HandoffDeadline::Fail
        );
    }
    #[test]
    fn a_newer_active_session_cancels_the_wait_and_old_sessions_cannot_end_it() {
        let t = Instant::now();
        let mut p = policy(false, true);
        p.observe(E::Ended(1), t);
        assert_eq!(
            p.observe(E::Active(2), t + ms(10_000)),
            [A::ReconnectCancelled { session: 1 }]
        );
        assert!(!p.startup_deferred(t + ms(10_000)));
        assert_eq!(
            p.observe(E::Failed(3), t + ms(11_000)),
            [A::FailureKept { session: 3 }]
        );
        assert!(p.observe(E::Ended(3), t + ms(11_000)).is_empty());
        assert!(p.poll(t + Duration::from_secs(60), false, false).is_empty());
        // The active session's own failure and end stay the host's to report.
        assert!(p.observe(E::Failed(2), t + ms(12_000)).is_empty());
        assert!(p.observe(E::Ended(2), t + ms(12_000)).is_empty());
        assert!(!p.startup_deferred(t + ms(12_000)));
    }
    #[test]
    fn active_ends_option_off_and_authentication_failures_get_no_wait() {
        let t = Instant::now();
        let mut p = policy(true, false);
        assert!(p.observe(E::Failed(1), t).is_empty());
        assert!(p.observe(E::Ended(1), t).is_empty());
        let mut p = policy(false, true);
        p.observe(E::Active(1), t);
        assert!(p.observe(E::Ended(1), t).is_empty());
        assert!(!p.startup_deferred(t));
        // A failed authentication keeps its session's failure and ends a running wait.
        let mut p = policy(false, true);
        p.observe(E::Ended(1), t);
        assert_eq!(
            p.observe(E::AuthenticationFailed(2), t),
            [A::ReconnectEnded {
                reason: "authentication"
            }]
        );
        assert!(p.observe(E::Failed(2), t).is_empty());
        assert!(p.observe(E::Ended(2), t).is_empty());
        assert!(!p.startup_deferred(t));
        assert_eq!(p.deadline(t), HandoffDeadline::Fail);
        let mut p = policy(false, true);
        p.observe(E::AuthenticationFailed(1), t);
        assert!(p.observe(E::Ended(1), t).is_empty());
        assert!(p.observe(E::Ended(2), t).is_empty());
    }
}
