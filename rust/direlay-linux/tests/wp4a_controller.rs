// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Linux controller tests with platform fakes.
use carplay_iap2::{
    mfi::{BaaCertificatePair, MfiAuthenticator},
    DuplexStream, Iap2WirelessCarPlayEndpoint, Iap2WirelessSecurity, Receive,
};
use openxplay_relay::{
    bluetooth_wake::{BluetoothChange, BluetoothListener, BluetoothSubscription, BluetoothWake},
    boot_timing::Milestone,
    hotspot_lifetime::HotspotLifetime,
    linux_car_play_controller::{BLUETOOTH_RETRY_INITIAL, BLUETOOTH_RETRY_MAX},
    *,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
fn wait_for(mut test: impl FnMut() -> bool) {
    let start = Instant::now();
    while !test() {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "Timed out waiting for controller"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
struct Mfi;
impl MfiAuthenticator for Mfi {
    fn protocol_major(&self) -> openxplay_relay::Result<u8> {
        Ok(3)
    }
    fn read_certificate(&self, _: usize) -> openxplay_relay::Result<Vec<u8>> {
        Ok(vec![1])
    }
    fn sign_challenge(&self, _: &[u8]) -> openxplay_relay::Result<Vec<u8>> {
        Ok(vec![2])
    }
    fn baa_certificates(&self) -> openxplay_relay::Result<BaaCertificatePair> {
        Err(std::io::Error::other("unused"))
    }
}
struct Shared {
    live_hotspot: Option<Arc<dyn WirelessHotspotManager>>,
    network_watch: bool,
    lifetime: Option<Arc<openxplay_relay::hotspot_lifetime::HotspotLifetime>>,
    hotspot_starts: AtomicUsize,
    closed: Mutex<Vec<String>>,
    callbacks: Mutex<Option<NetworkCallbacks>>,
    network_started: AtomicBool,
    gate_enabled: AtomicBool,
    gate_ready: AtomicBool,
    gate_failure: AtomicBool,
    connecting: AtomicBool,
    connected: AtomicBool,
    establish: AtomicBool,
    only_airplay: AtomicBool,
    hold_connect: AtomicBool,
    phone_error: AtomicBool,
    hotspot_error: AtomicBool,
    /// The next starts or validations that fail.
    hotspot_failures: AtomicUsize,
    validate_failures: AtomicUsize,
    bluetooth_error: AtomicBool,
    pairing: AtomicBool,
    pair_starts: AtomicUsize,
    bt_state: (Mutex<bool>, Condvar),
    /// Stations associated with the AP; `None` when they cannot be listed.
    stations: Mutex<Option<Vec<String>>>,
    /// A station that joins the AP once the bootstrap runs, like the iPhone.
    join_on_bootstrap: Mutex<Option<String>>,
    /// When the fake AP started and closed.
    hotspot_log: Mutex<Vec<(&'static str, Instant)>>,
    /// The adapter is missing until cleared.
    radio_down: AtomicBool,
    /// BlueZ changes reach the controller through this hub.
    wake: Option<Arc<BluetoothWake>>,
    /// The previous run was killed with its AP up; the stale-AP check runs on each create.
    stale_ap: AtomicBool,
    stale_checks: AtomicUsize,
    /// While set, the AP's start blocks, like NetworkManager bringing it up.
    hold_hotspot: (Mutex<bool>, Condvar),
    /// The SSID the platform plans for the AP before it is up.
    planned_ssid: Mutex<Option<String>>,
    /// What the bootstrap did: the SSID it identified with, when it was first authenticated and
    /// when it sent the AP's credentials.
    identified: Mutex<Option<String>>,
    authenticated: Mutex<Option<Instant>>,
    wifi_sent: Mutex<Option<Instant>>,
    station_reads: AtomicUsize,
    connects: AtomicUsize,
    /// The relay's Wi-Fi provisioning, which the AP's activation waits for.
    provisioning: Arc<openxplay_relay::provision::ProvisioningGate>,
    ap_cancelled: AtomicBool,
}
impl Default for Shared {
    fn default() -> Self {
        Self {
            live_hotspot: None,
            network_watch: false,
            lifetime: None,
            hotspot_starts: AtomicUsize::new(0),
            closed: Mutex::new(Vec::new()),
            callbacks: Mutex::new(None),
            network_started: AtomicBool::new(false),
            gate_enabled: AtomicBool::new(false),
            gate_ready: AtomicBool::new(false),
            gate_failure: AtomicBool::new(false),
            connecting: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            establish: AtomicBool::new(true),
            only_airplay: AtomicBool::new(false),
            hold_connect: AtomicBool::new(false),
            phone_error: AtomicBool::new(false),
            hotspot_error: AtomicBool::new(false),
            hotspot_failures: AtomicUsize::new(0),
            validate_failures: AtomicUsize::new(0),
            bluetooth_error: AtomicBool::new(false),
            pairing: AtomicBool::new(false),
            pair_starts: AtomicUsize::new(0),
            bt_state: (Mutex::new(false), Condvar::new()),
            stations: Mutex::new(None),
            join_on_bootstrap: Mutex::new(None),
            hotspot_log: Mutex::new(Vec::new()),
            radio_down: AtomicBool::new(false),
            wake: None,
            stale_ap: AtomicBool::new(false),
            stale_checks: AtomicUsize::new(0),
            hold_hotspot: (Mutex::new(false), Condvar::new()),
            planned_ssid: Mutex::new(Some("test".into())),
            identified: Mutex::new(None),
            authenticated: Mutex::new(None),
            wifi_sent: Mutex::new(None),
            station_reads: AtomicUsize::new(0),
            connects: AtomicUsize::new(0),
            provisioning: Arc::default(),
            ap_cancelled: AtomicBool::new(false),
        }
    }
}
impl Shared {
    fn hold_hotspot(&self, held: bool) {
        *self.hold_hotspot.0.lock().unwrap() = held;
        self.hold_hotspot.1.notify_all();
    }
    fn hotspot_ready_at(&self) -> Option<Instant> {
        self.hotspot_log
            .lock()
            .unwrap()
            .iter()
            .find(|(kind, _)| *kind == "ready")
            .map(|(_, at)| *at)
    }
}
struct Platform(Arc<Shared>);
struct Hotspot(Arc<Shared>);
fn take_failure(counter: &AtomicUsize) -> bool {
    counter
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
}
fn info() -> WirelessHotspotInfo {
    WirelessHotspotInfo {
        ssid: "test".into(),
        passphrase: "password".into(),
        channel: 36,
        security: Iap2WirelessSecurity::WpaWpa2,
        host_address: Some("10.42.0.1".parse().unwrap()),
        bssid: Some("AA:BB:CC:DD:EE:FF".into()),
        access_point_bssid: Some("AA:BB:CC:DD:EE:FF".into()),
        interface_name: Some("wlan0".into()),
    }
}
impl WirelessHotspotManager for Hotspot {
    fn start(&self, _: Duration) -> openxplay_relay::Result<WirelessHotspotInfo> {
        if let Some(hotspot) = &self.0.live_hotspot {
            return hotspot.start(Duration::from_secs(2));
        }
        self.0.hotspot_starts.fetch_add(1, Ordering::SeqCst);
        self.0
            .hotspot_log
            .lock()
            .unwrap()
            .push(("start", Instant::now()));
        // Like NetworkManager's activation, which waits for the provisioning.
        while !self.0.provisioning.wait(Duration::from_millis(5))
            && !self.0.ap_cancelled.load(Ordering::Acquire)
        {}
        let (held, released) = &self.0.hold_hotspot;
        let _held = released
            .wait_while(held.lock().unwrap(), |held| *held)
            .unwrap();
        drop(_held);
        if self.0.hotspot_error.load(Ordering::Acquire) || take_failure(&self.0.hotspot_failures) {
            self.0
                .hotspot_log
                .lock()
                .unwrap()
                .push(("failed", Instant::now()));
            Err(std::io::Error::other("hotspot failed"))
        } else {
            self.0
                .hotspot_log
                .lock()
                .unwrap()
                .push(("ready", Instant::now()));
            Ok(info())
        }
    }
    fn validate_ready(&self) -> openxplay_relay::Result<()> {
        if let Some(hotspot) = &self.0.live_hotspot {
            return hotspot.validate_ready();
        }
        if take_failure(&self.0.validate_failures) {
            Err(std::io::Error::other("hotspot address changed"))
        } else {
            Ok(())
        }
    }
    fn on_car_play_confirmed(&self) -> openxplay_relay::Result<()> {
        Ok(())
    }
    fn close(&self) -> openxplay_relay::Result<()> {
        if let Some(hotspot) = &self.0.live_hotspot {
            hotspot.close()?;
        }
        // Closing cancels a start in progress.
        self.0.ap_cancelled.store(true, Ordering::Release);
        self.0.hold_hotspot(false);
        self.0.closed.lock().unwrap().push("hotspot".into());
        self.0
            .hotspot_log
            .lock()
            .unwrap()
            .push(("close", Instant::now()));
        Ok(())
    }
}
struct Network(Arc<Shared>);
impl NetworkAttachment for Network {
    fn validate_ready(&self) -> openxplay_relay::Result<()> {
        self.0
            .live_hotspot
            .as_ref()
            .map_or(Ok(()), |hotspot| hotspot.validate_ready())
    }
    fn start(&self) -> openxplay_relay::Result<()> {
        self.0.network_started.store(true, Ordering::Release);
        Ok(())
    }
    fn endpoint(&self) -> openxplay_relay::Result<Iap2WirelessCarPlayEndpoint> {
        Err(std::io::Error::other("unused"))
    }
    fn close(&self) -> openxplay_relay::Result<()> {
        self.0.closed.lock().unwrap().push("network".into());
        Ok(())
    }
}
struct Bluetooth(Arc<Shared>);
impl DuplexStream for Bluetooth {
    fn send(&self, _: &[u8]) -> openxplay_relay::Result<()> {
        Ok(())
    }
    fn recv(&self, _: usize, _: Duration) -> openxplay_relay::Result<Receive> {
        Ok(Receive::Timeout)
    }
    fn close(&self) -> openxplay_relay::Result<()> {
        let mut s = self.0.bt_state.0.lock().unwrap();
        if !*s {
            self.0.closed.lock().unwrap().push("bluetooth".into());
        }
        *s = true;
        self.0.bt_state.1.notify_all();
        Ok(())
    }
}
impl BluetoothConnection for Bluetooth {
    fn connect(&self) -> openxplay_relay::Result<()> {
        self.0.connects.fetch_add(1, Ordering::SeqCst);
        self.0
            .hotspot_log
            .lock()
            .unwrap()
            .push(("connect", Instant::now()));
        self.0.connecting.store(true, Ordering::Release);
        let mut s = self.0.bt_state.0.lock().unwrap();
        while self.0.hold_connect.load(Ordering::Acquire) && !*s {
            s = self.0.bt_state.1.wait(s).unwrap();
        }
        if self.0.bluetooth_error.load(Ordering::Acquire) {
            return Err(std::io::Error::other("connect failed"));
        }
        self.0.connected.store(true, Ordering::Release);
        Ok(())
    }
}
struct Pairing(Arc<Shared>);
impl CloseResource for Pairing {
    fn close(&self) -> openxplay_relay::Result<()> {
        self.0.closed.lock().unwrap().push("pairing".into());
        Ok(())
    }
}
impl WirelessPlatform for Platform {
    fn network_watch_enabled(&self) -> bool {
        self.0.network_watch
    }
    fn car_ready(&self) -> openxplay_relay::Result<bool> {
        if self.0.gate_failure.load(Ordering::SeqCst) {
            return Err(std::io::Error::other(
                "Carlink output did not become ready before CarPlay startup",
            ));
        }
        Ok(!self.0.gate_enabled.load(Ordering::SeqCst) || self.0.gate_ready.load(Ordering::SeqCst))
    }
    fn check_bluetooth_radio(&self) -> openxplay_relay::Result<()> {
        if self.0.radio_down.load(Ordering::Acquire) {
            Err(std::io::Error::other(
                "Bluetooth adapter address unavailable",
            ))
        } else {
            Ok(())
        }
    }
    fn watch_bluetooth(&self, wake: BluetoothListener) -> Option<BluetoothSubscription> {
        Some(self.0.wake.as_ref()?.subscribe(wake))
    }
    fn prepare_mfi(&self) -> openxplay_relay::Result<MfiResource> {
        Ok(MfiResource {
            authenticator: Arc::new(Mfi),
        })
    }
    fn create_hotspot(&self) -> openxplay_relay::Result<Arc<dyn WirelessHotspotManager>> {
        if let Some(lifetime) = &self.0.lifetime {
            lifetime.create(|| Arc::new(Hotspot(self.0.clone())))
        } else {
            Ok(Arc::new(Hotspot(self.0.clone())))
        }
    }
    fn prepare_hotspot(&self) {
        if let Some(lifetime) = &self.0.lifetime {
            lifetime.remove_stale_once(|| {
                self.0.stale_checks.fetch_add(1, Ordering::SeqCst);
                Ok(self.0.stale_ap.load(Ordering::Acquire))
            });
        }
    }
    fn hotspot_reused(&self) -> bool {
        self.0.lifetime.as_ref().is_some_and(|l| l.reused())
    }
    fn keep_hotspot_for_retry(&self) {
        if let Some(lifetime) = &self.0.lifetime {
            lifetime.keep_for_retry();
        }
    }
    fn cancel_hotspot_retry(&self) -> openxplay_relay::Result<()> {
        match &self.0.lifetime {
            Some(lifetime) => lifetime.cancel_retry(),
            None => Ok(()),
        }
    }
    fn hotspot_stations(&self) -> Option<Vec<String>> {
        self.0.station_reads.fetch_add(1, Ordering::SeqCst);
        self.0.stations.lock().unwrap().clone()
    }
    fn hotspot_down_remaining(&self) -> Duration {
        self.0
            .lifetime
            .as_ref()
            .map_or(Duration::ZERO, |l| l.down_remaining())
    }
    fn planned_hotspot_ssid(&self) -> Option<String> {
        self.0.planned_ssid.lock().unwrap().clone()
    }
    fn start_pairing(&self) -> openxplay_relay::Result<Option<Arc<dyn CloseResource>>> {
        if self.0.pairing.load(Ordering::Acquire) {
            self.0.pair_starts.fetch_add(1, Ordering::AcqRel);
            Ok(Some(Arc::new(Pairing(self.0.clone()))))
        } else {
            Ok(None)
        }
    }
    fn select_iphone(&self) -> openxplay_relay::Result<BondedDevice> {
        if self.0.phone_error.load(Ordering::Acquire) {
            Err(std::io::Error::other(
                "No unambiguous bonded iPhone found; pair one iPhone",
            ))
        } else {
            Ok(BondedDevice {
                address: "AA:BB:CC:DD:EE:01".into(),
                name: "iPhone".into(),
                paired: true,
                ..Default::default()
            })
        }
    }
    fn create_bluetooth(
        &self,
        _: &BondedDevice,
    ) -> openxplay_relay::Result<Arc<dyn BluetoothConnection>> {
        Ok(Arc::new(Bluetooth(self.0.clone())))
    }
    fn create_network(
        &self,
        _: WirelessHotspotInfo,
        _: Arc<dyn MfiAuthenticator>,
        callbacks: NetworkCallbacks,
    ) -> openxplay_relay::Result<Arc<dyn NetworkAttachment>> {
        *self.0.callbacks.lock().unwrap() = Some(callbacks);
        Ok(Arc::new(Network(self.0.clone())))
    }
    /// Like the iAP2 bootstrap: identification and authentication need only the AP's SSID; the
    /// Wi-Fi configuration and StartSession wait for the started network.
    fn run_bootstrap(
        &self,
        _: Arc<dyn BluetoothConnection>,
        network: Arc<dyn PendingNetwork>,
        _: Arc<dyn MfiAuthenticator>,
        callbacks: BootstrapCallbacks,
    ) -> openxplay_relay::Result<()> {
        *self.0.identified.lock().unwrap() = Some(network.ssid(Duration::from_secs(2))?);
        self.0
            .authenticated
            .lock()
            .unwrap()
            .get_or_insert_with(Instant::now);
        (callbacks.on_authenticated)();
        while network.wait(Duration::from_millis(100))?.is_none() {}
        *self.0.wifi_sent.lock().unwrap() = Some(Instant::now());
        (callbacks.on_wifi_configuration_sent)();
        // The iPhone joins the AP once it has the credentials.
        if let Some(station) = self.0.join_on_bootstrap.lock().unwrap().clone() {
            if let Some(stations) = self.0.stations.lock().unwrap().as_mut() {
                if !stations.contains(&station) {
                    stations.push(station);
                }
            }
        }
        (callbacks.on_start_session)();
        if self.0.establish.load(Ordering::Acquire) {
            let cb = self.0.callbacks.lock().unwrap().clone().unwrap();
            (cb.on_air_play)();
            if !self.0.only_airplay.load(Ordering::Acquire) {
                (cb.on_tunnel_ready)();
            }
        }
        Ok(())
    }
}
struct Run {
    controller: Arc<LinuxCarPlayController>,
    thread: Option<std::thread::JoinHandle<()>>,
    events: Arc<Mutex<Vec<serde_json::Value>>>,
    json: Arc<JsonEvents>,
}
impl Run {
    fn new(fake: Arc<Shared>, timeout: Duration) -> Self {
        Self::with_retry(
            fake,
            timeout,
            Duration::from_secs(2),
            Duration::from_secs(30),
        )
    }
    fn with_retry(fake: Arc<Shared>, timeout: Duration, initial: Duration, max: Duration) -> Self {
        Self::build(
            fake,
            timeout,
            (initial, max),
            (BLUETOOTH_RETRY_INITIAL, BLUETOOTH_RETRY_MAX),
        )
    }
    /// A controller whose waits for the adapter and the iPhone back off from `initial` to `max`.
    fn with_bluetooth_retry(fake: Arc<Shared>, initial: Duration, max: Duration) -> Self {
        Self::build(
            fake,
            Duration::from_secs(2),
            (Duration::from_secs(2), Duration::from_secs(30)),
            (initial, max),
        )
    }
    fn build(
        fake: Arc<Shared>,
        timeout: Duration,
        retry: (Duration, Duration),
        bluetooth: (Duration, Duration),
    ) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        let output = events.clone();
        let json = Arc::new(JsonEvents::new(Arc::new(move |s| {
            output
                .lock()
                .unwrap()
                .push(serde_json::from_str(&s).unwrap())
        })));
        // Production records relay startup before the controller can begin its first attempt.
        json.milestone(Milestone::RelayStart);
        let controller = Arc::new(
            LinuxCarPlayController::new(Arc::new(Platform(fake)), json.clone(), timeout)
                .with_retry_delay(retry.0, retry.1)
                .with_bluetooth_retry(bluetooth.0, bluetooth.1),
        );
        let c = controller.clone();
        let thread = Some(std::thread::spawn(move || c.run().unwrap()));
        Self {
            controller,
            thread,
            events,
            json,
        }
    }
    fn stop(&mut self) {
        self.controller.close().unwrap();
        self.thread.take().unwrap().join().unwrap();
    }
    fn has_event(&self, name: &str) -> bool {
        self.events
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["event"] == name)
    }
    fn matching(&self, test: impl Fn(&serde_json::Value) -> bool) -> Vec<serde_json::Value> {
        let events = self.events.lock().unwrap();
        events.iter().filter(|v| test(v)).cloned().collect()
    }
    fn hotspot_ready(&self) -> Vec<serde_json::Value> {
        self.matching(|v| v["stage"] == "HotspotReady")
    }
    fn diagnostics(&self, prefix: &str) -> Vec<String> {
        self.matching(|v| {
            v["event"] == "diagnostic"
                && v["message"].as_str().is_some_and(|m| m.starts_with(prefix))
        })
        .iter()
        .map(|v| v["message"].as_str().unwrap().to_string())
        .collect()
    }
}
fn closed_count(f: &Shared, name: &str) -> usize {
    f.closed
        .lock()
        .unwrap()
        .iter()
        .filter(|s| *s == name)
        .count()
}
impl Drop for Run {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.stop();
        }
    }
}
#[test]
fn tesla_address_on_the_ap_does_not_retry_but_a_real_ap_change_does() {
    #[derive(Default)]
    struct Runner {
        profile: Mutex<String>,
        addresses: Mutex<String>,
        reads: AtomicUsize,
    }
    impl CommandRunner for Runner {
        fn run(&self, args: &[String], _: Duration) -> openxplay_relay::Result<CommandResult> {
            let output = if args.get(2).is_some_and(|s| s == "load") {
                *self.profile.lock().unwrap() = std::fs::read_to_string(&args[3])?;
                String::new()
            } else if args.iter().any(|s| s == "IP4.ADDRESS") {
                self.reads.fetch_add(1, Ordering::SeqCst);
                self.addresses.lock().unwrap().clone()
            } else if args.iter().any(|s| s == "GENERAL.CONNECTION") {
                self.profile
                    .lock()
                    .unwrap()
                    .lines()
                    .find_map(|s| s.strip_prefix("id="))
                    .unwrap()
                    .into()
            } else if args.iter().any(|s| s.starts_with("802-11-wireless.ssid,")) {
                let profile = self.profile.lock().unwrap();
                let ssid = profile
                    .lines()
                    .find_map(|s| s.strip_prefix("ssid="))
                    .unwrap();
                let ssid = String::from_utf8(
                    ssid.split(';')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.parse::<u8>().unwrap())
                        .collect(),
                )
                .unwrap();
                format!("{ssid}\n36\nwpa-psk")
            } else if args[0] == "iw" {
                "addr dc:a6:32:11:22:33\nchannel 36 (5180 MHz)".into()
            } else {
                String::new()
            };
            Ok(CommandResult {
                exit_code: 0,
                output,
            })
        }
    }
    for network_watch in [false, true] {
        let dir = std::env::temp_dir().join(format!(
            "tesla-ap-loop-{}",
            admin::auth::random_hex(8).unwrap()
        ));
        let runner = Arc::new(Runner::default());
        *runner.addresses.lock().unwrap() = "10.77.0.1/24".into();
        let hotspot = Arc::new(
            LinuxHotspot::new(dir.clone(), runner.clone(), "wlan0".into(), 36).with_runtime_dirs(
                dir.join("keyfiles"),
                public_name::local_dns::Paths::in_dir(dir.join("dns")),
            ),
        );
        let f = Arc::new(Shared {
            live_hotspot: Some(hotspot),
            network_watch,
            planned_ssid: Mutex::new(None),
            ..Default::default()
        });
        let mut run = Run::new(f, Duration::from_secs(2));
        wait_for(|| run.controller.stage() == "WirelessActive");
        // This is the N1 ordering immediately after tesla-address READY.
        *runner.addresses.lock().unwrap() = "3.3.3.1/32 | 10.77.0.1/24".into();
        let before = runner.reads.load(Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(7);
        while runner.reads.load(Ordering::SeqCst) < before + 4 {
            assert!(
                Instant::now() < deadline,
                "one-second polling must continue"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!run.has_event("retry"));
        assert_eq!(run.controller.stage(), "WirelessActive");
        assert_eq!(run.hotspot_ready().len(), 1);
        *runner.addresses.lock().unwrap() = "3.3.3.1/32 | 10.77.1.1/24".into();
        let deadline = Instant::now() + Duration::from_secs(6);
        while !run.has_event("retry") {
            assert!(Instant::now() < deadline, "real AP changes must retry");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            run.matching(|v| v["event"] == "retry")[0]["reason"],
            "Hotspot address changed"
        );
        run.stop();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn success_requires_both_airplay_and_authenticated_tunnel() {
    let f = Arc::new(Shared::default());
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| run.controller.stage() == "WirelessActive");
    assert!(run.controller.hotspot_info().is_some());
    // The AP's credentials went out once it was up, under the SSID the iPhone was told.
    assert!(f.wifi_sent.lock().unwrap().unwrap() >= f.hotspot_ready_at().unwrap());
    assert_eq!(f.identified.lock().unwrap().as_deref(), Some("test"));
    run.stop();
    assert_eq!(
        *f.closed.lock().unwrap(),
        ["bluetooth", "network", "hotspot"]
    );
    assert!(run.controller.hotspot_info().is_none());
    assert!(!run.controller.failed());
}
#[test]
fn pairing_does_not_block_selection_and_is_owned_until_shutdown() {
    let f = Arc::new(Shared::default());
    f.pairing.store(true, Ordering::Release);
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| run.controller.stage() == "WirelessActive");
    assert_eq!(f.pair_starts.load(Ordering::Acquire), 1);
    assert!(!f.closed.lock().unwrap().iter().any(|s| s == "pairing"));
    run.stop();
    assert_eq!(f.closed.lock().unwrap().last().unwrap(), "pairing");
}
#[test]
fn missing_phone_keeps_network_and_reports_retry() {
    let f = Arc::new(Shared::default());
    f.phone_error.store(true, Ordering::Release);
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| {
        run.events
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["retryMillis"] == 2000)
    });
    // The network does not wait for the iPhone.
    wait_for(|| f.network_started.load(Ordering::Acquire));
    assert!(f.closed.lock().unwrap().is_empty());
    run.stop();
    assert_eq!(*f.closed.lock().unwrap(), ["network", "hotspot"]);
}
#[test]
fn bluetooth_failure_closes_attempt_and_keeps_network() {
    let f = Arc::new(Shared::default());
    f.bluetooth_error.store(true, Ordering::Release);
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| {
        run.events
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["reason"] == "connect failed")
    });
    assert_eq!(*f.closed.lock().unwrap(), ["bluetooth"]);
    run.stop();
}
#[test]
fn hotspot_failure_tears_down_owned_resource() {
    let f = Arc::new(Shared::default());
    f.hotspot_error.store(true, Ordering::Release);
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| run.has_event("retry"));
    wait_for(|| closed_count(&f, "hotspot") == 1);
    // Bluetooth may have connected meanwhile; the network never started.
    assert_eq!(closed_count(&f, "network"), 0);
    assert!(!f.network_started.load(Ordering::Acquire));
    assert_eq!(
        run.matching(|v| v["event"] == "retry")[0]["reason"],
        "hotspot failed"
    );
    run.stop();
}
#[test]
fn shutdown_cancels_inflight_connect() {
    let f = Arc::new(Shared::default());
    f.hold_connect.store(true, Ordering::Release);
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| f.connecting.load(Ordering::Acquire) && f.network_started.load(Ordering::Acquire));
    let start = Instant::now();
    run.stop();
    assert!(start.elapsed() < Duration::from_secs(1));
    // The connection goes first; the network worker may still have been handing its network
    // over, so the network and the AP close in either order.
    let mut closed = f.closed.lock().unwrap().clone();
    assert_eq!(closed[0], "bluetooth");
    closed.sort();
    assert_eq!(closed, ["bluetooth", "hotspot", "network"]);
}
#[test]
fn airplay_without_tunnel_retries_instead_of_exiting() {
    let f = Arc::new(Shared::default());
    f.only_airplay.store(true, Ordering::Release);
    let mut run = Run::new(f.clone(), Duration::from_millis(30));
    wait_for(|| run.has_event("retry"));
    let e = run.events.lock().unwrap();
    assert!(e.iter().any(|v| v["reason"].as_str().is_some_and(|s| s
        == "No established AirPlay/iAP2 tunnel after StartSession"
        || s == "Bluetooth ended before AirPlay tunnel was ready")));
    assert!(!e.iter().any(|v| v["stage"] == "WirelessActive"));
    drop(e);
    run.stop();
}
#[test]
fn tunnel_failure_restarts_and_old_callbacks_are_fenced() {
    let f = Arc::new(Shared::default());
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| run.controller.stage() == "WirelessActive");
    let callbacks = f.callbacks.lock().unwrap().clone().unwrap();
    (callbacks.on_failure)(std::io::Error::other("tunnel failed"));
    wait_for(|| run.has_event("retry"));
    run.stop();
    (callbacks.on_air_play)();
    (callbacks.on_tunnel_ready)();
    (callbacks.on_failure)(std::io::Error::other("late"));
    assert_eq!(run.controller.stage(), "Stopped");
    assert!(!run.controller.failed());
}

