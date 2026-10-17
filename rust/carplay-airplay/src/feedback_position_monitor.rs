// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use std::sync::Mutex;
pub const OVERRUN_SECONDS: f64 = 0.1;
pub struct FeedbackPositionMonitor {
    clock_rate: u32,
    repeat_ns: u64,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    received: Option<u32>,
    baseline: bool,
    last_overrun: Option<u64>,
}
impl FeedbackPositionMonitor {
    pub fn new(clock_rate: u32) -> Self {
        Self::with_repeat(clock_rate, 10_000_000_000)
    }
    pub fn with_repeat(clock_rate: u32, repeat_ns: u64) -> Self {
        Self {
            clock_rate,
            repeat_ns,
            state: Mutex::new(State::default()),
        }
    }
    pub fn samples_ahead(sample: u32, base: u32) -> i64 {
        sample.wrapping_sub(base) as i32 as i64
    }
    pub fn observe_rtp(&self, sample: u32) {
        let mut s = self.state.lock().unwrap();
        if s.received
            .is_none_or(|old| Self::samples_ahead(sample, old) > 0)
        {
            s.received = Some(sample);
        }
    }
    pub fn on_feedback(
        &self,
        kind: u16,
        connection_id: u64,
        reported: u64,
        now_ns: u64,
    ) -> Option<String> {
        let mut s = self.state.lock().unwrap();
        let received = s.received?;
        let ahead = Self::samples_ahead(reported as u32, received);
        let overrun = ahead as f64 > OVERRUN_SECONDS * self.clock_rate as f64;
        if s.baseline
            && (!overrun
                || s.last_overrun
                    .is_some_and(|ns| now_ns.saturating_sub(ns) < self.repeat_ns))
        {
            return None;
        }
        s.baseline = true;
        if overrun {
            s.last_overrun = Some(now_ns);
        }
        Some(format!("Audio: feedback type={kind} id={connection_id} reported={reported} received={received} aheadMs={}{}", ahead * 1000 / self.clock_rate as i64, if overrun { " overrun=true" } else { "" }))
    }
}
