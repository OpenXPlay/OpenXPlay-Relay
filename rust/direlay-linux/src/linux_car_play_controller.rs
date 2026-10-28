// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::bluetooth_wake::{BluetoothChange, BluetoothListener, BluetoothSubscription};
use crate::boot_timing::Milestone;
use crate::Result;
use crate::*;
use carplay_iap2::{mfi::MfiAuthenticator, DuplexStream, Iap2WirelessCarPlayEndpoint};
use std::{
    collections::BTreeSet,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
pub trait CloseResource: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn close(&self) -> Result<()>;
}
/// `MfiResource` protocol data.
pub struct MfiResource {
    pub authenticator: Arc<dyn MfiAuthenticator>,
}
/// Platform interface for `NetworkAttachment`. Implementations must support bounded shutdown.
pub trait NetworkAttachment: Send + Sync {
    fn uses_startup_watchdog(&self) -> bool {
        false
    }
    fn start_session_sent(&self, _now: Instant) {}
    fn poll_startup(&self, _now: Instant) -> Result<()> {
        Ok(())
    }
    fn validate_ready(&self) -> Result<()> {
        Ok(())
    }
    /// Errors propagate to the lifecycle owner.
    fn start(&self) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn endpoint(&self) -> Result<Iap2WirelessCarPlayEndpoint>;
    /// Errors propagate to the lifecycle owner.
    fn close(&self) -> Result<()>;
}
/// The attempt's network as the Bluetooth bootstrap sees it. The access point and the AirPlay
/// receiver on it may still be starting while the iPhone identifies and authenticates over
/// Bluetooth; everything that carries the access point's credentials or address waits for
/// [`PendingNetwork::wait`].
pub trait PendingNetwork: Send + Sync {
    /// The SSID the access point has or will have, for the identification. Waits up to
    /// `timeout` for the access point when the platform cannot tell the SSID in advance.
    fn ssid(&self, timeout: Duration) -> Result<String>;
    /// Waits up to `timeout` for the started network: `Ok(None)` while it is still starting, an
    /// error once it failed or the attempt ended.
    fn wait(&self, timeout: Duration) -> Result<Option<Arc<dyn NetworkAttachment>>>;
}
/// What the bootstrap reports to the controller.
#[derive(Clone)]
pub struct BootstrapCallbacks {
    /// The iPhone accepted the identification and the MFi authentication.
    pub on_authenticated: Arc<dyn Fn() + Send + Sync>,
    /// A Wi-Fi configuration with the access point's credentials went to the iPhone.
    pub on_wifi_configuration_sent: Arc<dyn Fn() + Send + Sync>,
    /// StartSession went out.
    pub on_start_session: Arc<dyn Fn() + Send + Sync>,
}
/// Platform interface for `BluetoothConnection`. Implementations must support bounded shutdown.
pub trait BluetoothConnection: DuplexStream {
    /// Errors propagate to the lifecycle owner.
    fn connect(&self) -> Result<()>;
}
/// Platform interface for `WirelessPlatform`. Implementations must support bounded shutdown.
pub trait WirelessPlatform: Send + Sync {
    fn car_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn network_watch_enabled(&self) -> bool {
        false
    }
    /// Errors propagate to the lifecycle owner.
    fn check_bluetooth_radio(&self) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn prepare_mfi(&self) -> Result<MfiResource>;
    /// Errors propagate to the lifecycle owner.
    fn create_hotspot(&self) -> Result<Arc<dyn WirelessHotspotManager>>;
    /// What the current attempt's access point needs before it starts but Bluetooth need not
    /// wait for, such as removing the access points a killed run left behind. It runs with the
    /// access point's start, which then waits out a down gap it began.
    fn prepare_hotspot(&self) {}
    /// The SSID the current attempt's access point will have, when the platform knows it before
    /// the access point is up; without it the identification waits for the access point.
    fn planned_hotspot_ssid(&self) -> Option<String> {
        None
    }
    /// Whether the current attempt reused an already running access point.
    fn hotspot_reused(&self) -> bool {
        false
    }
    /// Keeps the current attempt's access point running across an in-controller retry, unless
    /// its start or validation failed.
    fn keep_hotspot_for_retry(&self) {}
    /// Withdraws that retention when the controller stops for good, closing the access point
    /// unless the relay kept it for a session restart.
    fn cancel_hotspot_retry(&self) -> Result<()> {
        Ok(())
    }
    /// MAC addresses of the stations associated with the attempt's access point, or `None` when
    /// they cannot be listed (the access point is not the relay's own, or the query failed).
    fn hotspot_stations(&self) -> Option<Vec<String>> {
        None
    }
    /// How long the attempt must still wait before it brings a new access point up, so the
    /// previous one stays down for the minimum gap; zero while an access point is up.
    fn hotspot_down_remaining(&self) -> Duration {
        Duration::ZERO
    }
    /// Subscribes `wake` to BlueZ changes that can end a Bluetooth retry wait early: an adapter
    /// that became usable or a device that connected. `None` without a change source; the waits
    /// then only poll.
    fn watch_bluetooth(&self, wake: BluetoothListener) -> Option<BluetoothSubscription> {
        None
    }
    /// Errors propagate to the lifecycle owner.
    fn start_pairing(&self) -> Result<Option<Arc<dyn CloseResource>>>;
    /// Errors propagate to the lifecycle owner.
    fn select_iphone(&self) -> Result<BondedDevice>;
    /// Errors propagate to the lifecycle owner.
    fn create_bluetooth(&self, device: &BondedDevice) -> Result<Arc<dyn BluetoothConnection>>;
    /// Errors propagate to the lifecycle owner.
    fn create_network(
        &self,
        info: WirelessHotspotInfo,
        mfi: Arc<dyn MfiAuthenticator>,
        callbacks: NetworkCallbacks,
    ) -> Result<Arc<dyn NetworkAttachment>>;
    /// Runs the iAP2 bootstrap over `connection`. It may begin before `network` is up: only what
    /// carries the access point's credentials or address waits for it.
    fn run_bootstrap(
        &self,
        connection: Arc<dyn BluetoothConnection>,
        network: Arc<dyn PendingNetwork>,
        mfi: Arc<dyn MfiAuthenticator>,
        callbacks: BootstrapCallbacks,
    ) -> Result<()>;
}
/// `NetworkCallbacks` protocol data.
#[derive(Clone)]
pub struct NetworkCallbacks {
    pub on_air_play: Arc<dyn Fn() + Send + Sync>,