#[test]
fn codec_restart_keeps_ap_and_restarts_session_resources() {
    let lifetime = Arc::new(openxplay_relay::hotspot_lifetime::HotspotLifetime::default());
    let first = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        ..Default::default()
    });
    let mut run = Run::new(first.clone(), Duration::from_secs(2));
    wait_for(|| run.controller.stage() == "WirelessActive");
    let input = DiPlayInput::new(Arc::new(JsonEvents::new(Arc::new(|_| {}))));
    assert!(!input.configure_video_codec());
    let mut caps: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../OpenXPlay-Stream/spec/vectors/wire/capabilities.json"
    ))
    .unwrap();
    caps["decoders"] = serde_json::json!([{"codecString":"hvc1.1.6.L120.90","hardware":true}]);
    input.control(&oxp_wire::Message::Json(caps)).unwrap();
    assert!(input.apply_requested_restart(|| {
        lifetime.keep_for_restart();
        run.stop();
    }));
    assert!(input.configure_video_codec());
    let closed = first.closed.lock().unwrap();
    assert!(closed.iter().any(|s| s == "network"));
    assert!(closed.iter().any(|s| s == "bluetooth"));
    assert!(!closed.iter().any(|s| s == "hotspot"));
    drop(closed);
    let second = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        ..Default::default()
    });
    let mut next = Run::new(second.clone(), Duration::from_secs(2));
    wait_for(|| next.controller.stage() == "WirelessActive");
    assert_eq!(first.hotspot_starts.load(Ordering::SeqCst), 1);
    assert_eq!(second.hotspot_starts.load(Ordering::SeqCst), 0);
    assert!(second.network_started.load(Ordering::Acquire));
    assert!(second.connected.load(Ordering::Acquire));
    assert!(next
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|v| v["stage"] == "HotspotReady" && v["reused"] == true));
    next.stop();
    lifetime.close().unwrap();
    assert_eq!(
        first
            .closed
            .lock()
            .unwrap()
            .iter()
            .filter(|s| *s == "hotspot")
            .count(),
        1
    );
}

