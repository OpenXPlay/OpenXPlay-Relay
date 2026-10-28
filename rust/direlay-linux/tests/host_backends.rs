// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

use openxplay_relay::{
    host_backends::*,
    linux_bluetooth::{PairingBackend, PairingSession, SelectedAdapter},
    linux_hotspot::Radio,
    secure_connections::Failure,
    settings::SharedWifiNetwork,
    *,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Host(Mutex<Vec<String>>);
impl Host {
    fn record(&self, call: String) {
        self.0.lock().unwrap().push(call);
    }
}
#[derive(Clone)]
struct Backend(Arc<Host>);
impl AccessPointFactory for Backend {
    fn prepare(&self) -> Result<bool> {
        self.0.record("prepare".into());
        Ok(false)
    }
    fn access_point(&self, config: AccessPointConfig) -> Arc<dyn WirelessHotspotManager> {
        self.0.record(format!(
            "ap:{}:{}",
            config.interface, config.hotspot.channel
        ));
        Arc::new(Ap(self.0.clone()))
    }
    fn stations(&self, interface: &str) -> Result<Vec<String>> {
        self.0.record(format!("stations:{interface}"));
        Ok(vec![])
    }
    fn radio(&self, _: &str) -> Result<Radio> {
        Ok(Radio::default())
    }
}
struct Ap(Arc<Host>);
impl WirelessHotspotManager for Ap {
    fn start(&self, _: Duration) -> Result<WirelessHotspotInfo> {
        self.0.record("start".into());
        Err(std::io::Error::other("fake AP"))
    }
    fn validate_ready(&self) -> Result<()> {
        Ok(())
    }
    fn on_car_play_confirmed(&self) -> Result<()> {
        Ok(())
    }
    fn close(&self) -> Result<()> {
        self.0.record("stop".into());
        Ok(())
    }
}
impl StationFactory for Backend {
    fn saved(&self, _: &str, _: &str, _: &Path) -> Result<Box<dyn StationBackend>> {
        unreachable!()
    }
    fn shared(
        &self,
        interface: &str,
        saved: &[SharedWifiNetwork],
    ) -> Result<(String, Arc<dyn WirelessHotspotManager>)> {
        self.0
            .record(format!("shared:{interface}:{}", saved[0].ssid));
        Ok((saved[0].ssid.clone(), Arc::new(Ap(self.0.clone()))))
    }
}
impl SecureConnectionsControl for Backend {
    fn set_secure_connections(&self, _: u16, _: bool) -> std::result::Result<(), Failure> {
        Ok(())
    }
}
impl BluetoothBackend for Backend {
    fn session(&self) -> Arc<dyn BluetoothBackend> {
        self.0.record("session".into());
        Arc::new(self.clone())
    }
    fn check_radio(&self) -> Result<()> {
        self.0.record("radio".into());
        Ok(())
    }
    fn register_profiles(&self) -> Result<()> {
        self.0.record("profiles".into());
        self.check_radio()
    }
    fn devices(&self) -> Result<Vec<BondedDevice>> {
        Ok(vec![])
    }
    fn select(&self, configured: Option<&str>) -> Result<BondedDevice> {
        self.0.record(format!("iphone:{}", configured.unwrap()));
        Ok(BondedDevice {
            address: configured.unwrap().into(),
            ..Default::default()
        })
    }
    fn local_address(&self) -> Result<String> {
        Ok("AA:BB:CC:DD:EE:FF".into())
    }
    fn channel(&self, address: &str, channel: Option<u8>) -> Result<u8> {
        self.0
            .record(format!("channel:{address}:{}", channel.unwrap()));
        Ok(channel.unwrap())
    }
    fn set_device_class(&self, _: u32) -> Result<()> {
        Ok(())
    }
    fn selected_adapter(&self) -> Option<SelectedAdapter> {
        None
    }
    fn pairing(&self) -> Arc<dyn PairingBackend> {
        Arc::new(Pairing)
    }
}
struct Pairing;
impl PairingBackend for Pairing {
    fn open(&self, _: u32, _: Instant) -> Result<Box<dyn PairingSession>> {
        unreachable!()
    }
}
#[test]
fn platform_uses_semantic_backends_and_current_phone_overrides() {
    let root: PathBuf = std::env::temp_dir().join(format!(
        "host-backends-{}",
        admin::auth::random_hex(8).unwrap()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let state = PersistentState::open(&root).unwrap();
    let mut options = Options::parse(&[
        "--state-dir".into(),
        root.to_string_lossy().into_owned(),
        "--iphone".into(),
        "AA:BB:CC:DD:EE:01".into(),
        "--rfcomm-channel".into(),
        "12".into(),
    ])
    .unwrap();
    let host = Arc::new(Host::default());
    let backends = HostBackends {
        access_point: Arc::new(Backend(host.clone())),
        station: Arc::new(Backend(host.clone())),
        bluetooth: Arc::new(Backend(host.clone())),
    };
    let events = Arc::new(JsonEvents::new(Arc::new(|_| {})));
    let input = Arc::new(DiPlayInput::new(events.clone()));
    let platform = |options: Options| {
        LinuxWirelessPlatform::new(
            options,
            backends.clone(),
            state.identity().unwrap(),
            state.pairings().unwrap(),
            Arc::new(RecordingMediaSink::discard(events.clone())),
            events.clone(),
            input.clone(),
        )
    };
    let ap = platform(options.clone());
    ap.check_bluetooth_radio().unwrap();
    let phone = ap.select_iphone().unwrap();
    ap.create_bluetooth(&phone).unwrap();
    ap.prepare_hotspot();
    let manager = ap.create_hotspot().unwrap();
    assert!(manager.start(Duration::from_secs(1)).is_err());
    manager.close().unwrap();
    assert_eq!(ap.hotspot_stations(), Some(vec![]));
    options.settings.network_mode = "shared-wifi".into();
    options.settings.shared_wifi_networks = vec![SharedWifiNetwork {
        ssid: "Home".into(),
        password: "password".into(),
    }];
    let shared = platform(options);
    let manager = shared.create_hotspot().unwrap();
    assert_eq!(shared.planned_hotspot_ssid().as_deref(), Some("Home"));
    manager.close().unwrap();
    let calls = host.0.lock().unwrap();
    assert_eq!(
        &calls[..7],
        [
            "session",
            "profiles",
            "radio",
            "iphone:AA:BB:CC:DD:EE:01",
            "channel:AA:BB:CC:DD:EE:01:12",
            "prepare",
            "ap:wlan0:36"
        ]
    );
    assert_eq!(calls.iter().filter(|s| *s == "session").count(), 2);
    assert!(calls.iter().any(|s| s == "stations:wlan0"));
    assert!(calls.iter().any(|s| s == "shared:wlan0:Home"));
    drop(calls);
    drop((manager, shared, ap, state));
    std::fs::remove_dir_all(root).unwrap();
}
