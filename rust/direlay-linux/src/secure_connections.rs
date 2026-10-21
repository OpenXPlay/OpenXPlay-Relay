// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! The relay's Bluetooth Secure Connections (SC) policy: on, and off only while a pairing window
//! is open. Some controllers cannot pair an iPhone while SC is on (the UFI003's WCN36xx fails
//! with `Simple Pairing Complete: Authentication Failure`), yet an iPhone bonded there reconnects
//! only while SC is on (with it off the iPhone ends the link right after encryption).
//!
//! bluetoothd turns SC on whenever it initialises an adapter, so the policy is applied again each
//! time the adapter becomes ready. BlueZ has no D-Bus property for SC; `btmgmt` switches it.
use crate::bluetooth_wake::{BluetoothChange, BluetoothSubscription, BluetoothWake};
use crate::platform::{CommandRunner, JsonEvents};
use crate::Result;
use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

/// Bound of one switch: `btmgmt` answers within milliseconds, and a hung one must not hold the
/// pairing or controller code that waits for it.
pub const SWITCH_TIMEOUT: Duration = Duration::from_secs(3);
/// First delay before a failed switch is tried again; it doubles per failure up to the maximum.
pub const RETRY_INITIAL: Duration = Duration::from_secs(2);
pub const RETRY_MAX: Duration = Duration::from_secs(30);

/// The index N of the selected adapter `hciN`; `None` means it is not present yet.
pub type AdapterIndex = Arc<dyn Fn() -> Option<u16> + Send + Sync>;

/// The `btmgmt` command for one switch. BlueZ 5.55 `btmgmt` quits before the controller answers
/// when its stdin is closed or `/dev/null` (as for a systemd service), so its stdin is its own
/// output pipe (`<&1`): open and never readable.
pub fn switch_command(index: u16, enable: bool) -> Vec<String> {
    vec![
        "sh".into(),
        "-c".into(),
        format!("exec btmgmt --index {index} sc {} <&1", word(enable)),
    ]
}
/// Like the SC switch, `info` must keep stdin open on BlueZ 5.55.
pub fn info_command(index: u16) -> Vec<String> {
    vec![
        "sh".into(),
        "-c".into(),
        format!("exec btmgmt --index {index} info <&1"),
    ]
}
fn word(enable: bool) -> &'static str {
    if enable {
        "on"
    } else {
        "off"
    }
}

/// Why a switch failed.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// The controller has no SC: nothing is switched any more.
    Unsupported(String),
    /// `btmgmt` is missing: tried again only on the next request.
    Unavailable(String),
    /// Anything else, e.g. no adapter yet: tried again after a delay.
    Failed(String),
}
/// Checks `btmgmt`'s reply: its new settings must show SC as requested. An empty reply is a
/// failure, since a `btmgmt` that cannot read its stdin exits 0 without doing anything.
pub(crate) fn verify(
    exit_code: i32,
    output: &str,
    enable: bool,
) -> std::result::Result<(), Failure> {
    let summary = output
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("no reply from btmgmt")
        .chars()
        .take(200)
        .collect::<String>();
    if exit_code == 127 {
        return Err(Failure::Unavailable(summary));
    }
    if output.contains("Not Supported") {
        return Err(Failure::Unsupported(summary));
    }
    let settings = output.lines().find_map(|l| {
        let (_, rest) = l.split_once("settings:")?;
        Some(rest.split_whitespace().any(|s| s == "secure-conn"))
    });
    match settings {
        Some(on) if exit_code == 0 && on == enable => Ok(()),
        _ => Err(Failure::Failed(summary)),
    }
}

#[derive(Default)]
struct State {
    /// Open pairing windows; SC is off while any is open.
    windows: usize,
    /// The setting the last switch left; `None` before it and after a failure.
    enabled: Option<bool>,
    unsupported: bool,
    /// A failure was logged and no switch has succeeded since.
    failing: bool,
    /// Work for the worker: apply now, or after the retry delay.
    pending: Option<String>,
    retry: Option<String>,
    /// Switches taken up and not finished yet.
    switching: usize,
    started: bool,
    closed: bool,
}

