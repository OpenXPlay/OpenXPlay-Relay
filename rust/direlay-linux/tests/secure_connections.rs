// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16
// Bluetooth Secure Connections policy: on, off only while a pairing window is open.
use openxplay_relay::bluetooth_wake::{BluetoothChange, BluetoothWake};
use openxplay_relay::linux_bluetooth::{PairingBackend, PairingSession};
use openxplay_relay::secure_connections::{SecureConnections, SWITCH_TIMEOUT};
use openxplay_relay::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// What happened, in order: switches (`sc on`, `sc off`) and the BlueZ side of the window.
type Sequence = Arc<Mutex<Vec<String>>>;

/// `btmgmt` behind `sh -c`, answering like BlueZ 5.55.
#[derive(Default)]
struct Btmgmt {
    sequence: Sequence,
    /// Replies given before switching works.
    failures: Mutex<VecDeque<CommandResult>>,
    unsupported: bool,
    /// How long a switch takes.
    delay: Duration,
}
impl CommandRunner for Btmgmt {
    fn run(&self, args: &[String], timeout: Duration) -> openxplay_relay::Result<CommandResult> {
        assert!(!timeout.is_zero() && timeout <= SWITCH_TIMEOUT);
        // The only command; its stdin is its own output pipe, so btmgmt does not quit early.
        let enable = match args {
            [sh, c, script] if sh == "sh" && c == "-c" => match script.as_str() {
                "exec btmgmt --index 0 sc on <&1" => true,
                "exec btmgmt --index 0 sc off <&1" => false,
                other => panic!("Unexpected script: {other}"),
            },
            _ => panic!("Unexpected command: {args:?}"),
        };
        self.sequence
            .lock()
            .unwrap()
            .push(format!("sc {}", if enable { "on" } else { "off" }));
        std::thread::sleep(self.delay);
        if let Some(reply) = self.failures.lock().unwrap().pop_front() {
            return Ok(reply);
        }
        let output = if self.unsupported {
            "Set Secure Connections for hci0 failed with status 0x0c (Not Supported)\n".into()
        } else {
            format!(
                "hci0 Set Secure Connections complete, settings: powered bondable ssp br/edr le{}\n",
                if enable { " secure-conn" } else { "" }
            )
        };
        Ok(CommandResult {
            exit_code: 0,
            output,
        })
    }
}

/// BlueZ's side of a pairing window.
struct Bluez {
    sequence: Sequence,
    fail: bool,
}
struct Session(Sequence);
impl PairingBackend for Bluez {
    fn open(&self, seconds: u32, _: Instant) -> openxplay_relay::Result<Box<dyn PairingSession>> {
        self.sequence
            .lock()
            .unwrap()
            .push(format!("discoverable {seconds}"));
        if self.fail {
            return Err(std::io::Error::other("BlueZ unavailable"));
        }
        Ok(Box::new(Session(self.sequence.clone())))
    }
}
impl PairingSession for Session {
    fn close(self: Box<Self>) {
        self.0.lock().unwrap().push("hidden".into());
    }
}

