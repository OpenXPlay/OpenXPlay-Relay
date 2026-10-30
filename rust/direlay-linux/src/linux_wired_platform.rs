// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-28

use crate::Result;
use crate::{
    linux_receiver::{build_receiver, ReceiverSetup, Watchdog},
    linux_wired_controller::*,
    linux_wired_link::{EthernetPort, FramePump, TapDevice},
    *,
};
use carplay_airplay::*;
use carplay_iap2::{mfi::MfiAuthenticator, *};
use sha2::{Digest, Sha256};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub struct LinuxWiredPlatform {
    head_unit: Arc<LinuxWirelessPlatform>,
    options: Options,
    identity: Arc<AirPlayIdentity>,
    pairings: Arc<dyn PairingStore>,
    sink: Arc<dyn MediaSink>,
    events: Arc<JsonEvents>,
    input: Arc<DiPlayInput>,
    bus: Arc<dyn UsbHostBus>,
}
impl LinuxWiredPlatform {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        head_unit: Arc<LinuxWirelessPlatform>,
        options: Options,
        identity: Arc<AirPlayIdentity>,
        pairings: Arc<dyn PairingStore>,
        sink: Arc<dyn MediaSink>,
        events: Arc<JsonEvents>,
        input: Arc<DiPlayInput>,
    ) -> Self {
        #[cfg(target_os = "linux")]
        let bus: Arc<dyn UsbHostBus> = Arc::new(LinuxUsbHostBus::new());
        #[cfg(not(target_os = "linux"))]
        let bus: Arc<dyn UsbHostBus> = Arc::new(UnsupportedBus);
        Self {
            head_unit,
            options,
            identity,
            pairings,
            sink,
            events,
            input,
            bus,
        }
    }
}
#[cfg(not(target_os = "linux"))]
struct UnsupportedBus;
#[cfg(not(target_os = "linux"))]
impl UsbHostBus for UnsupportedBus {
    fn enumerate(&self) -> UsbResult<Vec<UsbDeviceInfo>> {
        Err(UsbError::Protocol("USB host requires Linux".into()))
    }
    fn open(&self, _: &UsbDeviceInfo) -> UsbResult<Arc<dyn UsbHostDevice>> {
        Err(UsbError::Protocol("USB host requires Linux".into()))
    }
}
impl WiredPlatform for LinuxWiredPlatform {
    fn prepare_mfi(&self) -> Result<MfiResource> {
        self.head_unit.prepare_mfi()
    }
    fn create_hotspot(&self) -> Result<Arc<dyn WirelessHotspotManager>> {
        self.head_unit.create_hotspot()
    }
    fn prepare_hotspot(&self) {
        self.head_unit.prepare_hotspot();
    }
    fn hotspot_down_remaining(&self) -> Duration {
        self.head_unit.hotspot_down_remaining()
    }
    fn hotspot_reused(&self) -> bool {
        self.head_unit.hotspot_reused()
    }
    fn keep_hotspot_for_retry(&self) {
        self.head_unit.keep_hotspot_for_retry();
    }
    fn cancel_hotspot_retry(&self) -> Result<()> {
        self.head_unit.cancel_hotspot_retry()
    }
    fn car_ready(&self) -> Result<bool> {
        self.head_unit.car_ready()
    }
    fn enumerate(&self) -> Result<Vec<UsbDeviceInfo>> {
        self.bus.enumerate().map_err(Into::into)
    }
    fn open_device(
        &self,
        info: &UsbDeviceInfo,
        current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Arc<dyn WiredDevice>> {
        let device = self.bus.open(info).map_err(io::Error::from)?;
        let events = self.events.clone();
        Ok(Arc::new(Device {
            device,
            configuration: Mutex::default(),
            fallback_mac: identity_mac(&self.identity),
            diagnostic: Arc::new(move |s| {
                if current() {
                    events.log(&s);
                }
            }),
        }))
    }
    fn open_control(
        &self,
        stream: Arc<dyn DuplexStream>,
        mfi: Arc<dyn MfiAuthenticator>,
        callbacks: WiredCallbacks,
    ) -> Result<Arc<dyn WiredControl>> {
        let events = self.events.clone();
        let current = callbacks.current;
        let session = Iap2Session::open_wired(
            stream,
            Arc::new(move |s| {
                if current() {
                    events.trace(&s);
                }
            }),
        )?;
        let mut identification = crate::linux_wireless_platform::identification(
            &self.identity,
            "",
            "",
            self.input.plugins_enabled(),
        );
        identification.wireless = None;
        identification.car_play_usb_interface_number = 3;
        Ok(Arc::new(Control {
            session,
            mfi,
            identification,
            public_key: self.identity.public_key_hex(),
            charging: self.options.usb_charging_ma,
            input: self.input.clone(),
            events: self.events.clone(),
        }))
    }
    fn create_tap(&self, mac: [u8; 6]) -> Result<Arc<dyn WiredTap>> {
        Ok(Arc::new(Tap(Arc::new(TapDevice::create(
            mac,
            &crate::platform::ProcessCommandRunner,
        )?))))
    }
    fn start_pump(
        &self,
        tap: Arc<dyn WiredTap>,
        ncm: Arc<dyn WiredNcm>,
        failure: FailureCallback,
    ) -> Result<Arc<dyn CloseResource>> {
        Ok(Arc::new(Pump(FramePump::start_ports(
            tap.port(),
            Arc::new(NcmPort {
                ncm,
                closed: AtomicBool::new(false),
            }),
            failure,
        )?)))
    }
    fn create_receiver(
        &self,
        index: u32,
        mfi: Arc<dyn MfiAuthenticator>,
        callbacks: WiredCallbacks,
    ) -> Result<Arc<dyn WiredReceiver>> {
        let mut config = crate::linux_wireless_platform::air_play_config(
            mac_string(identity_mac(&self.identity)),
            String::new(),
            self.input.configure_session(),
            self.options.settings.car_bluetooth_audio,
            crate::carplay_policy::iap_channel_only_when_offered(&self.input.connection_settings()),
        );
        self.input.configure_display(&mut config.main);
        config.port = 7000;
        let closed = Arc::new(AtomicBool::new(false));
        let watchdog: Watchdog = Arc::default();
        let server = build_receiver(ReceiverSetup {
            address: BindAddress::new("fe80::2".parse().unwrap(), index),
            config,
            identity: self.identity.clone(),
            pairings: self.pairings.clone(),
            mfi,
            sink: self.sink.clone(),
            events: self.events.clone(),
            callbacks: NetworkCallbacks {
                on_air_play: callbacks.on_air_play,
                on_tunnel_ready: Arc::new(|| {}),
                on_failure: callbacks.on_failure,
                on_session: None,
            },
            closed: closed.clone(),
            input: Some((self.input.clone(), self.sink.clone(), self.events.clone())),
            watchdog: watchdog.clone(),
            tunnel: None,
            current: Some(callbacks.current),
        })?;
        Ok(Arc::new(Receiver {
            server,
            closed,
            watchdog,
        }))
    }
}

struct Device {
    device: Arc<dyn UsbHostDevice>,
    configuration: Mutex<Option<UsbConfiguration>>,
    fallback_mac: [u8; 6],
    diagnostic: Arc<dyn Fn(String) + Send + Sync>,
}
impl CloseResource for Device {
    fn close(&self) -> Result<()> {
        self.device.close().map_err(Into::into)
    }
}
impl WiredDevice for Device {
    fn discover(&self) -> Result<UsbDescriptors> {
        IphoneUsbHost::discover(self.device.as_ref()).map_err(Into::into)
    }
    fn request_mode(&self, descriptors: &UsbDescriptors) -> Result<CarPlayModeOutcome> {
        IphoneUsbHost::request_car_play_mode(self.device.as_ref(), descriptors).map_err(Into::into)
    }
    fn select_configuration(
        &self,
        descriptors: &UsbDescriptors,
        config: &UsbConfiguration,
    ) -> Result<()> {
        IphoneUsbHost::select_configuration(self.device.as_ref(), descriptors, config)
            .map_err(io::Error::from)?;
        *self.configuration.lock().unwrap() = Some(config.clone());
        Ok(())
    }
    fn open_mux(&self) -> Result<Arc<dyn WiredMux>> {
        let config = self
            .configuration
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| io::Error::other("USB configuration not selected"))?;
        let session =
            Iap2UsbSession::open(self.device.clone(), &config).map_err(io::Error::from)?;
        match Iap2UsbMuxHost::open(
            session.clone(),
            Duration::from_secs(10),
            self.diagnostic.clone(),
        ) {
            Ok(host) => Ok(Arc::new(Mux { host, session })),
            Err(e) => {
                let _ = session.close();
                Err(e)
            }
        }
    }
    fn open_ncm(&self) -> Result<Arc<dyn WiredNcm>> {
        let function = self
            .configuration
            .lock()
            .unwrap()
            .as_ref()
            .and_then(NcmFunctionDiscovery::find)
            .ok_or_else(|| io::Error::other("USB configuration has no NCM function"))?;
        Ok(Arc::new(Ncm(NcmUsbBridge::open(
            self.device.clone(),
            function,
            self.fallback_mac,
            self.diagnostic.clone(),
        )
        .map_err(io::Error::from)?)))
    }
}
struct Mux {
    host: Arc<Iap2UsbMuxHost>,
    session: Arc<Iap2UsbSession>,
}
impl CloseResource for Mux {
    fn close(&self) -> Result<()> {
        let result = self.host.close();
        result.and(self.session.close())
    }
}
impl WiredMux for Mux {
    fn pair(
        &self,
        host_id: &str,
        buid: &str,
        budget: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> std::result::Result<LockdownPairRecord, LockdownPairingError> {
        LockdownPairingClient::new(self.host.clone()).pair(
            "OpenXPlay",
            host_id,
            buid,
            budget,
            cancelled,
        )
    }
    fn open_carkit(
        &self,
        record: &LockdownPairRecord,
    ) -> std::result::Result<Arc<dyn DuplexStream>, LockdownPairingError> {
        Ok(
            LockdownCarKitClient::new(self.host.clone(), Arc::new(RustlsConnector))
                .open(record, "OpenXPlay")?,
        )
    }
}
struct Ncm(Arc<NcmUsbBridge>);
impl EthernetPort for Ncm {
    fn send_frame(&self, frame: &[u8], timeout: Duration) -> Result<()> {
        self.0.send(frame, timeout).map_err(Into::into)
    }
    fn receive_frame(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        self.0.recv(timeout).map_err(Into::into)
    }
    fn close(&self) -> Result<()> {
        self.0.close().map_err(Into::into)
    }
}
impl WiredNcm for Ncm {
    fn host_mac(&self) -> [u8; 6] {
        self.0.host_mac()
    }
}
// Pump shutdown stops the adapter. The controller releases NCM after AirPlay, pump and TAP.
struct NcmPort {
    ncm: Arc<dyn WiredNcm>,
    closed: AtomicBool,
}
impl EthernetPort for NcmPort {
    fn send_frame(&self, frame: &[u8], timeout: Duration) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(io::Error::other("NCM adapter closed"));
        }
        self.ncm.send_frame(frame, timeout)
    }
    fn receive_frame(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(None);
        }
        self.ncm.receive_frame(timeout)
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}
struct Tap(Arc<TapDevice>);
impl CloseResource for Tap {
    fn close(&self) -> Result<()> {
        self.0.close()
    }
}
impl WiredTap for Tap {
    fn index(&self) -> u32 {
        self.0.index()
    }
    fn port(&self) -> Arc<dyn EthernetPort> {
        self.0.clone()
    }
}
struct Pump(FramePump);
impl CloseResource for Pump {
    fn close(&self) -> Result<()> {
        self.0.close()
    }
}
struct Receiver {
    server: Arc<LinuxAirPlayServer>,
    closed: Arc<AtomicBool>,
    watchdog: Watchdog,
}
impl CloseResource for Receiver {
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        if let Some(w) = self.watchdog.lock().unwrap().as_mut() {
            w.terminate();
        }
        self.server.close()
    }
}
impl WiredReceiver for Receiver {
    fn start(&self) -> Result<u16> {
        self.server.start()?;
        Ok(self.server.config().port)
    }
    fn start_session_sent(&self, now: Instant) {
        arm_watchdog(&self.watchdog, now);
    }
    fn poll_startup(&self, now: Instant) -> Result<()> {
        poll_watchdog(&self.watchdog, now)
    }
}
fn arm_watchdog(watchdog: &Watchdog, now: Instant) {
    if let Some(w) = watchdog.lock().unwrap().as_mut() {
        // The portable watchdog has a fixed 30 s first-TCP budget. Wired allows 45 s;
        // offset its first-start origin without altering its accept or session deadlines.
        w.start_session_sent(now + Duration::from_secs(15));
    }
}
fn poll_watchdog(watchdog: &Watchdog, now: Instant) -> Result<()> {
    use carplay_airplay::first_tcp_watchdog::FirstTcpTimeout;
    match watchdog.lock().unwrap().as_mut().and_then(|w| w.poll(now)) {
        Some(FirstTcpTimeout::FirstTcp) => Err(io::Error::other(
            "first_tcp_timeout: no AirPlay connection over USB within 45 seconds of StartSession",
        )),
        Some(FirstTcpTimeout::Session) => Err(io::Error::other(
            "AirPlay session timeout after USB TCP accept",
        )),
        None => Ok(()),
    }
}