#[test]
fn retry_keeps_ap_unless_the_hotspot_failed() {
    let lifetime = Arc::new(openxplay_relay::hotspot_lifetime::HotspotLifetime::default());
    let f = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        ..Default::default()
    });
    let mut run = Run::with_retry(
        f.clone(),
        Duration::from_secs(2),
        Duration::from_millis(10),
        Duration::from_millis(50),
    );
    wait_for(|| run.controller.stage() == "WirelessActive");
    let callbacks = f.callbacks.lock().unwrap().clone().unwrap();
    // The iPhone side fails: the head unit keeps its Wi-Fi.
    (callbacks.on_failure)(std::io::Error::other("tunnel failed"));
    wait_for(|| run.hotspot_ready().len() == 2 && run.controller.stage() == "WirelessActive");
    assert_eq!(run.hotspot_ready()[1]["reused"], true);
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 1);
    assert_eq!(closed_count(&f, "hotspot"), 0);
    assert_eq!(closed_count(&f, "network"), 1);
    // A failed AP validation closes and recreates the AP.
    f.validate_failures.store(1, Ordering::SeqCst);
    wait_for(|| run.hotspot_ready().len() == 3 && run.controller.stage() == "WirelessActive");
    assert_eq!(run.hotspot_ready()[2]["reused"], false);
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 2);
    assert_eq!(closed_count(&f, "hotspot"), 1);
    run.stop();
    assert_eq!(closed_count(&f, "hotspot"), 2);
    lifetime.close().unwrap();
    assert_eq!(closed_count(&f, "hotspot"), 2);
}

