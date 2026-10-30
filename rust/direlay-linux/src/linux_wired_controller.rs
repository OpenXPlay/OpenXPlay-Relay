// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-28

//! Wired lifecycle. Protocol operations and Linux resources are supplied by WiredPlatform.
use crate::{carplay_policy::*, linux_wired_link::EthernetPort, *};
use carplay_iap2::{
    mfi::MfiAuthenticator, CarPlayModeOutcome, DuplexStream, IphoneCarPlayConfiguration,
    LockdownPairRecord, LockdownPairingError, UsbConfiguration, UsbDescriptors, UsbDeviceInfo,
    UsbError, WiredConfigurationAction, WiredSessionMemory,
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

pub type FailureCallback = Arc<dyn Fn(io::Error) + Send + Sync>;
/// Implementations must honor deadlines and wake pending operations when closed.
pub trait WiredDevice: CloseResource {
    fn discover(&self) -> Result<UsbDescriptors>;
    fn request_mode(&self, descriptors: &UsbDescriptors) -> Result<CarPlayModeOutcome>;
    fn select_configuration(
        &self,
        descriptors: &UsbDescriptors,
        config: &UsbConfiguration,
    ) -> Result<()>;
    fn open_mux(&self) -> Result<Arc<dyn WiredMux>>;
    fn open_ncm(&self) -> Result<Arc<dyn WiredNcm>>;
}
pub trait WiredMux: CloseResource {
    fn pair(
        &self,
        host_id: &str,
        system_buid: &str,
        budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> std::result::Result<LockdownPairRecord, LockdownPairingError>;
    fn open_carkit(
        &self,
        record: &LockdownPairRecord,
    ) -> std::result::Result<Arc<dyn DuplexStream>, LockdownPairingError>;
}
pub trait WiredNcm: EthernetPort {
    fn host_mac(&self) -> [u8; 6];
}
pub trait WiredTap: CloseResource {
    fn index(&self) -> u32;
    fn port(&self) -> Arc<dyn EthernetPort>;
}
pub trait WiredReceiver: CloseResource {
    fn start(&self) -> Result<u16>;
    fn start_session_sent(&self, now: Instant);
    fn poll_startup(&self, now: Instant) -> Result<()>;
}
#[derive(Clone)]
pub struct WiredCallbacks {
    pub on_start_session: Arc<dyn Fn() + Send + Sync>,
    pub on_authenticated: Arc<dyn Fn() + Send + Sync>,
    pub on_air_play: Arc<dyn Fn() + Send + Sync>,
    pub on_failure: FailureCallback,
    /// Gates platform callbacks, including input and trace events, after an attempt retires.
    pub current: Arc<dyn Fn() -> bool + Send + Sync>,
}
pub trait WiredControl: CloseResource {
    fn run(&self, port: u16, host_mac: [u8; 6], callbacks: WiredCallbacks) -> Result<()>;
}
/// Test seam for every external resource; fakes need no USB, TAP or network sockets.
pub trait WiredPlatform: Send + Sync {
    fn prepare_mfi(&self) -> Result<MfiResource>;
    fn create_hotspot(&self) -> Result<Arc<dyn WirelessHotspotManager>>;
    fn prepare_hotspot(&self) {}
    fn hotspot_down_remaining(&self) -> Duration {
        Duration::ZERO
    }
    fn hotspot_reused(&self) -> bool {
        false
    }
    fn keep_hotspot_for_retry(&self) {}
    fn cancel_hotspot_retry(&self) -> Result<()> {
        Ok(())
    }
    fn car_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn enumerate(&self) -> Result<Vec<UsbDeviceInfo>>;
    fn open_device(
        &self,
        info: &UsbDeviceInfo,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Arc<dyn WiredDevice>>;
    fn open_control(
        &self,
        stream: Arc<dyn DuplexStream>,
        mfi: Arc<dyn MfiAuthenticator>,
        callbacks: WiredCallbacks,
    ) -> Result<Arc<dyn WiredControl>>;
    fn create_tap(&self, host_mac: [u8; 6]) -> Result<Arc<dyn WiredTap>>;
    fn start_pump(
        &self,
        tap: Arc<dyn WiredTap>,
        ncm: Arc<dyn WiredNcm>,
        failure: FailureCallback,
    ) -> Result<Arc<dyn CloseResource>>;
    fn create_receiver(
        &self,
        index: u32,
        mfi: Arc<dyn MfiAuthenticator>,
        callbacks: WiredCallbacks,
    ) -> Result<Arc<dyn WiredReceiver>>;
}

#[derive(Clone, Copy)]
pub struct WiredTiming {
    pub usb_poll: Duration,
    pub unplug_settle: Duration,
    pub reenumeration_poll: Duration,
    pub reenumeration_timeout: Duration,
    pub retry_initial: Duration,
    pub retry_max: Duration,
    pub tick: Duration,
}
impl Default for WiredTiming {
    fn default() -> Self {
        Self {
            usb_poll: Duration::from_secs(2),
            unplug_settle: Duration::from_secs(10),
            reenumeration_poll: Duration::from_millis(250),
            reenumeration_timeout: Duration::from_secs(30),
            retry_initial: Duration::from_secs(2),
            retry_max: Duration::from_secs(30),
            tick: Duration::from_millis(100),
        }
    }
}
// Ordered independently of acquisition: USBMUX is opened before NCM and the control stream.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Hotspot,
    Device,
    Ncm,
    Tap,
    Pump,
    Receiver,
    Mux,
    Carkit,
    Control,
}
type Cleanup = Box<dyn FnOnce() -> Result<()> + Send>;
struct State {
    closed: bool,
    running: bool,
    started: bool,
    generation: u64,
    stage: String,
    failure: Option<String>,
    failed: bool,
    automatic: bool,
    armed: bool,
    restored: bool,
    hotspot: Option<WirelessHotspotInfo>,
    receiver: Option<Arc<dyn WiredReceiver>>,
    start_sent: bool,
    device: Option<UsbDeviceInfo>,
    restart_nodes: Vec<UsbDeviceInfo>,
    transitioning: bool,
    // Invalidates presence snapshots across node changes and mode transitions.
    presence_revision: u64,
    airplay: bool,
    resources: Vec<(Layer, Cleanup)>,
}
type Shared = Arc<(Mutex<State>, Condvar)>;
pub struct LinuxWiredController {
    platform: Arc<dyn WiredPlatform>,
    events: Arc<JsonEvents>,
    state_dir: PathBuf,
    state: Shared,
    memory: Arc<WiredSessionMemory>,
    timing: WiredTiming,
    head_unit_manager: Mutex<Option<Arc<dyn WirelessHotspotManager>>>,
}
impl LinuxWiredController {
    pub fn new(
        platform: Arc<dyn WiredPlatform>,
        events: Arc<JsonEvents>,
        state_dir: PathBuf,
    ) -> Self {
        Self {
            platform,
            events,
            state_dir,
            head_unit_manager: Mutex::default(),
            memory: Arc::default(),
            timing: WiredTiming::default(),
            state: Arc::new((
                Mutex::new(State {
                    closed: false,
                    running: false,
                    started: false,
                    generation: 0,
                    stage: "Idle".into(),
                    failure: None,
                    failed: false,
                    automatic: false,
                    armed: false,
                    restored: false,
                    hotspot: None,
                    receiver: None,
                    start_sent: false,
                    device: None,
                    restart_nodes: Vec::new(),
                    transitioning: false,
                    presence_revision: 0,
                    airplay: false,
                    resources: Vec::new(),
                }),
                Condvar::new(),
            )),
        }
    }
    pub fn with_timing(mut self, timing: WiredTiming) -> Self {
        self.timing = timing;
        self
    }
    pub fn stage(&self) -> String {
        self.state.0.lock().unwrap().stage.clone()
    }
    pub fn failed(&self) -> bool {
        let s = self.state.0.lock().unwrap();
        s.failed || s.failure.is_some()
    }
    pub fn running(&self) -> bool {
        self.state.0.lock().unwrap().running
    }
    pub fn armed(&self) -> bool {
        self.state.0.lock().unwrap().armed
    }
    pub fn hotspot_info(&self) -> Option<WirelessHotspotInfo> {
        self.state.0.lock().unwrap().hotspot.clone()
    }
    pub fn set_start_when_ready(&self, automatic: bool) {
        self.state.0.lock().unwrap().automatic = automatic;
        self.state.1.notify_all();
    }
    pub fn readiness_restored(&self) {
        let mut s = self.state.0.lock().unwrap();
        if s.armed {
            s.restored = true;
            self.state.1.notify_all();
        }
    }
    fn stage_event(&self, generation: u64, stage: &str, fields: serde_json::Value) -> Result<()> {
        let mut s = self.state.0.lock().unwrap();
        check_state(&s, generation)?;
        s.stage = stage.into();
        let mut fields = fields;
        fields["stage"] = stage.into();
        fields["generation"] = generation.into();
        self.events.emit("stage", &fields);
        Ok(())
    }
    fn check(&self, generation: u64) -> Result<()> {
        check_state(&self.state.0.lock().unwrap(), generation)
    }
    fn wait(&self, generation: u64, duration: Duration) -> Result<()> {
        let s = self.state.0.lock().unwrap();
        let (s, _) = self
            .state
            .1
            .wait_timeout_while(s, duration, |s| check_state(s, generation).is_ok())
            .unwrap();
        check_state(&s, generation)
    }
    fn own(&self, generation: u64, layer: Layer, close: Cleanup) -> Result<()> {
        let mut s = self.state.0.lock().unwrap();
        match check_state(&s, generation) {
            Ok(()) => {
                s.resources.push((layer, close));
                Ok(())
            }
            Err(e) => {
                drop(s);
                self.close_step(close);
                Err(e)
            }
        }
    }
    fn close_step(&self, close: Cleanup) {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let result = crate::platform::spawn_worker(
            "wired-close",
            Box::new(move || {
                let _ = tx.send(close());
            }),
        );
        let result = result.and_then(|_| {
            rx.recv_timeout(Duration::from_secs(2)).map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "Wired resource shutdown timed out")
            })?
        });
        if let Err(e) = result {
            self.events.log(&format!("Shutdown failed: {e}"));
        }
    }
    fn release(&self, keep_hotspot: bool) {
        let resources = {
            let mut s = self.state.0.lock().unwrap();
            s.generation += 1;
            s.failure = None;
            s.receiver = None;
            s.start_sent = false;
            s.device = None;
            s.transitioning = false;
            s.presence_revision += 1;
            s.airplay = false;
            if !keep_hotspot {
                s.hotspot = None;
            }
            self.state.1.notify_all();
            std::mem::take(&mut s.resources)
        };
        let mut resources = resources;
        resources.reverse();
        resources.sort_by_key(|(layer, _)| std::cmp::Reverse(*layer));
        let mut retained = Vec::new();
        for (layer, close) in resources {
            if keep_hotspot && layer == Layer::Hotspot {
                retained.push((layer, close));
            } else {
                self.close_step(close);
            }
        }
        let mut s = self.state.0.lock().unwrap();
        if s.closed {
            drop(s);
            for (_, close) in retained {
                self.close_step(close);
            }
        } else {
            s.resources.extend(retained);
        }
    }
    pub fn close(&self) -> Result<()> {
        {
            let mut s = self.state.0.lock().unwrap();
            if s.closed {
                return Ok(());
            }
            s.closed = true;
            s.running = false;
            s.armed = false;
            s.stage = "Stopped".into();
            s.generation += 1;
            self.state.1.notify_all();
        }
        self.release(false);
        let platform = self.platform.clone();
        self.close_step(Box::new(move || platform.cancel_hotspot_retry()));
        self.events
            .emit("stage", &serde_json::json!({"stage":"Stopped"}));
        Ok(())
    }
    pub fn run(&self) -> Result<()> {
        {
            let mut s = self.state.0.lock().unwrap();
            if s.closed || s.started {
                return Err(io::Error::other("Controller already running or closed"));
            }
            s.running = true;
            s.started = true;
        }
        // Presence belongs to the controller, rather than an attempt. It continues while idle
        // after a terminal failure, so removing the cable also clears the admin's product name.
        if let Err(e) = self.start_presence_watch() {
            self.close()?;
            return Err(e);
        }
        let mut budget = crate::start_policy::RetryBudget::default();
        let mut delay = self.timing.retry_initial;
        loop {
            let nodes = self.platform.enumerate().unwrap_or_default();
            let generation = {
                let mut s = self.state.0.lock().unwrap();
                if s.closed {
                    break;
                }
                s.generation += 1;
                s.restart_nodes = nodes;
                s.generation
            };
            let result = self.attempt(generation);
            if self.state.0.lock().unwrap().closed {
                break;
            }
            let unplug_node = self.state.0.lock().unwrap().device.clone();
            let failure = self.state.0.lock().unwrap().failure.clone();
            let error = match failure {
                Some(reason) => io::Error::other(reason),
                None => match result {
                    Err(e) => normalize_error(e),
                    Ok(()) => io::Error::other("Wired control ended"),
                },
            };
            let reason = crate::failure_text::error_text(&error);
            let code = classify_failure(&reason);
            self.state.0.lock().unwrap().failed = true;
            let stable = self
                .events
                .facts
                .lock()
                .unwrap()
                .stable_video(Instant::now());
            if stable {
                delay = self.timing.retry_initial;
            }
            // Unlocking can take arbitrarily long; only ordinary failures spend the budget.
            let retrying = code == "iphone_locked" || budget.failed(code, stable);
            let unplugged = code == "iphone_unplugged";
            self.events.emit(if retrying { "retry" } else { "startup-failed" },
                &serde_json::json!({"reason":reason,"retryMillis":if retrying && !unplugged {Some(delay.as_millis() as u64)} else {None}}));
            // Preserve the head-unit attachment unless it itself failed validation.
            let keep = !matches!(
                code,
                "hotspot_not_ready" | "hotspot_configuration" | "wifi_off"
            );
            let reset_network = matches!(
                code,
                "usb_trust_denied"
                    | "carlink_not_ready"
                    | "hotspot_configuration"
                    | "wifi_reset_required"
            );
            let automatic = self.state.0.lock().unwrap().automatic;
            let mut retained_hotspot = keep && (retrying || automatic && !reset_network);
            if !retrying {
                let mut s = self.state.0.lock().unwrap();
                if s.closed {
                    break;
                }
                // Arm before cleanup so a USB edge during shutdown is not lost.
                s.armed = true;
                s.restored = false;
            }
            self.release(retained_hotspot);
            if self.state.0.lock().unwrap().closed {
                break;
            }
            if !retrying {
                let mut s = self.state.0.lock().unwrap();
                if s.closed {
                    break;
                }
                s.failed = true;
                s.stage = "Failed".into();
                self.events
                    .emit("stage", &serde_json::json!({"stage":"Failed"}));
                while !s.closed && !s.restored {
                    if retained_hotspot && !s.automatic {
                        drop(s);
                        self.release(false);
                        retained_hotspot = false;
                        s = self.state.0.lock().unwrap();
                    } else {
                        s = self.state.1.wait(s).unwrap();
                    }
                }
                s.armed = false;
                if s.closed {
                    s.running = false;
                    drop(s);
                    self.release(false);
                    return Ok(());
                }
                s.failed = false;
                drop(s);
                budget = Default::default();
                delay = self.timing.retry_initial;
                self.events.facts.lock().unwrap().iphone.failure = None;
                continue;
            }
            if unplugged {
                // DeviceGone may precede sysfs removal. Give the old node time to disappear,
                // but retry after a bounded wait if it remains visible.
                if let Err(e) = self.wait_after_unplug(unplug_node.as_ref()) {
                    if self.state.0.lock().unwrap().closed {
                        break;
                    }
                    self.close()?;
                    return Err(e);
                }
            } else {
                let generation = self.state.0.lock().unwrap().generation;
                let _ = self.wait(generation, delay);
                delay = (delay * 2).min(self.timing.retry_max);
            }
        }
        Ok(())
    }
    fn start_presence_watch(&self) -> Result<()> {
        let (platform, state, events, interval) = (
            self.platform.clone(),
            self.state.clone(),
            self.events.clone(),
            self.timing.usb_poll,
        );
        crate::platform::spawn_worker(
            "wired-presence",
            Box::new(move || {
                let mut previous: Option<Vec<UsbDeviceInfo>> = None;
                loop {
                    let revision = {
                        let s = state.0.lock().unwrap();
                        if s.closed {
                            return;
                        }
                        s.presence_revision
                    };
                    if let Ok(devices) = platform.enumerate() {
                        {
                            let mut s = state.0.lock().unwrap();
                            if s.closed {
                                return;
                            }
                            if s.presence_revision != revision {
                                // This snapshot predates the current device or mode state.
                                continue;
                            }
                            if !s.armed
                                && !s.transitioning
                                && s.failure.is_none()
                                && s.device.as_ref().is_some_and(|old| {
                                    !devices.iter().any(|d| !different_node(d, old))
                                })
                            {
                                s.failure = Some("iPhone unplugged".into());
                                state.1.notify_all();
                            }
                            if s.armed {
                                // Include every node used by the failed attempt, so its own mode
                                // re-enumeration cannot look like a subsequent cable insertion.
                                if devices.iter().any(|d| {
                                    !s.restart_nodes.iter().any(|old| !different_node(d, old))
                                }) {
                                    s.restored = true;
                                    state.1.notify_all();
                                }
                                s.restart_nodes
                                    .retain(|old| devices.iter().any(|d| !different_node(d, old)));
                            }
                        }
                        if previous.as_ref() != Some(&devices) {
                            let s = state.0.lock().unwrap();
                            if s.closed {
                                return;
                            }
                            emit_presence(&events, devices.first());
                            previous = Some(devices);
                        }
                    }
                    let s = state.0.lock().unwrap();
                    let (s, _) = state
                        .1
                        .wait_timeout_while(s, interval, |s| !s.closed)
                        .unwrap();
                    if s.closed {
                        return;
                    }
                }
            }),
        )?;
        Ok(())
    }
    fn wait_after_unplug(&self, previous: Option<&UsbDeviceInfo>) -> Result<()> {
        let generation = self.state.0.lock().unwrap().generation;
        let deadline = Instant::now() + self.timing.unplug_settle;
        self.stage_event(
            generation,
            WAITING_FOR_IPHONE_USB,
            serde_json::json!({"reason":"iPhone unplugged"}),
        )?;
        loop {
            if self.state.0.lock().unwrap().closed {
                return Ok(());
            }
            let devices = self.platform.enumerate()?;
            emit_presence(&self.events, devices.first());
            if previous.is_none_or(|old| !devices.iter().any(|d| !different_node(d, old))) {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero()
                || self
                    .wait(generation, self.timing.usb_poll.min(remaining))
                    .is_err()
            {
                return Ok(());
            }
        }
    }
    fn attempt(&self, generation: u64) -> Result<()> {
        self.events
            .milestone(crate::boot_timing::Milestone::AttemptStart);
        self.stage_event(generation, DISCOVERING_MFI, serde_json::json!({}))?;
        let mfi = self.platform.prepare_mfi()?.authenticator;
        self.stage_event(generation, MFI_READY, serde_json::json!({}))?;
        self.head_unit_network(generation)?;
        self.stage_event(generation, WAITING_FOR_IPHONE_USB, serde_json::json!({}))?;
        let mut info = loop {
            self.check(generation)?;
            if let Some(info) = self.platform.enumerate()?.into_iter().next() {
                break info;
            }
            self.wait(generation, self.timing.usb_poll)?;
        };
        emit_presence(&self.events, Some(&info));
        let mut tried = 0;
        let (device, descriptors, configuration) = loop {
            self.check(generation)?;
            {
                let mut s = self.state.0.lock().unwrap();
                check_state(&s, generation)?;
                s.device = Some(info.clone());
                if !s.restart_nodes.iter().any(|d| !different_node(d, &info)) {
                    s.restart_nodes.push(info.clone());
                }
                s.transitioning = false;
                s.presence_revision += 1;
            }
            let current = self.callbacks(generation, node(&info)).current;
            let device = self.platform.open_device(&info, current)?;
            let d = device.clone();
            self.own(generation, Layer::Device, Box::new(move || d.close()))?;
            let descriptors = device.discover()?;
            let config = IphoneCarPlayConfiguration::find(&descriptors.configurations).cloned();
            let mode = config
                .as_ref()
                .is_some_and(IphoneCarPlayConfiguration::is_car_play_mode);
            let started = self.memory.started_on(&node(&info));
            let action =
                WiredConfigurationAction::choose(config.is_some(), mode, tried, 2, started);
            self.events.log(&format!(
                "wired-configuration action={} requests={tried}",
                action.diagnostic()
            ));
            match action {
                WiredConfigurationAction::Use
                | WiredConfigurationAction::UseWithoutAppleEthernet => {
                    break (device, descriptors, config.unwrap());
                }
                WiredConfigurationAction::Reject => {
                    return Err(io::Error::other(
                        "No usable iPhone CarPlay USB configuration after two mode requests",
                    ))
                }
                WiredConfigurationAction::RequestTransition => {
                    self.stage_event(generation, REQUESTING_CARPLAY_MODE, serde_json::json!({}))?;
                    tried += 1;
                    {
                        let mut s = self.state.0.lock().unwrap();
                        s.transitioning = true;
                        s.presence_revision += 1;
                    }
                    let outcome = device.request_mode(&descriptors)?;
                    match outcome {
                        CarPlayModeOutcome::Rejected { diagnostic, .. } => {
                            {
                                let mut s = self.state.0.lock().unwrap();
                                s.transitioning = false;
                                s.presence_revision += 1;
                            }
                            if WiredConfigurationAction::fallback_after_rejection(
                                config.is_some(),
                                mode,
                                started,
                            ) {
                                break (device, descriptors, config.unwrap());
                            }
                            return Err(io::Error::other(format!("Unlock the iPhone. It rejected the CarPlay USB mode request ({diagnostic})")));
                        }
                        CarPlayModeOutcome::ReenumerationRequested => {
                            // Release the old USB node before opening its replacement.
                            device.close()?;
                            self.stage_event(
                                generation,
                                WAITING_FOR_REENUMERATION,
                                serde_json::json!({}),
                            )?;
                            let deadline = Instant::now() + self.timing.reenumeration_timeout;
                            info = loop {
                                self.check(generation)?;
                                let devices = self.platform.enumerate()?;
                                emit_presence(&self.events, devices.first());
                                if let Some(next) =
                                    devices.into_iter().find(|d| different_node(d, &info))
                                {
                                    break next;
                                }
                                if Instant::now() >= deadline {
                                    return Err(io::Error::new(io::ErrorKind::TimedOut, "iPhone USB re-enumeration timed out after CarPlay mode request"));
                                }
                                self.wait(
                                    generation,
                                    self.timing
                                        .reenumeration_poll
                                        .min(deadline.saturating_duration_since(Instant::now())),
                                )?;
                            };
                        }
                    }
                }
            }
        };
        self.stage_event(
            generation,
            SELECTING_USB_CONFIGURATION,
            serde_json::json!({}),
        )?;
        device.select_configuration(&descriptors, &configuration)?;
        self.stage_event(generation, OPENING_USBMUX, serde_json::json!({}))?;
        let mux = device.open_mux()?;
        let resource = mux.clone();
        self.own(generation, Layer::Mux, Box::new(move || resource.close()))?;
        let ncm = device.open_ncm()?;
        let resource = ncm.clone();
        self.own(generation, Layer::Ncm, Box::new(move || resource.close()))?;
        let path = self.state_dir.join("lockdown/pair-record.plist");
        let mut record = match load_pair_record(&path)? {
            Some(record) => record,
            None => self.pair(generation, mux.as_ref(), &path)?,
        };
        self.stage_event(generation, CONNECTING_CONTROL, serde_json::json!({}))?;
        let stream = match mux.open_carkit(&record) {
            Ok(stream) => stream,
            Err(e) if e.invalidates_record() => {
                remove_pair_record(&path)?;
                record = self.pair(generation, mux.as_ref(), &path)?;
                self.stage_event(generation, CONNECTING_CONTROL, serde_json::json!({}))?;
                mux.open_carkit(&record).map_err(pairing_error)?
            }
            Err(e) => return Err(pairing_error(e)),
        };
        let resource = stream.clone();
        self.own(
            generation,
            Layer::Carkit,
            Box::new(move || resource.close()),
        )?;
        let callbacks = self.callbacks(generation, node(&info));
        let control = self
            .platform
            .open_control(stream, mfi.clone(), callbacks.clone())?;
        let resource = control.clone();
        self.own(
            generation,
            Layer::Control,
            Box::new(move || resource.close()),
        )?;
        let mac = ncm.host_mac();
        let tap = self.platform.create_tap(mac)?;
        let resource = tap.clone();
        self.own(generation, Layer::Tap, Box::new(move || resource.close()))?;
        let pump = self
            .platform
            .start_pump(tap.clone(), ncm, callbacks.on_failure.clone())?;
        self.own(generation, Layer::Pump, Box::new(move || pump.close()))?;
        let receiver = self
            .platform
            .create_receiver(tap.index(), mfi, callbacks.clone())?;
        let resource = receiver.clone();
        self.own(
            generation,
            Layer::Receiver,
            Box::new(move || resource.close()),
        )?;
        let port = receiver.start()?;
        {
            let mut s = self.state.0.lock().unwrap();
            check_state(&s, generation)?;
            s.receiver = Some(receiver.clone());
        }
        self.stage_event(
            generation,
            RUNNING_CONTROL,
            serde_json::json!({"port":port}),
        )?;
        let state = self.state.clone();
        crate::platform::spawn_worker(
            "wired-control",
            Box::new(move || {
                let result = control.run(port, mac, callbacks.clone());
                let error = result
                    .err()
                    .unwrap_or_else(|| io::Error::other("Wired iAP2 control channel ended"));
                fail(&state, generation, normalize_error(error).to_string());
            }),
        )?;
        let mut next_presence = Instant::now();
        loop {
            self.wait(generation, self.timing.tick)?;
            receiver.poll_startup(Instant::now())?;
            if Instant::now() >= next_presence {
                let devices = self.platform.enumerate()?;
                if !devices.iter().any(|d| !different_node(d, &info)) {
                    return Err(io::Error::from(UsbError::DeviceGone));
                }
                next_presence = Instant::now() + self.timing.usb_poll;
                let hotspot = self.state.0.lock().unwrap().hotspot.clone();
                if hotspot.is_some() {
                    // Validation is performed by the retained manager, without a station watch.
                    self.validate_head_unit()?;
                }
            }
        }
    }
    fn head_unit_network(&self, generation: u64) -> Result<()> {
        if self.state.0.lock().unwrap().hotspot.is_some() {
            self.validate_head_unit()?;
            while !self.platform.car_ready()? {
                self.wait(generation, self.timing.tick)?;
            }
            return Ok(());
        }
        self.stage_event(generation, "StartingHotspot", serde_json::json!({}))?;
        let hotspot = self.platform.create_hotspot()?;
        *self.head_unit_manager.lock().unwrap() = Some(hotspot.clone());
        let h = hotspot.clone();
        self.own(generation, Layer::Hotspot, Box::new(move || h.close()))?;
        self.platform.prepare_hotspot();
        self.wait(generation, self.platform.hotspot_down_remaining())?;
        let info = hotspot.start(Duration::from_secs(60))?;
        hotspot.validate_ready()?;
        {
            let mut s = self.state.0.lock().unwrap();
            check_state(&s, generation)?;
            s.hotspot = Some(info.clone());
        }
        let reused = self.platform.hotspot_reused();
        self.events.hotspot_milestone(reused);
        self.stage_event(generation, "HotspotReady", serde_json::json!({"reused":reused,"ssid":info.ssid,"channel":info.channel,"bssid":info.bssid,"address":info.host_address.map(|a|a.to_string())}))?;
        while !self.platform.car_ready()? {
            self.wait(generation, self.timing.tick)?;
        }
        Ok(())
    }
    fn validate_head_unit(&self) -> Result<()> {
        let manager = self.head_unit_manager.lock().unwrap().clone();
        if let Some(manager) = manager {
            manager.validate_ready()?;
        }
        Ok(())
    }
    fn pair(&self, generation: u64, mux: &dyn WiredMux, path: &Path) -> Result<LockdownPairRecord> {
        self.stage_event(generation, PAIRING, serde_json::json!({}))?;
        let host_id = fresh_uuid()?;
        let system_buid = fresh_uuid()?;
        let record = mux
            .pair(&host_id, &system_buid, Duration::from_secs(300), &|| {
                self.check(generation).is_err()
            })
            .map_err(pairing_error)?;
        self.check(generation)?;
        crate::platform::private_state_file(path, &record.serialize()?)?;
        Ok(record)
    }
    fn callbacks(&self, generation: u64, node: String) -> WiredCallbacks {
        let state = self.state.clone();
        let current = Arc::new(move || check_state(&state.0.lock().unwrap(), generation).is_ok());
        let (state, events, memory) =
            (self.state.clone(), self.events.clone(), self.memory.clone());
        let start = Arc::new(move || {
            let receiver = {
                let mut s = state.0.lock().unwrap();
                if check_state(&s, generation).is_err() || s.start_sent {
                    return;
                }
                s.start_sent = true;
                s.stage = START_SESSION_SENT.into();
                memory.started(node.clone());
                events.emit("stage", &serde_json::json!({"stage":START_SESSION_SENT}));
                s.receiver.clone()
            };
            if let Some(receiver) = receiver {
                receiver.start_session_sent(Instant::now());
            }
        });
        let (state, events) = (self.state.clone(), self.events.clone());
        let auth = Arc::new(move || {
            let s = state.0.lock().unwrap();
            if check_state(&s, generation).is_ok() {
                events.milestone(crate::boot_timing::Milestone::Authenticated);
            }
        });
        let (state, events) = (self.state.clone(), self.events.clone());
        let airplay = Arc::new(move || {
            let mut s = state.0.lock().unwrap();
            if check_state(&s, generation).is_err() || s.airplay {
                return;
            }
            s.airplay = true;
            s.failed = false;
            s.stage = "Session".into();
            events.facts.lock().unwrap().iphone.failure = None;
            events.emit("stage", &serde_json::json!({"stage":"Session"}));
            state.1.notify_all();
        });
        let state = self.state.clone();
        WiredCallbacks {
            on_start_session: start,
            on_authenticated: auth,
            on_air_play: airplay,
            on_failure: Arc::new(move |e| fail(&state, generation, normalize_error(e).to_string())),
            current,
        }
    }
}