struct Control {
    session: Arc<Iap2Session>,
    mfi: Arc<dyn MfiAuthenticator>,
    identification: Iap2IdentificationConfig,
    public_key: String,
    charging: u16,
    input: Arc<DiPlayInput>,
    events: Arc<JsonEvents>,
}
impl CloseResource for Control {
    fn close(&self) -> Result<()> {
        self.session.close()
    }
}
impl WiredControl for Control {
    fn run(&self, port: u16, host_mac: [u8; 6], callbacks: WiredCallbacks) -> Result<()> {
        let endpoint = Iap2WiredCarPlayEndpoint::new(
            port,
            host_mac,
            self.public_key.clone(),
            "950.7.1".into(),
        )?;
        let events = ControlEvents {
            callbacks,
            input: self.input.clone(),
            events: self.events.clone(),
        };
        let result = Iap2WiredControlClient::new(self.session.clone(), self.mfi.clone()).run(
            &self.identification,
            &endpoint,
            self.charging,
            None,
            &events,
        )?;
        Err(io::Error::other(format!(
            "Wired iAP2 control ended: {:?}",
            result.terminal
        )))
    }
}
struct ControlEvents {
    callbacks: WiredCallbacks,
    input: Arc<DiPlayInput>,
    events: Arc<JsonEvents>,
}
impl WiredControlEvents for ControlEvents {
    fn on_stage_changed(&self, stage: Iap2WiredControlStage) {
        if (self.callbacks.current)() {
            self.events.emit(
                "wired-iap2-stage",
                &serde_json::json!({"stage":format!("{stage:?}")}),
            );
        }
    }
    fn on_authenticated(&self) {
        (self.callbacks.on_authenticated)();
    }
    fn on_start_session_sent(&self, _: Iap2StartSessionSent) {
        (self.callbacks.on_start_session)();
    }
    fn on_incoming(&self, frame: &Iap2Frame) {
        if (self.callbacks.current)() {
            self.events.iap2_frame();
            self.input.iap2_context(frame);
        }
    }
    fn on_progress(&self, message: &str) {
        if (self.callbacks.current)() {
            self.events.log(message);
        }
    }
}
pub(crate) fn identity_mac(identity: &AirPlayIdentity) -> [u8; 6] {
    let hash = Sha256::digest(identity.public_key);
    let mut mac: [u8; 6] = hash[..6].try_into().unwrap();
    mac[0] = (mac[0] & 0xfc) | 0x02;
    mac
}
fn mac_string(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;
    use carplay_airplay::first_tcp_watchdog::FirstTcpWatchdog;
    #[test]
    fn wired_first_tcp_budget_is_45_seconds_and_starts_only_once() {
        let now = Instant::now();
        let watchdog = Arc::new(Mutex::new(Some(FirstTcpWatchdog::new(
            AirPlayListenerIdentity::new(1),
        ))));
        arm_watchdog(&watchdog, now);
        arm_watchdog(&watchdog, now + Duration::from_secs(20));
        assert!(poll_watchdog(&watchdog, now + Duration::from_secs(44)).is_ok());
        let error = poll_watchdog(&watchdog, now + Duration::from_secs(45)).unwrap_err();
        assert!(error.to_string().contains("over USB within 45 seconds"));
        assert_eq!(
            crate::carplay_policy::classify_failure(&error.to_string()),
            "first_tcp_timeout"
        );
    }
    #[test]
    fn wired_tcp_accept_uses_the_real_time_for_the_session_deadline() {
        let now = Instant::now();
        let identity = AirPlayListenerIdentity::new(1);
        let watchdog = Arc::new(Mutex::new(Some(
            FirstTcpWatchdog::new(identity.clone()).with_session_timeout(Duration::from_secs(45)),
        )));
        arm_watchdog(&watchdog, now);
        watchdog.lock().unwrap().as_mut().unwrap().accepted(
            &AirPlayTcpAccepted {
                listener: identity,
                internal: false,
                accepted_at_nanos: 0,
            },
            now + Duration::from_secs(44),
        );
        assert!(poll_watchdog(&watchdog, now + Duration::from_secs(88)).is_ok());
        assert!(poll_watchdog(&watchdog, now + Duration::from_secs(89))
            .unwrap_err()
            .to_string()
            .contains("after USB TCP accept"));
    }
    #[test]
    fn airplay_identity_mac_and_wired_host_mac_are_distinct_identifiers() {
        let identity = AirPlayIdentity {
            public_key: [0; 32],
            private_key: [0; 32],
            pairing_id: "test".into(),
        };
        let airplay_mac = identity_mac(&identity);
        assert_eq!(mac_string(airplay_mac), "66:68:7a:ad:f8:62");
        assert_eq!(airplay_mac[0] & 3, 2);
        let host_mac = [2, 3, 4, 5, 6, 7];
        let endpoint = Iap2WiredCarPlayEndpoint::new(
            7003,
            host_mac,
            identity.public_key_hex(),
            "950.7.1".into(),
        )
        .unwrap();
        assert_eq!(
            endpoint.device_identifier.as_deref(),
            Some("02:03:04:05:06:07")
        );
        assert_eq!(endpoint.ipv6_addresses, ["fe80::2"]);
        assert_ne!(endpoint.device_identifier.unwrap(), mac_string(airplay_mac));
    }
}