#[test]
fn hard_stop_during_retry_wait_closes_the_retained_ap() {
    let lifetime = Arc::new(openxplay_relay::hotspot_lifetime::HotspotLifetime::default());
    let f = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        ..Default::default()
    });
    let mut run = Run::with_retry(
        f.clone(),
        Duration::from_secs(2),
        Duration::from_secs(10),
        Duration::from_secs(10),
    );
    wait_for(|| run.controller.stage() == "WirelessActive");
    let callbacks = f.callbacks.lock().unwrap().clone().unwrap();
    (callbacks.on_failure)(std::io::Error::other("tunnel failed"));
    wait_for(|| run.has_event("retry") && closed_count(&f, "network") == 1);
    assert_eq!(closed_count(&f, "hotspot"), 0);
    let start = Instant::now();
    run.stop();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(closed_count(&f, "hotspot"), 1);
    lifetime.close().unwrap();
    assert_eq!(closed_count(&f, "hotspot"), 1);
}

#[test]
fn short_active_session_does_not_reset_retry_backoff() {
    let f = Arc::new(Shared::default());
    f.hotspot_failures.store(2, Ordering::SeqCst);
    let mut run = Run::with_retry(
        f.clone(),
        Duration::from_secs(2),
        Duration::from_millis(10),
        Duration::from_secs(1),
    );
    wait_for(|| run.controller.stage() == "WirelessActive");
    let callbacks = f.callbacks.lock().unwrap().clone().unwrap();
    (callbacks.on_failure)(std::io::Error::other("tunnel failed"));
    wait_for(|| run.matching(|v| v["event"] == "retry").len() == 3);
    let delays = run
        .matching(|v| v["event"] == "retry")
        .iter()
        .map(|v| v["retryMillis"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(delays, [10, 20, 40]);
    run.stop();
}

const HEAD_UNIT: &str = "02:00:00:00:00:01";
const IPHONE: &str = "02:00:00:00:00:02";
/// The AP of a relay that keeps it down for `gap` between teardown and the next start.
fn gapped(gap: Duration, stations: &[&str]) -> (Arc<HotspotLifetime>, Arc<Shared>) {
    let lifetime = Arc::new(HotspotLifetime::with_down_gap(gap, None));
    let f = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        stations: Mutex::new(Some(stations.iter().map(|s| s.to_string()).collect())),
        ..Default::default()
    });
    // StartSession goes out but the iPhone never reaches the relay over Wi-Fi.
    f.establish.store(false, Ordering::Release);
    (lifetime, f)
}
fn quick_retries(f: &Arc<Shared>) -> Run {
    Run::with_retry(
        f.clone(),
        Duration::from_millis(50),
        Duration::from_millis(10),
        Duration::from_millis(20),
    )
}
fn gap_millis(message: &str) -> u64 {
    message.rsplit('=').next().unwrap().parse().unwrap()
}
fn starts_and_closes(f: &Shared) -> (Vec<Instant>, Vec<Instant>) {
    let log = f.hotspot_log.lock().unwrap();
    let times = |kind| {
        log.iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, t)| *t)
            .collect()
    };
    (times("start"), times("close"))
}
#[test]
fn no_station_after_start_session_recreates_the_ap_after_the_gap() {
    for (stations, reason) in [
        (&[][..], "no-station"),
        // A head unit already on the AP is not the iPhone.
        (&[HEAD_UNIT][..], "no-iphone-station"),
    ] {
        let gap = Duration::from_millis(300);
        let (lifetime, f) = gapped(gap, stations);
        let mut run = quick_retries(&f);
        wait_for(|| f.hotspot_starts.load(Ordering::SeqCst) == 2);
        // The network worker reports the AP once its start returned.
        wait_for(|| run.hotspot_ready().len() == 2);
        let recreated = run.diagnostics("hotspot-recreate");
        assert!(recreated[0].starts_with(&format!("hotspot-recreate reason={reason} gapMs=")));
        assert!(gap_millis(&recreated[0]) > 200);
        assert!(gap_millis(&run.diagnostics("hotspot-gap waitMs=")[0]) > 0);
        let (starts, closes) = starts_and_closes(&f);
        assert!(starts[1].duration_since(closes[0]) >= gap);
        assert_eq!(run.hotspot_ready()[1]["reused"], false);
        run.stop();
        lifetime.close().unwrap();
    }
}
#[test]
fn iphone_on_the_ap_keeps_it_across_failed_attempts() {
    let (lifetime, f) = gapped(Duration::from_secs(5), &[HEAD_UNIT]);
    *f.join_on_bootstrap.lock().unwrap() = Some(IPHONE.into());
    let mut run = quick_retries(&f);
    wait_for(|| run.matching(|v| v["event"] == "retry").len() >= 3);
    // The iPhone joined during the first bootstrap and stays associated for the retries.
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 1);
    assert_eq!(closed_count(&f, "hotspot"), 0);
    assert!(run.hotspot_ready()[1..].iter().all(|v| v["reused"] == true));
    assert!(run.diagnostics("hotspot-recreate").is_empty());
    assert!(run.diagnostics("hotspot-gap").is_empty());
    run.stop();
    // A hard stop still takes the AP down and starts the gap.
    assert_eq!(closed_count(&f, "hotspot"), 1);
    assert!(lifetime.down_remaining() > Duration::from_secs(4));
}
#[test]
fn unknown_stations_keep_the_ap() {
    let lifetime = Arc::new(HotspotLifetime::with_down_gap(Duration::from_secs(5), None));
    let f = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        ..Default::default()
    });
    f.establish.store(false, Ordering::Release);
    let mut run = quick_retries(&f);
    wait_for(|| run.matching(|v| v["event"] == "retry").len() >= 3);
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 1);
    assert!(run.diagnostics("hotspot-recreate").is_empty());
    run.stop();
    lifetime.close().unwrap();
}
#[test]
fn codec_restart_keeps_the_ap_and_its_iphone() {
    let (lifetime, first) = gapped(Duration::from_secs(5), &[HEAD_UNIT]);
    first.establish.store(true, Ordering::Release);
    *first.join_on_bootstrap.lock().unwrap() = Some(IPHONE.into());
    let mut run = Run::new(first.clone(), Duration::from_secs(2));
    wait_for(|| run.controller.stage() == "WirelessActive");
    lifetime.keep_for_restart();
    run.stop();
    assert_eq!(closed_count(&first, "hotspot"), 0);
    assert_eq!(lifetime.down_remaining(), Duration::ZERO);
    // The new controller inherits the AP with the iPhone still on it; its session then fails.
    let second = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        stations: Mutex::new(Some(vec![HEAD_UNIT.into(), IPHONE.into()])),
        ..Default::default()
    });
    second.establish.store(false, Ordering::Release);
    let mut next = quick_retries(&second);
    wait_for(|| next.matching(|v| v["event"] == "retry").len() >= 2);
    assert_eq!(next.hotspot_ready()[0]["reused"], true);
    assert_eq!(first.hotspot_starts.load(Ordering::SeqCst), 1);
    assert_eq!(second.hotspot_starts.load(Ordering::SeqCst), 0);
    assert_eq!(closed_count(&first, "hotspot"), 0);
    assert!(next.diagnostics("hotspot-recreate").is_empty());
    assert!(next.diagnostics("hotspot-gap").is_empty());
    next.stop();
    lifetime.close().unwrap();
}
#[test]
fn first_ap_waits_out_a_teardown_from_the_previous_run() {
    let gap = Duration::from_millis(400);
    let dir = std::env::temp_dir().join(format!(
        "wp4a-gap-{}",
        openxplay_relay::admin::auth::random_hex(8).unwrap()
    ));
    let record = dir.join(openxplay_relay::hotspot_lifetime::AP_DOWN_RECORD);
    // The previous run's AP goes down at shutdown.
    let old = Arc::new(Shared::default());
    let previous = HotspotLifetime::with_down_gap(gap, Some(record.clone()));
    let lease = previous.create(|| Arc::new(Hotspot(old.clone()))).unwrap();
    lease.start(Duration::from_secs(1)).unwrap();
    previous.close().unwrap();
    let (_, closes) = starts_and_closes(&old);
    // The next run reads the teardown from the state directory.
    let lifetime = Arc::new(HotspotLifetime::with_down_gap(gap, Some(record)));
    let f = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        ..Default::default()
    });
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| run.controller.stage() == "WirelessActive");
    let waited = gap_millis(&run.diagnostics("hotspot-gap waitMs=")[0]);
    assert!(waited > 0 && waited <= 400);
    // The record has millisecond resolution.
    let (starts, _) = starts_and_closes(&f);
    assert!(starts[0].duration_since(closes[0]) >= gap - Duration::from_millis(2));
    run.stop();
    lifetime.close().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