pub struct SecureConnections {
    control: Arc<dyn crate::host_backends::SecureConnectionsControl>,
    events: Arc<JsonEvents>,
    index: AdapterIndex,
    managed: bool,
    require_adapter: bool,
    retry_delay: (Duration, Duration),
    state: Mutex<State>,
    changed: Condvar,
    /// Signalled when a switch finished.
    settled: Condvar,
    /// Serialises switches, so the last one always follows the latest window count.
    switching: Mutex<()>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    subscription: Mutex<Option<BluetoothSubscription>>,
}
impl SecureConnections {
    pub fn new(runner: Arc<dyn CommandRunner>, events: Arc<JsonEvents>) -> Self {
        Self {
            control: Arc::new(crate::host_backends::BtmgmtControl(runner)),
            events,
            index: Arc::new(|| None),
            managed: true,
            require_adapter: false,
            retry_delay: (RETRY_INITIAL, RETRY_MAX),
            state: Mutex::default(),
            changed: Condvar::new(),
            settled: Condvar::new(),
            switching: Mutex::new(()),
            worker: Mutex::new(None),
            subscription: Mutex::new(None),
        }
    }
    pub fn with_control(
        mut self,
        control: Arc<dyn crate::host_backends::SecureConnectionsControl>,
    ) -> Self {
        self.control = control;
        self
    }
    /// Uses the selected adapter index, waiting when none is present (standalone default: 0).
    pub fn with_adapter_index(mut self, index: AdapterIndex) -> Self {
        self.require_adapter = true;
        self.index = index;
        self
    }
    pub fn with_policy(mut self, policy: crate::host_profile::SecureConnectionsPolicy) -> Self {
        self.managed = policy == crate::host_profile::SecureConnectionsPolicy::PairingOff;
        self
    }
    /// The first delay and the cap of the retries after a failed switch.
    pub fn with_retry_delay(mut self, initial: Duration, maximum: Duration) -> Self {
        self.retry_delay = (initial, maximum);
        self
    }
    /// Applies the policy now and again whenever `wake` reports a ready adapter, on a worker
    /// thread: startup and BlueZ signals never wait for `btmgmt`.
    pub fn start(self: &Arc<Self>, wake: Option<&Arc<BluetoothWake>>) -> Result<()> {
        if !self.managed {
            return Ok(());
        }
        {
            let mut s = self.state.lock().unwrap();
            if s.started || s.closed {
                return Ok(());
            }
            s.started = true;
        }
        if let Some(wake) = wake {
            let policy = Arc::downgrade(self);
            *self.subscription.lock().unwrap() = Some(wake.subscribe(Arc::new(move |change| {
                if *change == BluetoothChange::AdapterReady {
                    if let Some(policy) = policy.upgrade() {
                        policy.request("adapter-ready");
                    }
                }
            })));
        }
        self.request("startup");
        let policy = self.clone();
        let worker = crate::platform::spawn_worker("bluetooth-sc", Box::new(move || policy.run()))?;
        *self.worker.lock().unwrap() = Some(worker);
        Ok(())
    }
    /// Asks the worker to apply the policy; never blocks.
    pub fn request(&self, reason: &str) {
        if !self.managed {
            return;
        }
        let mut s = self.state.lock().unwrap();
        if s.closed || s.unsupported {
            return;
        }
        s.pending = Some(reason.into());
        self.changed.notify_all();
    }
    /// Opens a pairing window: SC is off until it closes. Waits at most one switch.
    pub fn open_window(self: &Arc<Self>) -> PairingWindow {
        if !self.managed {
            return PairingWindow { policy: None };
        }
        self.state.lock().unwrap().windows += 1;
        self.apply("pairing");
        PairingWindow {
            policy: Some(self.clone()),
        }
    }
    fn close_window(&self, reason: &str) {
        let last = {
            let mut s = self.state.lock().unwrap();
            s.windows = s.windows.saturating_sub(1);
            s.windows == 0 && !s.closed
        };
        if last {
            self.apply(reason);
        }
    }
    /// The setting the relay last switched, if the last switch succeeded.
    pub fn enabled(&self) -> Option<bool> {
        self.state.lock().unwrap().enabled
    }
    /// Waits up to `timeout` until no switch is requested or running, so a connection made next
    /// finds the setting the policy wants, unless switching failed. A retry after a failure is
    /// not waited for.
    pub fn wait_settled(&self, timeout: Duration) {
        let s = self.state.lock().unwrap();
        let _ = self
            .settled
            .wait_timeout_while(s, timeout, |s| {
                !s.closed && ((s.started && s.pending.is_some()) || s.switching > 0)
            })
            .unwrap();
    }
    /// Stops the worker when the relay stops. A pairing window still open ends here: SC is
    /// switched on again, and windows that close later switch nothing.
    pub fn close(&self, reason: &str) {
        let open = {
            let mut s = self.state.lock().unwrap();
            if s.closed {
                return;
            }
            s.closed = true;
            s.pending = None;
            s.retry = None;
            self.changed.notify_all();
            self.settled.notify_all();
            s.windows > 0 && !s.unsupported
        };
        self.subscription.lock().unwrap().take();
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            let _ = worker.join();
        }
        if open {
            let _switching = self.switching.lock().unwrap();
            self.set(true, reason);
        }
    }
    /// Switches SC to what the policy wants now; `false` when that failed.
    fn apply(&self, reason: &str) -> bool {
        self.state.lock().unwrap().switching += 1;
        self.apply_taken(reason)
    }
    /// [`Self::apply`] for a switch already counted in `switching`.
    fn apply_taken(&self, reason: &str) -> bool {
        let ok = self.switch(reason);
        let mut s = self.state.lock().unwrap();
        s.switching -= 1;
        self.settled.notify_all();
        ok
    }
    fn switch(&self, reason: &str) -> bool {
        let _switching = self.switching.lock().unwrap();
        let enable = {
            let s = self.state.lock().unwrap();
            if s.unsupported {
                return false;
            }
            s.windows == 0
        };
        self.set(enable, reason)
    }
    /// One switch, with the `switching` lock held.
    fn set(&self, enable: bool, reason: &str) -> bool {
        if !self.managed {
            return true;
        }
        let selected = (self.index)();
        let index = selected.unwrap_or(0);
        let outcome = if self.require_adapter && selected.is_none() {
            Err(Failure::Failed(
                "selected Bluetooth adapter unavailable".into(),
            ))
        } else {
            self.control.set_secure_connections(index, enable)
        };
        let action = word(enable);
        let mut s = self.state.lock().unwrap();
        let message = match outcome {
            Ok(()) => {
                s.enabled = Some(enable);
                s.failing = false;
                s.retry = None;
                Some(format!("bluetooth-sc {action} reason={reason}"))
            }
            Err(Failure::Unsupported(e)) => {
                s.enabled = None;
                s.unsupported = true;
                s.pending = None;
                s.retry = None;
                Some(format!(
                    "bluetooth-sc unsupported reason={reason} error={e}"
                ))
            }
            Err(failure) => {
                let (retry, e) = match failure {
                    Failure::Failed(e) => (true, e),
                    Failure::Unavailable(e) | Failure::Unsupported(e) => (false, e),
                };
                s.enabled = None;
                if retry && !s.closed {
                    s.retry = Some(reason.into());
                    self.changed.notify_all();
                }
                // A lasting failure is logged once, not on every retry.
                (!std::mem::replace(&mut s.failing, true))
                    .then(|| format!("bluetooth-sc {action} failed reason={reason} error={e}"))
            }
        };
        let ok = s.enabled == Some(enable);
        drop(s);
        if let Some(message) = message {
            self.events.log(&message);
        }
        ok
    }
    fn run(&self) {
        let mut delay = self.retry_delay.0;
        loop {
            let reason = {
                let mut s = self.state.lock().unwrap();
                loop {
                    if s.closed {
                        return;
                    }
                    if let Some(reason) = s.pending.take() {
                        delay = self.retry_delay.0;
                        s.switching += 1;
                        break reason;
                    }
                    if s.retry.is_none() {
                        delay = self.retry_delay.0;
                        s = self.changed.wait(s).unwrap();
                        continue;
                    }
                    let (next, wait) = self.changed.wait_timeout(s, delay).unwrap();
                    s = next;
                    if wait.timed_out() && !s.closed && s.pending.is_none() {
                        if let Some(reason) = s.retry.take() {
                            delay = (delay * 2).min(self.retry_delay.1);
                            s.switching += 1;
                            break reason;
                        }
                    }
                }
            };
            self.apply_taken(&reason);
        }
    }
}

