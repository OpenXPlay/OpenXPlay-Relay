// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

pub struct RtpClockProbe {
    first: Option<(u32, u64)>,
    done: bool,
    window_ns: u64,
}
impl RtpClockProbe {
    pub fn new(_clock_rate: u32) -> Self {
        Self::with_window(2_000_000_000)
    }
    pub fn with_window(window_ns: u64) -> Self {
        Self {
            first: None,
            done: false,
            window_ns,
        }
    }
    pub fn observe(&mut self, sample: u32, arrival_nanos: u64) -> Option<String> {
        if self.done {
            return None;
        }
        let Some((first, ns)) = self.first else {
            self.first = Some((sample, arrival_nanos));
            return None;
        };
        let elapsed = arrival_nanos.saturating_sub(ns);
        if elapsed < self.window_ns || elapsed == 0 {
            return None;
        }
        self.done = true;
        Some(format!(
            "{}",
            (sample.wrapping_sub(first) as f64 * 1e9 / elapsed as f64).round() as u32
        ))
    }
}
