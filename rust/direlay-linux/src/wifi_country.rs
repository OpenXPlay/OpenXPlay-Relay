// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23

use crate::{
    platform::{command, CommandRunner},
    Result,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
/// Runtime persistence only: every process applies the configured code once, then on changes.
pub struct WifiCountry {
    configured: Mutex<Option<String>>,
    runner: Arc<dyn CommandRunner>,
}
impl WifiCountry {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            configured: Mutex::new(None),
            runner,
        }
    }
    pub fn apply(&self, country: &str, probe: impl FnOnce()) -> Result<()> {
        let mut configured = self.configured.lock().unwrap();
        if configured.as_deref() == Some(country) {
            return Ok(());
        }
        *configured = Some(country.into());
        if country == "system" {
            return Ok(());
        }
        let result = command(
            self.runner.as_ref(),
            &["iw", "reg", "set", country],
            Duration::from_secs(10),
        );
        // A failed set still needs a fresh effective domain and channel inventory for the warning.
        probe();
        result.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::CommandResult;
    #[derive(Default)]
    struct Runner(Mutex<Vec<Vec<String>>>);
    impl CommandRunner for Runner {
        fn run(&self, args: &[String], _: Duration) -> Result<CommandResult> {
            self.0.lock().unwrap().push(args.to_vec());
            Ok(CommandResult {
                exit_code: 0,
                output: String::new(),
            })
        }
    }
    #[test]
    fn system_is_untouched_codes_apply_once_per_start_and_change_then_probe() {
        let runner = Arc::new(Runner::default());
        let control = WifiCountry::new(runner.clone());
        control
            .apply("system", || panic!("system must not probe"))
            .unwrap();
        assert!(runner.0.lock().unwrap().is_empty());
        let mut probes = 0;
        for country in ["CN", "CN", "00", "00", "system", "US"] {
            control
                .apply(country, || {
                    assert_eq!(
                        runner.0.lock().unwrap().last().unwrap(),
                        &["iw", "reg", "set", country]
                    );
                    probes += 1;
                })
                .unwrap();
        }
        assert_eq!(probes, 3);
        assert_eq!(runner.0.lock().unwrap().len(), 3);
        WifiCountry::new(runner.clone())
            .apply("US", || probes += 1)
            .unwrap();
        assert_eq!(probes, 4, "every new process reapplies its code");
    }
}