fn stages(run: &Run) -> Vec<String> {
    run.events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|v| v["stage"].as_str().map(str::to_string))
        .collect()
}
fn retry_millis(run: &Run, stage: &str) -> Vec<u64> {
    run.matching(|v| v["stage"] == stage && v["retryMillis"].is_u64())
        .iter()
        .map(|v| v["retryMillis"].as_u64().unwrap())
        .collect()
}
/// Long Bluetooth waits: only a BlueZ change can end them within `wait_for`'s deadline.
const LONG_WAIT: Duration = Duration::from_secs(5);
#[test]
fn hotspot_comes_up_before_the_adapter_whose_appearance_ends_the_wait() {
    let wake = Arc::new(BluetoothWake::default());
    let f = Arc::new(Shared {
        wake: Some(wake.clone()),
        ..Default::default()
    });
    f.radio_down.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(f.clone(), LONG_WAIT, LONG_WAIT * 2);
    wait_for(|| run.controller.stage() == "WaitingForBluetooth");
    // The AP comes up for the head unit; nothing that needs the adapter has started.
    wait_for(|| run.hotspot_ready().len() == 1);
    assert!(run.controller.hotspot_info().is_some());
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 1);
    std::thread::sleep(Duration::from_millis(20));
    assert!(!f.network_started.load(Ordering::Acquire));
    assert!(!stages(&run).iter().any(|s| s == "AttachingNetwork"));
    assert_eq!(
        run.matching(|v| v["stage"] == "WaitingForBluetooth")[0]["reason"],
        "Bluetooth adapter address unavailable"
    );
    // A device connection cannot help while the adapter is missing.
    wake.publish(&BluetoothChange::DeviceConnected(
        "AA:BB:CC:DD:EE:01".into(),
    ));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(run.controller.stage(), "WaitingForBluetooth");
    assert!(run.diagnostics("bluetooth-wake").is_empty());
    // hci0 comes up: the wait ends at once instead of after its five seconds.
    f.radio_down.store(false, Ordering::Release);
    wake.publish(&BluetoothChange::AdapterReady);
    wait_for(|| run.controller.stage() == "WirelessActive");
    let woken = run.diagnostics("bluetooth-wake");
    assert_eq!(woken.len(), 1);
    assert!(woken[0].starts_with("bluetooth-wake waiting=adapter reason=adapter-ready afterMs="));
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 1);
    run.stop();
}
#[test]
fn iphone_connecting_ends_the_wait_for_it() {
    let wake = Arc::new(BluetoothWake::default());
    let f = Arc::new(Shared {
        wake: Some(wake.clone()),
        ..Default::default()
    });
    // The bonded iPhone is out of range: connecting to it fails.
    f.bluetooth_error.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(f.clone(), LONG_WAIT, LONG_WAIT * 2);
    wait_for(|| !retry_millis(&run, "WaitingForPairedIphone").is_empty());
    // Another device is not the iPhone.
    wake.publish(&BluetoothChange::DeviceConnected(
        "AA:BB:CC:DD:EE:99".into(),
    ));
    std::thread::sleep(Duration::from_millis(50));
    assert!(run.diagnostics("bluetooth-wake").is_empty());
    // The iPhone comes back into range and pages the accessory.
    f.bluetooth_error.store(false, Ordering::Release);
    wake.publish(&BluetoothChange::DeviceConnected(
        "AA:BB:CC:DD:EE:01".into(),
    ));
    wait_for(|| run.controller.stage() == "WirelessActive");
    assert_eq!(retry_millis(&run, "WaitingForPairedIphone"), [5000]);
    assert_eq!(
        run.diagnostics("bluetooth-wake")[0]
            .split(" afterMs=")
            .next()
            .unwrap(),
        "bluetooth-wake waiting=iphone reason=device-connected address=AA:BB:CC:DD:EE:01"
    );
    run.stop();
}
#[test]
fn a_change_outside_a_wait_does_not_skip_the_next_one() {
    let wake = Arc::new(BluetoothWake::default());
    let f = Arc::new(Shared {
        wake: Some(wake.clone()),
        ..Default::default()
    });
    f.hold_connect.store(true, Ordering::Release);
    f.bluetooth_error.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(f.clone(), LONG_WAIT, LONG_WAIT * 2);
    // The relay's own connection attempt is what connects the iPhone; the attempt still fails.
    wait_for(|| f.connecting.load(Ordering::Acquire));
    wake.publish(&BluetoothChange::DeviceConnected(
        "AA:BB:CC:DD:EE:01".into(),
    ));
    {
        let _connect = f.bt_state.0.lock().unwrap();
        f.hold_connect.store(false, Ordering::Release);
        f.bt_state.1.notify_all();
    }
    wait_for(|| !retry_millis(&run, "WaitingForPairedIphone").is_empty());
    std::thread::sleep(Duration::from_millis(50));
    assert!(run.diagnostics("bluetooth-wake").is_empty());
    assert_eq!(run.controller.stage(), "WaitingForPairedIphone");
    run.stop();
}
#[test]
fn bluetooth_waits_back_off_to_a_ten_second_cap() {
    assert_eq!(BLUETOOTH_RETRY_INITIAL, Duration::from_secs(2));
    assert_eq!(BLUETOOTH_RETRY_MAX, Duration::from_secs(10));
    let (initial, cap) = (Duration::from_millis(10), Duration::from_millis(40));
    // Without BlueZ changes the waits poll: the adapter ...
    let adapter = Arc::new(Shared::default());
    adapter.radio_down.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(adapter.clone(), initial, cap);
    wait_for(|| retry_millis(&run, "WaitingForBluetooth").len() >= 5);
    assert_eq!(
        retry_millis(&run, "WaitingForBluetooth")[..5],
        [10, 20, 40, 40, 40]
    );
    assert!(run.diagnostics("bluetooth-wake")[0]
        .starts_with("bluetooth-wake waiting=adapter reason=timer afterMs="));
    // ... becomes ready on a later poll.
    adapter.radio_down.store(false, Ordering::Release);
    wait_for(|| run.controller.stage() == "WirelessActive");
    run.stop();
    // ... and the iPhone.
    let iphone = Arc::new(Shared::default());
    iphone.phone_error.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(iphone.clone(), initial, cap);
    wait_for(|| retry_millis(&run, "WaitingForPairedIphone").len() >= 5);
    assert_eq!(
        retry_millis(&run, "WaitingForPairedIphone")[..5],
        [10, 20, 40, 40, 40]
    );
    assert!(run.diagnostics("bluetooth-wake")[0]
        .starts_with("bluetooth-wake waiting=iphone reason=timer afterMs="));
    run.stop();
}
#[test]
fn a_change_restarts_the_backoff() {
    let wake = Arc::new(BluetoothWake::default());
    let f = Arc::new(Shared {
        wake: Some(wake.clone()),
        ..Default::default()
    });
    f.phone_error.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(f.clone(), Duration::from_millis(100), LONG_WAIT);
    wait_for(|| retry_millis(&run, "WaitingForPairedIphone").len() >= 3);
    // Selection keeps failing, so any device that connects may be a newly bonded iPhone. The
    // change is repeated until a wait takes it: one that arrives during an attempt is ignored.
    wait_for(|| {
        wake.publish(&BluetoothChange::DeviceConnected(
            "AA:BB:CC:DD:EE:42".into(),
        ));
        !run.diagnostics("bluetooth-wake waiting=iphone reason=device-connected")
            .is_empty()
    });
    // Without a change the delays only grow; after it they start over at the first one.
    wait_for(|| retry_millis(&run, "WaitingForPairedIphone")[1..].contains(&100));
    run.stop();
}
#[test]
fn an_ap_left_up_by_a_killed_run_is_removed_and_kept_down_for_the_gap() {
    let gap = Duration::from_millis(300);
    let lifetime = Arc::new(HotspotLifetime::with_down_gap(gap, None));
    let f = Arc::new(Shared {
        lifetime: Some(lifetime.clone()),
        ..Default::default()
    });
    f.stale_ap.store(true, Ordering::Release);
    f.validate_failures.store(1, Ordering::SeqCst);
    let started = Instant::now();
    let mut run = Run::with_retry(
        f.clone(),
        Duration::from_secs(2),
        Duration::from_millis(10),
        Duration::from_millis(20),
    );
    wait_for(|| run.controller.stage() == "WirelessActive");
    // The first AP waited out the gap since the stale AP went down.
    let waited = gap_millis(&run.diagnostics("hotspot-gap waitMs=")[0]);
    assert!(waited > 200 && waited <= 300);
    let (starts, _) = starts_and_closes(&f);
    assert!(starts[0].duration_since(started) >= gap - Duration::from_millis(2));
    // That gap began only with the AP's own preparation: Bluetooth did not wait for it.
    assert!(f.authenticated.lock().unwrap().unwrap() < starts[0]);
    // The failed validation recreated the AP; the check ran only before the first one.
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 2);
    assert_eq!(f.stale_checks.load(Ordering::SeqCst), 1);
    run.stop();
    lifetime.close().unwrap();
}

