// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use std::{time::Duration, time::Instant};

/// State owned by `Iap2ControlDeadline`; construct through its factory.
pub struct Iap2ControlDeadline {
    timeout: Duration,
    started: Instant,
    unlimited: bool,
    authenticated: bool,
}
impl Iap2ControlDeadline {
    pub fn new(timeout: Duration, now: Instant) -> Self {
        Self {
            timeout,
            started: now,
            unlimited: timeout == Duration::MAX || timeout.as_millis() >= i64::MAX as u128,
            authenticated: false,
        }
    }

    pub fn authenticated(&mut self) {
        self.authenticated = true;
    }

    pub fn remaining_millis(&self, now: Instant) -> u64 {
        if self.unlimited && self.authenticated {
            return 30000;
        }
        let budget = if self.unlimited {
            Duration::from_secs(60)
        } else {
            self.timeout
        };
        let remaining = budget.saturating_sub(now.saturating_duration_since(self.started));
        remaining.as_nanos().div_ceil(1_000_000).min(30000) as u64
    }
}

/// Protocol message, field, or framing constant.
pub const HANDSHAKE_MILLIS: u64 = 60_000;
/// Protocol message, field, or framing constant.
pub const MAX_POLL_MILLIS: u64 = 30_000;