/// An open pairing window. SC is switched on again when the last open window closes or drops.
pub struct PairingWindow {
    policy: Option<Arc<SecureConnections>>,
}
impl PairingWindow {
    /// Closes the window; `reason` is logged if SC is switched on again.
    pub fn close(mut self, reason: &str) {
        if let Some(policy) = self.policy.take() {
            policy.close_window(reason);
        }
    }
}
impl Drop for PairingWindow {
    fn drop(&mut self) {
        if let Some(policy) = self.policy.take() {
            policy.close_window("pairing-closed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replies_count_only_when_the_new_settings_show_the_requested_setting() {
        let on =
            "hci0 Set Secure Connections complete, settings: powered ssp br/edr le secure-conn\n";
        let off = "hci0 Set Secure Connections complete, settings: powered ssp br/edr le\n";
        assert_eq!(verify(0, on, true), Ok(()));
        assert_eq!(verify(0, off, false), Ok(()));
        assert!(matches!(verify(0, off, true), Err(Failure::Failed(_))));
        assert!(matches!(verify(0, on, false), Err(Failure::Failed(_))));
        // btmgmt without a readable stdin exits 0 having done nothing.
        assert_eq!(
            verify(0, "", true),
            Err(Failure::Failed("no reply from btmgmt".into()))
        );
        assert!(matches!(verify(1, on, true), Err(Failure::Failed(_))));
        assert!(matches!(
            verify(
                0,
                "Set Secure Connections for hci0 failed with status 0x11 (Invalid Index)\n",
                true
            ),
            Err(Failure::Failed(_))
        ));
        assert_eq!(
            verify(
                0,
                "Set Secure Connections for hci0 failed with status 0x0c (Not Supported)\n",
                true
            ),
            Err(Failure::Unsupported(
                "Set Secure Connections for hci0 failed with status 0x0c (Not Supported)".into()
            ))
        );
        assert!(matches!(
            verify(127, "sh: exec: btmgmt: not found\n", true),
            Err(Failure::Unavailable(_))
        ));
        // A reply without the new settings proves nothing.
        assert!(matches!(
            verify(0, "hci0 secure-conn\n", true),
            Err(Failure::Failed(_))
        ));
        assert_eq!(
            switch_command(1, false),
            ["sh", "-c", "exec btmgmt --index 1 sc off <&1"]
        );
    }
}