#[test]
fn authentication_runs_while_the_ap_starts_and_its_credentials_wait_for_it() {
    let f = Arc::new(Shared::default());
    f.hold_hotspot(true);
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    // Bluetooth, identification and authentication go ahead while NetworkManager brings the AP up.
    wait_for(|| f.authenticated.lock().unwrap().is_some());
    assert!(f.connected.load(Ordering::Acquire));
    assert_eq!(run.controller.stage(), "RunningWireless");
    assert_eq!(f.identified.lock().unwrap().as_deref(), Some("test"));
    std::thread::sleep(Duration::from_millis(50));
    assert!(run.hotspot_ready().is_empty() && run.controller.hotspot_info().is_none());
    assert!(!f.network_started.load(Ordering::Acquire));
    assert!(f.wifi_sent.lock().unwrap().is_none());
    // The AP is up: its credentials go to the iPhone and the session starts.
    f.hold_hotspot(false);
    wait_for(|| run.controller.stage() == "WirelessActive");
    let ready = f.hotspot_ready_at().unwrap();
    assert!(f.authenticated.lock().unwrap().unwrap() < ready);
    assert!(f.wifi_sent.lock().unwrap().unwrap() >= ready);
    assert_eq!(run.hotspot_ready().len(), 1);
    run.stop();
    assert_eq!(
        *f.closed.lock().unwrap(),
        ["bluetooth", "network", "hotspot"]
    );
}
#[test]
fn an_ap_failure_after_authentication_fails_the_attempt_cleanly() {
    let f = Arc::new(Shared::default());
    f.hold_hotspot(true);
    let mut run = Run::with_retry(
        f.clone(),
        Duration::from_secs(2),
        Duration::from_millis(10),
        Duration::from_millis(50),
    );
    wait_for(|| f.authenticated.lock().unwrap().is_some());
    // NetworkManager fails to bring the AP up; the next attempt's AP comes up.
    f.hotspot_failures.store(1, Ordering::SeqCst);
    f.hold_hotspot(false);
    wait_for(|| run.has_event("retry"));
    let retry = run.matching(|v| v["event"] == "retry")[0].clone();
    assert_eq!(retry["reason"], "hotspot failed");
    // The iPhone never got credentials; the attempt's link and AP were released.
    assert!(f.wifi_sent.lock().unwrap().is_none());
    wait_for(|| closed_count(&f, "bluetooth") == 1 && closed_count(&f, "hotspot") >= 1);
    assert_eq!(closed_count(&f, "network"), 0);
    wait_for(|| run.controller.stage() == "WirelessActive");
    let order = run.matching(|v| v["event"] == "retry" || v["stage"] == "WirelessActive");
    assert_eq!(order[0]["event"], "retry");
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 2);
    assert_eq!(run.hotspot_ready().len(), 1);
    run.stop();
}
#[test]
fn without_a_planned_ssid_the_identification_waits_for_the_ap() {
    let f = Arc::new(Shared::default());
    *f.planned_ssid.lock().unwrap() = None;
    f.hold_hotspot(true);
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| f.connected.load(Ordering::Acquire));
    std::thread::sleep(Duration::from_millis(50));
    assert!(f.authenticated.lock().unwrap().is_none());
    f.hold_hotspot(false);
    wait_for(|| run.controller.stage() == "WirelessActive");
    assert_eq!(f.identified.lock().unwrap().as_deref(), Some("test"));
    assert!(f.authenticated.lock().unwrap().unwrap() >= f.hotspot_ready_at().unwrap());
    run.stop();
}
#[test]
fn an_ap_whose_ssid_differs_from_the_identification_fails_the_attempt() {
    let f = Arc::new(Shared::default());
    *f.planned_ssid.lock().unwrap() = Some("renamed".into());
    let mut run = quick_retries(&f);
    wait_for(|| run.has_event("retry"));
    assert_eq!(
        run.matching(|v| v["event"] == "retry")[0]["reason"],
        "Access point SSID differs from the identification"
    );
    assert!(f.wifi_sent.lock().unwrap().is_none());
    run.stop();
}
#[test]
fn the_first_active_session_logs_the_attempts_boot_timing() {
    let f = Arc::new(Shared {
        stations: Mutex::new(Some(vec![HEAD_UNIT.into()])),
        ..Default::default()
    });
    *f.join_on_bootstrap.lock().unwrap() = Some(IPHONE.into());
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    wait_for(|| run.controller.stage() == "WirelessActive");
    // Read before the credentials went out and once the session started.
    wait_for(|| f.station_reads.load(Ordering::SeqCst) >= 2);
    assert!(!run.has_event("boot-timing"));
    // The relay's input reports the AirPlay session connecting, then ACTIVE.
    run.json.milestone(Milestone::SessionConnecting);
    run.json.milestone(Milestone::SessionActive);
    run.json.milestone(Milestone::SessionActive);
    let timings = run.matching(|v| v["event"] == "boot-timing");
    assert_eq!(timings.len(), 1);
    let timing = &timings[0];
    assert_eq!(timing["attempt"], 1);
    assert_eq!(timing["hotspotReused"], false);
    let at = |name: &str| {
        timing[name]
            .as_u64()
            .unwrap_or_else(|| panic!("{name} missing in {timing}"))
    };
    for (earlier, later) in [
        ("relayStartMs", "attemptStartMs"),
        ("attemptStartMs", "adapterReadyMs"),
        ("adapterReadyMs", "btConnectStartMs"),
        ("btConnectStartMs", "authDoneMs"),
        ("authDoneMs", "wifiInfoSentMs"),
        ("hotspotReadyMs", "wifiInfoSentMs"),
        ("wifiInfoSentMs", "iphoneJoinedMs"),
        ("iphoneJoinedMs", "sessionConnectingMs"),
        ("sessionConnectingMs", "sessionActiveMs"),
    ] {
        assert!(
            at(earlier) <= at(later),
            "{earlier} after {later}: {timing}"
        );
    }
    run.stop();
}

