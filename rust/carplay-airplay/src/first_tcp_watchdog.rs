// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-27
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

use crate::{AirPlayListenerIdentity, AirPlayTcpAccepted};
use std::time::{Duration, Instant};

/// Expired wireless startup phase, emitted once by the host's watchdog poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstTcpTimeout {
    /// No external TCP connection followed the first successful StartSession write within 30 seconds.
    FirstTcp,
    /// TCP arrived but no session became active within the host's optional session timeout.
    Session,
}

/// Host-polled startup watchdog; a session deadline is disabled unless explicitly supplied.
pub struct FirstTcpWatchdog {
    listener: AirPlayListenerIdentity,
    first_start: Option<Instant>,
    first_accept: Option<Instant>,
    session_timeout: Option<Duration>,
    success: bool,
    terminated: bool,
}

impl FirstTcpWatchdog {
    /// Create one watchdog per listener attempt with the existing 30-second first-TCP budget.
    pub fn new(listener: AirPlayListenerIdentity) -> Self {
        Self {
            listener,
            first_start: None,
            first_accept: None,
            session_timeout: None,
            success: false,
            terminated: false,
        }
    }

    /// Opt into a fixed deadline from the first accepted external TCP to session activation.
    pub fn with_session_timeout(mut self, timeout: Duration) -> Self {
        self.session_timeout = Some(timeout);
        self
    }

    /// Arm after the first successful StartSession write; later writes never extend the budget.
    pub fn start_session_sent(&mut self, sent_at: Instant) {
        if !self.terminated && self.first_start.is_none() {
            self.first_start = Some(sent_at);
        }
    }

    /// Accept only this listener's external TCP events; repeated accepts never extend the session budget.
    pub fn accepted(&mut self, event: &AirPlayTcpAccepted, accepted_at: Instant) -> bool {
        if self.terminated
            || event.internal
            || event.listener.id != self.listener.id
            || event.listener.generation != self.listener.generation
        {
            return false;
        }
        self.first_accept.get_or_insert(accepted_at);
        true
    }

    /// Cancel both deadlines when RECORD activates the current attempt; false after termination.
    pub fn session_established(&mut self) -> bool {
        if self.terminated {
            return false;
        }
        self.success = true;
        true
    }

    /// Cancel the attempt and reject subsequent TCP, session and timeout events.
    pub fn terminate(&mut self) -> bool {
        !std::mem::replace(&mut self.terminated, true)
    }