struct Harness {
    runner: Arc<Btmgmt>,
    lines: Arc<Mutex<Vec<String>>>,
    events: Arc<JsonEvents>,
    policy: Arc<SecureConnections>,
}
impl Harness {
    fn new() -> Self {
        Self::with(Btmgmt::default())
    }
    fn with(runner: Btmgmt) -> Self {
        let runner = Arc::new(runner);
        let lines = Arc::new(Mutex::new(Vec::new()));
        let copy = lines.clone();
        let events = Arc::new(JsonEvents::new(Arc::new(move |line| {
            copy.lock().unwrap().push(line)
        })));
        let policy = Arc::new(
            SecureConnections::new(runner.clone(), events.clone())
                .with_retry_delay(Duration::from_millis(20), Duration::from_millis(40)),
        );
        Self {
            runner,
            lines,
            events,
            policy,
        }
    }
    fn pairing(&self, seconds: u32, fail: bool) -> PairingMode {
        PairingMode::new(seconds, self.events.clone())
            .with_backend(Arc::new(Bluez {
                sequence: self.runner.sequence.clone(),
                fail,
            }))
            .with_secure_connections(self.policy.clone())
    }
    fn sequence(&self) -> Vec<String> {
        self.runner.sequence.lock().unwrap().clone()
    }
    /// The policy's log lines.
    fn logs(&self) -> Vec<String> {
        self.events_named("diagnostic")
            .into_iter()
            .filter_map(|v| v["message"].as_str().map(str::to_string))
            .filter(|m| m.starts_with("bluetooth-sc "))
            .collect()
    }
    fn events_named(&self, name: &str) -> Vec<serde_json::Value> {
        self.lines
            .lock()
            .unwrap()
            .iter()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .filter(|v| v["event"] == name)
            .collect()
    }
}
fn wait(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "Timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn startup_switches_it_on_and_a_ready_adapter_gets_the_policy_again() {
    let h = Harness::new();
    let wake = Arc::new(BluetoothWake::default());
    h.policy.start(Some(&wake)).unwrap();
    wait("startup", || h.policy.enabled() == Some(true));
    assert_eq!(h.sequence(), ["sc on"]);
    assert_eq!(h.logs(), ["bluetooth-sc on reason=startup"]);
    // bluetoothd re-initialised the adapter (SC on) while a pairing window is open: off again.
    let window = h.policy.open_window();
    assert_eq!(h.policy.enabled(), Some(false));
    wake.publish(&BluetoothChange::DeviceConnected(
        "AA:BB:CC:DD:EE:01".into(),
    ));
    wake.publish(&BluetoothChange::AdapterReady);
    wait("adapter-ready", || h.sequence().len() == 3);
    window.close("pairing-ended");
    wake.publish(&BluetoothChange::AdapterReady);
    wait("adapter-ready", || h.sequence().len() == 5);
    assert_eq!(
        h.sequence(),
        ["sc on", "sc off", "sc off", "sc on", "sc on"]
    );
    assert_eq!(
        h.logs(),
        [
            "bluetooth-sc on reason=startup",
            "bluetooth-sc off reason=pairing",
            "bluetooth-sc off reason=adapter-ready",
            "bluetooth-sc on reason=pairing-ended",
            "bluetooth-sc on reason=adapter-ready",
        ]
    );
    h.policy.close("relay-stop");
    // Stopped: BlueZ changes switch nothing, and a stop without a window leaves SC as it is.
    wake.publish(&BluetoothChange::AdapterReady);
    h.policy.request("late");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(h.sequence().len(), 5);
}

#[test]
fn a_pairing_window_turns_it_off_and_its_end_restores_it() {
    let h = Harness::new();
    let mode = h.pairing(1, false);
    mode.start().unwrap();
    // Off before the adapter becomes discoverable.
    assert_eq!(h.sequence(), ["sc off", "discoverable 1"]);
    assert_eq!(h.policy.enabled(), Some(false));
    wait("window end", || h.events_named("pairing-ended").len() == 1);
    // Not pairable any more before SC is back on.
    assert_eq!(
        h.sequence(),
        ["sc off", "discoverable 1", "hidden", "sc on"]
    );
    assert_eq!(
        h.logs(),
        [
            "bluetooth-sc off reason=pairing",
            "bluetooth-sc on reason=pairing-ended"
        ]
    );
    assert_eq!(h.policy.enabled(), Some(true));
    assert_eq!(h.events_named("pairing-ended").len(), 1);
    mode.close().unwrap();
    drop(mode);
    assert_eq!(h.sequence().len(), 4);
}

#[test]
fn cancelling_the_window_restores_it() {
    let h = Harness::new();
    let mode = h.pairing(600, false);
    mode.start().unwrap();
    mode.end("pairing-cancelled");
    assert_eq!(
        h.sequence(),
        ["sc off", "discoverable 600", "hidden", "sc on"]
    );
    assert_eq!(
        h.logs(),
        [
            "bluetooth-sc off reason=pairing",
            "bluetooth-sc on reason=pairing-cancelled"
        ]
    );
    // Ending it again, or dropping it, switches nothing.
    mode.end("pairing-cancelled");
    drop(mode);
    assert_eq!(h.sequence().len(), 4);
    assert_eq!(h.events_named("pairing-ended").len(), 1);
}