#[test]
fn after_an_ap_failure_bluetooth_waits_for_the_next_ap() {
    let f = Arc::new(Shared::default());
    f.hotspot_failures.store(3, Ordering::SeqCst);
    let mut run = quick_retries(&f);
    wait_for(|| run.controller.stage() == "WirelessActive");
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 4);
    assert_eq!(run.matching(|v| v["event"] == "retry").len(), 3);
    // Only the first attempt, whose AP was not known to fail, may connect the iPhone before its
    // AP is up; after a failure the iPhone is connected once a working AP is.
    let log = f.hotspot_log.lock().unwrap().clone();
    let failures = log
        .iter()
        .enumerate()
        .filter(|(_, (k, _))| *k == "failed")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let ready = log.iter().find(|(k, _)| *k == "ready").unwrap().1;
    // From the second attempt on; the first one's may race its AP's failure.
    let later = log[failures[0] + 1..]
        .iter()
        .filter(|(k, at)| *k == "connect" && *at >= log[failures[1]].1)
        .map(|(_, at)| *at)
        .collect::<Vec<_>>();
    assert!(!later.is_empty());
    assert!(later.iter().all(|at| *at >= ready));
    assert!(f.connects.load(Ordering::SeqCst) <= 2);
    run.stop();
}

#[test]
fn bluetooth_starts_while_the_wifi_provisioning_still_blocks_the_ap() {
    let f = Arc::new(Shared::default());
    // At startup the provisioning takes the saved Wi-Fi client connections down; at boot nmcli
    // answers only once NetworkManager has set wlan0 up.
    let provisioning = f.provisioning.hold();
    let mut run = Run::new(f.clone(), Duration::from_secs(2));
    // Bluetooth, identification and authentication do not wait for it.
    wait_for(|| f.authenticated.lock().unwrap().is_some());
    assert!(f.connected.load(Ordering::Acquire));
    assert_eq!(f.hotspot_starts.load(Ordering::SeqCst), 1);
    std::thread::sleep(Duration::from_millis(50));
    assert!(run.hotspot_ready().is_empty());
    assert!(!f.network_started.load(Ordering::Acquire));
    assert!(f.wifi_sent.lock().unwrap().is_none());
    // The provisioning is done: the AP comes up and the iPhone gets its credentials.
    drop(provisioning);
    wait_for(|| run.controller.stage() == "WirelessActive");
    let ready = f.hotspot_ready_at().unwrap();
    assert!(f.authenticated.lock().unwrap().unwrap() < ready);
    assert!(f.wifi_sent.lock().unwrap().unwrap() >= ready);
    run.stop();
}