    /// Poll on the host's monotonic clock; the host performs recovery for the single returned timeout.
    pub fn poll(&mut self, now: Instant) -> Option<FirstTcpTimeout> {
        if self.terminated || self.success {
            return None;
        }
        let timeout = match self.first_accept {
            Some(accepted) => self.session_timeout.and_then(|budget| {
                (now.saturating_duration_since(accepted) >= budget)
                    .then_some(FirstTcpTimeout::Session)
            }),
            None => self.first_start.and_then(|started| {
                (now.saturating_duration_since(started) >= Duration::from_secs(30))
                    .then_some(FirstTcpTimeout::FirstTcp)
            }),
        };
        if timeout.is_some() {
            self.terminated = true;
        }
        timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(listener: &AirPlayListenerIdentity, internal: bool) -> AirPlayTcpAccepted {
        AirPlayTcpAccepted {
            listener: listener.clone(),
            internal,
            accepted_at_nanos: 0,
        }
    }

    #[test]
    fn no_start_does_not_arm_and_repeated_start_does_not_extend() {
        let now = Instant::now();
        let mut watch = FirstTcpWatchdog::new(AirPlayListenerIdentity::new(7));
        assert_eq!(watch.poll(now + Duration::from_secs(60)), None);
        watch.start_session_sent(now);
        watch.start_session_sent(now + Duration::from_secs(20));
        assert_eq!(watch.poll(now + Duration::from_secs(29)), None);
        assert_eq!(
            watch.poll(now + Duration::from_secs(30)),
            Some(FirstTcpTimeout::FirstTcp)
        );
        assert_eq!(watch.poll(now + Duration::from_secs(60)), None);
        assert!(!watch.session_established());
        assert!(!watch.terminate());
    }

    #[test]
    fn local_probes_and_other_listeners_cannot_cancel_deadline() {
        let now = Instant::now();
        let owner = AirPlayListenerIdentity::new(7);
        let mut watch = FirstTcpWatchdog::new(owner.clone());
        watch.start_session_sent(now);
        assert!(!watch.accepted(&event(&owner, true), now));
        for other in [
            AirPlayListenerIdentity::new(6),
            AirPlayListenerIdentity::new(7),
        ] {
            assert!(!watch.accepted(&event(&other, false), now));
        }
        assert_eq!(
            watch.poll(now + Duration::from_secs(30)),
            Some(FirstTcpTimeout::FirstTcp)
        );
        assert!(!watch.accepted(&event(&owner, false), now));
    }

    #[test]
    fn early_accept_and_queued_timeout_race_preserve_first_outcome() {
        let now = Instant::now();
        let owner = AirPlayListenerIdentity::new(7);
        for early in [true, false] {
            let mut watch = FirstTcpWatchdog::new(owner.clone());
            if early {
                assert!(watch.accepted(&event(&owner, false), now));
            }
            watch.start_session_sent(now);
            assert!(watch.accepted(&event(&owner, false), now + Duration::from_secs(30)));
            assert_eq!(watch.poll(now + Duration::from_secs(60)), None);
            assert!(watch.session_established());
        }
    }

    #[test]
    fn opt_in_session_deadline_never_extends_on_repeated_accepts() {
        let now = Instant::now();
        let owner = AirPlayListenerIdentity::new(7);
        let mut watch =
            FirstTcpWatchdog::new(owner.clone()).with_session_timeout(Duration::from_secs(20));
        watch.start_session_sent(now);
        assert!(watch.accepted(&event(&owner, false), now + Duration::from_secs(5)));
        assert!(watch.accepted(&event(&owner, false), now + Duration::from_secs(10)));
        assert_eq!(watch.poll(now + Duration::from_secs(24)), None);
        assert_eq!(
            watch.poll(now + Duration::from_secs(25)),
            Some(FirstTcpTimeout::Session)
        );
        assert!(!watch.session_established());
        assert_eq!(watch.poll(now + Duration::from_secs(60)), None);
    }

    #[test]
    fn activation_and_cancellation_reject_queued_timeouts_for_both_phases() {
        let now = Instant::now();
        let owner = AirPlayListenerIdentity::new(7);
        for accepted in [true, false] {
            for established in [true, false] {
                let mut old = FirstTcpWatchdog::new(owner.clone())
                    .with_session_timeout(Duration::from_secs(20));
                old.start_session_sent(now);
                if accepted {
                    old.accepted(&event(&owner, false), now);
                }
                if established {
                    assert!(old.session_established());
                } else {
                    assert!(old.terminate());
                }
                let mut current = FirstTcpWatchdog::new(AirPlayListenerIdentity::new(8));
                current.start_session_sent(now + Duration::from_secs(60));
                assert_eq!(old.poll(now + Duration::from_secs(60)), None);
                assert_eq!(current.poll(now + Duration::from_secs(60)), None);
            }
        }
    }

    #[test]
    fn concurrent_accept_and_timeout_have_exactly_one_outcome() {
        use std::sync::{Barrier, Mutex};
        for _ in 0..50 {
            let now = Instant::now();
            let owner = AirPlayListenerIdentity::new(7);
            let mut watch = FirstTcpWatchdog::new(owner.clone());
            watch.start_session_sent(now);
            let watch = Mutex::new(watch);
            let start = Barrier::new(2);
            std::thread::scope(|scope| {
                let accepted = scope.spawn(|| {
                    start.wait();
                    watch.lock().unwrap().accepted(&event(&owner, false), now)
                });
                let expired = scope.spawn(|| {
                    start.wait();
                    watch
                        .lock()
                        .unwrap()
                        .poll(now + Duration::from_secs(30))
                        .is_some()
                });
                assert_ne!(accepted.join().unwrap(), expired.join().unwrap());
            });
        }
    }
}