    pub on_tunnel_ready: Arc<dyn Fn() + Send + Sync>,

    pub on_failure: Arc<dyn Fn(std::io::Error) + Send + Sync>,
}

type Cleanup = Box<dyn Fn() -> Result<()> + Send + Sync>;
/// The order in which an attempt releases its resources: the Bluetooth link first, the access
/// point last, whichever of the attempt's threads acquired them first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Hotspot,
    Network,
    Bluetooth,
}
/// First delay of the waits for the Bluetooth adapter; it doubles per failure.
pub const BLUETOOTH_RETRY_INITIAL: Duration = Duration::from_secs(2);
/// Cap of adapter waits when no BlueZ change ends the wait early.
pub const BLUETOOTH_RETRY_MAX: Duration = Duration::from_secs(10);
/// An absent iPhone never exhausts startup retries, even when no BlueZ edge announces its return.
pub const IPHONE_RETRY_INITIAL: Duration = Duration::from_secs(10);
pub const IPHONE_RETRY_MAX: Duration = Duration::from_secs(30);
/// What a Bluetooth retry wait waits for; only a change that can help ends it early.
enum Awaited {
    Adapter,
    /// The iPhone with this address, or any device while none was selected yet.
    Iphone(Option<String>),
}
impl Awaited {
    fn name(&self) -> &'static str {
        match self {
            Self::Adapter => "adapter",
            Self::Iphone(_) => "iphone",
        }
    }
    fn woken_by(&self, change: &BluetoothChange) -> bool {
        match (self, change) {
            // An adapter that comes back also helps an iPhone wait, e.g. after a guard recovery.
            (_, BluetoothChange::AdapterReady) => true,
            (Self::Iphone(selected), BluetoothChange::DeviceConnected(address)) => selected
                .as_deref()
                .is_none_or(|s| s.eq_ignore_ascii_case(address)),
            (Self::Adapter, BluetoothChange::DeviceConnected(_)) => false,
        }
    }
}
/// How often the AP's stations are read while the iPhone is expected to join.
const STATION_POLL: Duration = Duration::from_secs(1);
/// Tells whether the iPhone reached the AP during an attempt. The iPhone joins once the bootstrap
/// has handed it the AP's credentials, so a station counts when it was not yet associated when
/// the bootstrap was about to hand them out. A station that was, such as the head unit, does not
/// count, except on an AP inherited from the previous controller (a codec restart), whose
/// stations count as they are.
#[derive(Default)]
struct StationWatch {
    /// Stations that joined the current AP while a bootstrap ran.
    joined: BTreeSet<String>,
    /// Stations associated when the attempt's bootstrap was about to hand out the AP's
    /// credentials; `None` when unknown.
    before: Option<BTreeSet<String>>,
    had_ap: bool,
    /// The controller's first AP was reused and its stations are not yet counted.
    inherited: bool,
    /// Since then: a station list was read, a station was associated, and one of the associated
    /// stations was the iPhone's.
    sampled: bool,
    any: bool,
    iphone: bool,
}
impl StationWatch {
    fn ap_ready(&mut self, reused: bool) {
        if !reused {
            self.joined.clear();
        }
        self.inherited = reused && (self.inherited || !self.had_ap);
        self.had_ap = true;
    }
    fn begin(&mut self, stations: Option<Vec<String>>) {
        let before = stations.map(|s| s.into_iter().collect::<BTreeSet<_>>());
        if std::mem::take(&mut self.inherited) {
            self.joined.extend(before.iter().flatten().cloned());
        }
        self.before = before;
        self.sampled = false;
        self.any = false;
        self.iphone = false;
    }
    fn observe(&mut self, stations: Option<Vec<String>>) {
        let Some(stations) = stations else {
            return;
        };
        self.sampled = true;
        for station in stations {
            self.any = true;
            let new = self
                .before
                .as_ref()
                .is_none_or(|before| !before.contains(&station));
            if new || self.joined.contains(&station) {
                self.iphone = true;
                self.joined.insert(station);
            }
        }
    }
    /// Why the AP should be recreated, when its stations could be read and none was the iPhone's.
    fn missing_iphone(&self) -> Option<&'static str> {
        if !self.sampled || self.iphone {
            None
        } else if self.any {
            Some("no-iphone-station")
        } else {
            Some("no-station")
        }
    }
}
struct ControllerState {
    hotspot: Option<WirelessHotspotInfo>,
    /// The attempt's started network: its AP and the AirPlay receiver on it are up.
    network: Option<Arc<dyn NetworkAttachment>>,
    /// The attempt found the Bluetooth adapter usable; the network needs its address.
    adapter_ready: bool,
    /// The last AP failed to start; Bluetooth then waits for the next one.
    hotspot_failed: bool,
    closed: bool,
    running: bool,
    generation: u64,
    stage: String,
    failure: Option<String>,
    /// StartSession went out in this attempt.
    start_session: bool,
    stations: StationWatch,
    airplay: bool,
    tunnel: bool,
    active: bool,
    bootstrap: Option<Arc<dyn BluetoothConnection>>,
    resources: Vec<(Layer, Cleanup)>,
    pairing: Option<Arc<dyn CloseResource>>,
    /// The armed Bluetooth wait; a matching BlueZ change sets `woken` to its reason.
    awaited: Option<Awaited>,
    woken: Option<String>,
    terminal_failure: bool,
    automatic: bool,
    armed: bool,
    selected_iphone: Option<String>,
}
type SharedState = Arc<(Mutex<ControllerState>, Condvar)>;
pub struct LinuxCarPlayController {
    platform: Arc<dyn WirelessPlatform>,
    events: Arc<JsonEvents>,
    timeout: Duration,
    retry_delay: (Duration, Duration),
    bluetooth_retry: (Duration, Duration),
    iphone_retry: (Duration, Duration),
    state: SharedState,
    retry_budget: Mutex<crate::start_policy::RetryBudget>,
}
impl LinuxCarPlayController {
    pub fn new(
        platform: Arc<dyn WirelessPlatform>,
        events: Arc<JsonEvents>,
        handoff_timeout: Duration,
    ) -> Self {
        Self {
            platform,
            events,
            timeout: handoff_timeout,
            retry_delay: (Duration::from_secs(2), Duration::from_secs(30)),
            bluetooth_retry: (BLUETOOTH_RETRY_INITIAL, BLUETOOTH_RETRY_MAX),
            iphone_retry: (IPHONE_RETRY_INITIAL, IPHONE_RETRY_MAX),
            state: Arc::new((
                Mutex::new(ControllerState {
                    hotspot: None,
                    network: None,
                    adapter_ready: false,
                    hotspot_failed: false,
                    closed: false,
                    running: false,
                    generation: 0,
                    stage: "Idle".into(),
                    failure: None,
                    start_session: false,
                    stations: StationWatch::default(),
                    airplay: false,
                    tunnel: false,
                    active: false,
                    bootstrap: None,
                    resources: Vec::new(),
                    pairing: None,
                    awaited: None,
                    woken: None,
                    terminal_failure: false,
                    automatic: false,
                    armed: false,
                    selected_iphone: None,
                }),
                Condvar::new(),
            )),
            retry_budget: Mutex::default(),
        }
    }
    /// The first delay between failed attempts and its cap; the delay doubles per failure and
    /// starts over only after 60 seconds of video.
    pub fn with_retry_delay(mut self, initial: Duration, maximum: Duration) -> Self {
        self.retry_delay = (initial, maximum);
        self
    }
    /// Overrides both Bluetooth wait cadences, e.g. for accelerated controller tests.
    pub fn with_bluetooth_retry(mut self, initial: Duration, maximum: Duration) -> Self {
        self.bluetooth_retry = (initial, maximum);
        self.iphone_retry = (initial, maximum);
        self
    }
    fn set_stage(&self, stage: &str, fields: serde_json::Value) {
        let mut s = self.state.0.lock().unwrap();
        if s.closed || s.failure.is_some() {
            return;
        }
        s.stage = stage.into();
        let mut fields = fields;
        fields["stage"] = stage.into();
        self.events.emit("stage", &fields);
    }
    /// A stage of the attempt's network worker: logged like the others, while [`Self::stage`]
    /// keeps following the Bluetooth side, which ends in WirelessActive.
    fn network_stage(&self, generation: u64, stage: &str, fields: serde_json::Value) {
        let s = self.state.0.lock().unwrap();
        if s.closed || s.failure.is_some() || s.generation != generation {
            return;
        }
        let mut fields = fields;
        fields["stage"] = stage.into();
        self.events.emit("stage", &fields);
    }
    fn check(&self) -> Result<()> {
        let s = self.state.0.lock().unwrap();
        if s.closed {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Controller closed",
            ));
        }
        if let Some(e) = &s.failure {
            return Err(std::io::Error::other(e.clone()));
        }
        Ok(())
    }
    fn own(&self, layer: Layer, close: Cleanup) -> Result<()> {
        let mut s = self.state.0.lock().unwrap();
        if s.closed || s.failure.is_some() {
            drop(s);
            let _ = close();
            return self.check();
        }
        s.resources.push((layer, close));
        Ok(())
    }
    fn wait(&self, t: Duration) -> Result<()> {
        let (s, c) = &*self.state;
        let g = s.lock().unwrap();
        let _ = c
            .wait_timeout_while(g, t, |s| !s.closed && s.failure.is_none())
            .unwrap();
        self.check()
    }
    /// From now on a BlueZ change that helps `awaited` ends the next `wait_armed` early.
    fn arm(&self, awaited: Awaited) {
        let mut s = self.state.0.lock().unwrap();
        s.awaited = Some(awaited);
        s.woken = None;
    }
    fn disarm(&self) {
        let mut s = self.state.0.lock().unwrap();
        s.awaited = None;
        s.woken = None;
    }
    /// Waits up to `delay` for the armed change, logs what ended the wait and returns the next
    /// delay: the backoff doubles up to its cap and starts over after a change, which means the
    /// adapter or the iPhone is around again.
    fn wait_armed(&self, delay: Duration, retry: (Duration, Duration)) -> Result<Duration> {
        let started = Instant::now();
        let (m, c) = &*self.state;
        let s = m.lock().unwrap();
        let (mut s, _) = c
            .wait_timeout_while(s, delay, |s| {
                !s.closed && s.failure.is_none() && s.woken.is_none()
            })
            .unwrap();
        let awaited = s.awaited.take();
        let woken = s.woken.take();
        drop(s);
        self.check()?;
        self.events.log(&format!(
            "bluetooth-wake waiting={} reason={} afterMs={}",
            awaited.as_ref().map_or("none", Awaited::name),
            woken.as_deref().unwrap_or("timer"),
            started.elapsed().as_millis()
        ));
        let (initial, maximum) = retry;
        Ok(if woken.is_some() {
            initial
        } else {
            (delay * 2).min(maximum)
        })
    }
    /// Waits until the Bluetooth adapter is usable. The AP comes up meanwhile, so the head unit
    /// can join it; BlueZ reporting the adapter ready ends each wait at once.
    fn wait_for_adapter(&self) -> Result<()> {
        let mut delay = self.bluetooth_retry.0;
        loop {
            // Armed before the check, so an adapter that comes up during it is not missed.
            self.arm(Awaited::Adapter);
            let result = self.platform.check_bluetooth_radio();
            self.check()?;
            match result {
                Ok(()) => {
                    self.disarm();
                    return Ok(());
                }
                Err(e) => {
                    if !self.retry_failure(&e, delay) {
                        return Err(e);
                    }
                    self.set_stage(
                        "WaitingForBluetooth",
                        serde_json::json!({"reason":crate::failure_text::error_text(&e),"retryMillis":delay.as_millis() as u64}),
                    );
                    delay = self.wait_armed(delay, self.bluetooth_retry)?;
                }
            }
        }
    }
    /// Opens iAP2 to the bonded iPhone. Unreachable attempts wait indefinitely at 10–30 seconds;
    /// a returning iPhone or adapter ends the wait early and resets its cadence, not its budget.
    fn connect_iphone(&self) -> Result<Arc<dyn BluetoothConnection>> {
        let mut delay = self.iphone_retry.0;
        let mut selected = None;
        loop {
            self.set_stage(
                "WaitingForPairedIphone",
                serde_json::json!({"reason":"Selecting bonded iPhone"}),
            );
            let previous = selected.clone();
            match self.try_connect_iphone(&mut selected) {
                Ok(c) => return Ok(c),
                Err(e) => {
                    self.check()?;
                    if selected != previous {
                        delay = self.iphone_retry.0;
                    }
                    let unreachable = crate::carplay_policy::iphone_unreachable(&e);
                    // Armed only now: the failed attempt itself may have connected the iPhone.
                    if !unreachable && !self.retry_failure(&e, delay) {
                        return Err(e);
                    }
                    self.arm(Awaited::Iphone(selected.clone()));
                    self.set_stage(
                        "WaitingForPairedIphone",
                        serde_json::json!({"reason":crate::failure_text::error_text(&e),"retryMillis":delay.as_millis() as u64,"unreachable":unreachable}),
                    );
                    delay = self.wait_armed(delay, self.iphone_retry)?;
                }
            }
        }
    }
    /// One connection attempt; `selected` keeps the address of the last selected iPhone.
    fn try_connect_iphone(
        &self,
        selected: &mut Option<String>,
    ) -> Result<Arc<dyn BluetoothConnection>> {
        let device = self.platform.select_iphone().inspect_err(|e| {
            if crate::carplay_policy::iphone_unreachable(e) {
                *selected = None;
                self.state.0.lock().unwrap().selected_iphone = None;
            }
        })?;
        *selected = Some(device.address.clone());
        self.state.0.lock().unwrap().selected_iphone = selected.clone();
        self.check()?;
        self.set_stage(
            "ConnectingBluetooth",
            serde_json::json!({"address":device.address}),
        );
        self.events.milestone(Milestone::BluetoothConnectStart);
        let c = self.platform.create_bluetooth(&device)?;
        {
            let mut s = self.state.0.lock().unwrap();
            if s.closed {
                let _ = c.close();
                return Err(std::io::Error::other("Controller closed"));
            }
            s.bootstrap = Some(c.clone());
        }
        if let Err(e) = c.connect() {
            let _ = c.close();
            self.state.0.lock().unwrap().bootstrap = None;
            return Err(e);
        }
        self.check()?;
        Ok(c)
    }
    fn retry_failure(&self, error: &std::io::Error, delay: Duration) -> bool {
        let reason = crate::failure_text::error_text(error);
        let code = crate::carplay_policy::classify_failure(&reason);
        let stable = self
            .events
            .facts
            .lock()
            .unwrap()
            .stable_video(Instant::now());
        let retrying = self.retry_budget.lock().unwrap().failed(code, stable);
        self.state.0.lock().unwrap().terminal_failure = !retrying;
        self.events.emit(if retrying { "retry" } else { "startup-failed" },
            &serde_json::json!({"reason":reason,"retryMillis":if retrying {Some(delay.as_millis() as u64)} else {None}}));
        retrying
    }
    pub fn set_start_when_ready(&self, automatic: bool) {
        self.state.0.lock().unwrap().automatic = automatic;
        self.state.1.notify_all();
    }
    pub fn armed(&self) -> bool {
        self.state.0.lock().unwrap().armed
    }
    /// Polling can observe the same restoration as BlueZ. Both use this attempt's wake latch.
    pub fn readiness_restored(&self) {
        let mut state = self.state.0.lock().unwrap();
        if state.armed && state.woken.is_none() {
            state.woken = Some("readiness-restored".into());
            self.state.1.notify_all();
        }
    }
    /// A headless relay gets a fresh bounded budget only after a relevant BlueZ edge. Timers
    /// cannot rescue an exhausted budget; Stop closes this wait and still wins over a late edge.
    fn await_recovery(&self, error: &std::io::Error) -> bool {
        let code = crate::carplay_policy::classify_failure(&error.to_string());
        if matches!(
            code,
            "carlink_not_ready"
                | "hotspot_configuration"
                | "hotspot_not_ready"
                | "wifi_off"
                | "wifi_reset_required"
                | "usb_trust_denied"
                | "iphone_locked"
        ) {
            return false;
        }
        {
            let mut state = self.state.0.lock().unwrap();
            if !state.automatic || state.closed {
                return false;
            }
            state.armed = true;
            state.stage = "Failed".into();
            state.awaited = Some(if matches!(code, "bluetooth_off" | "bluetooth_unusable") {
                Awaited::Adapter
            } else {
                Awaited::Iphone(state.selected_iphone.clone())
            });
            state.woken = None;
        }
        self.release_attempt(true);
        self.events
            .emit("stage", &serde_json::json!({"stage":"Failed"}));
        let (mutex, changed) = &*self.state;
        let mut state = changed
            .wait_while(mutex.lock().unwrap(), |s| {
                !s.closed && s.automatic && s.woken.is_none()
            })
            .unwrap();
        state.armed = false;
        state.awaited = None;
        if state.closed || !state.automatic {
            return false;
        }
        let reason = state.woken.take().unwrap();
        state.terminal_failure = false;
        drop(state);
        *self.retry_budget.lock().unwrap() = Default::default();
        self.events.facts.lock().unwrap().iphone.failure = None;
        self.events
            .log(&format!("carplay_radio_recover reason={reason}"));
        true
    }
    pub fn running(&self) -> bool {
        self.state.0.lock().unwrap().running
    }
    pub fn run(&self) -> Result<()> {
        let mut s = self.state.0.lock().unwrap();
        if s.running || s.closed {
            return Err(std::io::Error::other(
                "Controller already running or closed",
            ));
        }
        s.running = true;
        drop(s);
        let state = self.state.clone();
        let _watch = self.platform.watch_bluetooth(Arc::new(move |change| {
            let (m, c) = &*state;
            let mut s = m.lock().unwrap();
            if s.woken.is_none() && s.awaited.as_ref().is_some_and(|a| a.woken_by(change)) {
                s.woken = Some(change.reason());
                c.notify_all();
            }
        }));
        let (initial, maximum) = self.retry_delay;
        let mut restart = initial;
        let mut pairing_started = false;
        while !self.state.0.lock().unwrap().closed {
            let generation = {
                let mut s = self.state.0.lock().unwrap();
                s.generation += 1;
                s.generation
            };
            let result = std::thread::scope(|scope| {
                let result = self.attempt(scope, generation, &mut pairing_started);
                if let Err(e) = &result {
                    // Ends the attempt's other waits. The scope then waits for its network
                    // worker, so an AP that is still starting can be kept for the next attempt.
                    abort(&self.state, generation, crate::failure_text::error_text(e));
                }
                result
            });
            if let Err(e) = result {
                let (stable, no_tunnel, terminal) = {
                    let s = self.state.0.lock().unwrap();
                    if s.closed {
                        break;
                    }
                    // StartSession went out, but neither AirPlay nor the tunnel reached the relay.
                    let no_tunnel = s.start_session && !s.airplay && !s.tunnel && !s.active;
                    (
                        self.events
                            .facts
                            .lock()
                            .unwrap()
                            .stable_video(Instant::now()),
                        no_tunnel,
                        s.terminal_failure,
                    )
                };
                if stable {
                    restart = initial;
                }
                let recreate = if no_tunnel {
                    sample_stations(
                        self.platform.as_ref(),
                        &self.state,
                        &self.events,
                        generation,
                    );
                    self.state.0.lock().unwrap().stations.missing_iphone()
                } else {
                    None
                };
                if terminal || !self.retry_failure(&e, restart) {
                    if self.await_recovery(&e) {
                        restart = initial;
                        continue;
                    }
                    self.close()?;
                    let mut state = self.state.0.lock().unwrap();
                    state.stage = "Failed".into();
                    state.running = false;
                    drop(state);
                    // Resource shutdown must not replace the frozen failure's step and timing.
                    self.events
                        .emit("stage", &serde_json::json!({"stage":"Failed"}));
                    return Ok(());
                }
                // The head unit stays on the AP while the iPhone is retried, unless the iPhone
                // never joined it: then the AP goes down for the gap, so the Wi-Fi driver and the
                // iPhone drop their stale association before the next attempt brings it back.
                let recovery = e.to_string().contains("changed")
                    || e.to_string().contains("association was lost");
                self.release_attempt(recreate.is_none() && !recovery);
                if let Some(reason) = recreate {
                    self.events.log(&format!(
                        "hotspot-recreate reason={reason} gapMs={}",
                        self.platform.hotspot_down_remaining().as_millis()
                    ));
                }
                let _ = self.wait(restart);
                restart = (restart * 2).min(maximum);
            }
        }
        self.close()
    }
    /// One attempt. The AP and the AirPlay receiver on it come up on a worker thread of `scope`
    /// while Bluetooth connects to the iPhone, which identifies and authenticates meanwhile; only
    /// the answers that carry the AP's credentials or address wait for the AP.
    fn attempt<'scope, 'env>(
        &'env self,
        scope: &'scope std::thread::Scope<'scope, 'env>,
        generation: u64,
        pairing_started: &mut bool,
    ) -> Result<()> {
        self.events.milestone(Milestone::AttemptStart);
        self.set_stage("DiscoveringMfi", serde_json::json!({}));
        let mfi = self.platform.prepare_mfi()?;
        self.check()?;
        self.set_stage("MfiReady", serde_json::json!({}));
        // The AP does not wait for Bluetooth: after power-on the head unit can join it and open
        // the receiver while the adapter is still starting.
        self.set_stage("StartingHotspot", serde_json::json!({}));
        let hotspot = self.platform.create_hotspot()?;
        let h = hotspot.clone();
        self.own(Layer::Hotspot, Box::new(move || h.close()))?;
        let gap = self.platform.hotspot_down_remaining();
        if !gap.is_zero() {
            // The previous AP went down moments ago, perhaps in the previous run. Bluetooth waits
            // too, so the iPhone is not left waiting for the AP's credentials meanwhile.
            self.events
                .log(&format!("hotspot-gap waitMs={}", gap.as_millis()));
            self.wait(gap)?;
        }
        let planned_ssid = self.platform.planned_hotspot_ssid();
        {
            let (hotspot, mfi) = (hotspot.clone(), mfi.authenticator.clone());
            std::thread::Builder::new()
                .name("carplay-network".into())
                .stack_size(crate::platform::WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    if let Err(e) = self.start_network(generation, hotspot.as_ref(), mfi) {
                        abort(&self.state, generation, crate::failure_text::error_text(&e));
                    }
                })?;
        }
        self.wait_for_adapter()?;
        {
            let mut s = self.state.0.lock().unwrap();
            s.adapter_ready = true;
            self.state.1.notify_all();
        }
        self.events.milestone(Milestone::AdapterReady);
        if !*pairing_started {
            let mode = self.platform.start_pairing()?;
            let mut s = self.state.0.lock().unwrap();
            if s.closed {
                if let Some(m) = mode {
                    let _ = m.close();
                }
            } else {
                s.pairing = mode;
            }
            *pairing_started = true;
        }
        if self.state.0.lock().unwrap().hotspot_failed {
            // The last AP did not come up. The iPhone is connected once this one is, so an AP
            // that keeps failing does not keep connecting it for attempts that cannot finish.
            while await_attempt(&self.state, generation, Duration::from_secs(60), |s| {
                s.hotspot.as_ref().map(|_| ())
            })?
            .is_none()
            {}
        }
        let connection = self.connect_iphone()?;
        let c = connection.clone();
        self.own(Layer::Bluetooth, Box::new(move || c.close()))?;
        self.set_stage("RunningWireless", serde_json::json!({}));
        let network = Arc::new(AttemptNetwork {
            state: self.state.clone(),
            platform: self.platform.clone(),
            generation,
            planned_ssid,
            identified: Mutex::new(None),
            stations_read: Mutex::new(false),
        });
        let events = self.events.clone();
        let on_authenticated = Arc::new(move || events.milestone(Milestone::Authenticated));
        let events = self.events.clone();
        let on_wifi_configuration_sent =
            Arc::new(move || events.milestone(Milestone::WifiInformationSent));
        let state = self.state.clone();
        let platform = self.platform.clone();
        let events = self.events.clone();
        let timeout = self.timeout;
        let on_start_session = Arc::new(move || {
            {
                let mut s = state.0.lock().unwrap();
                if s.closed || s.generation != generation {
                    return;
                }
                s.start_session = true;
                if let Some(n) = &s.network {
                    n.start_session_sent(Instant::now());
                }
            }
            let state = state.clone();
            let platform = platform.clone();
            let events = events.clone();
            let _ = crate::platform::spawn_worker(
                "wireless-handoff-watchdog",
                Box::new(move || {
                    let (m, c) = &*state;
                    let deadline = Instant::now() + timeout;
                    loop {
                        // The iPhone joins the AP while its session starts.
                        sample_stations(platform.as_ref(), &state, &events, generation);
                        let s = m.lock().unwrap();
                        let uses_watchdog = s
                            .network
                            .as_ref()
                            .is_some_and(|n| n.uses_startup_watchdog());
                        let slice = if uses_watchdog {
                            STATION_POLL
                        } else {
                            deadline
                                .saturating_duration_since(Instant::now())
                                .min(STATION_POLL)
                        };
                        let (s, _) = c
                            .wait_timeout_while(s, slice, |s| handing_off(s, generation))
                            .unwrap();
                        if !handing_off(&s, generation) {
                            return;
                        }
                        let network = s.network.clone();
                        let uses_watchdog =
                            network.as_ref().is_some_and(|n| n.uses_startup_watchdog());
                        drop(s);
                        if let Some(e) = network.and_then(|n| n.poll_startup(Instant::now()).err())
                        {
                            abort(&state, generation, crate::failure_text::error_text(&e));
                            return;
                        }
                        if !uses_watchdog && Instant::now() >= deadline {
                            abort(
                                &state,
                                generation,
                                "No established AirPlay/iAP2 tunnel after StartSession".into(),
                            );
                            return;
                        }
                    }
                }),
            );
        });
        let result = self.platform.run_bootstrap(
            connection,
            network,
            mfi.authenticator,
            BootstrapCallbacks {
                on_authenticated,
                on_wifi_configuration_sent,
                on_start_session,
            },
        );
        if !self.state.0.lock().unwrap().active {
            // A failed AP or session explains why the bootstrap ended.
            self.check()?;
            result?;
        }
        self.check()?;
        let (m, c) = &*self.state;
        let s = m.lock().unwrap();
        let (s, _) = c
            .wait_timeout_while(s, self.timeout, |s| {
                !s.closed && s.failure.is_none() && !s.active
            })
            .unwrap();
        if !s.active {
            drop(s);
            self.check()?;
            return Err(std::io::Error::other(
                "Bluetooth ended before AirPlay tunnel was ready",
            ));
        }
        drop(s);
        hotspot.on_car_play_confirmed()?;
        loop {
            self.wait(Duration::from_secs(1))?;
            if !self.platform.network_watch_enabled() {
                hotspot.validate_ready()?;
            }
        }
    }
    /// The attempt's network worker: prepares and starts the AP, then, once the adapter is
    /// usable, the AirPlay receiver on it, and hands the started network to the bootstrap.
    fn start_network(
        &self,
        generation: u64,
        hotspot: &dyn WirelessHotspotManager,
        mfi: Arc<dyn MfiAuthenticator>,
    ) -> Result<()> {
        self.platform.prepare_hotspot();
        let gap = self.platform.hotspot_down_remaining();
        if !gap.is_zero() {
            // An AP a killed run left up went down only now.
            self.events
                .log(&format!("hotspot-gap waitMs={}", gap.as_millis()));
            await_attempt(&self.state, generation, gap, |_| None::<()>)?;
        }
        let info = hotspot.start(Duration::from_secs(60)).inspect_err(|_| {
            let mut s = self.state.0.lock().unwrap();
            // A start the controller's close cancelled is no failed AP.
            if !s.closed && s.generation == generation {
                s.hotspot_failed = true;
            }
        })?;
        let reused = self.platform.hotspot_reused();
        {
            let mut s = self.state.0.lock().unwrap();
            attempt_check(&s, generation)?;
            s.hotspot_failed = false;
            s.hotspot = Some(info.clone());
            s.stations.ap_ready(reused);
            self.state.1.notify_all();
        }
        self.events.hotspot_milestone(reused);
        self.network_stage(generation, "HotspotReady",serde_json::json!({"reused":reused,"ssid":info.ssid,"channel":info.channel,"bssid":info.bssid,"address":info.host_address.map(|a|a.to_string()),"band":if info.channel>14{"5 GHz"}else{"2.4 GHz"}}));
        // The network needs the adapter's address.
        while await_attempt(&self.state, generation, Duration::from_secs(60), |s| {
            s.adapter_ready.then_some(())
        })?
        .is_none()
        {}
        while !self.platform.car_ready()? {
            await_attempt(&self.state, generation, Duration::from_millis(100), |_| {
                None::<()>
            })?;
        }
        self.network_stage(generation, "AttachingNetwork", serde_json::json!({}));
        let state = self.state.clone();
        let e = self.events.clone();
        let a = Arc::new(move || proof(&state, &e, generation, false));
        let state = self.state.clone();
        let e = self.events.clone();
        let t = Arc::new(move || proof(&state, &e, generation, true));
        let state = self.state.clone();
        let failure = Arc::new(move |e: std::io::Error| {
            abort(&state, generation, crate::failure_text::error_text(&e))
        });
        let network = self.platform.create_network(
            info,
            mfi,
            NetworkCallbacks {
                on_air_play: a,
                on_tunnel_ready: t,
                on_failure: failure,
            },
        )?;
        // Owned once started: a close that comes meanwhile then finds it complete.
        if let Err(e) = hotspot.validate_ready().and_then(|()| network.start()) {
            let _ = network.close();
            return Err(e);
        }
        let n = network.clone();
        self.own(Layer::Network, Box::new(move || n.close()))?;
        let mut s = self.state.0.lock().unwrap();
        attempt_check(&s, generation)?;
        s.network = Some(network.clone());
        self.state.1.notify_all();
        drop(s);
        if self.platform.network_watch_enabled() {
            let mut watch = carplay_airplay::car_play_controller::WirelessNetworkWatch::new(true);
            let mut iphone_stations = BTreeSet::new();
            loop {
                if await_attempt(&self.state, generation, STATION_POLL, |_| None::<()>).is_err() {
                    return Ok(());
                }
                let stations = self.platform.hotspot_stations();
                let sent = self.state.0.lock().unwrap().start_session;
                watch.check(|| {
                    network.validate_ready()?;
                    if sent {
                        if let Some(stations) = &stations {
                            let current = stations.iter().cloned().collect::<BTreeSet<_>>();
                            if iphone_stations.is_empty() {
                                iphone_stations.extend(
                                    self.state.0.lock().unwrap().stations.joined.iter().cloned(),
                                );
                            }
                            let before = self
                                .state
                                .0
                                .lock()
                                .unwrap()
                                .stations
                                .before
                                .clone()
                                .unwrap_or_default();
                            let joined = current
                                .difference(&before)
                                .cloned()
                                .collect::<BTreeSet<_>>();
                            if !iphone_stations.is_empty() && iphone_stations.is_disjoint(&current)
                            {
                                return Err(std::io::Error::other("iPhone association was lost"));
                            }
                            iphone_stations.extend(joined);
                        }
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
    fn release_attempt(&self, keep_hotspot: bool) {
        let resources = {
            let mut s = self.state.0.lock().unwrap();
            // Decided under the lock, so `close` either sees the retention and withdraws it, or
            // the attempt releases the AP.
            if keep_hotspot && !s.closed {
                self.platform.keep_hotspot_for_retry();
            }
            s.generation += 1;
            s.bootstrap = None;
            s.failure = None;
            s.start_session = false;
            s.active = false;
            s.hotspot = None;
            s.network = None;
            s.adapter_ready = false;
            s.airplay = false;
            s.tunnel = false;
            if !s.armed {
                s.awaited = None;
                s.woken = None;
            }
            self.state.1.notify_all();
            std::mem::take(&mut s.resources)
        };
        let mut resources = resources;
        // Latest first within a layer, and the layers from the Bluetooth link to the AP.
        resources.reverse();
        resources.sort_by_key(|(layer, _)| std::cmp::Reverse(*layer));
        for (_, close) in resources {
            if let Err(e) = close() {
                self.events.log(&format!("Shutdown failed: {e}"));
            }
        }
    }
    pub fn hotspot_info(&self) -> Option<WirelessHotspotInfo> {
        self.state.0.lock().unwrap().hotspot.clone()
    }
    pub fn stage(&self) -> String {
        self.state.0.lock().unwrap().stage.clone()
    }
    pub fn failed(&self) -> bool {
        let state = self.state.0.lock().unwrap();
        state.failure.is_some() || state.terminal_failure
    }
    pub fn close(&self) -> Result<()> {
        let (pairing, bootstrap) = {
            let mut s = self.state.0.lock().unwrap();
            if s.closed {
                return Ok(());
            }
            s.closed = true;
            s.armed = false;
            s.running = false;
            s.stage = "Stopped".into();
            s.generation += 1;
            self.state.1.notify_all();
            (s.pairing.take(), s.bootstrap.take())
        };
        if let Some(b) = bootstrap {
            let _ = b.close();
        }
        self.release_attempt(false);
        if let Err(e) = self.platform.cancel_hotspot_retry() {
            self.events.log(&format!("Shutdown failed: {e}"));
        }
        if let Some(p) = pairing {
            let _ = p.close();
        }
        self.events
            .emit("stage", &serde_json::json!({"stage":"Stopped"}));
        Ok(())
    }
}
/// The bootstrap's view of the attempt's network.
struct AttemptNetwork {
    state: SharedState,
    platform: Arc<dyn WirelessPlatform>,
    generation: u64,
    planned_ssid: Option<String>,
    /// The SSID the identification named.
    identified: Mutex<Option<String>>,
    /// The stations before the iPhone gets the AP's credentials were read.
    stations_read: Mutex<bool>,
}
impl PendingNetwork for AttemptNetwork {
    fn ssid(&self, timeout: Duration) -> Result<String> {
        let ssid = match &self.planned_ssid {
            Some(ssid) => ssid.clone(),
            None => await_attempt(&self.state, self.generation, timeout, |s| {
                s.hotspot.as_ref().map(|info| info.ssid.clone())
            })?
            .ok_or_else(|| std::io::Error::other("Access point not up for the identification"))?,
        };
        *self.identified.lock().unwrap() = Some(ssid.clone());
        Ok(ssid)
    }
    fn wait(&self, timeout: Duration) -> Result<Option<Arc<dyn NetworkAttachment>>> {
        let Some((network, ssid)) = await_attempt(&self.state, self.generation, timeout, |s| {
            Some((s.network.clone()?, s.hotspot.as_ref()?.ssid.clone()))
        })?
        else {
            return Ok(None);
        };
        if self
            .identified
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|identified| *identified != ssid)
        {
            return Err(std::io::Error::other(
                "Access point SSID differs from the identification",
            ));
        }
        let mut read = self.stations_read.lock().unwrap();
        if !*read {
            // The stations associated before the iPhone gets the AP's credentials, such as the
            // head unit, are not the iPhone.
            let before = self.platform.hotspot_stations();
            let mut s = self.state.0.lock().unwrap();
            attempt_check(&s, self.generation)?;
            s.stations.begin(before);
            *read = true;
        }
        drop(read);
        if self.platform.network_watch_enabled() {
            let known = self.state.0.lock().unwrap().stations.joined.clone();
            if !known.is_empty() {
                if let Some(current) = self.platform.hotspot_stations() {
                    if known.is_disjoint(&current.into_iter().collect()) {
                        return Err(std::io::Error::other("iPhone association was lost"));
                    }
                }
            }
        }
        Ok(Some(network))
    }
}
/// The attempt `generation` goes on: neither closed, failed nor replaced.
fn attempt_check(s: &ControllerState, generation: u64) -> Result<()> {
    if s.closed {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Controller closed",
        ));
    }
    if s.generation != generation {
        return Err(std::io::Error::other("CarPlay attempt ended"));
    }
    if let Some(e) = &s.failure {
        return Err(std::io::Error::other(e.clone()));
    }
    Ok(())
}
/// Waits up to `timeout` until `get` finds what the attempt `generation` waits for; an error once
/// the attempt ended.
fn await_attempt<T>(
    state: &(Mutex<ControllerState>, Condvar),
    generation: u64,
    timeout: Duration,
    get: impl Fn(&ControllerState) -> Option<T>,
) -> Result<Option<T>> {
    let (m, c) = state;
    let s = m.lock().unwrap();
    let (s, _) = c
        .wait_timeout_while(s, timeout, |s| {
            attempt_check(s, generation).is_ok() && get(s).is_none()
        })
        .unwrap();
    attempt_check(&s, generation)?;
    Ok(get(&s))
}
/// The attempt `generation` waits for its session to become active.
fn handing_off(s: &ControllerState, generation: u64) -> bool {
    !s.closed && s.generation == generation && s.failure.is_none() && !s.active
}
/// Records the AP's stations for the attempt `generation`, until the iPhone's has been seen.
fn sample_stations(
    platform: &dyn WirelessPlatform,
    state: &(Mutex<ControllerState>, Condvar),
    events: &JsonEvents,
    generation: u64,
) {
    {
        let s = state.0.lock().unwrap();
        if s.closed || s.generation != generation || s.stations.iphone {
            return;
        }
    }
    let stations = platform.hotspot_stations();
    let mut s = state.0.lock().unwrap();
    if !s.closed && s.generation == generation {
        s.stations.observe(stations);
        if s.stations.iphone {
            drop(s);
            events.milestone(Milestone::IphoneJoined);
        }
    }
}
fn abort(state: &(Mutex<ControllerState>, Condvar), generation: u64, error: String) {
    let bt = {
        let mut s = state.0.lock().unwrap();
        if s.closed || s.generation != generation || s.failure.is_some() {
            return;
        }
        s.failure = Some(error);
        state.1.notify_all();
        s.bootstrap.clone()
    };
    if let Some(b) = bt {
        let _ = b.close();
    }
}
fn proof(
    state: &(Mutex<ControllerState>, Condvar),
    events: &JsonEvents,
    generation: u64,
    tunnel: bool,
) {
    let bt = {
        let mut s = state.0.lock().unwrap();
        if s.closed || s.generation != generation || s.failure.is_some() {
            return;
        }
        if tunnel {
            s.tunnel = true
        } else {
            s.airplay = true
        }
        if !s.airplay || !s.tunnel || s.active {
            return;
        }
        s.active = true;
        s.stage = "WirelessActive".into();
        events.emit("stage", &serde_json::json!({"stage":"WirelessActive"}));
        state.1.notify_all();
        s.bootstrap.clone()
    };
    if let Some(b) = bt {
        let _ = b.close();
    }
}

#[cfg(test)]
mod station_tests {
    use super::*;
    fn list(stations: &[&str]) -> Option<Vec<String>> {
        Some(stations.iter().map(|s| s.to_string()).collect())
    }
    const HEAD_UNIT: &str = "02:00:00:00:00:01";
    const IPHONE: &str = "02:00:00:00:00:02";
    #[test]
    fn only_a_station_that_joined_during_a_bootstrap_counts_as_the_iphone() {
        let mut watch = StationWatch::default();
        watch.ap_ready(false);
        // Unknown stations never take the AP down.
        watch.begin(None);
        assert_eq!(watch.missing_iphone(), None);
        watch.observe(None);
        assert_eq!(watch.missing_iphone(), None);
        // An empty AP.
        watch.begin(list(&[]));
        watch.observe(list(&[]));
        assert_eq!(watch.missing_iphone(), Some("no-station"));
        // The head unit was on the AP before the bootstrap began.
        watch.begin(list(&[HEAD_UNIT]));
        watch.observe(list(&[HEAD_UNIT]));
        assert_eq!(watch.missing_iphone(), Some("no-iphone-station"));
        // The iPhone joins during the bootstrap, and still counts on the next retry.
        watch.begin(list(&[HEAD_UNIT]));
        watch.observe(list(&[HEAD_UNIT, IPHONE]));
        assert_eq!(watch.missing_iphone(), None);
        watch.ap_ready(true);
        watch.begin(list(&[HEAD_UNIT, IPHONE]));
        watch.observe(list(&[HEAD_UNIT, IPHONE]));
        assert_eq!(watch.missing_iphone(), None);
        // Without a read after the bootstrap began nothing is decided.
        watch.begin(list(&[HEAD_UNIT]));
        assert_eq!(watch.missing_iphone(), None);
        // A new AP forgets it.
        watch.ap_ready(false);
        watch.begin(list(&[HEAD_UNIT, IPHONE]));
        watch.observe(list(&[HEAD_UNIT, IPHONE]));
        assert_eq!(watch.missing_iphone(), Some("no-iphone-station"));
    }
    #[test]
    fn stations_of_an_inherited_ap_count_once() {
        let mut watch = StationWatch::default();
        // A codec restart hands the running AP and its iPhone to the new controller.
        watch.ap_ready(true);
        watch.begin(list(&[IPHONE]));
        watch.observe(list(&[IPHONE]));
        assert_eq!(watch.missing_iphone(), None);
        // A station that joins before a later bootstrap does not count.
        watch.ap_ready(true);
        watch.begin(list(&[IPHONE, HEAD_UNIT]));
        watch.observe(list(&[HEAD_UNIT]));
        assert_eq!(watch.missing_iphone(), Some("no-iphone-station"));
        // The first AP of a controller that started it itself is not inherited.
        let mut fresh = StationWatch::default();
        fresh.ap_ready(false);
        fresh.ap_ready(true);
        fresh.begin(list(&[HEAD_UNIT]));
        fresh.observe(list(&[HEAD_UNIT]));
        assert_eq!(fresh.missing_iphone(), Some("no-iphone-station"));
    }
}
