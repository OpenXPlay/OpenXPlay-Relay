// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use std::time::{Duration, Instant};

pub type StartGate = std::sync::Arc<dyn Fn() -> crate::Result<bool> + Send + Sync>;

/// One start owns both budgets; polling and controller retries cannot extend them.
#[derive(Default)]
pub struct StartPolicy {
    carlink: Option<Instant>,
    screen: Option<Instant>,
    requested: bool,
    failed: bool,
}
impl StartPolicy {
    pub fn request(&mut self, now: Instant, carlink: bool, screen_known: bool) {
        self.failed = false;
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
        self.requested = true;
        self.carlink = None;
        self.screen = None;
    }
    pub fn fail(&mut self) {
        self.skip();
        self.failed = true;
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