#[test]
fn screen_wait_prepares_hotspot_before_airplay_and_is_cancelable() {
    for release in [false, true] {
        let f = Arc::new(Shared::default());
        f.gate_enabled.store(true, Ordering::SeqCst);
        let mut run = Run::with_retry(
            f.clone(),
            Duration::from_secs(2),
            Duration::from_millis(10),
            Duration::from_millis(20),
        );
        wait_for(|| {
            f.hotspot_starts.load(Ordering::SeqCst) > 0 && f.connecting.load(Ordering::SeqCst)
        });
        assert!(!f.network_started.load(Ordering::SeqCst));
        if release {
            f.gate_ready.store(true, Ordering::SeqCst);
            wait_for(|| run.controller.stage() == "WirelessActive");
        }
        let start = Instant::now();
        run.stop();
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}

#[test]
fn startup_stops_after_five_retries_including_bluetooth_waits() {
    let f = Arc::new(Shared::default());
    f.phone_error.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(
        f.clone(),
        Duration::from_millis(1),
        Duration::from_millis(2),
    );
    wait_for(|| run.controller.stage() == "Failed");
    assert_eq!(run.matching(|v| v["event"] == "retry").len(), 5);
    assert_eq!(run.matching(|v| v["event"] == "startup-failed").len(), 1);
    assert!(!run.controller.running());
    assert!(run.controller.hotspot_info().is_none());
    let failure = run.json.runtime_path_facts().iphone.failure.unwrap();
    assert!(!failure.retrying);
    assert!(!failure.trail.is_empty());
    run.stop();
}

#[test]
fn exhausted_budget_waits_for_matching_iphone_edges_and_stop_wins() {
    let wake = Arc::new(BluetoothWake::default());
    let f = Arc::new(Shared {
        wake: Some(wake.clone()),
        lifetime: Some(Arc::new(HotspotLifetime::default())),
        ..Default::default()
    });
    f.bluetooth_error.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(
        f.clone(),
        Duration::from_millis(1),
        Duration::from_millis(2),
    );
    run.controller.set_start_when_ready(true);
    wait_for(|| run.controller.armed());
    assert!(run.controller.running());
    assert!(
        !run.json
            .runtime_path_facts()
            .iphone
            .failure
            .unwrap()
            .retrying
    );
    let attempts = f.connects.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        f.connects.load(Ordering::SeqCst),
        attempts,
        "timers cannot reset the budget"
    );
    wake.publish(&BluetoothChange::DeviceConnected(
        "AA:BB:CC:DD:EE:02".into(),
    ));
    assert!(run.controller.armed());
    // A real edge earns another bounded budget, including after the first rescue.
    for _ in 0..2 {
        let before = run.matching(|v| v["event"] == "startup-failed").len();
        wake.publish(&BluetoothChange::DeviceConnected(
            "AA:BB:CC:DD:EE:01".into(),
        ));
        wait_for(|| {
            run.controller.armed()
                && run.matching(|v| v["event"] == "startup-failed").len() > before
        });
    }
    f.bluetooth_error.store(false, Ordering::Release);
    wake.publish(&BluetoothChange::DeviceConnected(
        "AA:BB:CC:DD:EE:01".into(),
    ));
    wait_for(|| run.controller.stage() == "WirelessActive");
    assert!(run.json.runtime_path_facts().iphone.failure.is_none());
    assert_eq!(
        f.hotspot_starts.load(Ordering::SeqCst),
        1,
        "armed recovery retains the AP"
    );
    run.stop();
    let attempts = f.connects.load(Ordering::SeqCst);
    wake.publish(&BluetoothChange::AdapterReady);
    assert_eq!(f.connects.load(Ordering::SeqCst), attempts);
    assert!(!run.controller.armed());
}
#[test]
fn exhausted_adapter_budget_recovers_on_adapter_return_and_stop_cancels_armed_wait() {
    let wake = Arc::new(BluetoothWake::default());
    let f = Arc::new(Shared {
        wake: Some(wake.clone()),
        ..Default::default()
    });
    f.radio_down.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(
        f.clone(),
        Duration::from_millis(1),
        Duration::from_millis(2),
    );
    run.controller.set_start_when_ready(true);
    wait_for(|| run.controller.armed());
    f.radio_down.store(false, Ordering::Release);
    wake.publish(&BluetoothChange::AdapterReady);
    wait_for(|| run.controller.stage() == "WirelessActive");
    run.stop();
    let f = Arc::new(Shared::default());
    f.radio_down.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(
        f.clone(),
        Duration::from_millis(1),
        Duration::from_millis(2),
    );
    run.controller.set_start_when_ready(true);
    wait_for(|| run.controller.armed());
    f.radio_down.store(false, Ordering::Release);
    run.controller.readiness_restored();
    run.controller.readiness_restored();
    wait_for(|| run.controller.stage() == "WirelessActive");
    run.stop();
    // A different armed run can still be closed promptly without any restoration.
    f.radio_down.store(true, Ordering::Release);
    let mut run = Run::with_bluetooth_retry(f, Duration::from_millis(1), Duration::from_millis(2));
    run.controller.set_start_when_ready(true);
    wait_for(|| run.controller.armed());
    run.stop();
    assert!(!run.controller.running());
    assert!(!run.controller.armed());
}
#[test]
fn carlink_gate_failure_on_network_worker_fails_only_the_start() {
    let f = Arc::new(Shared::default());
    f.gate_failure.store(true, Ordering::SeqCst);
    let mut run = Run::with_retry(
        f.clone(),
        Duration::from_secs(1),
        Duration::from_millis(1),
        Duration::from_millis(2),
    );
    wait_for(|| run.controller.stage() == "Failed");
    assert!(!run.controller.running());
    assert!(!f.network_started.load(Ordering::SeqCst));
    let failure = run.json.runtime_path_facts().iphone.failure.unwrap();
    assert_eq!(failure.code, "carlink_not_ready");
    assert!(!failure.retrying);
    assert!(run.matching(|v| v["event"] == "retry").is_empty());
    run.stop();
}
