// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-27
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

use crate::Result;

/// Host-polled wireless network guard; check once a second per attempt, disabled by default.
#[derive(Default)]
pub struct WirelessNetworkWatch {
    enabled: bool,
    failures: u8,
}

impl WirelessNetworkWatch {
    /// Capture the host's recovery opt-in at the beginning of one wireless attempt.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            failures: 0,
        }
    }

    /// Run the host validator when enabled; reset on success and return the third consecutive failure for recovery.
    pub fn check(&mut self, validate: impl FnOnce() -> Result<()>) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        match validate() {
            Ok(()) => {
                self.failures = 0;
                Ok(())
            }
            Err(error) => {
                self.failures = self.failures.saturating_add(1);
                if self.failures >= 3 {
                    Err(error)
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_explicit_opt_out_never_observe_network() {
        for mut watch in [
            WirelessNetworkWatch::default(),
            WirelessNetworkWatch::new(false),
        ] {
            watch
                .check(|| panic!("recovery must be host opt-in"))
                .unwrap();
        }
    }

    #[test]
    fn transient_failures_reset_and_three_consecutive_failures_recover() {
        let mut watch = WirelessNetworkWatch::new(true);
        let fail = || Err(std::io::Error::other("network changed"));
        watch.check(fail).unwrap();
        watch.check(fail).unwrap();
        watch.check(|| Ok(())).unwrap();
        watch.check(fail).unwrap();
        watch.check(fail).unwrap();
        assert_eq!(
            watch.check(fail).unwrap_err().to_string(),
            "network changed"
        );
        WirelessNetworkWatch::new(true).check(fail).unwrap();
    }
}