#[test]
fn closing_the_controllers_pair_window_restores_it() {
    // The controller closes its `--pair` window as a `CloseResource` when it stops.
    let h = Harness::new();
    let mode: Arc<dyn CloseResource> = {
        let mode = Arc::new(h.pairing(600, false));
        mode.start().unwrap();
        mode
    };
    mode.close().unwrap();
    assert_eq!(
        h.sequence(),
        ["sc off", "discoverable 600", "hidden", "sc on"]
    );
    assert_eq!(h.logs()[1], "bluetooth-sc on reason=pairing-closed");
}

#[test]
fn a_window_that_cannot_open_restores_it() {
    let h = Harness::new();
    let mode = h.pairing(30, true);
    assert!(mode.start().is_err());
    assert_eq!(h.sequence(), ["sc off", "discoverable 30", "sc on"]);
    assert_eq!(
        h.logs(),
        [
            "bluetooth-sc off reason=pairing",
            "bluetooth-sc on reason=pairing-failed"
        ]
    );
    assert!(h.events_named("pairing").is_empty());
    drop(mode);
    assert_eq!(h.sequence().len(), 3);
}

#[test]
fn the_relay_stopping_during_a_window_restores_it() {
    let h = Harness::new();
    h.policy.start(None).unwrap();
    wait("startup", || h.policy.enabled() == Some(true));
    let mode = h.pairing(600, false);
    mode.start().unwrap();
    assert_eq!(h.policy.enabled(), Some(false));
    h.policy.close("relay-stop");
    assert_eq!(h.policy.enabled(), Some(true));
    assert_eq!(
        h.logs(),
        [
            "bluetooth-sc on reason=startup",
            "bluetooth-sc off reason=pairing",
            "bluetooth-sc on reason=relay-stop"
        ]
    );
    // The window itself closes later, e.g. with the controller: BlueZ only, no second switch.
    mode.close().unwrap();
    assert_eq!(
        h.sequence(),
        ["sc on", "sc off", "discoverable 600", "sc on", "hidden"]
    );
    h.policy.close("relay-stop");
    assert_eq!(h.sequence().len(), 5);
}

#[test]
fn overlapping_windows_keep_it_off_until_the_last_closes() {
    let h = Harness::new();
    let admin = h.pairing(600, false);
    let option = h.pairing(30, false);
    admin.start().unwrap();
    option.start().unwrap();
    admin.end("pairing-cancelled");
    assert_eq!(h.policy.enabled(), Some(false));
    option.end("pairing-ended");
    assert_eq!(
        h.sequence(),
        [
            "sc off",
            "discoverable 600",
            "sc off",
            "discoverable 30",
            "hidden",
            "hidden",
            "sc on"
        ]
    );
    assert_eq!(
        h.logs().last().unwrap(),
        "bluetooth-sc on reason=pairing-ended"
    );
    assert_eq!(h.policy.enabled(), Some(true));
}

#[test]
fn a_failed_switch_is_retried_and_a_lasting_failure_logged_once() {
    // btmgmt with a closed stdin exits 0 without a reply; a hung one is killed by the runner.
    let quiet = CommandResult {
        exit_code: 0,
        output: String::new(),
    };
    let h = Harness::with(Btmgmt {
        failures: Mutex::new(VecDeque::from([quiet.clone(), quiet])),
        ..Default::default()
    });
    h.policy.start(None).unwrap();
    wait("retries", || h.policy.enabled() == Some(true));
    assert_eq!(h.sequence(), ["sc on", "sc on", "sc on"]);
    assert_eq!(
        h.logs(),
        [
            "bluetooth-sc on failed reason=startup error=no reply from btmgmt",
            "bluetooth-sc on reason=startup"
        ]
    );
    h.policy.close("relay-stop");
}

#[test]
fn a_rejected_switch_off_is_retried_while_the_window_is_open() {
    let h = Harness::new();
    h.policy.start(None).unwrap();
    wait("startup", || h.policy.enabled() == Some(true));
    let rejected = "Set Secure Connections for hci0 failed with status 0x0b (Rejected)";
    h.runner.failures.lock().unwrap().push_back(CommandResult {
        exit_code: 0,
        output: format!("{rejected}\n"),
    });
    let window = h.policy.open_window();
    wait("retry", || h.policy.enabled() == Some(false));
    assert_eq!(h.sequence(), ["sc on", "sc off", "sc off"]);
    assert_eq!(
        h.logs(),
        [
            "bluetooth-sc on reason=startup".to_string(),
            format!("bluetooth-sc off failed reason=pairing error={rejected}"),
            "bluetooth-sc off reason=pairing".to_string(),
        ]
    );
    window.close("pairing-ended");
    assert_eq!(h.policy.enabled(), Some(true));
    h.policy.close("relay-stop");
    assert_eq!(h.sequence().len(), 4);
}

