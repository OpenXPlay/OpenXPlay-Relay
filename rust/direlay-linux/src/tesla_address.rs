// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Tesla address policy and live, session-scoped NetworkManager runtime changes.
use crate::{
    platform::{command, CommandRunner},
    settings::Settings,
};
use serde::{Deserialize, Serialize};
use std::{net::Ipv4Addr, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Off,
    VirtualName,
    PublicAddress,
    Teslamirror,
    TeslaAndroid,
}
impl Mode {
    pub fn uses_name(self) -> bool {
        matches!(self, Self::VirtualName | Self::Teslamirror)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Activation {
    Off,
    NeedsBrowser,
    NeedsRelayHotspot,
    NeedsName,
    Ready,
}
pub fn activation(mode: Mode, browser: bool, relay_hotspot: bool, name: bool) -> Activation {
    match () {
        _ if mode == Mode::Off => Activation::Off,
        _ if !browser => Activation::NeedsBrowser,
        _ if !relay_hotspot => Activation::NeedsRelayHotspot,
        _ if mode.uses_name() && !name => Activation::NeedsName,
        _ => Activation::Ready,
    }
}
/// Android's numeric dotted-quad policy, including usable link-local and shared space.
pub fn valid(address: &str) -> bool {
    let Ok(ip) = address.parse::<Ipv4Addr>() else {
        return false;
    };
    let [a, b, c, _] = ip.octets();
    !matches!(a, 0 | 10 | 127 | 224..=239)
        && !(a == 169 && b == 254 && !(1..=254).contains(&c))
        && !(a == 172 && (16..=31).contains(&b))
        && !(a == 192 && b == 168)
        && ip != Ipv4Addr::BROADCAST
}
pub fn tls_address(settings: &Settings) -> Option<Ipv4Addr> {
    (!settings.tesla_browser.uses_name())
        .then(|| settings.tesla_address().filter(|a| valid(a))?.parse().ok())
        .flatten()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub mode: Mode,
    pub address: Ipv4Addr,
}
#[derive(Clone)]
struct Input {
    browser: bool,
    hotspot: bool,
    interface: String,
}
impl Input {
    fn new(s: &Settings) -> Self {
        Self {
            browser: s.output != "carlink",
            hotspot: s.network_mode == "device-hotspot",
            interface: s.wifi_interface.clone(),
        }
    }
}
/// Captures the running input's network/output choices; Tesla choices always follow live settings.
/// Errors stay here and never propagate into the CarPlay controller.
pub struct Session {
    runner: Arc<dyn CommandRunner>,
    pub active: Option<Address>,
    pub failure: Option<String>,
    pub since_millis: Option<u64>,
    prepared: Option<Input>,
    input: Option<Input>,
    running: bool,
    disabled: bool,
    attempted: bool,
    requested: Option<(Mode, Option<String>)>,
    owned: Option<(String, Ipv4Addr)>,
    journal_state: Option<serde_json::Value>,
}
impl Session {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            runner,
            active: None,
            failure: None,
            since_millis: None,
            prepared: None,
            input: None,
            running: false,
            disabled: false,
            attempted: false,
            requested: None,
            owned: None,
            journal_state: None,
        }
    }
    /// A bounded, secret-free state record; command output never enters this event.
    pub fn journal_change(&mut self, settings: &Settings) -> Option<serde_json::Value> {
        let phase = if settings.tesla_browser == Mode::Off {
            "OFF"
        } else if self.failure.is_some() {
            "FAILED"
        } else if self.disabled || !self.running {
            "STOPPED"
        } else if self.active.is_some() {
            "READY"
        } else {
            "STARTING"
        };
        let state = serde_json::json!({
            "mode":settings.tesla_browser, "phase":phase, "address":settings.tesla_address(),
            "failureCode":self.failure.as_ref().map(|_| "tesla_address_failed")
        });
        if self.journal_state.as_ref() == Some(&state) {
            return None;
        }
        self.journal_state = Some(state.clone());
        Some(state)
    }
    pub fn prepare(&mut self, settings: &Settings, now: u64) {
        self.stop(now);
        self.running = false;
        self.input = None;
        self.prepared = Some(Input::new(settings));
        self.since_millis = Some(now);
    }
    pub fn activation(&self, settings: &Settings, name: bool) -> Activation {
        let live = Input::new(settings);
        let input = self
            .input
            .as_ref()
            .filter(|_| self.running)
            .unwrap_or(&live);
        activation(settings.tesla_browser, input.browser, input.hotspot, name)
    }
    fn change(&self, interface: &str, address: Ipv4Addr, add: bool) -> crate::Result<()> {
        // `device modify` changes only the applied connection: no saved profile or AP restart.
        command(
            self.runner.as_ref(),
            &[
                "nmcli",
                "--wait",
                "15",
                "device",
                "modify",
                interface,
                if add {
                    "+ipv4.addresses"
                } else {
                    "-ipv4.addresses"
                },
                &format!("{address}/32"),
            ],
            Duration::from_secs(15),
        )
        .map(|_| ())
    }
    fn remove(&mut self) -> bool {
        self.active = None;
        if let Some((interface, address)) = &self.owned {
            if let Err(e) = self.change(interface, *address, false) {
                self.failure = Some(e.to_string());
                return false;
            }
        }
        self.owned = None;
        true
    }
    pub fn stop(&mut self, now: u64) {
        let before = (self.active.clone(), self.failure.clone());
        self.prepared = None;
        self.disabled = true;
        self.failure = None;
        self.requested = None;
        self.remove();
        if before != (self.active.clone(), self.failure.clone()) {
            self.since_millis = Some(now);
        }
    }
    pub fn tick(
        &mut self,
        running: bool,
        hotspot_ready: bool,
        settings: &Settings,
        name: bool,
        addresses: impl FnOnce() -> crate::Result<Vec<Ipv4Addr>>,
        now: u64,
    ) {
        let before = (self.active.clone(), self.failure.clone());
        self.tick_inner(running, hotspot_ready, settings, name, addresses);
        if before != (self.active.clone(), self.failure.clone()) {
            self.since_millis = Some(now);
        }
    }
    fn tick_inner(
        &mut self,
        running: bool,
        hotspot_ready: bool,
        settings: &Settings,
        name: bool,
        addresses: impl FnOnce() -> crate::Result<Vec<Ipv4Addr>>,
    ) {
        if !running {
            if self.running {
                self.stop(0);
                self.input = None;
            }
            self.running = false;
            return;
        }
        if !self.running {
            self.input = Some(self.prepared.take().unwrap_or_else(|| Input::new(settings)));
            self.disabled = false;
            self.attempted = false;
            self.failure = None;
            self.running = true;
        }
        if self.disabled {
            return;
        }
        let previous = self.active.as_ref().map(|a| a.address);
        let next = (
            settings.tesla_browser,
            settings.tesla_address().map(str::to_owned),
        );
        if self.requested.as_ref() != Some(&next) {
            if !self.remove() {
                return;
            }
            self.failure = None;
            self.attempted = false;
            self.requested = Some(next);
        }
        let input = self.input.as_ref().unwrap().clone();
        if activation(settings.tesla_browser, input.browser, input.hotspot, name)
            != Activation::Ready
            || !hotspot_ready
        {
            if self.owned.is_some() && self.remove() {
                self.attempted = false;
            }
            return;
        }
        if self.attempted {
            return;
        }
        self.attempted = true;
        let result = (|| {
            let text = settings.tesla_address().unwrap();
            let address = text.parse::<Ipv4Addr>().map_err(std::io::Error::other)?;
            if !valid(text)
                || addresses()?
                    .iter()
                    .any(|a| *a == address && Some(*a) != previous)
            {
                return Err(std::io::Error::other(format!(
                    "Invalid or already assigned Tesla address: {text}"
                )));
            }
            // Keep ownership on an ambiguous timeout, so cleanup still removes an applied address.
            self.owned = Some((input.interface.clone(), address));
            self.change(&input.interface, address, true)?;
            self.active = Some(Address {
                mode: settings.tesla_browser,
                address,
            });
            Ok(())
        })();
        if let Err(e) = result {
            self.failure = Some(e.to_string());
            self.remove();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.remove();
    }
}

pub fn address_key(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::Off => None,
        Mode::VirtualName => Some("teslaVirtualAddress"),
        Mode::PublicAddress => Some("teslaPublicAddress"),
        Mode::Teslamirror => Some("teslaMirrorAddress"),
        Mode::TeslaAndroid => Some("teslaAndroidAddress"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::CommandResult;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };
    #[derive(Default)]
    struct Runner {
        calls: Mutex<Vec<Vec<String>>>,
        fail: AtomicBool,
    }
    impl CommandRunner for Runner {
        fn run(&self, args: &[String], timeout: Duration) -> crate::Result<CommandResult> {
            assert_eq!(timeout, Duration::from_secs(15));
            self.calls.lock().unwrap().push(args.to_vec());
            if self.fail.load(Ordering::Acquire) && args.contains(&"+ipv4.addresses".into()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "NM address timeout",
                ));
            }
            Ok(CommandResult {
                exit_code: 0,
                output: String::new(),
            })
        }
    }
    fn settings(mode: Mode) -> Settings {
        Settings {
            tesla_browser: mode,
            ..Default::default()
        }
    }
    fn tick(s: &mut Session, settings: &Settings) {
        s.tick(true, true, settings, true, || Ok(vec![]), 100);
    }
    #[test]
    fn tesla_defaults_policy_and_settings_round_trip() {
        for (mode, address) in [
            (Mode::VirtualName, "169.254.88.1"),
            (Mode::PublicAddress, "169.254.88.1"),
            (Mode::Teslamirror, "100.88.0.1"),
            (Mode::TeslaAndroid, "3.3.3.1"),
        ] {
            let s = settings(mode);
            assert_eq!(s.tesla_address(), Some(address));
            assert!(s.validate().is_ok());
            assert_eq!(
                serde_json::from_value::<Settings>(serde_json::to_value(&s).unwrap()).unwrap(),
                s
            );
            assert_eq!(
                tls_address(&s),
                (!mode.uses_name()).then(|| address.parse().unwrap())
            );
        }
        assert_eq!(Settings::default().tesla_browser, Mode::Off);
        assert_eq!(
            serde_json::from_str::<Settings>("{}")
                .unwrap()
                .tesla_address(),
            None
        );
        for ip in [
            "169.254.1.0",
            "169.254.254.255",
            "100.88.0.1",
            "3.3.3.1",
            "240.3.3.3",
            "255.255.255.254",
        ] {
            assert!(valid(ip), "{ip}");
        }
        for ip in [
            "",
            "localhost",
            "1.2.3",
            "01.2.3.4",
            "1.2.3.256",
            "1.2.3.4 ",
            "0.1.2.3",
            "10.1.2.3",
            "127.0.0.1",
            "169.254.0.1",
            "169.254.255.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "224.0.0.1",
            "239.255.1.2",
            "255.255.255.255",
        ] {
            assert!(!valid(ip), "{ip}");
        }
        let mut off = settings(Mode::Off);
        off.tesla_public_address = "parked edit".into();
        assert!(off.validate().is_ok());
        off.tesla_browser = Mode::PublicAddress;
        assert!(off.validate().is_err());
    }
    #[test]
    fn tesla_activation_priority_for_every_mode_and_output() {
        for mode in [
            Mode::Off,
            Mode::VirtualName,
            Mode::PublicAddress,
            Mode::Teslamirror,
            Mode::TeslaAndroid,
        ] {
            for browser in [false, true] {
                for hotspot in [false, true] {
                    for name in [false, true] {
                        let expected = if mode == Mode::Off {
                            Activation::Off
                        } else if !browser {
                            Activation::NeedsBrowser
                        } else if !hotspot {
                            Activation::NeedsRelayHotspot
                        } else if mode.uses_name() && !name {
                            Activation::NeedsName
                        } else {
                            Activation::Ready
                        };
                        assert_eq!(activation(mode, browser, hotspot, name), expected);
                    }
                }
            }
        }
    }
    #[test]
    fn tesla_live_switches_keep_captured_input_and_use_only_runtime_changes() {
        let runner = Arc::new(Runner::default());
        let mut session = Session::new(runner.clone());
        let off = settings(Mode::Off);
        session.prepare(&off, 1);
        tick(&mut session, &off);
        let mut live = settings(Mode::PublicAddress);
        tick(&mut session, &live);
        for mode in [
            Mode::VirtualName,
            Mode::Teslamirror,
            Mode::TeslaAndroid,
            Mode::PublicAddress,
        ] {
            live.tesla_browser = mode;
            live.output = "carlink".into();
            live.network_mode = "shared-wifi".into();
            live.wifi_interface = "wlan9".into();
            let previous = session.active.as_ref().unwrap().address;
            session.tick(true, true, &live, true, || Ok(vec![previous]), 20);
            assert_eq!(session.active.as_ref().unwrap().mode, mode);
            assert!(session.failure.is_none());
            tick(&mut session, &live);
        }
        live.tesla_public_address = "169.254.88.2".into();
        tick(&mut session, &live);
        live.tesla_mirror_address = "100.88.0.2".into();
        tick(&mut session, &live);
        tick(&mut session, &settings(Mode::Off));
        assert!(session.active.is_none());
        tick(&mut session, &live);
        assert!(session.active.is_some());
        session.stop(30);
        tick(&mut session, &live);
        assert!(session.active.is_none());
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 14);
        for call in calls.iter() {
            assert_eq!(
                &call[..6],
                ["nmcli", "--wait", "15", "device", "modify", "wlan0"]
            );
            assert!(call[7].ends_with("/32"));
            assert_eq!(call.len(), 8);
        }
    }
    #[test]
    fn tesla_waits_for_hotspot_and_name_then_stops_with_input() {
        let runner = Arc::new(Runner::default());
        let mut session = Session::new(runner.clone());
        let live = settings(Mode::Teslamirror);
        session.tick(false, false, &live, true, || panic!("not starting"), 1);
        session.tick(true, false, &live, true, || panic!("no AP"), 2);
        session.tick(true, true, &live, false, || panic!("no name"), 3);
        tick(&mut session, &live);
        assert!(session.active.is_some());
        session.tick(false, false, &live, true, || panic!("stopping"), 200);
        assert!(session.active.is_none());
        assert_eq!(session.since_millis, Some(200));
        assert_eq!(runner.calls.lock().unwrap().len(), 2);
        for output in ["web", "both", "carlink"] {
            for network in ["shared-wifi", "device-hotspot"] {
                let mut value = settings(Mode::PublicAddress);
                value.output = output.into();
                value.network_mode = network.into();
                session.prepare(&value, 201);
                tick(&mut session, &value);
                assert_eq!(
                    session.active.is_some(),
                    output != "carlink" && network == "device-hotspot"
                );
            }
        }
        session.stop(300);
    }
    #[test]
    fn tesla_failures_and_collisions_do_not_retry_until_a_switch_or_new_input() {
        let runner = Arc::new(Runner::default());
        let mut session = Session::new(runner.clone());
        let live = settings(Mode::PublicAddress);
        session.tick(
            true,
            true,
            &live,
            true,
            || Ok(vec!["169.254.88.1".parse().unwrap()]),
            1,
        );
        assert!(session
            .failure
            .as_ref()
            .unwrap()
            .contains("already assigned"));
        tick(&mut session, &live);
        assert!(runner.calls.lock().unwrap().is_empty());
        tick(&mut session, &settings(Mode::Off));
        runner.fail.store(true, Ordering::Release);
        tick(&mut session, &live);
        tick(&mut session, &live);
        assert_eq!(session.failure.as_deref(), Some("NM address timeout"));
        assert_eq!(
            runner.calls.lock().unwrap().len(),
            2,
            "one attempt and cleanup after ambiguous timeout"
        );
        runner.fail.store(false, Ordering::Release);
        tick(&mut session, &settings(Mode::TeslaAndroid));
        assert!(session.active.is_some());
        assert!(session.failure.is_none());
        session.tick(false, false, &live, true, || Ok(vec![]), 5);
        tick(&mut session, &live);
        assert!(session.active.is_some());
        session.stop(6);
    }
}
