// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

use crate::{
    admin::{WifiClient, WifiNetwork},
    linux_bluetooth::{AdapterSelection, BondedDevice, PairingBackend},
    linux_hotspot::Radio,
    provision::{NetworkManagerCli, WifiProvisioner},
    settings::SharedWifiNetwork,
    *,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// Runs with the access point's address just before NetworkManager activates it.
pub type BeforeActivation = Arc<dyn Fn(std::net::Ipv4Addr) + Send + Sync>;
pub struct AccessPointConfig {
    pub state_dir: PathBuf,
    pub interface: String,
    pub hotspot: crate::device_preset::HotspotResolution,
    pub capabilities: Option<Arc<crate::host_capabilities::CapabilityCache>>,
    pub provisioning: Option<Arc<provision::ProvisioningGate>>,
    pub before_activation: Option<BeforeActivation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiBand {
    Ghz2,
    Ghz5,
}
#[derive(Clone, Debug)]
pub enum AccessPointCredentials {
    Saved(PathBuf),
    Supplied { ssid: String, password: String },
}
#[derive(Clone, Debug)]
pub struct AccessPointRequest {
    pub credentials: AccessPointCredentials,
    pub band: WifiBand,
    pub channel: u8,
}
impl AccessPointRequest {
    pub fn saved(state_dir: PathBuf, band: WifiBand, channel: u8) -> Self {
        Self {
            credentials: AccessPointCredentials::Saved(state_dir),
            band,
            channel,
        }
    }
}
/// The AP session needs credentials, band and channel; it owns its activation and teardown.
pub trait AccessPointBackend: Send + Sync {
    fn start(&self, request: &AccessPointRequest, timeout: Duration)
        -> Result<WirelessHotspotInfo>;
    fn validate(&self) -> Result<()>;
    fn stations(&self) -> Result<Vec<String>>;
    fn radio(&self) -> Result<Radio>;
    fn confirmed(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}
pub trait AccessPointFactory: Send + Sync {
    fn wifi_enabled(&self) -> Option<bool> {
        None
    }
    fn prepare(&self) -> Result<bool>;
    fn access_point(&self, config: AccessPointConfig) -> Arc<dyn WirelessHotspotManager>;
    fn stations(&self, interface: &str) -> Result<Vec<String>>;
    fn radio(&self, interface: &str) -> Result<Radio>;
}
struct AccessPointManager {
    backend: Arc<dyn AccessPointBackend>,
    request: AccessPointRequest,
}
impl WirelessHotspotManager for AccessPointManager {
    fn start(&self, timeout: Duration) -> Result<WirelessHotspotInfo> {
        self.backend.start(&self.request, timeout)
    }
    fn validate_ready(&self) -> Result<()> {
        self.backend.validate()
    }
    fn on_car_play_confirmed(&self) -> Result<()> {
        self.backend.confirmed()
    }
    fn close(&self) -> Result<()> {
        self.backend.stop()
    }
}
impl AccessPointBackend for LinuxHotspot {
    fn start(
        &self,
        request: &AccessPointRequest,
        timeout: Duration,
    ) -> Result<WirelessHotspotInfo> {
        self.start_requested(request, timeout)
    }
    fn validate(&self) -> Result<()> {
        self.validate_ready()
    }
    fn stations(&self) -> Result<Vec<String>> {
        self.associated_stations()
    }
    fn radio(&self) -> Result<Radio> {
        self.radio_info()
    }
    fn confirmed(&self) -> Result<()> {
        self.on_car_play_confirmed()
    }
    fn stop(&self) -> Result<()> {
        self.close()
    }
}
/// Saved station networks and the setup AP are owned together so they never fight for a radio.
pub trait StationBackend: Send + Sync {
    fn status(&self) -> WifiClient;
    fn scan(&mut self) -> Result<Vec<WifiNetwork>>;
    fn join(&mut self, ssid: &str, password: &str) -> Result<()>;
    fn forget(&mut self) -> Result<()>;
    fn sync(&mut self, networks: &[SharedWifiNetwork]) -> Result<()>;
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn stop_setup(&mut self) -> Result<()>;
    fn start_with_network(&mut self, ssid: &str, password: &str) -> Result<()>;
    fn hold_for_carlink(&mut self, held: bool) -> bool;
    fn carlink_network(&self, ssid: &str) -> Result<SharedWifiNetwork>;
    fn current_ssid(&self) -> Option<String>;
    fn notices(&self) -> &[String];
    fn setup_ssid(&self) -> &str;
}
pub trait StationFactory: Send + Sync {
    fn saved(&self, interface: &str, mac: &str, state: &Path) -> Result<Box<dyn StationBackend>>;
    fn shared(
        &self,
        interface: &str,
        saved: &[SharedWifiNetwork],
    ) -> Result<(String, Arc<dyn WirelessHotspotManager>)>;
}
pub trait SecureConnectionsControl: Send + Sync {
    fn set_secure_connections(
        &self,
        index: u16,
        enabled: bool,
    ) -> std::result::Result<(), secure_connections::Failure>;
}
pub struct BtmgmtControl(pub Arc<dyn CommandRunner>);
impl SecureConnectionsControl for BtmgmtControl {
    fn set_secure_connections(
        &self,
        index: u16,
        enabled: bool,
    ) -> std::result::Result<(), secure_connections::Failure> {
        match self.0.run(
            &secure_connections::switch_command(index, enabled),
            secure_connections::SWITCH_TIMEOUT,
        ) {
            Ok(r) => secure_connections::verify(r.exit_code, &r.output, enabled),
            Err(e) => Err(secure_connections::Failure::Failed(e.to_string())),
        }
    }
}
pub trait BluetoothBackend: SecureConnectionsControl {
    /// A controller attempt owns its service registration independently of the admin shell.
    fn session(&self) -> Arc<dyn BluetoothBackend>;
    /// Readiness only: never acquires a profile registration.
    fn check_radio(&self) -> Result<()>;
    fn power_on(&self) -> Result<()> {
        Err(std::io::Error::other("Bluetooth power control unavailable"))
    }
    /// Controller-only profile lease; released when the session backend is dropped.
    fn register_profiles(&self) -> Result<()>;
    fn devices(&self) -> Result<Vec<BondedDevice>>;
    fn select(&self, configured: Option<&str>) -> Result<BondedDevice>;
    fn local_address(&self) -> Result<String>;
    fn channel(&self, address: &str, channel: Option<u8>) -> Result<u8>;
    fn set_device_class(&self, class: u32) -> Result<()>;
    fn selected_adapter(&self) -> Option<linux_bluetooth::SelectedAdapter>;
    fn pairing(&self) -> Arc<dyn PairingBackend>;
}
/// One composition point for the host implementations. Consumers accept these interfaces.
#[derive(Clone)]
pub struct HostBackends {
    pub access_point: Arc<dyn AccessPointFactory>,
    pub station: Arc<dyn StationFactory>,
    pub bluetooth: Arc<dyn BluetoothBackend>,
}
impl HostBackends {
    pub fn for_controller(&self) -> Self {
        Self {
            access_point: self.access_point.clone(),
            station: self.station.clone(),
            bluetooth: self.bluetooth.session(),
        }
    }
    pub fn linux(
        options: &Options,
        runner: Arc<dyn CommandRunner>,
        events: Arc<JsonEvents>,
    ) -> Self {
        let mut paths = crate::runtime_paths::RuntimePaths::new(&options.state_dir);
        paths.runner = runner;
        Self::linux_with_runtime_paths(options, paths, events)
    }
    pub fn linux_with_runtime_paths(
        options: &Options,
        paths: crate::runtime_paths::RuntimePaths,
        events: Arc<JsonEvents>,
    ) -> Self {
        let runner = paths.runner.clone();
        let selection = AdapterSelection(options.host_profile.profile.bluetooth_adapter.clone());
        let nm = Arc::new(NetworkManagerBackend {
            runner: runner.clone(),
            station_cli: "nmcli".into(),
            paths,
        });
        Self {
            access_point: nm.clone(),
            station: nm,
            bluetooth: Arc::new(BluezBackend {
                control: BtmgmtControl(runner.clone()),
                bluetooth: LinuxBluetooth::new(runner, None, None)
                    .with_adapter_selection(selection.clone()),
                selection: selection.clone(),
                pairing: Arc::new(
                    linux_bluetooth::BluezPairing::new(events).with_adapter_selection(selection),
                ),
            }),
        }
    }
}
struct NetworkManagerBackend {
    runner: Arc<dyn CommandRunner>,
    station_cli: PathBuf,
    paths: crate::runtime_paths::RuntimePaths,
}
#[cfg(test)]
pub(crate) fn station_with_cli(
    runner: Arc<dyn CommandRunner>,
    station_cli: PathBuf,
    paths: crate::runtime_paths::RuntimePaths,
) -> Arc<dyn StationFactory> {
    Arc::new(NetworkManagerBackend {
        runner,
        station_cli,
        paths,
    })
}
impl AccessPointFactory for NetworkManagerBackend {
    fn wifi_enabled(&self) -> Option<bool> {
        let value = platform::command(
            self.runner.as_ref(),
            &["nmcli", "radio", "wifi"],
            Duration::from_secs(3),
        )
        .ok()?;
        match value.trim() {
            "enabled" => Some(true),
            "disabled" => Some(false),
            _ => None,
        }
    }
    fn prepare(&self) -> Result<bool> {
        linux_hotspot::remove_stale_hotspots(self.runner.as_ref())
    }
    fn access_point(&self, config: AccessPointConfig) -> Arc<dyn WirelessHotspotManager> {
        let band = if config.hotspot.band == "5" {
            WifiBand::Ghz5
        } else {
            WifiBand::Ghz2
        };
        let request =
            AccessPointRequest::saved(config.state_dir.clone(), band, config.hotspot.channel);
        let ap = LinuxHotspot::new(
            config.state_dir,
            self.runner.clone(),
            config.interface,
            config.hotspot.channel,
        )
        .with_runtime_dirs(self.paths.keyfiles.clone(), self.paths.local_dns.clone());
        let ap = match config.capabilities {
            Some(cache) => ap.with_resolution(cache, config.hotspot),
            None => ap,
        };
        let ap = match config.provisioning {
            Some(gate) => ap.with_provisioning(gate),
            None => ap,
        };
        let backend = Arc::new(match config.before_activation {
            Some(hook) => ap.with_before_activation(hook),
            None => ap,
        });
        Arc::new(AccessPointManager { backend, request })
    }
    fn stations(&self, interface: &str) -> Result<Vec<String>> {
        linux_hotspot::associated_stations(self.runner.as_ref(), interface)
    }
    fn radio(&self, interface: &str) -> Result<Radio> {
        platform::command(
            self.runner.as_ref(),
            &["iw", "dev", interface, "info"],
            Duration::from_secs(3),
        )
        .map(|s| linux_hotspot::WifiOutput::iw_info(&s))
    }
}
impl StationFactory for NetworkManagerBackend {
    fn saved(&self, interface: &str, mac: &str, state: &Path) -> Result<Box<dyn StationBackend>> {
        Ok(Box::new(
            WifiProvisioner::new(
                provision::ProcessNetworkManagerCli {
                    executable: self.station_cli.clone(),
                },
                interface.into(),
                mac,
                state,
            )?
            .with_local_dns(self.paths.managed_dns()),
        ))
    }
    fn shared(
        &self,
        interface: &str,
        saved: &[SharedWifiNetwork],
    ) -> Result<(String, Arc<dyn WirelessHotspotManager>)> {
        let rows = platform::command(
            self.runner.as_ref(),
            &[
                "nmcli",
                "-t",
                "-f",
                "IN-USE,SSID",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
                "ifname",
                interface,
            ],
            Duration::from_secs(5),
        )?;
        let network = shared_network(saved, &rows)
            .ok_or_else(|| std::io::Error::other("No saved Wi-Fi is connected or in range"))?;
        Ok((
            network.ssid.clone(),
            Arc::new(linux_hotspot::ExistingWifiManager::new(
                self.runner.clone(),
                interface.into(),
                network.ssid.clone(),
                network.password.clone(),
            )),
        ))
    }
}
pub(crate) fn shared_network<'a>(
    saved: &'a [SharedWifiNetwork],
    rows: &str,
) -> Option<&'a SharedWifiNetwork> {
    let rows = rows
        .lines()
        .map(provision::terse_fields)
        .collect::<Vec<_>>();
    if let Some(active) = rows.iter().find(|f| f.len() == 2 && f[0] == "*") {
        saved.iter().find(|n| n.ssid == active[1])
    } else {
        saved
            .iter()
            .find(|n| rows.iter().any(|f| f.len() == 2 && f[1] == n.ssid))
    }
}
impl<C: NetworkManagerCli> StationBackend for WifiProvisioner<C> {
    fn status(&self) -> WifiClient {
        self.status()
    }
    fn scan(&mut self) -> Result<Vec<WifiNetwork>> {
        self.scan()
    }
    fn join(&mut self, ssid: &str, password: &str) -> Result<()> {
        self.join(ssid, password)
    }
    fn forget(&mut self) -> Result<()> {
        self.forget()
    }
    fn sync(&mut self, networks: &[SharedWifiNetwork]) -> Result<()> {
        self.sync(networks)
    }
    fn start(&mut self) -> Result<()> {
        self.start()
    }
    fn stop(&mut self) -> Result<()> {
        self.stop()
    }
    fn stop_setup(&mut self) -> Result<()> {
        self.stop_setup()
    }
    fn start_with_network(&mut self, ssid: &str, password: &str) -> Result<()> {
        self.start_with_network(ssid, password)
    }
    fn hold_for_carlink(&mut self, held: bool) -> bool {
        self.hold_for_carlink(held)
    }
    fn carlink_network(&self, ssid: &str) -> Result<SharedWifiNetwork> {
        self.carlink_network(ssid)
    }
    fn current_ssid(&self) -> Option<String> {
        self.current_ssid()
    }
    fn notices(&self) -> &[String] {
        self.notices()
    }
    fn setup_ssid(&self) -> &str {
        self.setup_ssid()
    }
}
struct BluezBackend {
    control: BtmgmtControl,
    bluetooth: LinuxBluetooth,
    selection: AdapterSelection,
    pairing: Arc<dyn PairingBackend>,
}
impl BluetoothBackend for BluezBackend {
    fn session(&self) -> Arc<dyn BluetoothBackend> {
        Arc::new(Self {
            bluetooth: LinuxBluetooth::new(self.control.0.clone(), None, None)
                .with_adapter_selection(self.selection.clone()),
            control: BtmgmtControl(self.control.0.clone()),
            selection: self.selection.clone(),
            pairing: self.pairing.clone(),
        })
    }
    fn check_radio(&self) -> Result<()> {
        self.bluetooth.check_radio()
    }
    fn power_on(&self) -> Result<()> {
        self.bluetooth.power_on()
    }
    fn register_profiles(&self) -> Result<()> {
        self.bluetooth.register_profiles()
    }
    fn devices(&self) -> Result<Vec<BondedDevice>> {
        self.bluetooth.devices()
    }
    fn select(&self, configured: Option<&str>) -> Result<BondedDevice> {
        linux_bluetooth::select_iphone(&self.bluetooth.devices()?, configured)
    }
    fn local_address(&self) -> Result<String> {
        self.bluetooth.local_address()
    }
    fn channel(&self, address: &str, channel: Option<u8>) -> Result<u8> {
        if !linux_bluetooth::valid_mac(address) {
            return Err(std::io::Error::other("Invalid iPhone address"));
        }
        match channel {
            Some(n) => Ok(n),
            None => self.bluetooth.channel(address),
        }
    }
    fn set_device_class(&self, class: u32) -> Result<()> {
        let adapter = self
            .selection
            .resolved()
            .ok_or_else(|| std::io::Error::other("Bluetooth adapter unavailable"))?;
        linux_bluetooth::set_device_class(self.control.0.as_ref(), &adapter.name, class)
    }
    fn selected_adapter(&self) -> Option<linux_bluetooth::SelectedAdapter> {
        self.selection.resolved()
    }
    fn pairing(&self) -> Arc<dyn PairingBackend> {
        self.pairing.clone()
    }
}

impl SecureConnectionsControl for BluezBackend {
    fn set_secure_connections(
        &self,
        index: u16,
        enabled: bool,
    ) -> std::result::Result<(), secure_connections::Failure> {
        self.control.set_secure_connections(index, enabled)
    }
}