#[test]
fn a_controller_without_secure_connections_is_left_alone() {
    let h = Harness::with(Btmgmt {
        unsupported: true,
        ..Default::default()
    });
    h.policy.start(None).unwrap();
    wait("startup", || !h.logs().is_empty());
    let window = h.policy.open_window();
    window.close("pairing-ended");
    h.policy.request("adapter-ready");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(h.sequence(), ["sc on"]);
    assert_eq!(
        h.logs(),
        ["bluetooth-sc unsupported reason=startup error=Set Secure Connections for hci0 failed with status 0x0c (Not Supported)"]
    );
    assert_eq!(h.policy.enabled(), None);
    h.policy.close("relay-stop");
}

#[test]
fn a_connection_waits_for_a_switch_in_progress_but_not_for_a_retry() {
    let h = Harness::with(Btmgmt {
        delay: Duration::from_millis(200),
        ..Default::default()
    });
    // Retries come late enough not to overlap the checks below.
    let policy = Arc::new(
        SecureConnections::new(h.runner.clone(), h.events.clone())
            .with_retry_delay(Duration::from_secs(5), Duration::from_secs(5)),
    );
    // Nothing to wait for before the policy runs.
    let started = Instant::now();
    policy.wait_settled(SWITCH_TIMEOUT);
    assert!(started.elapsed() < Duration::from_millis(100));
    // The startup switch completes before the controller connects to the iPhone.
    policy.start(None).unwrap();
    let started = Instant::now();
    policy.wait_settled(SWITCH_TIMEOUT);
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(policy.enabled(), Some(true));
    // So does the switch after BlueZ reset the adapter.
    policy.request("adapter-ready");
    policy.wait_settled(SWITCH_TIMEOUT);
    assert_eq!(h.sequence(), ["sc on", "sc on"]);
    // A failed switch is retried later; that retry is not waited for.
    h.runner.failures.lock().unwrap().push_back(CommandResult {
        exit_code: 0,
        output: "Set Secure Connections for hci0 failed with status 0x11 (Invalid Index)\n".into(),
    });
    policy.request("adapter-ready");
    policy.wait_settled(SWITCH_TIMEOUT);
    assert_eq!(policy.enabled(), None);
    let started = Instant::now();
    policy.wait_settled(SWITCH_TIMEOUT);
    assert!(started.elapsed() < Duration::from_millis(100));
    policy.close("relay-stop");
}

#[test]
fn untouched_never_switches_at_start_pairing_reset_or_shutdown() {
    let runner = Arc::new(Btmgmt::default());
    let events = Arc::new(JsonEvents::new(Arc::new(|_| {})));
    let policy = Arc::new(
        SecureConnections::new(runner.clone(), events)
            .with_adapter_index(Arc::new(|| {
                panic!("untouched must not look up the btmgmt index")
            }))
            .with_policy(host_profile::SecureConnectionsPolicy::Untouched),
    );
    let wake = Arc::new(BluetoothWake::default());
    policy.start(Some(&wake)).unwrap();
    let window = policy.open_window();
    wake.publish(&BluetoothChange::AdapterReady);
    policy.request("adapter-ready");
    window.close("test");
    policy.close("shutdown");
    assert_eq!(policy.enabled(), None);
    assert!(runner.sequence.lock().unwrap().is_empty());
}

#[test]
fn an_absent_selected_adapter_never_switches_the_default_adapter() {
    let runner = Arc::new(Btmgmt::default());
    let policy = Arc::new(
        SecureConnections::new(runner.clone(), Arc::new(JsonEvents::new(Arc::new(|_| {}))))
            .with_adapter_index(Arc::new(|| None)),
    );
    policy.start(None).unwrap();
    policy.wait_settled(SWITCH_TIMEOUT);
    let window = policy.open_window();
    window.close("test");
    policy.close("shutdown");
    assert!(runner.sequence.lock().unwrap().is_empty());
}