fn check_state(s: &State, generation: u64) -> Result<()> {
    if s.closed || s.generation != generation {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Wired attempt cancelled",
        ))
    } else if let Some(e) = &s.failure {
        Err(io::Error::other(e.clone()))
    } else {
        Ok(())
    }
}
fn fail(state: &Shared, generation: u64, reason: String) {
    let mut s = state.0.lock().unwrap();
    if check_state(&s, generation).is_ok() {
        s.failure = Some(reason);
        state.1.notify_all();
    }
}
fn node(info: &UsbDeviceInfo) -> String {
    format!(
        "{}:{}:{}",
        info.bus_number,
        info.device_number,
        info.sysfs_path.display()
    )
}
fn different_node(a: &UsbDeviceInfo, b: &UsbDeviceInfo) -> bool {
    a.bus_number != b.bus_number
        || a.device_number != b.device_number
        || a.sysfs_path != b.sysfs_path
}
fn emit_presence(events: &JsonEvents, info: Option<&UsbDeviceInfo>) {
    let facts = events.runtime_path_facts();
    let product = info.and_then(|d| d.product.clone());
    if facts.usb.present != info.is_some() || facts.usb.product != product {
        events.emit(
            "usb-presence",
            &serde_json::json!({"present":info.is_some(),"product":product}),
        );
    }
}
pub(crate) fn normalize_error(error: io::Error) -> io::Error {
    let mut gone = false;
    let mut busy =
        error.kind() == io::ErrorKind::ResourceBusy || error.to_string().contains("usbmuxd?");
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = cause {
        if let Some(usb) = current.downcast_ref::<UsbError>() {
            gone |= matches!(usb, UsbError::DeviceGone);
            busy |= matches!(usb, UsbError::InterfaceBusy(_));
        }
        if let Some(io) = current.downcast_ref::<io::Error>() {
            gone |= matches!(io.raw_os_error(), Some(19 | 108));
            // io::Error::source skips its immediate boxed error on some Rust versions.
            if let Some(usb) = io.get_ref().and_then(|e| e.downcast_ref::<UsbError>()) {
                gone |= matches!(usb, UsbError::DeviceGone);
                busy |= matches!(usb, UsbError::InterfaceBusy(_));
            }
        }
        cause = current.source();
    }
    if gone {
        io::Error::from(UsbError::DeviceGone)
    } else if busy {
        io::Error::other("USB interface busy: another program (usbmuxd?) holds the iPhone")
    } else {
        error
    }
}

fn pairing_error(error: LockdownPairingError) -> io::Error {
    io::Error::other(error)
}
fn load_pair_record(path: &Path) -> Result<Option<LockdownPairRecord>> {
    match crate::logs::read_private(path, 1024 * 1024)? {
        Some(bytes) => match LockdownPairRecord::parse(&bytes) {
            Ok(record) => Ok(Some(record)),
            Err(_) => {
                remove_pair_record(path)?;
                Ok(None)
            }
        },
        None => Ok(None),
    }
}
fn remove_pair_record(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
fn fresh_uuid() -> Result<String> {
    use rand_core::{OsRng, RngCore};
    let mut bytes = [0; 16];
    OsRng.try_fill_bytes(&mut bytes).map_err(io::Error::other)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

impl Drop for LinuxWiredController {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
