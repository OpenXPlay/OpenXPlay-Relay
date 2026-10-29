// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-28

use carplay_iap2::{
    mfi::{BaaCertificatePair, MfiAuthenticator},
    CarPlayModeOutcome, DuplexStream, LockdownPairRecord, LockdownPairingError, Receive,
    UsbConfiguration, UsbDescriptors, UsbDeviceInfo, UsbError, UsbInterface,
};
use openxplay_relay::{linux_wired_controller::*, linux_wired_link::EthernetPort, *};
use std::{
    collections::VecDeque,
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

fn wait_for(mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f() {
        assert!(Instant::now() < deadline, "controller did not progress");
        thread::sleep(Duration::from_millis(2));
    }
}
fn info(number: u16) -> UsbDeviceInfo {
    UsbDeviceInfo {
        bus_number: 1,
        device_number: number,
        sysfs_path: format!("/usb/1-{number}").into(),
        speed_mbps: 480.,
        product: Some("Test iPhone".into()),
        serial: Some("test".into()),
    }
}
fn descriptors(mode: Option<bool>) -> UsbDescriptors {
    let interface = |number, class, subclass, protocol| UsbInterface {
        number,
        alternate_setting: 0,
        class,
        subclass,
        protocol,
        endpoints: Vec::new(),
        extra: Vec::new(),
    };
    let configurations = mode
        .map(|mode| {
            let mut interfaces = vec![interface(0, 0xff, 0xfe, 2), interface(1, 2, 0x0d, 0)];
            if mode {
                interfaces.push(interface(3, 0xff, 0xfd, 1));
            }
            UsbConfiguration {
                value: 1,
                interfaces,
            }
        })
        .into_iter()
        .collect();
    UsbDescriptors {
        device: Vec::new(),
        configurations,
    }
}
fn record() -> LockdownPairRecord {
    static RECORD: OnceLock<LockdownPairRecord> = OnceLock::new();
    RECORD
        .get_or_init(|| {
            LockdownPairRecord::parse(include_bytes!("fixtures/wired-pair-record.plist")).unwrap()
        })
        .clone()
}
struct Mfi;
impl MfiAuthenticator for Mfi {
    fn protocol_major(&self) -> io::Result<u8> {
        Ok(3)
    }
    fn read_certificate(&self, _: usize) -> io::Result<Vec<u8>> {
        Ok(vec![1])
    }
    fn sign_challenge(&self, _: &[u8]) -> io::Result<Vec<u8>> {
        Ok(vec![1])
    }
    fn baa_certificates(&self) -> io::Result<BaaCertificatePair> {
        Err(io::Error::other("unused"))
    }
}
#[derive(Clone, Copy)]
enum Failure {
    Unplug,
    Busy,
    Other,
}
#[derive(Default)]
struct Shared {
    devices: Mutex<Vec<UsbDeviceInfo>>,
    descriptors: Mutex<VecDeque<Option<bool>>>,
    modes: Mutex<VecDeque<Option<&'static str>>>,
    mode_requests: Mutex<Vec<Instant>>,
    fail: Mutex<VecDeque<(&'static str, Failure)>>,
    log: Mutex<Vec<String>>,
    closed: Mutex<Vec<&'static str>>,
    callbacks: Mutex<Vec<WiredCallbacks>>,
    receivers: Mutex<Vec<WiredCallbacks>>,
    pair_calls: AtomicUsize,
    carkit_calls: AtomicUsize,
    invalid_records: AtomicUsize,
    deny: Mutex<Option<&'static str>>,
    pair_hold: AtomicBool,
    airplay: AtomicBool,
    change_node: AtomicBool,
    gate: AtomicBool,
    gate_check: AtomicUsize,
    watchdog: AtomicBool,
    actual_ids: Mutex<Vec<(String, String)>>,
    pair_path: Mutex<PathBuf>,
    check_deleted: AtomicBool,
}
impl Shared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            devices: Mutex::new(vec![info(1)]),
            descriptors: Mutex::new(VecDeque::from([Some(true)])),
            airplay: AtomicBool::new(true),
            change_node: AtomicBool::new(true),
            gate: AtomicBool::new(true),
            ..Default::default()
        })
    }
    fn step(&self, name: &'static str) -> io::Result<()> {
        self.log.lock().unwrap().push(name.into());
        let mut failures = self.fail.lock().unwrap();
        if failures.front().is_some_and(|(at, _)| *at == name) {
            let (_, failure) = failures.pop_front().unwrap();
            return Err(match failure {
                Failure::Unplug => {
                    self.devices.lock().unwrap().clear();
                    UsbError::DeviceGone.into()
                }
                Failure::Busy => UsbError::InterfaceBusy(0).into(),
                Failure::Other => io::Error::other("attempt failed"),
            });
        }
        Ok(())
    }
    fn closed(&self, name: &'static str) {
        self.closed.lock().unwrap().push(name);
    }
    fn count(&self, name: &str) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.as_str() == name)
            .count()
    }
}
struct Platform(Arc<Shared>);
struct Hotspot(Arc<Shared>);
impl WirelessHotspotManager for Hotspot {
    fn start(&self, timeout: Duration) -> io::Result<WirelessHotspotInfo> {
        assert_eq!(timeout, Duration::from_secs(60));
        self.0.step("hotspot")?;
        Ok(WirelessHotspotInfo {
            ssid: "head-unit".into(),
            passphrase: "private".into(),
            channel: 36,
            security: carplay_iap2::Iap2WirelessSecurity::WpaWpa2,
            host_address: Some("192.0.2.1".parse().unwrap()),
            bssid: None,
            access_point_bssid: None,
            interface_name: Some("fake".into()),
        })
    }
    fn validate_ready(&self) -> io::Result<()> {
        Ok(())
    }
    fn on_car_play_confirmed(&self) -> io::Result<()> {
        Ok(())
    }
    fn close(&self) -> io::Result<()> {
        self.0.closed("hotspot");
        Ok(())
    }
}
impl WiredPlatform for Platform {
    fn prepare_mfi(&self) -> io::Result<MfiResource> {
        self.0.step("mfi")?;
        Ok(MfiResource {
            authenticator: Arc::new(Mfi),
        })
    }
    fn create_hotspot(&self) -> io::Result<Arc<dyn WirelessHotspotManager>> {
        Ok(Arc::new(Hotspot(self.0.clone())))
    }
    fn car_ready(&self) -> io::Result<bool> {
        self.0.gate_check.fetch_add(1, Ordering::SeqCst);
        Ok(self.0.gate.load(Ordering::Acquire))
    }
    fn enumerate(&self) -> io::Result<Vec<UsbDeviceInfo>> {
        Ok(self.0.devices.lock().unwrap().clone())
    }
    fn open_device(
        &self,
        _: &UsbDeviceInfo,
        _: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> io::Result<Arc<dyn WiredDevice>> {
        self.0.step("device")?;
        Ok(Arc::new(Device {
            shared: self.0.clone(),
            closed: AtomicBool::new(false),
        }))
    }
    fn open_control(
        &self,
        _: Arc<dyn DuplexStream>,
        _: Arc<dyn MfiAuthenticator>,
        _: WiredCallbacks,
    ) -> io::Result<Arc<dyn WiredControl>> {
        self.0.step("control")?;
        Ok(Arc::new(Control {
            shared: self.0.clone(),
            stopped: (Mutex::new(false), Condvar::new()),
        }))
    }
    fn create_tap(&self, mac: [u8; 6]) -> io::Result<Arc<dyn WiredTap>> {
        assert_eq!(mac, [2, 3, 4, 5, 6, 7]);
        self.0.step("tap")?;
        Ok(Arc::new(Tap(self.0.clone())))
    }
    fn start_pump(
        &self,
        _: Arc<dyn WiredTap>,
        _: Arc<dyn WiredNcm>,
        _: FailureCallback,
    ) -> io::Result<Arc<dyn CloseResource>> {
        self.0.step("pump")?;
        Ok(Arc::new(Pump(self.0.clone())))
    }
    fn create_receiver(
        &self,
        index: u32,
        _: Arc<dyn MfiAuthenticator>,
        callbacks: WiredCallbacks,
    ) -> io::Result<Arc<dyn WiredReceiver>> {
        assert_eq!(index, 17);
        self.0.step("receiver")?;
        self.0.receivers.lock().unwrap().push(callbacks);
        Ok(Arc::new(Receiver(self.0.clone())))
    }
}
struct Device {
    shared: Arc<Shared>,
    closed: AtomicBool,
}
impl CloseResource for Device {
    fn close(&self) -> io::Result<()> {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.shared.closed("device");
        }
        Ok(())
    }
}
impl WiredDevice for Device {
    fn discover(&self) -> io::Result<UsbDescriptors> {
        self.shared.step("descriptors")?;
        let mut d = self.shared.descriptors.lock().unwrap();
        let mode = if d.len() > 1 {
            d.pop_front().unwrap()
        } else {
            d.front().copied().unwrap_or(Some(true))
        };
        Ok(descriptors(mode))
    }
    fn request_mode(&self, _: &UsbDescriptors) -> io::Result<CarPlayModeOutcome> {
        self.shared.step("mode")?;
        self.shared
            .mode_requests
            .lock()
            .unwrap()
            .push(Instant::now());
        if let Some(Some(reason)) = self.shared.modes.lock().unwrap().pop_front() {
            Ok(CarPlayModeOutcome::Rejected {
                transferred: 0,
                diagnostic: reason.into(),
            })
        } else {
            if self.shared.change_node.load(Ordering::Acquire) {
                let mut d = self.shared.devices.lock().unwrap();
                let next = d[0].device_number + 1;
                *d = vec![info(next)];
            }
            Ok(CarPlayModeOutcome::ReenumerationRequested)
        }
    }
    fn select_configuration(&self, _: &UsbDescriptors, _: &UsbConfiguration) -> io::Result<()> {
        self.shared.step("configuration")
    }
    fn open_mux(&self) -> io::Result<Arc<dyn WiredMux>> {
        self.shared.step("mux")?;
        Ok(Arc::new(Mux(self.shared.clone())))
    }
    fn open_ncm(&self) -> io::Result<Arc<dyn WiredNcm>> {
        self.shared.step("ncm")?;
        Ok(Arc::new(Ncm(self.shared.clone())))
    }
}
struct Mux(Arc<Shared>);
impl CloseResource for Mux {
    fn close(&self) -> io::Result<()> {
        self.0.closed("mux");
        Ok(())
    }
}
impl WiredMux for Mux {
    fn pair(
        &self,
        host: &str,
        buid: &str,
        budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> std::result::Result<LockdownPairRecord, LockdownPairingError> {
        self.0.step("pair").map_err(LockdownPairingError::Io)?;
        assert_eq!(budget, Duration::from_secs(300));
        assert_eq!(host.len(), 36);
        assert_eq!(host, host.to_ascii_uppercase());
        assert_ne!(host, buid);
        self.0
            .actual_ids
            .lock()
            .unwrap()
            .push((host.into(), buid.into()));
        self.0.pair_calls.fetch_add(1, Ordering::SeqCst);
        if self.0.check_deleted.load(Ordering::Acquire) {
            assert!(!self.0.pair_path.lock().unwrap().exists());
        }
        while self.0.pair_hold.load(Ordering::Acquire) {
            if cancelled() {
                return Err(LockdownPairingError::Cancelled);
            }
            thread::sleep(Duration::from_millis(1));
        }
        if let Some(reason) = *self.0.deny.lock().unwrap() {
            return Err(if reason == "denied" {
                LockdownPairingError::UserDenied
            } else {
                LockdownPairingError::PasswordProtected
            });
        }
        let mut record = record();
        record.host_id = host.into();
        record.system_buid = buid.into();
        Ok(record)
    }
    fn open_carkit(
        &self,
        record: &LockdownPairRecord,
    ) -> std::result::Result<Arc<dyn DuplexStream>, LockdownPairingError> {
        self.0.step("carkit").map_err(LockdownPairingError::Io)?;
        self.0.carkit_calls.fetch_add(1, Ordering::SeqCst);
        if self
            .0
            .invalid_records
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(LockdownPairingError::InvalidHostId);
        }
        assert!(!record.host_id.is_empty());
        Ok(Arc::new(Stream(self.0.clone())))
    }
}
struct Stream(Arc<Shared>);
impl DuplexStream for Stream {
    fn send(&self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn recv(&self, _: usize, _: Duration) -> io::Result<Receive> {
        Ok(Receive::Timeout)
    }
    fn close(&self) -> io::Result<()> {
        self.0.closed("carkit");
        Ok(())
    }
}
struct Ncm(Arc<Shared>);
impl EthernetPort for Ncm {
    fn send_frame(&self, _: &[u8], _: Duration) -> io::Result<()> {
        Ok(())
    }
    fn receive_frame(&self, _: Duration) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn close(&self) -> io::Result<()> {
        self.0.closed("ncm");
        Ok(())
    }
}
impl WiredNcm for Ncm {
    fn host_mac(&self) -> [u8; 6] {
        [2, 3, 4, 5, 6, 7]
    }
}
struct Tap(Arc<Shared>);
impl CloseResource for Tap {
    fn close(&self) -> io::Result<()> {
        self.0.closed("tap");
        Ok(())
    }
}
impl WiredTap for Tap {
    fn index(&self) -> u32 {
        17
    }
    fn port(&self) -> Arc<dyn EthernetPort> {
        Arc::new(Ncm(self.0.clone()))
    }
}
struct Pump(Arc<Shared>);
impl CloseResource for Pump {
    fn close(&self) -> io::Result<()> {
        self.0.closed("pump");
        Ok(())
    }
}
struct Receiver(Arc<Shared>);
impl CloseResource for Receiver {
    fn close(&self) -> io::Result<()> {
        self.0.closed("receiver");
        Ok(())
    }
}
impl WiredReceiver for Receiver {
    fn start(&self) -> io::Result<u16> {
        self.0.step("listen")?;
        Ok(7003)
    }
    fn start_session_sent(&self, _: Instant) {
        self.0.log.lock().unwrap().push("watchdog-armed".into());
    }
    fn poll_startup(&self, _: Instant) -> io::Result<()> {
        if self.0.watchdog.load(Ordering::Acquire) {
            Err(io::Error::other(
                "first_tcp_timeout: wired AirPlay did not connect",
            ))
        } else {
            Ok(())
        }
    }
}
struct Control {
    shared: Arc<Shared>,
    stopped: (Mutex<bool>, Condvar),
}
impl CloseResource for Control {
    fn close(&self) -> io::Result<()> {
        *self.stopped.0.lock().unwrap() = true;
        self.stopped.1.notify_all();
        self.shared.closed("control");
        Ok(())
    }
}
impl WiredControl for Control {
    fn run(&self, port: u16, mac: [u8; 6], callbacks: WiredCallbacks) -> io::Result<()> {
        assert_eq!(port, 7003);
        assert_eq!(mac, [2, 3, 4, 5, 6, 7]);
        self.shared.step("run")?;
        self.shared
            .callbacks
            .lock()
            .unwrap()
            .push(callbacks.clone());
        (callbacks.on_authenticated)();
        (callbacks.on_start_session)();
        if self.shared.airplay.load(Ordering::Acquire) {
            (callbacks.on_air_play)();
        }
        drop(
            self.stopped
                .1
                .wait_while(self.stopped.0.lock().unwrap(), |s| !*s)
                .unwrap(),
        );
        Ok(())
    }
}
struct Run {
    controller: Arc<LinuxWiredController>,
    worker: Option<thread::JoinHandle<()>>,
    events: Arc<Mutex<Vec<serde_json::Value>>>,
    dir: PathBuf,
    json: Arc<JsonEvents>,
}
impl Run {
    fn new(shared: Arc<Shared>) -> Self {
        Self::build(shared, true)
    }
    fn build(shared: Arc<Shared>, saved: bool) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "wired-controller-{}",
            openxplay_relay::admin::auth::random_hex(8).unwrap()
        ));
        let path = dir.join("lockdown/pair-record.plist");
        std::fs::create_dir_all(&dir).unwrap();
        if saved {
            openxplay_relay::platform::private_state_file(&path, &record().serialize().unwrap())
                .unwrap();
        }
        *shared.pair_path.lock().unwrap() = path;
        let events = Arc::new(Mutex::new(Vec::new()));
        let copy = events.clone();
        let json = Arc::new(JsonEvents::new(Arc::new(move |line| {
            copy.lock()
                .unwrap()
                .push(serde_json::from_str(&line).unwrap())
        })));
        let timing = WiredTiming {
            usb_poll: Duration::from_millis(8),
            reenumeration_poll: Duration::from_millis(2),
            reenumeration_timeout: Duration::from_millis(30),
            retry_initial: Duration::from_millis(3),
            retry_max: Duration::from_millis(12),
            tick: Duration::from_millis(2),
        };
        let controller = Arc::new(
            LinuxWiredController::new(Arc::new(Platform(shared)), json.clone(), dir.clone())
                .with_timing(timing),
        );
        let c = controller.clone();
        let worker = thread::spawn(move || c.run().unwrap());
        Self {
            controller,
            worker: Some(worker),
            events,
            dir,
            json,
        }
    }
    fn count(&self, event: &str) -> usize {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["event"] == event)
            .count()
    }
    fn stage(&self, name: &str) -> bool {
        self.events
            .lock()
            .unwrap()
            .iter()
            .any(|v| v["stage"] == name && v["event"] == "stage")
    }
    fn failure(&self) -> String {
        self.json.runtime_path_facts().iphone.failure.unwrap().code
    }
    fn stop(&mut self) {
        self.controller.close().unwrap();
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn happy_path_uses_existing_record_and_emits_ordered_stages() {
    let f = Shared::new();
    let mut run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    assert_eq!(f.pair_calls.load(Ordering::Acquire), 0);
    assert_eq!(f.carkit_calls.load(Ordering::Acquire), 1);
    let stages: Vec<String> = run
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|v| v["event"] == "stage")
        .map(|v| v["stage"].as_str().unwrap().into())
        .collect();
    assert_eq!(
        stages,
        [
            "DiscoveringMfi",
            "MfiReady",
            "StartingHotspot",
            "HotspotReady",
            "WaitingForIphoneUsb",
            "SelectingUsbConfiguration",
            "OpeningUsbmux",
            "ConnectingControl",
            "RunningControl",
            "StartSessionSent",
            "Session"
        ]
    );
    assert!(run.json.runtime_path_facts().usb.present);
    assert_eq!(run.json.runtime_path_facts().iphone.step, "SESSION");
    assert_eq!(f.count("watchdog-armed"), 1);
    run.stop();
    assert_eq!(
        *f.closed.lock().unwrap(),
        ["control", "carkit", "mux", "receiver", "pump", "tap", "ncm", "device", "hotspot"]
    );
}
#[test]
fn fresh_pairing_saves_private_record_with_fresh_uppercase_uuids() {
    use std::os::unix::fs::PermissionsExt;
    let f = Shared::new();
    let run = Run::build(f.clone(), false);
    wait_for(|| run.controller.stage() == "Session");
    assert!(run.stage("Pairing"));
    let path = run.dir.join("lockdown/pair-record.plist");
    let r = LockdownPairRecord::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(r.host_id, f.actual_ids.lock().unwrap()[0].0);
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(run.dir.join("lockdown"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(!run
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|v| v.to_string().contains(&r.host_id)));
}
#[test]
fn invalid_pair_record_is_deleted_then_paired_once_and_retried() {
    let f = Shared::new();
    f.invalid_records.store(1, Ordering::Release);
    f.check_deleted.store(true, Ordering::Release);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    assert_eq!(f.pair_calls.load(Ordering::Acquire), 1);
    assert_eq!(f.carkit_calls.load(Ordering::Acquire), 2);
    assert_ne!(
        LockdownPairRecord::parse(
            &std::fs::read(run.dir.join("lockdown/pair-record.plist")).unwrap()
        )
        .unwrap()
        .host_id,
        "STORED-HOST"
    );
}
#[test]
fn invalid_record_repair_is_limited_to_once_per_attempt() {
    let f = Shared::new();
    f.invalid_records.store(100, Ordering::Release);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Failed");
    assert_eq!(f.pair_calls.load(Ordering::Acquire), 6);
    assert_eq!(f.carkit_calls.load(Ordering::Acquire), 12);
    assert_eq!(run.count("retry"), 5);
}
#[test]
fn mode_request_reenumerates_and_uses_the_new_node() {
    let f = Shared::new();
    *f.descriptors.lock().unwrap() = VecDeque::from([Some(false), Some(true)]);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    assert!(run.stage("RequestingCarPlayMode"));
    assert!(run.stage("WaitingForReenumeration"));
    assert_eq!(f.count("mode"), 1);
    assert_eq!(f.count("device"), 2);
    assert_eq!(run.count("retry"), 0);
}
#[test]
fn rejected_mode_falls_back_without_apple_ethernet() {
    let f = Shared::new();
    *f.descriptors.lock().unwrap() = VecDeque::from([Some(false)]);
    f.modes.lock().unwrap().push_back(Some("mode read 3:3:3:0"));
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    assert_eq!(f.count("mode"), 1);
    assert!(!run.stage("WaitingForReenumeration"));
    assert_eq!(run.count("retry"), 0);
}
#[test]
fn reenumeration_timeout_fails_with_a_bounded_retry_budget() {
    let f = Shared::new();
    *f.descriptors.lock().unwrap() = VecDeque::from([Some(false)]);
    f.change_node.store(false, Ordering::Release);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Failed");
    assert_eq!(run.count("retry"), 5);
    assert!(run.events.lock().unwrap().iter().any(|v| v["reason"]
        .as_str()
        .is_some_and(|s| s.contains("re-enumeration timed out"))));
    assert_eq!(f.count("mux"), 0);
}
#[test]
fn absent_configuration_asks_for_mode_at_most_twice_per_attempt() {
    let f = Shared::new();
    *f.descriptors.lock().unwrap() = VecDeque::from([None]);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Failed");
    assert_eq!(f.count("mode"), 12);
    assert_eq!(f.count("mux"), 0);
}
#[test]
fn denied_trust_waits_for_a_new_usb_node_then_pairs_again() {
    for (automatic, missed_absence) in [(false, false), (false, true), (true, false), (true, true)]
    {
        let f = Shared::new();
        *f.deny.lock().unwrap() = Some("denied");
        let run = Run::build(f.clone(), false);
        run.controller.set_start_when_ready(automatic);
        wait_for(|| run.controller.armed() && run.controller.stage() == "Failed");
        let failure = run.json.runtime_path_facts().iphone.failure.unwrap();
        assert_eq!(failure.code, "usb_trust_denied");
        assert!(!failure.retrying);
        assert!(run.controller.running());
        *f.deny.lock().unwrap() = None;
        thread::sleep(Duration::from_millis(40));
        assert_eq!(run.count("retry"), 0);
        assert_eq!(f.pair_calls.load(Ordering::Acquire), 1);
        if !missed_absence {
            f.devices.lock().unwrap().clear();
            wait_for(|| !run.json.runtime_path_facts().usb.present);
            thread::sleep(Duration::from_millis(20));
            assert_eq!(f.pair_calls.load(Ordering::Acquire), 1);
        }
        *f.devices.lock().unwrap() = vec![info(2)];
        wait_for(|| run.controller.stage() == "Session");
        assert_eq!(f.pair_calls.load(Ordering::Acquire), 2);
        assert_eq!(run.count("startup-failed"), 1);
        assert_eq!(run.count("retry"), 0);
    }
}
#[test]
fn rejected_mode_request_retries_with_backoff_then_recovers_when_accepted() {
    assert_eq!(WiredTiming::default().retry_initial, Duration::from_secs(2));
    assert_eq!(WiredTiming::default().retry_max, Duration::from_secs(30));
    let f = Shared::new();
    *f.descriptors.lock().unwrap() = VecDeque::from([None, None, None, None, None, Some(true)]);
    f.modes
        .lock()
        .unwrap()
        .extend([Some("mode read 0:0:0:0"); 4]);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    let events = run.events.lock().unwrap();
    let retries: Vec<_> = events.iter().filter(|v| v["event"] == "retry").collect();
    assert_eq!(retries.len(), 4);
    let requests = f.mode_requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    for ((retry, requests), delay) in retries.iter().zip(requests.windows(2)).zip([3, 6, 12, 12]) {
        let reason = retry["reason"].as_str().unwrap();
        assert_eq!(
            reason,
            "Unlock the iPhone. It rejected the CarPlay USB mode request (mode read 0:0:0:0)"
        );
        assert_eq!(
            openxplay_relay::carplay_policy::classify_failure(reason),
            "iphone_locked"
        );
        assert_eq!(retry["retryMillis"], delay);
        assert!(requests[1].duration_since(requests[0]) >= Duration::from_millis(delay));
    }
    drop(events);
    assert_eq!(run.count("startup-failed"), 0);
    assert!(run.json.runtime_path_facts().iphone.failure.is_none());
}
#[test]
fn locked_mode_retries_preserve_the_previously_spent_budget() {
    let f = Shared::new();
    f.fail.lock().unwrap().push_back(("mfi", Failure::Other));
    *f.descriptors.lock().unwrap() = VecDeque::from([
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(true),
    ]);
    f.modes
        .lock()
        .unwrap()
        .extend([Some("mode read 0:0:0:0"); 8]);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    assert_eq!(run.count("retry"), 9);
    // One ordinary failure was already spent before eight locked retries.
    for n in 0..5 {
        let attempts = f.callbacks.lock().unwrap().len();
        let callback = f.callbacks.lock().unwrap().last().unwrap().clone();
        (callback.on_failure)(io::Error::other("ordinary failure"));
        if n == 4 {
            wait_for(|| run.controller.stage() == "Failed");
        } else {
            wait_for(|| {
                f.callbacks.lock().unwrap().len() > attempts && run.controller.stage() == "Session"
            });
        }
    }
    assert_eq!(run.count("retry"), 13);
    assert_eq!(run.count("startup-failed"), 1);
}
#[test]
fn password_protected_pairing_keeps_its_failure_visible_and_does_not_spend_retries() {
    let f = Shared::new();
    *f.deny.lock().unwrap() = Some("locked");
    let run = Run::build(f.clone(), false);
    wait_for(|| run.count("retry") >= 8);
    let failure = run.json.runtime_path_facts().iphone.failure.unwrap();
    assert_eq!(failure.code, "iphone_locked");
    assert!(failure.retrying);
    assert!(run.controller.failed());
    assert!(run.controller.running());
    assert_eq!(run.count("startup-failed"), 0);
    *f.deny.lock().unwrap() = None;
    wait_for(|| run.controller.stage() == "Session");
    let locked_retries = run.count("retry");
    for n in 0..6 {
        let attempts = f.callbacks.lock().unwrap().len();
        let callback = f.callbacks.lock().unwrap().last().unwrap().clone();
        (callback.on_failure)(io::Error::other("ordinary failure"));
        if n == 5 {
            wait_for(|| run.controller.stage() == "Failed");
        } else {
            wait_for(|| {
                f.callbacks.lock().unwrap().len() > attempts && run.controller.stage() == "Session"
            });
        }
    }
    assert_eq!(run.count("retry"), locked_retries + 5);
    assert_eq!(run.count("startup-failed"), 1);
}
#[test]
fn usb_busy_has_an_explicit_usbmuxd_failure() {
    let f = Shared::new();
    f.fail.lock().unwrap().push_back(("mux", Failure::Busy));
    let run = Run::new(f);
    wait_for(|| run.controller.stage() == "Session");
    let events = run.events.lock().unwrap();
    let retry = events.iter().find(|v| v["event"] == "retry").unwrap();
    assert!(retry["reason"]
        .as_str()
        .unwrap()
        .contains("another program (usbmuxd?) holds the iPhone"));
    assert_eq!(
        openxplay_relay::carplay_policy::classify_failure(retry["reason"].as_str().unwrap()),
        "usb_interface_busy"
    );
}
#[test]
fn unplug_at_each_resource_stage_waits_for_replug_without_consuming_retries() {
    for at in [
        "device",
        "descriptors",
        "configuration",
        "mux",
        "ncm",
        "pair",
        "carkit",
        "control",
        "tap",
        "pump",
        "receiver",
        "listen",
        "run",
    ] {
        let f = Shared::new();
        f.fail.lock().unwrap().push_back((at, Failure::Unplug));
        let run = Run::build(f.clone(), at != "pair");
        wait_for(|| run.count("retry") == 1 && run.controller.stage() == "WaitingForIphoneUsb");
        assert_eq!(run.failure(), "iphone_unplugged", "{at}");
        *f.devices.lock().unwrap() = vec![info(2)];
        wait_for(|| run.controller.stage() == "Session");
        assert_eq!(f.count("hotspot"), 1, "head-unit network retained at {at}");
    }
}
#[test]
fn repeated_unplug_does_not_spend_the_failure_budget() {
    let f = Shared::new();
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    for n in 0..8 {
        let callback = f.callbacks.lock().unwrap().last().unwrap().clone();
        f.devices.lock().unwrap().clear();
        (callback.on_failure)(UsbError::DeviceGone.into());
        wait_for(|| run.controller.stage() == "WaitingForIphoneUsb");
        *f.devices.lock().unwrap() = vec![info(n + 2)];
        wait_for(|| run.controller.stage() == "Session");
    }
    // After eight unplugs there is still the complete ordinary failure budget.
    for n in 0..6 {
        let count = run.count("retry");
        let attempts = f.callbacks.lock().unwrap().len();
        let callback = f.callbacks.lock().unwrap().last().unwrap().clone();
        (callback.on_failure)(io::Error::other("control failure"));
        if n == 5 {
            wait_for(|| run.controller.stage() == "Failed");
        } else {
            wait_for(|| {
                run.count("retry") > count
                    && f.callbacks.lock().unwrap().len() > attempts
                    && run.controller.stage() == "Session"
            });
        }
    }
    assert_eq!(run.count("retry"), 13);
    assert_eq!(run.count("startup-failed"), 1);
}
#[test]
fn stale_callbacks_cannot_mutate_a_new_attempt_or_restart_a_closed_controller() {
    let f = Shared::new();
    let mut run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    let old = f.callbacks.lock().unwrap()[0].clone();
    (old.on_failure)(io::Error::other("retry this attempt"));
    wait_for(|| f.callbacks.lock().unwrap().len() == 2 && run.controller.stage() == "Session");
    let before = run.events.lock().unwrap().len();
    assert!(!(old.current)());
    (old.on_start_session)();
    (old.on_authenticated)();
    (old.on_air_play)();
    (old.on_failure)(UsbError::DeviceGone.into());
    assert_eq!(run.events.lock().unwrap().len(), before);
    assert_eq!(run.controller.stage(), "Session");
    run.stop();
    let before = run.events.lock().unwrap().len();
    let latest = f.callbacks.lock().unwrap().last().unwrap().clone();
    (latest.on_start_session)();
    (latest.on_air_play)();
    (latest.on_failure)(io::Error::other("late"));
    run.controller.readiness_restored();
    assert_eq!(run.controller.stage(), "Stopped");
    assert_eq!(run.events.lock().unwrap().len(), before);
}
#[test]
fn session_memory_requests_mode_again_on_the_same_enumeration() {
    let f = Shared::new();
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    let old = f.callbacks.lock().unwrap()[0].clone();
    (old.on_failure)(io::Error::other("try again"));
    wait_for(|| f.callbacks.lock().unwrap().len() == 2 && run.controller.stage() == "Session");
    assert_eq!(f.count("mode"), 1);
}
#[test]
fn presence_is_reported_while_idle_and_after_terminal_failure() {
    let f = Shared::new();
    f.devices.lock().unwrap().clear();
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "WaitingForIphoneUsb");
    assert!(!run.json.runtime_path_facts().usb.present);
    *f.devices.lock().unwrap() = vec![info(1)];
    wait_for(|| run.controller.stage() == "Session");
    assert!(run.json.runtime_path_facts().usb.present);
    let callback = f.callbacks.lock().unwrap()[0].clone();
    (callback.on_failure)(io::Error::other("UserDeniedPairing"));
    wait_for(|| run.controller.stage() == "Failed");
    f.devices.lock().unwrap().clear();
    wait_for(|| !run.json.runtime_path_facts().usb.present);
    assert!(run.json.runtime_path_facts().usb.product.is_none());
}
#[test]
fn close_cancels_trust_wait_and_head_unit_gate_without_waiting_for_their_budget() {
    for pairing in [false, true] {
        let f = Shared::new();
        f.pair_hold.store(pairing, Ordering::Release);
        f.gate.store(pairing, Ordering::Release);
        let mut run = Run::build(f.clone(), false);
        wait_for(|| {
            if pairing {
                f.pair_calls.load(Ordering::Acquire) == 1
            } else {
                f.gate_check.load(Ordering::Acquire) > 0
            }
        });
        let start = Instant::now();
        run.stop();
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
#[test]
fn wired_watchdog_failure_retries_and_start_session_is_emitted_once() {
    let f = Shared::new();
    f.airplay.store(false, Ordering::Release);
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "StartSessionSent");
    let callback = f.callbacks.lock().unwrap()[0].clone();
    (callback.on_start_session)();
    assert_eq!(f.count("watchdog-armed"), 1);
    f.watchdog.store(true, Ordering::Release);
    wait_for(|| run.count("retry") > 0);
    assert_eq!(run.failure(), "first_tcp_timeout");
}
#[test]
fn retry_budget_is_preserved_across_unplug_between_other_failures() {
    let f = Shared::new();
    f.fail.lock().unwrap().extend([
        ("configuration", Failure::Other),
        ("configuration", Failure::Unplug),
    ]);
    let run = Run::new(f.clone());
    wait_for(|| run.count("retry") == 2 && run.controller.stage() == "WaitingForIphoneUsb");
    f.fail
        .lock()
        .unwrap()
        .extend((0..5).map(|_| ("configuration", Failure::Other)));
    *f.devices.lock().unwrap() = vec![info(2)];
    wait_for(|| run.controller.stage() == "Failed");
    assert_eq!(run.count("retry"), 6);
    assert_eq!(run.count("startup-failed"), 1);
}

#[test]
fn reconnect_mode_rejection_reuses_previously_started_carplay_descriptors() {
    let f = Shared::new();
    let run = Run::new(f.clone());
    wait_for(|| run.controller.stage() == "Session");
    f.modes.lock().unwrap().push_back(Some("request rejected"));
    let old = f.callbacks.lock().unwrap()[0].clone();
    (old.on_failure)(io::Error::other("retry this device"));
    wait_for(|| f.callbacks.lock().unwrap().len() == 2 && run.controller.stage() == "Session");
    assert_eq!(f.count("mode"), 1);
    assert!(!run.stage("WaitingForReenumeration"));
}
#[test]
fn automatic_exhaustion_waits_for_replug_with_the_head_unit_network_kept() {
    let f = Shared::new();
    let mut run = Run::new(f.clone());
    run.controller.set_start_when_ready(true);
    wait_for(|| run.controller.stage() == "Session");
    for n in 0..6 {
        let attempts = f.callbacks.lock().unwrap().len();
        let callback = f.callbacks.lock().unwrap().last().unwrap().clone();
        (callback.on_failure)(io::Error::other("failed startup"));
        if n == 5 {
            wait_for(|| run.controller.armed());
        } else {
            wait_for(|| {
                f.callbacks.lock().unwrap().len() > attempts && run.controller.stage() == "Session"
            });
        }
    }
    assert!(run.controller.running());
    assert_eq!(f.count("hotspot"), 1);
    let attempts = f.callbacks.lock().unwrap().len();
    thread::sleep(Duration::from_millis(30));
    assert_eq!(f.callbacks.lock().unwrap().len(), attempts);
    f.devices.lock().unwrap().clear();
    wait_for(|| !run.json.runtime_path_facts().usb.present);
    *f.devices.lock().unwrap() = vec![info(20)];
    wait_for(|| run.controller.stage() == "Session");
    assert_eq!(f.count("hotspot"), 1);
    assert_eq!(f.callbacks.lock().unwrap().len(), attempts + 1);
    run.stop();
    run.controller.readiness_restored();
    assert!(!run.controller.armed());
}
#[test]
fn close_cancels_reenumeration_without_waiting_for_its_deadline() {
    let f = Shared::new();
    *f.descriptors.lock().unwrap() = VecDeque::from([Some(false)]);
    f.change_node.store(false, Ordering::Release);
    let mut run = Run::new(f);
    wait_for(|| run.controller.stage() == "WaitingForReenumeration");
    run.stop();
    assert_eq!(run.count("retry"), 0);
    assert_eq!(run.controller.stage(), "Stopped");
}

#[test]
fn disabling_automatic_start_releases_an_armed_head_unit_network() {
    let f = Shared::new();
    f.fail
        .lock()
        .unwrap()
        .extend((0..6).map(|_| ("configuration", Failure::Other)));
    let run = Run::new(f.clone());
    run.controller.set_start_when_ready(true);
    wait_for(|| run.controller.armed());
    assert!(!f.closed.lock().unwrap().contains(&"hotspot"));
    run.controller.set_start_when_ready(false);
    wait_for(|| f.closed.lock().unwrap().contains(&"hotspot"));
    assert!(run.controller.running());
    assert!(run.controller.armed());
    *f.devices.lock().unwrap() = vec![info(20)];
    wait_for(|| run.controller.stage() == "Session");
    assert_eq!(f.count("hotspot"), 2);
}
#[test]
fn a_new_usb_node_recovers_exhaustion_with_a_fresh_budget_regardless_of_automatic_start() {
    for automatic in [false, true] {
        let f = Shared::new();
        f.fail
            .lock()
            .unwrap()
            .extend((0..6).map(|_| ("configuration", Failure::Other)));
        let run = Run::new(f.clone());
        run.controller.set_start_when_ready(automatic);
        wait_for(|| run.controller.armed() && run.controller.stage() == "Failed");
        assert_eq!(run.count("retry"), 5);
        f.fail
            .lock()
            .unwrap()
            .extend((0..5).map(|_| ("configuration", Failure::Other)));
        *f.devices.lock().unwrap() = vec![info(20)];
        wait_for(|| run.controller.stage() == "Session");
        assert_eq!(run.count("retry"), 10);
        assert_eq!(f.count("hotspot"), if automatic { 1 } else { 2 });
    }
}
#[test]
fn readiness_restored_recovers_denied_trust_regardless_of_automatic_start() {
    for automatic in [false, true] {
        let f = Shared::new();
        *f.deny.lock().unwrap() = Some("denied");
        let run = Run::build(f.clone(), false);
        run.controller.set_start_when_ready(automatic);
        wait_for(|| run.controller.armed() && run.controller.stage() == "Failed");
        *f.deny.lock().unwrap() = None;
        run.controller.readiness_restored();
        wait_for(|| run.controller.stage() == "Session");
        assert_eq!(f.pair_calls.load(Ordering::Acquire), 2);
    }
}
