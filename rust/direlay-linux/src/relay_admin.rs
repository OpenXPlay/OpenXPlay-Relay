// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::carplay_controller::CarPlayController;
use crate::linux_wired_controller::LinuxWiredController;
use crate::linux_wired_platform::LinuxWiredPlatform;
use crate::Result;
use crate::*;
use crate::{
    admin::*,
    linux_hotspot::DEFAULT_HOTSPOT_PASSWORD,
    provision::*,
    settings::{Settings, SharedWifiNetwork},
    supervisor::*,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
#[path = "carlink_follow.rs"]
pub(crate) mod carlink_follow;
#[path = "carlink_vehicle.rs"]
mod carlink_vehicle;
use crate::runtime_paths::InventoryRefresh;
use carlink_follow::CarlinkFollow;
type Provision = Box<dyn crate::host_backends::StationBackend>;
/// The admin page opens Bluetooth pairing for ten minutes.
pub const PAIRING_SECONDS: u32 = 600;
enum Operation {
    Start,
    Stop,
    Restart,
    Forget,
    ForgetCarlink,
    ClearSavedHeadUnit,
    RegeneratePairingCode,
    ConnectCarlink(Option<ArmedPin>),
    Join(String, String),
    /// The startup provisioning: like `Initialize`, but in device-hotspot mode the controller
    /// starts first and only its AP waits for the provisioning.
    Startup,
    Initialize,
    Outputs(Option<Box<Operation>>),
    /// Apply the complete receiver TLS set, coalescing certificate and address changes.
    ReceiverTls,
    Carlink,
    Hotspot(Option<Box<Operation>>),
    Roles(Vec<String>),
    Network {
        mode_changed: bool,
        reconnect: bool,
    },
}
impl Operation {
    /// The name the stall watchdog reports for a blocked admin worker.
    fn name(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Forget => "forget_wifi",
            Self::ForgetCarlink => "forget_carlink",
            Self::ClearSavedHeadUnit => "clear_head_unit",
            Self::RegeneratePairingCode => "pairing_code",
            Self::ConnectCarlink(_) => "connect_carlink",
            Self::Join(..) => "join_wifi",
            Self::Startup => "startup",
            Self::Initialize => "initialize",
            Self::Outputs(_) => "outputs",
            Self::ReceiverTls => "receiver_tls",
            Self::Carlink => "carlink",
            Self::Hotspot(_) => "hotspot",
            Self::Roles(_) => "network_roles",
            Self::Network { .. } => "network",
        }
    }
}
struct ArmedPin {
    pin: String,
    expires: std::time::Instant,
}
fn expire_pin(pin: &mut Option<ArmedPin>) {
    if pin
        .as_ref()
        .is_some_and(|p| p.expires <= std::time::Instant::now())
    {
        *pin = None;
    }
}
fn apply_carlink_event(
    status: &mut AdminCarlink,
    armed: &mut Option<ArmedPin>,
    state: &std::path::Path,
    event: &serde_json::Value,
) -> Option<serde_json::Value> {
    let _ = state;
    expire_pin(armed);
    status.apply_event(event, crate::admin::auth::monotonic_seconds());
    let response = if (event["type"] == "pin-request"
        || event["type"] == "receiver-started" && status.pin_requested)
        && !status.pin_confirmation
    {
        armed.take().and_then(|pin| status.pin_request.map(|request| serde_json::json!({"type":"clink-pairing","request":request,"action":"pin","pin":pin.pin})))
    } else {
        None
    };
    if carlink_attempt_failed(event)
        || (event["type"] == "state"
            && matches!(status.state.as_str(), "playing" | "active" | "connected"))
    {
        *armed = None;
    }
    response
}
fn carlink_event_sink(
    carlink: Arc<Mutex<AdminCarlink>>,
    armed: Arc<Mutex<Option<ArmedPin>>>,
    state_dir: std::path::PathBuf,
    events: Arc<JsonEvents>,
    controls: Arc<Mutex<Option<ControlPipe>>>,
) -> EventSink {
    Arc::new(move |v| {
        let v = events.filtered(&v);
        let response = if controls.lock().unwrap().is_some() {
            apply_carlink_event(
                &mut carlink.lock().unwrap(),
                &mut armed.lock().unwrap(),
                &state_dir,
                &v,
            )
        } else {
            carlink
                .lock()
                .unwrap()
                .apply_event(&v, crate::admin::auth::monotonic_seconds());
            expire_pin(&mut armed.lock().unwrap());
            None
        };
        if let Some(response) = response {
            if controls
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|p| p.send_control(&response).is_ok())
            {
                carlink.lock().unwrap().clear_pin();
            }
        }
        let name = v["type"]
            .as_str()
            .or_else(|| v["event"].as_str())
            .unwrap_or("diagnostic");
        events.emit(name, &v);
    })
}
/// Browser diagnostics stay in memory for the admin page; per-second stats never reach the log.
fn receiver_event(
    receivers: &crate::receiver_diagnostics::ReceiverDiagnostics,
    event: &serde_json::Value,
) -> bool {
    receivers.observe(event, std::time::Instant::now()) && event["type"] == "stats"
}
fn likely_iphone(device: &BondedDevice) -> bool {
    device.iap2 || device.phone_class || device.name.to_lowercase().contains("iphone")
}
pub struct LinuxRelayAdmin {
    log_exports: crate::logs::Service,
    release_checks: crate::update_check::Checker,
    /// The open pairing window and when it closes, in `auth::monotonic_seconds`.
    pairing: Mutex<Option<(crate::linux_bluetooth::PairingMode, u64)>>,
    /// BlueZ for the admin pairing window; a fake in tests.
    pairing_backend: Arc<dyn crate::linux_bluetooth::PairingBackend>,
    /// Secure Connections on, and off only while a pairing window is open.
    secure_connections: Arc<crate::secure_connections::SecureConnections>,
    options: Options,
    backends: crate::host_backends::HostBackends,
    capabilities: Arc<crate::host_capabilities::CapabilityCache>,
    inventory: Arc<crate::net_inventory::InventoryCache>,
    runtime: crate::runtime_paths::RuntimePaths,
    admin_hotspot: Arc<crate::admin_hotspot::AdminHotspot>,
    wifi_country: crate::wifi_country::WifiCountry,
    automatic_choices: Mutex<crate::network_roles::AutomaticChoices>,
    running_links: Mutex<Option<crate::network_roles::RunningLinks>>,
    browser_target: Arc<Mutex<Option<String>>>,
    settings: Arc<Mutex<Settings>>,
    state: Arc<PersistentState>,
    events: Arc<JsonEvents>,
    provision: Arc<Mutex<Option<Provision>>>,
    provision_interface: Mutex<String>,
    net_root: std::path::PathBuf,
    controller: Mutex<Option<Arc<dyn CarPlayController>>>,
    hotspot_lifetime: Arc<crate::hotspot_lifetime::HotspotLifetime>,
    /// BlueZ changes for every controller's Bluetooth waits; the bus listener outlives them.
    bluetooth_wake: Arc<crate::bluetooth_wake::BluetoothWake>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    operation_worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    supervisor: Mutex<Option<RelaySupervisor>>,
    /// Outlives the supervisors that output changes, Start and Restart replace.
    carlink_backoff: Mutex<CarlinkBackoff>,
    carlink_hevc_disabled: Arc<AtomicBool>,
    child_factory: Arc<dyn ChildFactory>,
    input: Arc<DiPlayInput>,
    stream: Arc<StreamConnection>,
    follow: Arc<Mutex<CarlinkFollow>>,
    network_override: Mutex<Option<SharedWifiNetwork>>,
    lifecycle: Mutex<()>,
    sink: Arc<RecordingMediaSink>,
    desired: Arc<AtomicBool>,
    auto_start_gate: Mutex<crate::start_policy::AutoStartGate>,
    arrival: Mutex<crate::start_policy::Arrival>,
    recovery: Mutex<crate::start_policy::RecoverOnce>,
    start_policy: Arc<Mutex<crate::start_policy::StartPolicy>>,
    operations: std::sync::mpsc::SyncSender<Operation>,
    pending: Mutex<HashMap<std::thread::ThreadId, Operation>>,
    operation_rx: Mutex<Option<std::sync::mpsc::Receiver<Operation>>>,
    busy: Arc<AtomicBool>,
    closed: AtomicBool,
    restart_required: AtomicBool,
    apply_pending: AtomicBool,
    carlink: Arc<Mutex<AdminCarlink>>,
    armed_pin: Arc<Mutex<Option<ArmedPin>>>,
    vehicle: Arc<Mutex<carlink_vehicle::Recorder>>,
    receivers: Arc<crate::receiver_diagnostics::ReceiverDiagnostics>,
    trace_status: Arc<crate::admin::server::trace_files::TraceStatus>,
    /// Set when an admin operation finished: the main loop ticks at once.
    tick_wake: (Mutex<bool>, std::sync::Condvar),
    /// Held while a provisioning step must finish before the controller's AP starts.
    provisioning: Arc<crate::provision::ProvisioningGate>,
    /// Trusted HTTPS with a free domain name; off without `public-name.json`.
    public_name: Arc<crate::public_name::PublicNameService>,
    identity_imports: Mutex<crate::identity::Imports>,
    user_tls_lock: Mutex<()>,
    receiver_default_tls: Arc<Mutex<Option<crate::identity::tls::DefaultTls>>>,
    /// The admin page's certificate for that name, chosen by SNI.
    admin_certificate: crate::admin::server::NamedCertificate,
    /// What the running head-unit page serves; `None` while there is no web output.
    receiver_tls: Mutex<Option<Option<crate::public_name::Served>>>,
    /// Self-signed IP SANs frozen with the running child (up to 16, retaining old networks).
    receiver_tls_sans: Mutex<Vec<std::net::Ipv4Addr>>,
    /// The public-name generation the admin certificate follows.
    public_name_generation: std::sync::atomic::AtomicU64,
    receiver_tls_queued: AtomicBool,
    tesla: Mutex<crate::tesla_address::Session>,
    /// Selected direct Tesla SAN covered by the currently spawned receiver.
    receiver_tesla_tls: Mutex<Option<std::net::Ipv4Addr>>,
}
/// The relay as the trusted-HTTPS worker sees it.
struct RelayHost(std::sync::Weak<LinuxRelayAdmin>);
impl crate::public_name::Host for RelayHost {
    fn address(&self) -> Option<std::net::Ipv4Addr> {
        self.0.upgrade()?.head_unit_address()
    }
}
impl LinuxRelayAdmin {
    pub fn new(
        options: Options,
        state: Arc<PersistentState>,
        events: Arc<JsonEvents>,
    ) -> Result<Self> {
        let runtime = crate::runtime_paths::RuntimePaths::new(&options.state_dir);
        let backends = crate::host_backends::HostBackends::linux_with_runtime_paths(
            &options,
            runtime.clone(),
            events.clone(),
        );
        let provision = match std::fs::read_to_string(
            runtime
                .root
                .join("sys/class/net")
                .join(&options.wifi_interface)
                .join("address"),
        ) {
            Ok(mac) => Some(backends.station.saved(
                &options.wifi_interface,
                mac.trim(),
                &options.state_dir,
            )?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let admin = Self::with_host_backends(options, state, events, provision, backends, runtime)?;
        // Without BlueZ changes the controller's Bluetooth waits only poll.
        let signals = crate::linux_bluetooth::BluezSignals::new(admin.events.clone());
        if let Err(e) = admin.bluetooth_wake.listen(&signals) {
            admin
                .events
                .log(&format!("bluetooth-events unavailable: {e}; waits poll"));
        }
        Ok(admin)
    }
    #[cfg(test)]
    fn with_provisioner(
        options: Options,
        state: Arc<PersistentState>,
        events: Arc<JsonEvents>,
        provision: WifiProvisioner<ProcessNetworkManagerCli>,
        runtime: crate::runtime_paths::RuntimePaths,
    ) -> Result<Self> {
        let provision = provision.with_local_dns(runtime.managed_dns());
        Self::with_optional_provisioner(options, state, events, Some(Box::new(provision)), runtime)
    }
    #[cfg(test)]
    fn with_optional_provisioner(
        options: Options,
        state: Arc<PersistentState>,
        events: Arc<JsonEvents>,
        provision: Option<Provision>,
        runtime: crate::runtime_paths::RuntimePaths,
    ) -> Result<Self> {
        let backends = crate::host_backends::HostBackends::linux_with_runtime_paths(
            &options,
            runtime.clone(),
            events.clone(),
        );
        Self::with_host_backends(options, state, events, provision, backends, runtime)
    }
    fn with_host_backends(
        options: Options,
        state: Arc<PersistentState>,
        events: Arc<JsonEvents>,
        provision: Option<Provision>,
        backends: crate::host_backends::HostBackends,
        runtime: crate::runtime_paths::RuntimePaths,
    ) -> Result<Self> {
        let input = Arc::new(DiPlayInput::with_plugins(
            events.clone(),
            options.settings.plugins,
        ));
        let stream =
            StreamConnection::new(options.stream_socket.clone(), input.clone(), events.clone());
        let sink = Arc::new(match &options.record_dir {
            Some(d) => RecordingMediaSink::new(d.clone(), events.clone())?,
            None => RecordingMediaSink::discard(events.clone()),
        });
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        let settings = Settings::load_with_defaults(
            &options.state_dir,
            &options.host_profile.profile.settings_defaults(),
        )?;
        if !settings.unknown.is_empty() {
            let keys = settings.unknown.keys().cloned().collect::<Vec<_>>();
            events.log(&format!(
                "settings unknown fields kept: {}",
                keys.join(", ")
            ));
        }
        input.initialize_video_state(&options.state_dir);
        input.configure_runtime_settings(&settings)?;
        {
            let mut display = input.display.lock().unwrap();
            display.disable_view_areas = !settings.car_play_view_areas;
            display
                .resize
                .set_enabled(settings.carlink_reconnect_on_resize);
        }
        // The AP stays down for a gap before the next one comes up, also across relay restarts.
        let hotspot_lifetime = Arc::new(crate::hotspot_lifetime::HotspotLifetime::with_down_gap(
            Duration::from_secs(options.hotspot_down_gap_seconds()),
            Some(
                options
                    .state_dir
                    .join(crate::hotspot_lifetime::AP_DOWN_RECORD),
            ),
        ));
        if let Some(error) = &options.host_profile.error {
            events.log(&format!("host-profile ignored: {error}"));
        } else if !options.host_profile.unknown.is_empty() {
            events.log(&format!(
                "host-profile unknown fields ignored: {}",
                options.host_profile.unknown.join(", ")
            ));
        }
        let selection = crate::linux_bluetooth::AdapterSelection(
            options.host_profile.profile.bluetooth_adapter.clone(),
        );
        let capabilities = Arc::new(crate::host_capabilities::CapabilityCache::new(
            runtime.runner.clone(),
            runtime.root.clone(),
            selection.clone(),
        ));
        let sc_backend = backends.bluetooth.clone();
        let secure_connections = Arc::new(
            crate::secure_connections::SecureConnections::new(
                runtime.runner.clone(),
                events.clone(),
            )
            .with_control(backends.bluetooth.clone())
            .with_adapter_index(Arc::new(move || {
                sc_backend
                    .selected_adapter()
                    .and_then(|a| crate::host_profile::adapter_name_index(&a.name))
            }))
            .with_policy(options.host_profile.profile.secure_connections),
        );
        let public_name = Arc::new(crate::public_name::PublicNameService::new(
            &options.state_dir,
            crate::public_name::Network::internet(runtime.managed_dns()),
        ));
        let inventory = Arc::new(
            crate::net_inventory::InventoryCache::new(runtime.runner.clone(), runtime.root.clone())
                .with_addresses(runtime.network_address_source())
                .with_refresh(runtime.inventory_refresh != InventoryRefresh::Manual),
        );
        if runtime.inventory_refresh == InventoryRefresh::Automatic {
            inventory.refresh();
        }
        let auto_start_gate =
            crate::start_policy::AutoStartGate::load(&options.state_dir, &runtime.root)?;
        let desired = settings.boot_start && !auto_start_gate.paused();
        events.failure_secrets(failure_secrets(&settings, &options.state_dir));
        events
            .diagnostics
            .set_enabled(settings.effective_experimental().stall_watchdog);
        Ok(Self {
            auto_start_gate: Mutex::new(auto_start_gate),
            arrival: Mutex::default(),
            recovery: Mutex::default(),
            log_exports: crate::logs::Service::new(options.state_dir.clone()),
            release_checks: crate::update_check::Checker::default(),
            admin_hotspot: Arc::new(
                crate::admin_hotspot::AdminHotspot::new(
                    options.state_dir.clone(),
                    Duration::from_secs(options.hotspot_down_gap_seconds()),
                    runtime.runner.clone(),
                )
                .with_runtime_paths(runtime.clone()),
            ),
            wifi_country: crate::wifi_country::WifiCountry::new(runtime.runner.clone()),
            inventory,
            automatic_choices: Mutex::default(),
            running_links: Mutex::new(None),
            browser_target: Arc::default(),
            public_name,
            identity_imports: Mutex::default(),
            user_tls_lock: Mutex::default(),
            receiver_default_tls: Arc::default(),
            admin_certificate: Default::default(),
            receiver_tls: Mutex::new(None),
            receiver_tls_sans: Mutex::new(Vec::new()),
            public_name_generation: std::sync::atomic::AtomicU64::new(u64::MAX),
            receiver_tls_queued: AtomicBool::new(false),
            tesla: Mutex::new(
                crate::tesla_address::Session::new(Arc::new(crate::tesla_address::KernelAddresses))
                    .with_ownership(runtime.tesla_address.clone()),
            ),
            receiver_tesla_tls: Mutex::new(None),
            pairing: Mutex::new(None),
            pairing_backend: backends.bluetooth.pairing(),
            backends,
            capabilities,
            secure_connections,
            provision_interface: Mutex::new(options.wifi_interface.clone()),
            net_root: runtime.root.join("sys/class/net"),
            runtime,
            vehicle: Arc::new(Mutex::new(carlink_vehicle::Recorder::new(
                &options.state_dir,
                options.settings.vehicle_information,
            ))),
            options,
            settings: Arc::new(Mutex::new(settings.clone())),
            state,
            events,
            provision: Arc::new(Mutex::new(provision)),
            controller: Mutex::new(None),
            hotspot_lifetime,
            bluetooth_wake: Arc::default(),
            worker: Mutex::new(None),
            operation_worker: Mutex::new(None),
            supervisor: Mutex::new(None),
            carlink_backoff: Mutex::default(),
            carlink_hevc_disabled: Arc::new(AtomicBool::new(false)),
            child_factory: Arc::new(ProcessChildFactory),
            input,
            stream: Arc::new(stream),
            follow: Arc::new(Mutex::new(CarlinkFollow::default())),
            network_override: Mutex::new(None),
            lifecycle: Mutex::new(()),
            sink,
            desired: Arc::new(AtomicBool::new(desired)),
            start_policy: Arc::default(),
            operations: tx,
            pending: Mutex::new(HashMap::new()),
            operation_rx: Mutex::new(Some(rx)),
            busy: Arc::new(AtomicBool::new(false)),
            closed: AtomicBool::new(false),
            restart_required: AtomicBool::new(false),
            apply_pending: AtomicBool::new(false),
            armed_pin: Arc::new(Mutex::new(None)),
            receivers: Arc::default(),
            trace_status: Arc::default(),
            tick_wake: Default::default(),
            provisioning: Arc::default(),
            carlink: Arc::new(Mutex::new(AdminCarlink {
                state: "idle".into(),
                ..Default::default()
            })),
        })
    }
    fn wifi_present(&self, interface: &str) -> bool {
        self.net_root.join(interface).join("address").exists()
    }
    fn prepare_wifi(&self) -> Result<bool> {
        let options = self.effective_options()?;
        let mut provision = self.provision.lock().unwrap();
        let mut interface = self.provision_interface.lock().unwrap();
        if *interface != options.wifi_interface {
            if let Some(p) = provision.as_mut().filter(|_| self.wifi_present(&interface)) {
                p.stop()?;
            }
            *provision = None;
            *interface = options.wifi_interface.clone();
        }
        if provision.is_none() {
            let mac = match std::fs::read_to_string(
                self.net_root.join(&options.wifi_interface).join("address"),
            ) {
                Ok(mac) => mac,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(e) => return Err(e),
            };
            *provision = Some(self.backends.station.saved(
                &options.wifi_interface,
                mac.trim(),
                &options.state_dir,
            )?);
        }
        Ok(true)
    }
    fn network_status(&self, settings: &Settings) -> crate::network_roles::NetworkStatus {
        self.network_status_with_choices(settings, &mut self.automatic_choices.lock().unwrap())
    }
    fn network_status_with_choices(
        &self,
        settings: &Settings,
        choices: &mut crate::network_roles::AutomaticChoices,
    ) -> crate::network_roles::NetworkStatus {
        let mut inventory = self.inventory.get();
        inventory.country.configured = Some(settings.wifi_country.clone());
        let mut options = self.options.clone();
        options.settings = settings.clone();
        let _ = options.apply_overrides();
        let legacy = crate::network_roles::Legacy {
            wifi_interface: &options.wifi_interface,
            carlink_interface: &options.settings.carlink_interface,
            network_mode: &options.settings.network_mode,
            hotspot_band: &options.settings.hotspot_band,
            hotspot_channel: options.wifi_channel,
            cli: &options.overrides,
        };
        let preset = options.device_preset(self.capabilities.root());
        let plan = choices.resolve(
            &inventory,
            settings,
            &legacy,
            &preset.id,
            &options.settings.output,
        );
        let probing = inventory.probed_at.is_none();
        let checks = if probing {
            Vec::new()
        } else {
            crate::network_roles::checks(&plan, &inventory)
        };
        crate::network_roles::NetworkStatus {
            inventory,
            plan,
            checks,
            applied: !probing,
            probing,
        }
    }
    fn propose_roles(&self, values: Values, save: bool) -> crate::network_roles::SaveResult {
        use crate::network_roles::{Proposal, SaveResult, Update};
        if self.inventory.get().probed_at.is_none() {
            return SaveResult::Probing;
        }
        if ["adminHotspotPassword", "wifiCountry"]
            .iter()
            .any(|key| values.get(*key).is_some_and(|v| !v.is_string()))
        {
            return SaveResult::Invalid("error.invalid".into());
        }
        let update: Update = match serde_json::from_value(serde_json::Value::Object(values)) {
            Ok(update) => update,
            Err(_) => return SaveResult::Invalid("error.invalid".into()),
        };
        let mut current = self.settings.lock().unwrap();
        let mut next = current.clone();
        next.network_roles = update.network_roles;
        if let Some(password) = update.admin_hotspot_password {
            if !crate::network_roles::valid_password(&password) {
                return SaveResult::Checks(vec![crate::network_roles::Check {
                    level: "error".into(),
                    code: "admin-hotspot-password".into(),
                    roles: vec!["adminHotspot".into()],
                    interfaces: Vec::new(),
                    params: serde_json::json!({}),
                }]);
            }
            next.admin_hotspot_password = Some(password);
        }
        if let Some(country) = update.wifi_country {
            next.wifi_country = country;
        }
        if next.validate().is_err() {
            return SaveResult::Invalid("error.invalid".into());
        }
        let before = self.network_status(&current);
        // Editing only admin settings must not reassign the running phone or Carlink links.
        let mut old_roles = current.network_roles.clone();
        old_roles.admin_hotspot = next.network_roles.admin_hotspot.clone();
        let admin_or_country_changed = current.network_roles.admin_hotspot
            != next.network_roles.admin_hotspot
            || current.admin_hotspot_password != next.admin_hotspot_password
            || current.wifi_country != next.wifi_country;
        let same_links = old_roles == next.network_roles
            || (current.network_roles.mode == "auto" && next.network_roles.mode == "auto");
        let mut choices = if same_links && admin_or_country_changed {
            self.automatic_choices.lock().unwrap().clone()
        } else {
            crate::network_roles::AutomaticChoices::default()
        };
        let status = self.network_status_with_choices(&next, &mut choices);
        let mut changes = crate::network_roles::changes(&before.plan, &status.plan);
        if next.wifi_country != current.wifi_country {
            changes.push("wifiCountry".into());
        }
        if next.admin_hotspot_password != current.admin_hotspot_password
            && !changes.iter().any(|c| c == "adminHotspot")
        {
            changes.push("adminHotspot".into());
        }
        if save {
            if status.checks.iter().any(|c| c.level == "error") {
                return SaveResult::Checks(status.checks);
            }
            if !changes.is_empty() && self.defer(Operation::Roles(changes.clone())).is_err() {
                return SaveResult::Invalid("error.busy".into());
            }
            if next.save(&self.options.state_dir).is_err() {
                self.response_failed();
                return SaveResult::Storage;
            }
            *current = next;
            *self.automatic_choices.lock().unwrap() = choices;
        }
        SaveResult::Saved(Box::new(Proposal {
            plan: status.plan,
            checks: status.checks,
            changes,
            applied: save.then_some(true),
        }))
    }
    fn validate_hotspot_security_change(
        &self,
        current: &Settings,
        next: &Settings,
    ) -> std::result::Result<(), BTreeMap<String, String>> {
        if next.hotspot_security != current.hotspot_security
            && !crate::linux_hotspot::security_supported(
                &next.hotspot_security,
                &self.supported_hotspot_security(&next.wifi_interface),
            )
        {
            return Err(BTreeMap::from([(
                "hotspotSecurity".into(),
                "error.hotspotSecurity".into(),
            )]));
        }
        Ok(())
    }
    pub fn receiver_port(&self) -> Result<u16> {
        Ok(self.effective_options()?.settings.receiver_port)
    }
    fn supported_hotspot_security(&self, interface: &str) -> Vec<String> {
        self.capabilities
            .cached_wifi(interface)
            .map(|c| c.hotspot_security)
            .filter(|values| !values.is_empty())
            .unwrap_or_else(|| vec!["open".into(), "wpa2".into()])
    }
    fn effective_options(&self) -> Result<Options> {
        Ok(self.effective_options_with_status()?.0)
    }
    fn effective_options_with_status(
        &self,
    ) -> Result<(Options, crate::network_roles::NetworkStatus)> {
        let mut o = self.options.clone();
        o.settings = self.settings.lock().unwrap().effective_developer();
        o.apply_overrides()?;
        let status = self.network_status(&o.settings);
        if status.probing {
            *self.browser_target.lock().unwrap() = Some(o.wifi_interface.clone());
        } else {
            crate::network_roles::apply_plan(&status.plan, &mut o);
            *self.browser_target.lock().unwrap() =
                status.plan.car_browser.role.name().map(str::to_string);
        }
        Ok((o, status))
    }
    fn links(
        &self,
        options: &Options,
        status: &crate::network_roles::NetworkStatus,
    ) -> crate::network_roles::RunningLinks {
        if status.probing {
            crate::network_roles::RunningLinks::legacy(options, |n| self.wifi_present(n))
        } else {
            crate::network_roles::RunningLinks::from_plan(&status.plan, |n| self.wifi_present(n))
        }
    }
    pub fn start_services(self: &Arc<Self>) -> Result<()> {
        self.start_services_with_probe_timeout(Duration::from_secs(10))
    }
    fn start_services_with_probe_timeout(self: &Arc<Self>, timeout: Duration) -> Result<()> {
        self.apply_wifi_country();
        if self.runtime.network_services {
            crate::public_name::local_dns::reset(&self.runtime.local_dns)?;
            self.admin_hotspot.prepare()?;
        }
        self.inventory.wait_for_probe(timeout);
        let mut settings = self.effective_options()?.settings;
        if let Some((interface, _)) = self.active_hotspot() {
            settings.wifi_interface = interface;
        }
        self.tesla.lock().unwrap().cleanup_stale(&settings);
        // Secure Connections on from the start, and again whenever bluetoothd resets the adapter.
        self.secure_connections.start(Some(&self.bluetooth_wake))?;
        self.stream.start()?;
        // Trusted HTTPS: without public-name.json the worker only watches for one.
        self.public_name
            .start(Arc::new(RelayHost(Arc::downgrade(self))))?;
        let shell = self.clone();
        let rx = self
            .operation_rx
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| std::io::Error::other("Services already started"))?;
        *self.operation_worker.lock().unwrap() = Some(crate::platform::spawn_worker(
            "relay-admin-actions",
            Box::new(move || {
                while !shell.closed.load(Ordering::Acquire) {
                    expire_pin(&mut shell.armed_pin.lock().unwrap());
                    let op = match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(op) => Some(op),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                        Err(_) => break,
                    };
                    if op.is_none() && !shell.follow.lock().unwrap().ready() {
                        continue;
                    }
                    let _progress = shell.events.diagnostics.admin.enter(
                        op.as_ref().map_or("follow_over", Operation::name),
                        crate::logs::diagnostics::OPERATION_BUDGET,
                    );
                    shell.busy.store(true, Ordering::Release);
                    let lifecycle = shell.lifecycle.lock().unwrap();
                    if let Err(e) = shell.reconcile_carlink_network() {
                        shell
                            .events
                            .emit("error", &serde_json::json!({"message":e.to_string()}));
                    }
                    let operated = op.is_some();
                    if let Some(op) = op {
                        let network = matches!(
                            op,
                            Operation::Join(..)
                                | Operation::Network { .. }
                                | Operation::Initialize
                                | Operation::Startup
                        );
                        let result = shell.operation(op);
                        if let Err(e) = result {
                            shell.events.emit("error", &serde_json::json!({"message": if network { "error.networks".to_string() } else { e.to_string() }}));
                        }
                    }
                    drop(lifecycle);
                    shell.busy.store(false, Ordering::Release);
                    if operated {
                        shell.wake_tick();
                    }
                }
            }),
        )?);
        self.busy.store(true, Ordering::Release);
        self.operations
            .try_send(Operation::Startup)
            .map_err(std::io::Error::other)?;
        self.start_supervisor()?;
        Ok(())
    }
    fn start_supervisor(&self) -> Result<()> {
        let (mut o, status) = self.effective_options_with_status()?;
        self.input.configure_runtime_settings(&o.settings)?;
        if o.settings.experimental_bluetooth_address.is_empty() {
            o.settings.experimental_bluetooth_address = self
                .backends
                .bluetooth
                .selected_adapter()
                .map(|a| a.address)
                .unwrap_or_default();
        }
        let running_links = self.links(&o, &status);
        if !o.shared_carlink_radio() {
            self.reconcile_carlink_network()?;
        }
        self.input.set_plugins_enabled(o.settings.plugins);
        let addresses = self.receiver_sans()?;
        let occupied = self
            .runtime
            .network_addresses()?
            .iter()
            .map(|(_, a, p)| format!("{a}/{p}"))
            .collect::<Vec<_>>();
        let ble = o.settings.output != "web" && o.settings.carlink_link == "ble";
        let ble_adapter = if ble {
            self.backends
                .bluetooth
                .selected_adapter()
                .map(|a| a.name)
                .or_else(|| {
                    o.host_profile
                        .profile
                        .bluetooth_adapter
                        .as_deref()
                        .and_then(crate::host_profile::adapter_name_index)
                        .map(|n| format!("hci{n}"))
                })
        } else {
            None
        };
        let pairing_id = if ble {
            self.state.identity()?.pairing_id.clone()
        } else {
            String::new()
        };
        let station_5ghz = ble
            && crate::linux_hotspot::supports_5ghz(
                self.runtime.runner.as_ref(),
                o.carlink_interface(),
            );
        let shared_phy =
            !status.probing && crate::network_roles::shared_radio(&status.inventory, &status.plan);
        let mut settings = crate::supervisor_settings::supervisor_settings(
            &o,
            addresses,
            &occupied,
            pairing_id,
            ble_adapter,
            station_5ghz,
        );
        if let Some(ble) = settings.carlink.as_mut().and_then(|c| c.ble.as_mut()) {
            ble.default_phone_mac = self
                .backends
                .bluetooth
                .selected_adapter()
                .and_then(|adapter| crate::carlink_policy::bluetooth_address(&adapter.address));
        }
        settings.addresses.sort();
        settings.addresses.dedup();
        settings.addresses.truncate(16);
        if !self.wifi_present(o.carlink_interface()) {
            settings.carlink = None;
        }
        if let Some(carlink) = &mut settings.carlink {
            carlink.picture = crate::supervisor_settings::carlink_picture(
                &o.settings,
                self.input.display.lock().unwrap().pictures.trial,
            );
            carlink.hevc = crate::supervisor_settings::carlink_hevc_with_encoder(
                &o.settings,
                &self.input.display.lock().unwrap().saved.video_codec,
                self.input
                    .transcoding
                    .lock()
                    .unwrap()
                    .clink
                    .converts("H265", carlink.picture),
            )
            .into();
        }
        let tesla_tls = crate::tesla_address::tls_address(&o.settings);
        let served = self
            .public_name
            .certificate_snapshot(&self.options.state_dir)?;
        settings.default_tls = crate::identity::tls::snapshot(&self.options.state_dir)?;
        let default_tls = settings.default_tls.clone();
        settings.pem_tls = served.as_ref().map(|s| PemTls {
            name: s.name.clone(),
            cert: s.cert.clone(),
            key: s.key.clone(),
        });
        if served.is_none() {
            for name in ["named-cert.pem", "named-key.pem"] {
                match std::fs::remove_file(self.options.state_dir.join("receiver-tls").join(name)) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
        }
        let receiver_tls = (settings.web_port != 0).then_some(served);
        let receiver_sans = settings.addresses.clone();
        *self.carlink.lock().unwrap() = AdminCarlink {
            state: "waiting".into(),
            ..Default::default()
        };
        let controls = Arc::new(Mutex::new(None));
        let vehicle = self.vehicle.clone();
        let event_sink = carlink_event_sink(
            self.carlink.clone(),
            self.armed_pin.clone(),
            self.options.state_dir.clone(),
            self.events.clone(),
            controls.clone(),
        );
        {
            let mut display = self.input.display.lock().unwrap();
            display
                .resize
                .set_enabled(o.settings.carlink_reconnect_on_resize);
            display.disable_view_areas = !o.settings.car_play_view_areas;
        }
        let input = self.input.clone();
        let shared_radio = o.shared_carlink_radio();
        let follow = self.follow.clone();
        let interface = o.wifi_interface.clone();
        let browser_target = self.browser_target.clone();
        let inventory = self.inventory.clone();
        let carlink_interface = o.carlink_interface().to_string();
        let stream = self.stream.clone();
        let timeline_events = self.events.clone();
        let receivers = self.receivers.clone();
        let web_port = settings.web_port;
        let carlink_retry = settings.carlink.as_ref().is_some_and(|c| c.auto_reconnect);
        {
            let mut facts = self.events.facts.lock().unwrap();
            facts.web.port = web_port;
            facts.web.phase = if web_port == 0 { "OFF" } else { "STARTING" }.into();
            facts.carlink.phase = if settings.carlink.is_some() {
                "SCANNING"
            } else {
                "OFF"
            }
            .into();
        }
        let trace_status = self.trace_status.clone();
        let hotspot = self.hotspot_lifetime.clone();
        let admin_hotspot = self.admin_hotspot.clone();
        let addresses = self.runtime.network_address_source();
        let receiver_default_tls = self.receiver_default_tls.clone();
        let launched_default_tls = default_tls.clone();
        let tls_state = self.options.state_dir.clone();
        *self.receiver_default_tls.lock().unwrap() = None;
        let capability_settings = self.settings.clone();
        let capability_tx = self.operations.clone();
        let event_sink: EventSink = Arc::new(move |mut event| {
            if event["type"] == "tls-fingerprint" {
                if let Some(base) = &launched_default_tls {
                    if event["defaultFingerprint"].as_str() == Some(&base.fingerprint)
                        && event["fingerprint"].as_str() == Some(&base.fingerprint)
                        && crate::identity::tls::snapshot(&tls_state)
                            .ok()
                            .flatten()
                            .as_ref()
                            == Some(base)
                    {
                        *receiver_default_tls.lock().unwrap() = Some(base.clone());
                    }
                }
            }
            if event["type"] == "receiver-starting" {
                *input.transcoding.lock().unwrap() = Default::default();
                input.display.lock().unwrap().transcode_geometry = false;
            }
            if event["type"] == "transcode-capabilities" {
                let effective = capability_settings.lock().unwrap().effective_developer();
                let picture = crate::supervisor_settings::carlink_picture(
                    &effective,
                    input.display.lock().unwrap().pictures.trial,
                );
                let mut caps = input.transcoding.lock().unwrap();
                let previous = caps.clink.converts("H265", picture);
                if caps.observe(&event) && event["output"] == "clink" {
                    let encoder = caps.clink.converts("H265", picture);
                    let geometry = caps.clink.geometry(picture);
                    drop(caps);
                    input.display.lock().unwrap().transcode_geometry = geometry;
                    if previous != encoder && effective.output != "web" {
                        let _ = capability_tx.try_send(Operation::Carlink);
                    }
                }
            }
            if vehicle
                .lock()
                .unwrap()
                .observe(&event, std::time::Instant::now())
            {
                return;
            }
            if event["type"] == "pairing-url" {
                let rows = addresses().unwrap_or_default();
                if let Some(target) = browser_target.lock().unwrap().as_deref() {
                    prefer_pairing_address(&mut event, target, &rows);
                }
            }
            if trace_status.observe(&event) {
                return;
            }
            input.receiver_playback(&event);
            {
                let now = crate::platform::wall_clock().as_millis() as u64;
                let summary = {
                    let mut facts = timeline_events.facts.lock().unwrap();
                    if web_port != 0 {
                        let phase = match event["type"].as_str() {
                            Some("pairing-url") => Some("LISTENING"),
                            Some("receiver-starting") => Some("STARTING"),
                            Some("receiver-error") => Some("FAILED"),
                            Some("receiver-stopped") => Some("OFF"),
                            _ => None,
                        };
                        if let Some(phase) = phase {
                            if facts.web.phase != phase {
                                facts.web.phase = phase.into();
                                facts.web.since_millis = Some(now);
                            }
                            facts.web.code = (phase == "FAILED").then(|| {
                                if event["message"]
                                    .as_str()
                                    .is_some_and(|s| s.contains("Address already in use"))
                                {
                                    "web_port_busy"
                                } else {
                                    "web_failed"
                                }
                                .into()
                            });
                        }
                    }
                    if event["type"] == "receiver-restarting" && carlink_retry {
                        facts.carlink.retrying = true;
                    }
                    facts.carlink_event(&timeline_events.filtered(&event), now)
                };
                if let Some(summary) = summary {
                    timeline_events.log(&summary);
                }
            }
            timeline_events.log_observation(event["type"].as_str().unwrap_or(""), &event);
            if receiver_event(&receivers, &event) {
                return;
            }
            if event["type"] == "receiver-starting" || event["type"] == "carlink-starting" {
                input.display.lock().unwrap().resize.start_session();
            }
            if event["type"] == "receiver-started" {
                stream.receiver_started();
            } else if event["type"] == "receiver-stopped" {
                stream.receiver_stopped();
            }
            if event["type"] == "clink-view-size" {
                if let Ok(size) = serde_json::from_value(event.clone()) {
                    input.display.lock().unwrap().resize.view_size(size);
                }
            }
            admin_hotspot.observe(&event, &carlink_interface);
            if (event["type"] == "link"
                && matches!(event["state"].as_str(), Some("joined" | "lost")))
                || carlink_follow::carlink_link_left(&event)
            {
                inventory.refresh_now();
            }
            if shared_phy && !shared_radio {
                if event["type"] == "link"
                    && event["state"] == "joined"
                    && event["network"]["interface"] == carlink_interface
                {
                    hotspot.hold_for_carlink(true);
                } else if carlink_follow::carlink_link_left(&event) {
                    hotspot.hold_for_carlink(false);
                }
            }
            follow.lock().unwrap().apply(
                &event,
                &interface,
                shared_radio,
                &timeline_events,
                &hotspot,
            );
            event_sink(event);
        });
        let mut supervisor = RelaySupervisor::new(settings, self.child_factory.clone(), event_sink)
            .with_control_handle(controls)
            .with_carlink_backoff(self.carlink_backoff.lock().unwrap().clone())
            .with_hevc_disabled(self.carlink_hevc_disabled.clone());
        supervisor.start()?;
        *self.receiver_tesla_tls.lock().unwrap() = tesla_tls.filter(|a| receiver_sans.contains(a));
        *self.receiver_tls.lock().unwrap() = receiver_tls;
        *self.receiver_tls_sans.lock().unwrap() = receiver_sans;
        *self.supervisor.lock().unwrap() = Some(supervisor);
        *self.running_links.lock().unwrap() = Some(running_links);
        Ok(())
    }
    fn carplay_options(&self) -> Result<Options> {
        let mut options = self.effective_options()?;
        if let Some(network) = self
            .network_override
            .lock()
            .unwrap()
            .clone()
            .filter(|_| options.shared_carlink_radio())
        {
            options.settings.network_mode = "shared-wifi".into();
            options.settings.shared_wifi_networks = vec![network];
        }
        Ok(options)
    }
    fn stop_controller(&self) {
        self.stop_controller_for_restart(false);
    }
    fn stop_controller_for_restart(&self, keep_hotspot: bool) {
        self.input.expect_stop();
        let settings = self.settings.lock().unwrap().clone();
        let mut tesla = self.tesla.lock().unwrap();
        tesla.stop(crate::platform::wall_clock().as_millis() as u64);
        let changes = tesla.journal_changes(&settings);
        drop(tesla);
        for change in changes {
            self.events.emit("tesla-address", &change);
        }
        self.public_name.published_address_override(None);
        if keep_hotspot {
            self.hotspot_lifetime.keep_for_restart();
        }
        if let Some(c) = self.controller.lock().unwrap().take() {
            let _ = c.close();
        }
        if let Some(w) = self.worker.lock().unwrap().take() {
            let _ = w.join();
        }
        if !keep_hotspot {
            if let Err(e) = self.hotspot_lifetime.close() {
                self.events.log(&format!("Hotspot shutdown failed: {e}"));
            }
        }
        let _ = self.input.stop();
    }
    fn operation(&self, op: Operation) -> Result<()> {
        use crate::logs::diagnostics::{self, WaitGuard};
        let name = op.name();
        let _progress = diagnostics::nested(name, diagnostics::OPERATION_BUDGET);
        let d = &self.events.diagnostics;
        // Start while the Carlink or car-screen budget runs is "Start now", not a new start.
        let start_now =
            matches!(op, Operation::Start) && self.start_policy.lock().unwrap().waiting();
        let request = matches!(op, Operation::Stop | Operation::Restart)
            || matches!(op, Operation::Start) && !start_now;
        if request {
            d.new_request();
            let outcome = if matches!(op, Operation::Stop) {
                "stopped"
            } else {
                "restarted"
            };
            d.start_ended(&self.events, outcome);
        }
        let mut wait = match op {
            Operation::Restart => Some(WaitGuard::new(&self.events, "restart")),
            Operation::Stop => Some(WaitGuard::new(&self.events, "stop")),
            _ => None,
        };
        let result = self.perform_operation(op);
        match (&result, wait.as_mut()) {
            (Ok(()), Some(wait)) => wait.finish("done"),
            (Err(error), _) if request && name != "stop" => {
                d.start_ended(&self.events, "failed");
                let reason = diagnostics::start_failure_reason(error);
                d.refuse(&self.events, reason, serde_json::json!({"action":name}));
            }
            _ => {}
        }
        drop(wait);
        self.inventory.refresh_now();
        result
    }
    // Chained actions share a 256 KiB worker stack; keep their Options snapshots in separate handlers.
    fn perform_operation(&self, op: Operation) -> Result<()> {
        match op {
            Operation::Roles(changes) => self.apply_roles(changes)?,
            Operation::Startup => self.startup()?,
            Operation::Initialize => self.initialize_network()?,
            Operation::Start => {
                if self.start_policy.lock().unwrap().waiting() {
                    self.start_policy.lock().unwrap().skip();
                    self.events.diagnostics.end_each(
                        &self.events,
                        &["carlink", "car_screen"],
                        "start_now",
                    );
                    self.desired.store(true, Ordering::Release);
                    return Ok(());
                }
                self.resume_start()?;
                self.start_policy.lock().unwrap().cancel();
                self.stop_controller();
                self.operation(Operation::Initialize)?;
                self.stop_supervisor();
                self.start_supervisor()?;
                self.desired.store(true, Ordering::Release);
            }
            Operation::Stop => {
                // Persist before changing resources: a failed write must not pretend Stop survived.
                self.auto_start_gate.lock().unwrap().set_paused(true)?;
                self.arrival.lock().unwrap().cancel();
                self.desired.store(false, Ordering::Release);
                self.start_policy.lock().unwrap().cancel();
                self.input.clear_picture_trial();
                self.stop_controller();
            }
            Operation::Hotspot(after) => {
                if self.effective_options()?.settings.network_mode == "device-hotspot"
                    && self.network_override.lock().unwrap().is_none()
                {
                    self.stop_controller();
                }
                self.apply_pending.store(false, Ordering::Release);
                if let Some(after) = after {
                    self.operation(*after)?;
                }
            }
            Operation::Outputs(network) => {
                // Output processes can be replaced while the controller and AP keep running.
                self.stop_supervisor();
                self.carlink_hevc_disabled.store(false, Ordering::Release);
                self.reconcile_carlink_network()?;
                if let Some(network) = network {
                    self.operation(*network)?;
                }
                self.start_supervisor()?;
                self.apply_pending.store(false, Ordering::Release);
            }
            Operation::Carlink => {
                let settings = self.settings.lock().unwrap().effective_developer();
                self.carlink_hevc_disabled.store(false, Ordering::Release);
                self.vehicle
                    .lock()
                    .unwrap()
                    .enable(settings.vehicle_information);
                let (picture, codec) = {
                    let d = self.input.display.lock().unwrap();
                    (
                        crate::supervisor_settings::carlink_picture(&settings, d.pictures.trial),
                        d.saved.video_codec.clone(),
                    )
                };
                let (encoder, geometry) = {
                    let caps = self.input.transcoding.lock().unwrap();
                    (
                        caps.clink.converts("H265", picture),
                        caps.clink.geometry(picture),
                    )
                };
                self.input.display.lock().unwrap().transcode_geometry = geometry;
                let hevc = crate::supervisor_settings::carlink_hevc_with_encoder(
                    &settings, &codec, encoder,
                );
                if let Some(s) = self
                    .supervisor
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|_| settings.output != "web")
                {
                    s.restart_carlink(picture, hevc, settings.vehicle_information)?;
                }
            }
            Operation::ReceiverTls => {
                let result = self.update_receiver_tls(Duration::from_secs(3));
                self.receiver_tls_queued.store(false, Ordering::Release);
                result?;
            }
            Operation::Restart => {
                self.resume_start()?;
                self.input.accept_picture_default();
                self.start_policy.lock().unwrap().cancel();
                // An explicit restart starts over like Start, closing the AP too; only receiver
                // renegotiation keeps it (see tick).
                self.stop_controller();
                self.operation(Operation::Initialize)?;
                self.stop_supervisor();
                self.start_supervisor()?;
                self.desired.store(true, Ordering::Release);
                self.restart_required.store(false, Ordering::Release);
                self.apply_pending.store(false, Ordering::Release);
            }
            Operation::Forget => {
                self.stop_controller();
                if let Some(p) = self.provision.lock().unwrap().as_mut() {
                    p.forget()?;
                }
                let mut settings = self.settings.lock().unwrap();
                settings.shared_wifi_networks.clear();
                settings.save(&self.options.state_dir)?;
            }
            Operation::ConnectCarlink(mut pin) => {
                let supplied = pin.is_some();
                expire_pin(&mut pin);
                if supplied && pin.is_none() {
                    return Err(std::io::Error::other("Pairing code expired"));
                }
                remove_carlink_pin(&self.options.state_dir)?;
                if pin.is_some() {
                    self.carlink.lock().unwrap().pin_incorrect = false;
                }
                *self.armed_pin.lock().unwrap() = pin;
                if let Some(s) = self.supervisor.lock().unwrap().as_ref() {
                    s.connect_carlink();
                }
            }
            Operation::RegeneratePairingCode => {
                self.stop_supervisor();
                self.start_supervisor()?;
            }
            Operation::ClearSavedHeadUnit => {
                self.input.display.lock().unwrap().clear_saved()?;
            }
            Operation::ForgetCarlink => self.forget_carlink()?,
            Operation::Network {
                mode_changed,
                reconnect,
            } => self.change_network(mode_changed, reconnect)?,
            Operation::Join(ssid, password) => {
                self.stop_controller();
                self.provision
                    .lock()
                    .unwrap()
                    .as_mut()
                    .ok_or_else(|| std::io::Error::other("Wi-Fi interface unavailable"))?
                    .join(&ssid, &password)?;
                let mut settings = self.settings.lock().unwrap();
                settings.network_mode = "shared-wifi".into();
                if let Some(network) = settings
                    .shared_wifi_networks
                    .iter_mut()
                    .find(|n| n.ssid == ssid)
                {
                    network.password = password;
                } else {
                    settings
                        .shared_wifi_networks
                        .push(SharedWifiNetwork { ssid, password });
                }
                settings.save(&self.options.state_dir)?;
            }
        }
        Ok(())
    }
    fn forget_carlink(&self) -> Result<()> {
        *self.armed_pin.lock().unwrap() = None;
        let supervisor = self.supervisor.lock().unwrap().take();
        let restart = supervisor.is_some();
        if let Some(mut supervisor) = supervisor {
            supervisor.stop();
        }
        // The band escalation belonged to the forgotten head unit.
        *self.carlink_backoff.lock().unwrap() = CarlinkBackoff::default();
        let mut carlink = self.carlink.lock().unwrap();
        *carlink = AdminCarlink {
            state: "idle".into(),
            ..Default::default()
        };
        remove_carlink_pin(&self.options.state_dir)?;
        clear_carlink_trust(&self.options.state_dir)?;
        let mut settings = self.settings.lock().unwrap();
        settings.carlink_address = None;
        settings.save(&self.options.state_dir)?;
        drop(settings);
        drop(carlink);
        if restart {
            self.start_supervisor()?;
        }
        Ok(())
    }
    fn stop_supervisor(&self) {
        if let Some(mut supervisor) = self.supervisor.lock().unwrap().take() {
            supervisor.stop();
        }
    }
    fn apply_roles(&self, mut changes: Vec<String>) -> Result<()> {
        if changes.iter().any(|c| c == "wifiCountry") {
            self.apply_wifi_country();
            let (options, status) = self.effective_options_with_status()?;
            if let Some(old) = self.running_links.lock().unwrap().as_ref() {
                for change in old.changes(&self.links(&options, &status)) {
                    if !changes.contains(&change) {
                        changes.push(change);
                    }
                }
            }
        }
        let network = changes
            .iter()
            .any(|c| c == "controller")
            .then_some(Operation::Network {
                mode_changed: true,
                reconnect: true,
            });
        if changes.iter().any(|c| c == "carlink") {
            self.operation(Operation::Outputs(network.map(Box::new)))?;
        } else if let Some(network) = network {
            self.operation(network)?;
        }
        let (options, status) = self.effective_options_with_status()?;
        if !status.probing {
            *self.running_links.lock().unwrap() = Some(self.links(&options, &status));
        }
        self.public_name.head_unit_address(self.head_unit_address());
        self.reconcile_admin_hotspot();
        Ok(())
    }
    fn startup(&self) -> Result<()> {
        if !self.prepare_wifi()? {
            return Ok(());
        }
        // In device-hotspot mode the provisioning only takes the saved Wi-Fi client
        // connections down, which matters to the AP alone; at boot its nmcli calls end
        // once NetworkManager has set wlan0 up. So the controller starts now, Bluetooth
        // and iAP2 go ahead, and only the AP's activation waits for the provisioning.
        // In shared-Wi-Fi mode the provisioning brings the network up and decides
        // whether CarPlay runs at all, so the controller still follows it.
        let early = self.network_override.lock().unwrap().is_none()
            && self.desired.load(Ordering::Acquire)
            && self.effective_options()?.settings.network_mode == "device-hotspot";
        let _provisioning = early.then(|| self.provisioning.hold());
        if early {
            if let Err(e) = self.ensure_controller() {
                self.fail_start(&e);
            }
        }
        self.operation(Operation::Initialize)?;
        Ok(())
    }
    fn initialize_network(&self) -> Result<()> {
        if self.network_override.lock().unwrap().is_some() {
            return Ok(());
        }
        let mut wait = crate::logs::diagnostics::WaitGuard::new(&self.events, "provisioning");
        let options = self.effective_options()?;
        if !self.prepare_wifi()? {
            wait.finish("no_wifi");
            return Ok(());
        }
        let mut provision_guard = self.provision.lock().unwrap();
        let provision = provision_guard.as_mut().unwrap();
        provision.sync(&options.settings.shared_wifi_networks)?;
        if options.settings.network_mode == "shared-wifi" {
            provision.start()?;
        } else {
            provision.stop()?;
        }
        wait.finish("ready");
        Ok(())
    }
    fn change_network(&self, mode_changed: bool, reconnect: bool) -> Result<()> {
        if mode_changed || reconnect {
            self.stop_controller();
        }
        let options = self.effective_options()?;
        if !self.prepare_wifi()? {
            return Ok(());
        }
        let mut provision_guard = self.provision.lock().unwrap();
        let provision = provision_guard.as_mut().unwrap();
        provision.sync(&options.settings.shared_wifi_networks)?;
        if options.settings.network_mode == "shared-wifi" {
            if mode_changed || reconnect || provision.status().ssid.is_none() {
                provision.start()?;
            }
        } else if mode_changed {
            provision.stop()?;
        }
        Ok(())
    }
    pub fn host_addresses(&self) -> Result<Vec<std::net::Ipv4Addr>> {
        let options = self.effective_options()?;
        let following_carlink = self.network_override.lock().unwrap().is_some();
        let carplay_hotspot = !following_carlink
            && options.settings.network_mode == "device-hotspot"
            && self.controller.lock().unwrap().is_some();
        let mut addresses = self
            .runtime
            .network_addresses()?
            .into_iter()
            .filter(|(i, a, _)| {
                !i.starts_with("oxpusb")
                    && !a.is_loopback()
                    && (!carplay_hotspot || i != &options.wifi_interface)
            })
            .map(|(_, a, _)| a)
            .collect::<Vec<_>>();
        addresses.sort();
        addresses.dedup();
        Ok(addresses)
    }
    /// TLS also covers the CarPlay-facing address, which mDNS intentionally leaves out.
    pub fn admin_addresses(&self) -> Result<Vec<std::net::Ipv4Addr>> {
        let mut addresses = self.host_addresses()?;
        if self
            .settings
            .lock()
            .unwrap()
            .network_roles
            .admin_hotspot
            .as_ref()
            .is_some_and(|a| a.enabled)
        {
            addresses.push(std::net::Ipv4Addr::new(10, 79, 0, 1));
        }
        if let Some(address) = self.head_unit_address() {
            addresses.retain(|a| *a != address);
            addresses.insert(0, address);
        }
        Ok(addresses)
    }
    fn resume_start(&self) -> Result<()> {
        self.auto_start_gate.lock().unwrap().set_paused(false)?;
        self.arrival.lock().unwrap().cancel();
        self.recovery.lock().unwrap().reset();
        self.events.facts.lock().unwrap().reset_phone();
        Ok(())
    }
    /// Read-only snapshot for E-b2; reads bounded memory and never probes the host.
    pub fn runtime_path_facts(&self) -> crate::path_facts::RuntimePathFacts {
        let mut facts = self.input.runtime_path_facts();
        let now = std::time::Instant::now();
        let wall = crate::platform::wall_clock().as_millis() as u64;
        let settings = self.settings.lock().unwrap();
        facts.auto_start_paused = self.auto_start_gate.lock().unwrap().paused();
        facts.automatic = settings.auto_start && !facts.auto_start_paused;
        facts.boot_start = settings.boot_start;
        facts.arrival_remaining_millis = self
            .arrival
            .lock()
            .unwrap()
            .deadline
            .map(|d| d.saturating_duration_since(now).as_millis() as u64);
        facts.arrival_until_millis = facts
            .arrival_remaining_millis
            .map(|ms| wall.saturating_add(ms));
        facts.iphone.armed = self.desired.load(Ordering::Acquire)
            && !facts.auto_start_paused
            && !self.start_policy.lock().unwrap().waiting()
            && self.controller.lock().unwrap().as_ref().map_or(
                settings.auto_start || facts.iphone.failure.as_ref().is_none_or(|f| f.retrying),
                |controller| controller.armed(),
            );
        drop(settings);
        facts.iphone.car_wait_until_millis =
            self.start_policy.lock().unwrap().deadline().map(|d| {
                (crate::platform::wall_clock() + d.saturating_duration_since(now)).as_millis()
                    as u64
            });
        facts.pages = self.receivers.page_count(now);
        let settings = self.settings.lock().unwrap().clone();
        let tesla = self.tesla.lock().unwrap();
        facts.tesla.failure = tesla.failure.clone();
        facts.tesla.since_millis = tesla.since_millis;
        facts.tesla.activation =
            Some(tesla.activation(&settings, self.public_name.has_configuration()));
        drop(tesla);
        facts.tesla.url = self.tesla_url();
        let carlink = self.carlink.lock().unwrap();
        facts.carlink.trusted =
            has_trusted_carlink_for(&self.options.state_dir, settings.carlink_link == "ble");
        facts.carlink.pin_pending = carlink.pin_requested
            && carlink
                .pin_expiry
                .is_some_and(|t| t > crate::admin::auth::monotonic_seconds());
        if facts.carlink.phase == "PIN" && !facts.carlink.pin_pending {
            facts.carlink.phase = "AUTHENTICATING".into();
        }
        facts
    }
    pub fn try_carlink_picture(&self, size: Option<crate::display::Size>) -> AdminResult {
        if self.settings.lock().unwrap().output == "web" {
            return Err("error.conflict".into());
        }
        let changed = self
            .input
            .try_carlink_picture(size)
            .map_err(|_| "error.invalid".to_owned())?;
        if changed {
            self.defer(Operation::Carlink)?;
        }
        self.wake_tick();
        Ok(())
    }
    /// Waits up to `timeout`; a completed admin operation wakes the main loop early.
    pub fn wait_for_tick(&self, timeout: Duration) {
        let (woken, changed) = &self.tick_wake;
        let woken = woken.lock().unwrap();
        let mut woken = changed
            .wait_timeout_while(woken, timeout, |woken| !*woken)
            .unwrap()
            .0;
        *woken = false;
    }
    fn wake_tick(&self) {
        *self.tick_wake.0.lock().unwrap() = true;
        self.tick_wake.1.notify_all();
    }
    fn active_hotspot(&self) -> Option<(String, std::net::Ipv4Addr)> {
        let options = self.carplay_options().ok()?;
        if options.settings.network_mode != "device-hotspot" {
            return None;
        }
        if let Some(info) = self.hotspot_lifetime.ready_info() {
            if let Some(address) = info.host_address {
                return Some((
                    info.interface_name.unwrap_or(options.wifi_interface),
                    address,
                ));
            }
        }
        // Also covers an active NetworkManager AP inherited at daemon startup. Require our
        // profile and AP mode, so a station address or the separate admin AP cannot qualify.
        let inventory = self.inventory.get();
        let interface =
            inventory.interfaces.iter().find(|interface| {
                interface.name == options.wifi_interface
                    && interface.activity.mode.as_deref() == Some("AP")
                    && interface.activity.profile.as_deref().is_some_and(|p| {
                        p.starts_with(crate::linux_hotspot::HOTSPOT_CONNECTION_PREFIX)
                    })
            })?;
        self.runtime
            .network_addresses()
            .ok()?
            .into_iter()
            .find(|(name, address, prefix)| {
                name == &interface.name
                    && *prefix != 32
                    && !address.is_loopback()
                    && !address.is_unspecified()
            })
            .map(|(name, address, _)| (name, address))
    }
    /// The head-unit-facing IPv4 address: the CarPlay hotspot's (or, in shared-Wi-Fi mode, the
    /// relay's address on that network as the controller uses it), else the Wi-Fi interface's
    /// address in shared-Wi-Fi mode.
    fn head_unit_address(&self) -> Option<std::net::Ipv4Addr> {
        let (options, status) = self.effective_options_with_status().ok()?;
        let addresses = self.runtime.network_addresses().unwrap_or_default();
        let iphone = self
            .controller
            .lock()
            .unwrap()
            .clone()
            .and_then(|c| c.hotspot_info())
            .and_then(|info| info.host_address)
            .or_else(|| self.active_hotspot().map(|(_, address)| address))
            .or_else(|| {
                (options.settings.network_mode == "shared-wifi")
                    .then(|| {
                        addresses
                            .iter()
                            .find(|(name, ip, _)| {
                                name == &options.wifi_interface && !ip.is_loopback()
                            })
                            .map(|(_, ip, _)| *ip)
                    })
                    .flatten()
            });
        if status.probing {
            iphone
        } else {
            crate::network_roles::browser_address(&status.plan, iphone, &addresses)
        }
    }
    /// The admin page's handle on the trusted-HTTPS certificate (SNI).
    pub fn admin_certificate(&self) -> crate::admin::server::NamedCertificate {
        self.admin_certificate.clone()
    }
    fn validate_tesla_change(
        &self,
        current: &Settings,
        next: &Settings,
    ) -> std::result::Result<(), BTreeMap<String, String>> {
        if next.tesla_address() == current.tesla_address() {
            return Ok(());
        }
        let Some(address) = next.tesla_address() else {
            return Ok(());
        };
        let owned = self
            .tesla
            .lock()
            .unwrap()
            .active
            .as_ref()
            .map(|a| a.address.to_string());
        let collision = (self.runtime.addresses)().map(|rows| {
            rows.iter()
                .any(|(_, a, _)| a.to_string() == address && owned.as_deref() != Some(address))
        });
        if collision.unwrap_or(true) {
            return Err(BTreeMap::from([(
                crate::tesla_address::address_key(next.tesla_browser)
                    .unwrap()
                    .into(),
                "error.teslaAddress".into(),
            )]));
        }
        Ok(())
    }
    fn refresh_tesla_address(&self) {
        let mut settings = self
            .effective_options()
            .map(|o| o.settings)
            .unwrap_or_else(|_| self.settings.lock().unwrap().clone());
        let active = self.active_hotspot();
        if let Some((interface, _)) = &active {
            settings.wifi_interface = interface.clone();
        }
        let running =
            self.desired.load(Ordering::Acquire) || self.controller.lock().unwrap().is_some();
        let ready = active.is_some();
        let mut tesla = self.tesla.lock().unwrap();
        tesla.tick(
            running,
            ready,
            &settings,
            self.public_name.has_configuration(),
            || (self.runtime.addresses)().map(|rows| rows.into_iter().map(|(_, a, _)| a).collect()),
            crate::platform::wall_clock().as_millis() as u64,
        );
        let published = tesla
            .active
            .as_ref()
            .filter(|a| a.mode.uses_name())
            .map(|a| a.address);
        let changes = tesla.journal_changes(&settings);
        drop(tesla);
        self.public_name.published_address_override(published);
        for change in changes {
            self.events.emit("tesla-address", &change);
        }
    }
    /// Compare selected Tesla coverage with the last acknowledged receiver set.
    fn tesla_tls_changed(&self) -> bool {
        let desired = crate::tesla_address::tls_address(&self.settings.lock().unwrap());
        desired != *self.receiver_tesla_tls.lock().unwrap()
    }
    fn tesla_url(&self) -> Option<String> {
        let web = self.input.runtime_path_facts().web;
        if web.phase != "LISTENING" {
            return None;
        }
        let active = self.tesla.lock().unwrap().active.clone()?;
        let host = if active.mode.uses_name() {
            let status = self.public_name.status();
            if status.address.as_deref() != Some(active.address.to_string().as_str()) {
                return None;
            }
            let name = status.trusted_name?;
            let served = self.receiver_tls.lock().unwrap().clone().flatten()?;
            if served.name != name {
                return None;
            }
            name
        } else {
            if *self.receiver_tesla_tls.lock().unwrap() != Some(active.address) {
                return None;
            }
            active.address.to_string()
        };
        Some(format!("https://{host}:{}/", web.port))
    }
    /// Follows the trusted-HTTPS certificate: the admin page swaps it at once, the head-unit
    /// page through its acknowledged control channel.
    fn apply_public_name(&self) {
        let served = self.public_name.served();
        let generation = self.public_name.generation();
        if self
            .public_name_generation
            .swap(generation, Ordering::AcqRel)
            != generation
        {
            let files = served
                .as_ref()
                .and_then(|s| Some((s, std::fs::read(&s.cert).ok()?, std::fs::read(&s.key).ok()?)));
            match files {
                Some((s, chain, key))
                    if self.admin_certificate.set(&s.name, &chain, &key).is_ok() => {}
                _ => self.admin_certificate.clear(),
            }
        }
        if let Ok(sans) = self.receiver_sans() {
            self.apply_receiver_tls(served, sans);
        }
    }
    fn receiver_sans(&self) -> Result<Vec<std::net::Ipv4Addr>> {
        let rows = self.runtime.network_addresses()?;
        let occupied: Vec<_> = rows
            .iter()
            .map(|(_, address, prefix)| format!("{address}/{prefix}"))
            .collect();
        let mut addresses: Vec<_> = rows
            .into_iter()
            .map(|(_, address, _)| address)
            .filter(|a| !a.is_loopback())
            .collect();
        if let Some(address) = crate::tesla_address::tls_address(&self.settings.lock().unwrap()) {
            addresses.push(address);
        }
        addresses.push(std::net::Ipv4Addr::new(10, 78, 0, 1));
        if let Ok(address) = crate::HotspotAddressPolicy::select_subnet(&occupied) {
            addresses.push(address);
        }
        addresses.sort();
        addresses.dedup();
        for address in self.receiver_tls_sans.lock().unwrap().iter() {
            if !addresses.contains(address) {
                addresses.push(*address);
            }
        }
        addresses.truncate(16);
        addresses.sort();
        Ok(addresses)
    }
    /// Single production queue for trusted material, network SANs and Tesla coverage.
    /// The action reads the latest desired set when it runs, coalescing waiting changes.
    fn queue_user_tls(&self) {
        if let Ok(sans) = self.receiver_sans() {
            self.apply_receiver_tls(self.public_name.served(), sans);
        }
    }
    fn apply_receiver_tls(
        &self,
        served: Option<crate::public_name::Served>,
        sans: Vec<std::net::Ipv4Addr>,
    ) {
        let Some(current) = self.receiver_tls.lock().unwrap().clone() else {
            return;
        };
        let changed = crate::public_name::service::should_restart(
            current.as_ref(),
            served.as_ref(),
            false,
            crate::public_name::service::trusted_clock(),
        ) || *self.receiver_tls_sans.lock().unwrap() != sans
            || self.tesla_tls_changed()
            || crate::identity::tls::snapshot(&self.options.state_dir)
                .ok()
                .flatten()
                != *self.receiver_default_tls.lock().unwrap();
        if changed
            && !self.receiver_tls_queued.swap(true, Ordering::AcqRel)
            && self.operations.try_send(Operation::ReceiverTls).is_err()
        {
            self.receiver_tls_queued.store(false, Ordering::Release);
        }
    }
    fn update_receiver_tls(&self, timeout: Duration) -> Result<()> {
        let Some(current) = self.receiver_tls.lock().unwrap().clone() else {
            return Ok(());
        };
        // Freeze complete private PEMs before sending paths. A failed request never overwrites
        // the pair used by automatic retries of the currently served child.
        let served = self
            .public_name
            .certificate_snapshot(&self.options.state_dir)?;
        let sans = self.receiver_sans()?;
        let default_tls = crate::identity::tls::snapshot(&self.options.state_dir)?;
        let tesla = crate::tesla_address::tls_address(&self.settings.lock().unwrap());
        let pem = served.as_ref().map(|s| PemTls {
            name: s.name.clone(),
            cert: s.cert.clone(),
            key: s.key.clone(),
        });
        let swapped = self
            .supervisor
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotConnected))
            .and_then(|supervisor| {
                supervisor.update_web_tls_with_default(
                    pem,
                    sans.clone(),
                    default_tls.clone(),
                    timeout,
                )
            });
        match swapped {
            Ok(()) => {
                *self.receiver_tls.lock().unwrap() = Some(served);
                *self.receiver_default_tls.lock().unwrap() = default_tls;
                let _ = crate::identity::tls::prune(&self.options.state_dir);
                *self.receiver_tls_sans.lock().unwrap() = sans.clone();
                *self.receiver_tesla_tls.lock().unwrap() = tesla.filter(|a| sans.contains(a));
                self.events.log("receiver TLS updated live");
            }
            Err(error) => {
                self.events
                    .log(&format!("receiver TLS update not acknowledged: {error}"));
                // Recheck now: a session may have begun while the request was pending.
                let restart = !self.input.session_active()
                    || crate::public_name::service::should_restart(
                        current.as_ref(),
                        served.as_ref(),
                        true,
                        crate::public_name::service::trusted_clock(),
                    );
                if restart {
                    let running = self.supervisor.lock().unwrap().take();
                    if let Some(mut supervisor) = running {
                        supervisor.stop();
                        *self.receiver_tls.lock().unwrap() = None;
                        self.start_supervisor()?;
                    }
                }
            }
        }
        Ok(())
    }
    fn apply_wifi_country(&self) {
        let country = self.settings.lock().unwrap().wifi_country.clone();
        if self
            .wifi_country
            .apply(&country, || self.inventory.reprobe())
            .is_err()
        {
            self.events
                .log("Wi-Fi country could not be applied; see the network checks");
        }
    }
    fn reconcile_admin_hotspot(&self) {
        let settings = self.settings.lock().unwrap().clone();
        let status = self.network_status(&settings);
        if !status.probing {
            self.admin_hotspot.reconcile(
                &status.plan.admin_hotspot,
                settings.admin_hotspot_password.as_deref(),
                &status.inventory,
            );
        }
    }
    pub fn tick(&self) -> Result<()> {
        if let Err(error) = self.tick_inner() {
            // Startup/network failures belong to this start, not the daemon's main loop.
            let _lifecycle = self.lifecycle.lock().unwrap();
            self.fail_start(&error);
        }
        Ok(())
    }
    fn fail_start(&self, error: &std::io::Error) {
        let d = &self.events.diagnostics;
        d.start_ended(&self.events, "failed");
        if self.desired.load(Ordering::Acquire) && self.controller.lock().unwrap().is_none() {
            let reason = crate::logs::diagnostics::start_failure_reason(error);
            d.refuse(&self.events, reason, serde_json::json!({}));
        }
        self.events.facts.lock().unwrap().failure(
            &error.to_string(),
            false,
            crate::platform::wall_clock().as_millis() as u64,
        );
        self.events
            .emit("stage", &serde_json::json!({"stage":"Failed"}));
        self.stop_controller_for_restart(true);
    }
    fn tick_inner(&self) -> Result<()> {
        if let Ok(mut imports) = self.identity_imports.try_lock() {
            imports.expire();
        }
        if self.runtime.inventory_refresh == InventoryRefresh::Automatic {
            self.inventory.refresh();
        }
        self.vehicle.lock().unwrap().tick(
            std::time::Instant::now(),
            crate::platform::wall_clock().as_millis() as u64,
        );
        let inventory = self.inventory.get();
        let interfaces: Vec<_> = inventory.interfaces.iter().map(|interface| serde_json::json!({
            "interface":interface.name, "address":interface.mac, "state":interface.operstate,
            "ipv4":interface.ipv4, "activity":interface.activity
        })).collect();
        self.events.log_observation("network-observed", &serde_json::json!({
            "interfaces":interfaces, "bluetooth":self.capabilities.get().and_then(|caps| caps.adapters)
        }));
        // Address ownership and name/TLS refresh must progress during controller retries.
        self.refresh_tesla_address();
        self.apply_public_name();
        self.public_name.head_unit_address(self.head_unit_address());
        let playing = self.carlink.lock().unwrap().ready_for_carplay();
        if self.input.display.lock().unwrap().resize.reconnect(playing) {
            // The control channel replaces only Carlink, preserving the browser.
            if let Some(supervisor) = self.supervisor.lock().unwrap().as_ref() {
                supervisor.connect_carlink();
            }
        }
        if self.capabilities.is_started() {
            let options = self.effective_options()?;
            self.capabilities
                .refresh(&options.wifi_interface, options.carlink_interface());
        }
        let Ok(_lifecycle) = self.lifecycle.try_lock() else {
            return Ok(());
        };
        if !self.busy.load(Ordering::Acquire) && self.pending.lock().unwrap().is_empty() {
            let (options, status) = self.effective_options_with_status()?;
            if !status.probing {
                let links = self.links(&options, &status);
                let changes = self
                    .running_links
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|old| old.changes(&links))
                    .unwrap_or_default();
                if !changes.is_empty() {
                    self.operation(Operation::Roles(changes))?;
                }
                *self.running_links.lock().unwrap() = Some(links);
            }
        }
        if !self.busy.load(Ordering::Acquire) && self.pending.lock().unwrap().is_empty() {
            self.reconcile_admin_hotspot();
        }
        if self.start_policy.lock().unwrap().take_failure() {
            self.stop_controller();
        }
        self.observe_automatic_start()?;
        if self.busy.load(Ordering::Acquire)
            || !self.desired.load(Ordering::Acquire)
            || !self.pending.lock().unwrap().is_empty()
        {
            return Ok(());
        }
        if self
            .events
            .runtime_path_facts()
            .iphone
            .failure
            .as_ref()
            .is_some_and(|f| !f.retrying)
        {
            return Ok(());
        }
        if !self.wifi_present(&self.effective_options()?.wifi_interface) {
            self.stop_controller();
            self.hold_start("radio_unavailable", serde_json::json!({"wifi":"absent"}));
            return Ok(());
        }
        if *self.provision_interface.lock().unwrap() != self.effective_options()?.wifi_interface {
            self.stop_controller();
            self.prepare_wifi()?;
            if self.provision.lock().unwrap().is_some() {
                self.operation(Operation::Startup)?;
            }
        }
        if self.provision.lock().unwrap().is_none() {
            if !self.wifi_present(&self.effective_options()?.wifi_interface) {
                return Ok(());
            }
            self.operation(Operation::Startup)?;
        }
        self.ensure_controller()
    }
    fn observe_automatic_start(&self) -> Result<()> {
        let options = self.effective_options()?;
        let usb = options.settings.transport == "usb";
        let bluetooth = if usb {
            None
        } else {
            match self.backends.bluetooth.check_radio() {
                Ok(()) => Some(true),
                Err(e) => matches!(
                    crate::carplay_policy::classify_failure(&e.to_string()),
                    "bluetooth_off" | "bluetooth_unusable"
                )
                .then_some(false),
            }
        };
        let wifi = crate::start_policy::wifi_enabled(&self.runtime.root, &options.wifi_interface);
        let inventory = self.inventory.get();
        let hotspot = inventory
            .interfaces
            .iter()
            .find(|i| i.name == options.wifi_interface)
            .filter(|_| inventory.probed_at.is_some())
            .map(|i| i.activity.mode.as_deref() == Some("AP"));
        let paused = self.auto_start_gate.lock().unwrap().paused();
        let failure = self.events.runtime_path_facts().iphone.failure;
        if let Some(controller) = self.controller.lock().unwrap().as_ref() {
            controller.set_start_when_ready(options.settings.auto_start);
        }
        let devices = if usb {
            None
        } else {
            self.backends.bluetooth.devices().ok()
        };
        let iphone = if usb {
            Some(self.events.runtime_path_facts().usb.present)
        } else {
            devices.as_ref().map(|devices| {
                crate::linux_bluetooth::IphoneSelection::select(options.iphone.as_deref(), devices)
                    .device()
                    .is_some_and(|device| device.connected)
            })
        };
        let restored = self.recovery.lock().unwrap().observe(
            failure.as_ref(),
            crate::start_policy::RadioReadings {
                bluetooth,
                wifi,
                hotspot,
                carlink: Some(
                    options.settings.output == "web"
                        || self.carlink.lock().unwrap().ready_for_carplay(),
                ),
                iphone,
            },
            paused,
            options.settings.auto_start,
        );
        if restored {
            let controller = self
                .controller
                .lock()
                .unwrap()
                .clone()
                .filter(|c| c.running());
            if let Some(controller) = controller {
                controller.readiness_restored();
            } else {
                self.stop_controller_for_restart(true);
                self.start_policy.lock().unwrap().cancel();
                self.events.facts.lock().unwrap().iphone.failure = None;
                self.desired.store(true, Ordering::Release);
                self.events.log("carplay_radio_recover");
            }
        }
        if self.desired.load(Ordering::Acquire)
            || paused
            || !options.settings.auto_start
            || failure.is_some()
        {
            self.arrival.lock().unwrap().cancel();
            return Ok(());
        }
        let devices = if usb {
            Vec::new()
        } else {
            self.backends.bluetooth.devices().unwrap_or_default()
        };
        let selection =
            crate::linux_bluetooth::IphoneSelection::select(options.iphone.as_deref(), &devices);
        let identity = self.options.test_input.is_some()
            || crate::mfi_sources::readiness(
                &options.settings,
                &options.state_dir,
                &options.identity_dir,
            ) == "ready";
        let network_ready = self.wifi_present(&options.wifi_interface)
            && wifi != Some(false)
            && (options.settings.network_mode != "shared-wifi"
                || self.wifi_status().ssid.is_some());
        let selected = matches!(
            selection.reason(),
            "Selected" | "Connected" | "OnlyCandidate"
        );
        let refused = if !identity {
            Some("identity")
        } else if !network_ready
            && options.settings.network_mode == "shared-wifi"
            && self.wifi_present(&options.wifi_interface)
            && wifi != Some(false)
        {
            Some("shared_network")
        } else if !network_ready || !usb && bluetooth == Some(false) {
            Some("radio_unavailable")
        } else if !usb && !selected {
            Some("iphone_selection")
        } else {
            None
        };
        if let Some(reason) = refused {
            self.events.diagnostics.refuse(
                &self.events,
                reason,
                serde_json::json!({"source":"auto"}),
            );
        }
        let ready = identity
            && network_ready
            && (usb
                || bluetooth == Some(true)
                    && matches!(
                        selection.reason(),
                        "Selected" | "Connected" | "OnlyCandidate"
                    ))
            && !self.busy.load(Ordering::Acquire)
            && self.pending.lock().unwrap().is_empty();
        if self
            .arrival
            .lock()
            .unwrap()
            .ready(std::time::Instant::now(), ready)
        {
            self.arrival.lock().unwrap().cancel();
            self.desired.store(true, Ordering::Release);
        }
        Ok(())
    }
    /// A requested start that waits for its network or radios; a running session is no start.
    fn hold_start(&self, reason: &'static str, detail: serde_json::Value) {
        if self.controller.lock().unwrap().is_none() {
            self.events.diagnostics.hold(&self.events, reason, detail);
        }
    }
    /// Runs the CarPlay controller: starts one unless it runs, Carlink or the setup hotspot owns
    /// the radio, and applies a requested session restart. The caller holds `lifecycle`.
    fn ensure_controller(&self) -> Result<()> {
        if self
            .events
            .runtime_path_facts()
            .iphone
            .failure
            .as_ref()
            .is_some_and(|f| !f.retrying)
        {
            return Ok(());
        }
        if self.provision.lock().unwrap().is_none() {
            self.hold_start("provisioning", serde_json::json!({}));
            return Ok(());
        }
        if !self.wifi_present(&self.effective_options()?.wifi_interface) {
            self.hold_start("radio_unavailable", serde_json::json!({"wifi":"absent"}));
            return Ok(());
        }
        let following = self.effective_options()?.shared_carlink_radio() && {
            let follow = self.follow.lock().unwrap();
            let current = self.network_override.lock().unwrap();
            follow
                .target
                .as_deref()
                .is_some_and(|ssid| current.as_ref().map(|n| n.ssid.as_str()) != Some(ssid))
        };
        if following {
            self.hold_start("follow_over", serde_json::json!({}));
            return Ok(());
        }
        if self.network_override.lock().unwrap().is_none()
            && self
                .provision
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|p| p.status().setup_hotspot)
        {
            self.hold_start("shared_network", serde_json::json!({}));
            return Ok(());
        }
        let controller_running = self.controller.lock().unwrap().is_some();
        if controller_running {
            self.input
                .apply_requested_restart(|| self.stop_controller_for_restart(true));
            if self.controller.lock().unwrap().is_some() {
                return Ok(());
            }
        }
        let mut options = self.carplay_options()?;
        // A requested start remains armed while a known radio problem needs the user.
        let bluetooth_problem = options.settings.transport != "usb"
            && self
                .backends
                .bluetooth
                .check_radio()
                .err()
                .is_some_and(|e| {
                    matches!(
                        crate::carplay_policy::classify_failure(&e.to_string()),
                        "bluetooth_off" | "bluetooth_unusable"
                    )
                });
        let wifi_off =
            crate::start_policy::wifi_enabled(&self.runtime.root, &options.wifi_interface)
                == Some(false)
                && self
                    .runtime
                    .root
                    .join("sys/class/net")
                    .join(&options.wifi_interface)
                    .join("address")
                    .exists();
        if bluetooth_problem || wifi_off {
            let state = |off: bool| if off { "off" } else { "on" };
            self.hold_start(
                "radio_unavailable",
                serde_json::json!({"bluetooth":state(bluetooth_problem),"wifi":state(wifi_off)}),
            );
            return Ok(());
        }
        self.events
            .failure_secrets(failure_secrets(&options.settings, &options.state_dir));
        self.input.configure_runtime_settings(&options.settings)?;
        self.input.configure_connection_experiments();
        let (carlink_budget, screen_budget) = {
            let mut policy = self.start_policy.lock().unwrap();
            let carlink = options.settings.output != "web";
            let screen = !self.input.screen_known();
            let requested = !policy.requested();
            if requested {
                policy.request(std::time::Instant::now(), carlink, !screen);
            }
            (requested && carlink, requested && screen)
        };
        // The budgets of start_policy: 60 s for Carlink, 20 s for the real car screen.
        if carlink_budget {
            let detail = serde_json::json!({"maxMillis":60_000});
            self.events
                .diagnostics
                .begin(&self.events, "carlink", detail);
        }
        if screen_budget {
            let detail = serde_json::json!({"maxMillis":20_000});
            self.events
                .diagnostics
                .begin(&self.events, "car_screen", detail);
        }
        let start_gate: crate::start_policy::StartGate = {
            let (policy, carlink, input, settings, desired, events) = (
                self.start_policy.clone(),
                self.carlink.clone(),
                self.input.clone(),
                self.settings.clone(),
                self.desired.clone(),
                self.events.clone(),
            );
            Arc::new(move || {
                if !desired.load(Ordering::Acquire) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Start cancelled",
                    ));
                }
                let enabled = settings.lock().unwrap().output != "web";
                let (ready, reported) = {
                    let c = carlink.lock().unwrap();
                    (c.ready_for_carplay(), c.view_size.is_some())
                };
                let screen = input.screen_reported() || reported;
                let result =
                    policy
                        .lock()
                        .unwrap()
                        .ready(std::time::Instant::now(), enabled, ready, screen);
                events
                    .diagnostics
                    .start_gate(&events, &result, ready, screen);
                if let Err(e) = &result {
                    policy.lock().unwrap().fail();
                    events.facts.lock().unwrap().failure(
                        &e.to_string(),
                        false,
                        crate::platform::wall_clock().as_millis() as u64,
                    );
                    events.emit("stage", &serde_json::json!({"stage":"Failed"}));
                }
                result
            })
        };
        // A concurrent/browser path prepares its AP before waiting for a screen report.
        // On one radio, Carlink must own that radio before the controller prepares a network.
        match start_gate() {
            Err(_) => return Ok(()), // The failed request is published; the daemon keeps serving.
            Ok(false) if options.shared_carlink_radio() => return Ok(()),
            _ => {}
        }
        if self
            .pairing
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(_, d)| *d > crate::admin::auth::monotonic_seconds())
        {
            options.pair_seconds = None;
        }
        self.events.diagnostics.admitted(&self.events);
        self.events.follow_over("controller-start");
        self.tesla.lock().unwrap().prepare(
            &options.settings,
            crate::platform::wall_clock().as_millis() as u64,
        );
        let start_when_ready = options.settings.auto_start;
        let wired_options = options.clone();
        let platform = Arc::new(
            LinuxWirelessPlatform::new(
                options,
                self.backends.clone(),
                self.state.identity()?,
                self.state.pairings()?,
                self.sink.clone(),
                self.events.clone(),
                self.input.clone(),
            )
            .with_start_gate(start_gate)
            .with_inventory(self.inventory.clone())
            .with_capabilities(self.capabilities.clone())
            .with_hotspot_lifetime(self.hotspot_lifetime.clone())
            .with_bluetooth_wake(self.bluetooth_wake.clone())
            .with_secure_connections(self.secure_connections.clone())
            .with_provisioning(self.provisioning.clone())
            .with_before_activation({
                // Trusted HTTPS: the local DNS answer for the hotspot about to start.
                let public_name = self.public_name.clone();
                let target = self.browser_target.clone();
                let iphone = self.effective_options()?.wifi_interface;
                let addresses = self.runtime.network_address_source();
                Arc::new(move |address| {
                    let target = target.lock().unwrap();
                    let selected = if target.as_deref() == Some(&iphone) {
                        Some(address)
                    } else {
                        addresses()
                            .unwrap_or_default()
                            .iter()
                            .find(|(name, _, _)| Some(name) == target.as_ref())
                            .map(|(_, ip, _)| *ip)
                    };
                    public_name.head_unit_address(selected);
                })
            }),
        );
        let controller: Arc<dyn CarPlayController> = if wired_options.settings.transport == "usb" {
            Arc::new(LinuxWiredController::new(
                Arc::new(LinuxWiredPlatform::new(
                    platform,
                    wired_options.clone(),
                    self.state.identity()?,
                    self.state.pairings()?,
                    self.sink.clone(),
                    self.events.clone(),
                    self.input.clone(),
                )),
                self.events.clone(),
                wired_options.state_dir,
            ))
        } else {
            // Test mode: a simulated iPhone behind the same hotspot.
            let platform: Arc<dyn WirelessPlatform> = match &self.options.test_input {
                Some(test) => test.platform(
                    platform,
                    self.input.clone(),
                    self.sink.clone(),
                    self.events.clone(),
                ),
                None => platform,
            };
            Arc::new(LinuxCarPlayController::new(
                platform,
                self.events.clone(),
                Duration::from_secs(45),
            ))
        };
        controller.set_start_when_ready(start_when_ready);
        *self.controller.lock().unwrap() = Some(controller.clone());
        let events = self.events.clone();
        *self.worker.lock().unwrap() = Some(crate::platform::spawn_worker(
            "carplay-controller",
            Box::new(move || {
                if let Err(e) = controller.run() {
                    events.emit("error", &serde_json::json!({"message":e.to_string()}));
                }
            }),
        )?);
        Ok(())
    }
    pub fn shutdown(&self) {
        self.public_name.stop();
        if let Some((pairing, _)) = self.pairing.lock().unwrap().take() {
            pairing.end("relay-stop");
        }
        self.closed.store(true, Ordering::Release);
        self.desired.store(false, Ordering::Release);
        if let Some(worker) = self.operation_worker.lock().unwrap().take() {
            let _ = worker.join();
        }
        self.stop_controller();
        let _ = self.admin_hotspot.close();
        // After the controller's `--pair` window has closed too.
        self.secure_connections.close("relay-stop");
        if let Some(p) = self.provision.lock().unwrap().as_mut() {
            let _ = p.stop_setup();
        }
        if let Some(mut s) = self.supervisor.lock().unwrap().take() {
            s.stop();
        }
        let _ = self.stream.close();
        let _ = self.sink.close();
    }
    /// A config import can change both the hotspot and normal settings in one response.
    fn defer_hotspot(&self) -> AdminResult {
        let owner = std::thread::current().id();
        let mut pending = self.pending.lock().unwrap();
        if self.busy.load(Ordering::Acquire) || pending.keys().any(|key| *key != owner) {
            return Err("error.busy".into());
        }
        let after = pending.remove(&owner).map(Box::new);
        pending.insert(owner, Operation::Hotspot(after));
        Ok(())
    }
    fn defer(&self, op: Operation) -> AdminResult {
        let mut pending = self.pending.lock().unwrap();
        if self.busy.load(Ordering::Acquire) || !pending.is_empty() {
            return Err("error.busy".into());
        }
        pending.insert(std::thread::current().id(), op);
        Ok(())
    }
}
/// The head-unit address under the trusted-HTTPS name, first in the list while the name is
/// trusted (certificate valid, current address published), also while a renewal fails; the
/// other addresses stay as fallbacks.
fn public_name_url(
    status: &crate::public_name::PublicNameStatus,
    receiver_urls: &[String],
    port: u16,
) -> Option<String> {
    let name = status.trusted_name.as_ref()?;
    // Only with a web output, and with its pairing code.
    let first = receiver_urls.first()?;
    let fragment = first
        .split_once('#')
        .map_or(String::new(), |(_, f)| format!("#{f}"));
    Some(format!("https://{name}:{port}/{fragment}"))
}
/// A stored certificate is not necessarily loaded by the page child yet.
fn receiver_public_name(
    mut status: crate::public_name::PublicNameStatus,
    served: Option<&crate::public_name::Served>,
    now: Option<u64>,
) -> crate::public_name::PublicNameStatus {
    status.trusted_name = status.trusted_name.filter(|name| {
        served.is_some_and(|s| s.name == *name && now.is_none_or(|now| now < s.not_after))
    });
    status.not_after = served.map(|s| s.not_after);
    status
}
fn admin_urls(
    status: &crate::public_name::PublicNameStatus,
    head_unit: Option<std::net::Ipv4Addr>,
    hosts: &[std::net::Ipv4Addr],
) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(name) = status.trusted_name.as_ref().filter(|_| {
        head_unit.is_some_and(|a| status.address.as_deref() == Some(a.to_string().as_str()))
    }) {
        urls.push(format!("https://{name}:6975/"));
    }
    for address in head_unit.into_iter().chain(hosts.iter().copied()) {
        let url = format!("https://{address}:6975/");
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    let local = "https://openxplay-linux.local:6975/".to_string();
    if !urls.contains(&local) {
        urls.push(local);
    }
    urls
}
fn carlink_pin_path(state: &std::path::Path) -> std::path::PathBuf {
    state.join("carlink/pin")
}
fn remove_carlink_pin(state: &std::path::Path) -> Result<()> {
    match std::fs::remove_file(carlink_pin_path(state)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
fn clear_carlink_trust(state: &std::path::Path) -> Result<()> {
    let entries = match std::fs::read_dir(state.join("carlink")) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_name() == "pin" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
fn picture_section() -> Section {
    Section {
        id: "picture".into(),
        title_key: "section.linuxPicture".into(),
        fields: [
            (
                "resolution",
                vec!["auto", "native", "1080p", "720p", "screen"],
            ),
            ("screenWidth", vec!["125", "200", "250", "300"]),
            ("videoCodec", vec!["auto", "h265", "h264"]),
            (
                "frameRate",
                vec!["auto", "30", "35", "40", "45", "50", "55", "60"],
            ),
        ]
        .into_iter()
        .map(|(key, choices)| Field {
            key: key.into(),
            field_type: "choice".into(),
            label_key: format!("field.linux.{key}"),
            hint_key: Some(
                match key {
                    "screenWidth" => "hint.linux.screenWidth",
                    "resolution" => "hint.resolution",
                    "videoCodec" => "hint.videoCodec",
                    _ => "hint.frameRate",
                }
                .into(),
            ),
            options: Some(
                choices
                    .into_iter()
                    .map(|value| FieldOption {
                        labels: None,
                        value: if key == "screenWidth" {
                            serde_json::json!(value.parse::<u32>().unwrap())
                        } else {
                            value.into()
                        },
                        label_key: format!("option.linux.{key}.{value}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            visible_when: None,
            min: None,
            max: None,
        })
        .collect(),
    }
}
fn display_overrides_section() -> Section {
    let mut section = runtime_section(
        "display",
        &[
            ("displayMode", "choice"),
            ("displayWidth", "number"),
            ("displayHeight", "number"),
            ("displayDensity", "number"),
            ("physicalWidthEnabled", "bool"),
            ("physicalWidthOverrideMm", "choice"),
        ],
    );
    for field in &mut section.fields {
        if field.key == "displayMode" {
            field.options = Some(
                ["automatic", "fixed"]
                    .iter()
                    .map(|v| FieldOption {
                        value: (*v).into(),
                        label_key: format!("option.displayMode.{v}"),
                        labels: None,
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            );
            field.hint_key = Some("hint.displayMode".into());
        } else if field.key == "physicalWidthOverrideMm" {
            field.options = Some(
                (100..=400)
                    .step_by(25)
                    .map(|v| FieldOption {
                        value: serde_json::json!(v),
                        label_key: String::new(),
                        labels: Some(BTreeMap::from([
                            ("en".into(), format!("{v} mm")),
                            ("zh-CN".into(), format!("{v} mm")),
                        ])),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            );
            field.visible_when = Some(VisibleWhen {
                key: "physicalWidthEnabled".into(),
                equals: vec![serde_json::json!(true)],
            });
        } else if field.key == "physicalWidthEnabled" {
            field.hint_key = Some("hint.physicalWidth".into());
        } else if field.key.starts_with("display") {
            let (min, max) = match field.key.as_str() {
                "displayWidth" => (320., 3840.),
                "displayHeight" => (240., 2160.),
                _ => (120., 640.),
            };
            field.min = Some(min);
            field.max = Some(max);
            field.visible_when = Some(VisibleWhen {
                key: "displayMode".into(),
                equals: vec!["fixed".into()],
            });
        }
    }
    section
}
fn tesla_section() -> Section {
    let mut section = runtime_section(
        "tesla",
        &[
            ("teslaBrowser", "choice"),
            ("teslaVirtualAddress", "text"),
            ("teslaPublicAddress", "text"),
            ("teslaMirrorAddress", "text"),
            ("teslaAndroidAddress", "text"),
        ],
    );
    section.title_key = "tesla.title".into();
    for field in &mut section.fields {
        if field.key == "teslaBrowser" {
            field.label_key = "tesla.mode".into();
            field.options = Some(
                [
                    "teslamirror",
                    "virtual-name",
                    "public-address",
                    "tesla-android",
                    "off",
                ]
                .into_iter()
                .map(|mode| FieldOption {
                    value: mode.into(),
                    label_key: format!("tesla.mode.{mode}"),
                    labels: None,
                    hint_key: (mode != "off").then(|| {
                        if ["teslamirror", "virtual-name"].contains(&mode) {
                            "tesla.nameHint"
                        } else {
                            "tesla.directHint"
                        }
                        .into()
                    }),
                    illustration: None,
                })
                .collect(),
            );
        } else {
            let mode = match field.key.as_str() {
                "teslaVirtualAddress" => "virtual-name",
                "teslaPublicAddress" => "public-address",
                "teslaMirrorAddress" => "teslamirror",
                _ => "tesla-android",
            };
            field.label_key = "tesla.address".into();
            field.visible_when = Some(VisibleWhen {
                key: "teslaBrowser".into(),
                equals: vec![mode.into()],
            });
        }
    }
    section
}
/// Trusted HTTPS with a free domain name, stored in `public-name.json` (not `settings.json`).
fn public_name_section() -> Section {
    let field = |key: &str, kind: &str, label: &str, hint: Option<&str>| Field {
        key: key.into(),
        field_type: kind.into(),
        label_key: format!("publicName.{label}"),
        hint_key: hint.map(|h| format!("publicName.{h}")),
        options: (key == "publicNameProvider").then(|| {
            crate::public_name::config::Provider::ALL
                .into_iter()
                .map(|p| FieldOption {
                    labels: None,
                    value: p.id().into(),
                    label_key: format!("option.publicNameProvider.{}", p.id()),
                    hint_key: None,
                    illustration: None,
                })
                .collect()
        }),
        visible_when: None,
        min: None,
        max: None,
    };
    Section {
        id: "publicName".into(),
        title_key: "publicName.title".into(),
        fields: vec![
            field("publicNameEnabled", "bool", "enabled", Some("summary")),
            field("publicNameProvider", "choice", "provider", None),
            field("publicName", "text", "name", Some("nameHint")),
            field("publicNameToken", "secret", "token", Some("tokenHint")),
            field("publicNameAgreeTerms", "bool", "terms", None),
        ],
    }
}
fn runtime_section(id: &str, keys: &[(&str, &str)]) -> Section {
    Section {
        id: id.into(),
        title_key: format!("section.{id}"),
        fields: keys
            .iter()
            .map(|(key, kind)| Field {
                key: (*key).into(),
                field_type: (*kind).into(),
                label_key: format!("field.{key}"),
                hint_key: (key.starts_with("experimental")
                    || matches!(*key, "developerActive" | "carlinkPictureSize"))
                .then(|| format!("hint.field.{key}")),
                options: (*key == "carlinkPictureSize").then(|| {
                    ["window", "mode"]
                        .into_iter()
                        .map(|v| FieldOption {
                            labels: None,
                            value: v.into(),
                            label_key: format!("option.carlinkPictureSize.{v}"),
                            hint_key: None,
                            illustration: None,
                        })
                        .collect()
                }),
                visible_when: key.starts_with("carlinkPicture").then(|| VisibleWhen {
                    key: "output".into(),
                    equals: vec!["carlink".into(), "both".into()],
                }),
                min: None,
                max: None,
            })
            .collect(),
    }
}
fn car_section(car: &crate::car_models::CarSelection) -> Section {
    let catalog = crate::car_models::CarModels::embedded();
    let brand = catalog.brands.iter().find(|b| b.id == car.brand);
    let model = brand.and_then(|b| b.models.iter().find(|m| m.id == car.model));
    let mut section = runtime_section(
        "car",
        &[
            ("carBrand", "choice"),
            ("carBrandText", "text"),
            ("carModel", "choice"),
            ("carModelText", "text"),
            ("carYear", "choice"),
            ("carYearText", "text"),
        ],
    );
    for field in &mut section.fields {
        let rows: Vec<(&str, &BTreeMap<String, String>)> = match field.key.as_str() {
            "carBrand" => catalog
                .brands
                .iter()
                .map(|b| (b.id.as_str(), &b.names))
                .collect(),
            "carModel" => brand
                .map(|b| b.models.iter().map(|m| (m.id.as_str(), &m.names)).collect())
                .unwrap_or_default(),
            "carYear" => model
                .map(|m| m.years.iter().map(|y| (y.id.as_str(), &y.names)).collect())
                .unwrap_or_default(),
            _ => vec![],
        };
        if field.field_type == "choice" {
            let mut options = vec![FieldOption {
                labels: None,
                value: "".into(),
                label_key: format!("option.{}.", field.key),
                hint_key: None,
                illustration: None,
            }];
            options.extend(rows.into_iter().map(|(id, names)| FieldOption {
                labels: Some(names.clone()),
                value: id.into(),
                label_key: String::new(),
                hint_key: None,
                illustration: None,
            }));
            options.push(FieldOption {
                labels: None,
                value: "other".into(),
                label_key: format!("option.{}.other", field.key),
                hint_key: None,
                illustration: None,
            });
            field.options = Some(options);
        }
        if matches!(
            field.key.as_str(),
            "carBrandText" | "carModelText" | "carYearText"
        ) {
            field.visible_when = Some(VisibleWhen {
                key: if field.key == "carBrandText" {
                    "carBrand"
                } else if field.key == "carModelText" {
                    "carModel"
                } else {
                    "carYear"
                }
                .into(),
                equals: vec!["other".into()],
            });
        }
    }
    section
}
pub fn settings_schema() -> Vec<Section> {
    let fields: Vec<Field> = [
        ("mfiTarget", "choice"),
        ("mfiI2cPath", "text"),
        ("mfiRemoteServer", "text"),
        ("mfiRemoteToken", "secret"),
        ("networkMode", "choice"),
        ("networkRoles", "object"),
        ("adminHotspotPassword", "secret"),
        ("wifiCountry", "text"),
        ("devicePreset", "choice"),
        ("sharedWifiNetworks", "networks"),
        ("hotspotBand", "choice"),
        ("hotspotSecurity", "choice"),
        ("hotspotChannel", "number"),
        ("transport", "choice"),
        ("iphone", "phone"),
        ("wifiInterface", "text"),
        ("rfcommChannel", "number"),
        ("requirePairingCode", "bool"),
        ("output", "choice"),
        ("carlinkName", "text"),
        ("carlinkModel", "text"),
        ("carlinkAutoReconnect", "bool"),
        ("vehicleInformation", "bool"),
        ("carlinkReconnectOnResize", "bool"),
        ("carlinkHevc", "choice"),
        ("webTranscode", "bool"),
        ("playCarPlayAudio", "bool"),
        ("microphoneSourceEnabled", "bool"),
        ("microphoneSource", "choice"),
        ("carlinkTrace", "choice"),
        ("autoStart", "bool"),
        ("bootStart", "bool"),
        ("videoStats", "choice"),
        ("carPlayViewAreas", "bool"),
        ("receiverPort", "number"),
        ("carlinkLink", "choice"),
        ("carlinkInterface", "text"),
        ("carlinkAddress", "text"),
        ("carlinkHeadUnit", "text"),
        ("carlinkLocal", "text"),
    ]
    .into_iter()
    .map(|(key, kind)| Field {
        key: key.into(),
        field_type: kind.into(),
        label_key: format!(
            "field.{}",
            if key == "hotspotChannel" {
                "wifiChannel"
            } else {
                key
            }
        ),
        hint_key: if key == "bootStart" {
            Some("hint.field.bootStart".into())
        } else if key == "autoStart" {
            Some("hint.field.linuxAutoStart".into())
        } else {
            matches!(
                key,
                "output"
                    | "carlinkAutoReconnect"
                    | "carlinkReconnectOnResize"
                    | "carlinkHevc"
                    | "videoStats"
                    | "carPlayViewAreas"
                    | "hotspotChannel"
                    | "carlinkLink"
                    | "carlinkInterface"
                    | "carlinkAddress"
                    | "carlinkHeadUnit"
                    | "carlinkLocal"
            )
            .then(|| format!("hint.field.{key}"))
        },
        options: match key {
            "mfiTarget" => Some(
                ["local", "remote", "usb-ch341", "i2c"]
                    .into_iter()
                    .map(|v| FieldOption {
                        value: v.into(),
                        label_key: format!("option.mfiTarget.{v}"),
                        labels: None,
                        illustration: None,
                        hint_key: None,
                    })
                    .collect(),
            ),
            "microphoneSource" => Some(
                ["head-unit", "this-device"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.microphoneSource.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            "carlinkHevc" | "carlinkTrace" | "videoStats" => Some(
                (if key == "carlinkHevc" {
                    vec!["auto", "off", "signal"]
                } else if key == "carlinkTrace" {
                    vec!["off", "on"]
                } else {
                    vec!["off", "5", "15"]
                })
                .into_iter()
                .map(|v| FieldOption {
                    labels: None,
                    value: v.into(),
                    label_key: format!("option.{key}.{v}"),
                    hint_key: None,
                    illustration: None,
                })
                .collect(),
            ),
            "devicePreset" => Some(
                ["auto", "msm8916", "phicomm-n1", "generic"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.devicePreset.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            "hotspotSecurity" => Some(
                ["open", "wpa2", "wpa3-transition", "wpa3"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.hotspotSecurity.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            "hotspotBand" => Some(
                ["auto", "2.4", "5"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.hotspotBand.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            "transport" => Some(
                ["wireless", "usb"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.transport.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            "networkMode" => Some(
                ["device-hotspot", "shared-wifi"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.networkMode.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            "carlinkLink" => Some(
                ["ble", "static"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.carlinkLink.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            "output" => Some(
                ["web", "carlink", "both"]
                    .into_iter()
                    .map(|v| FieldOption {
                        labels: None,
                        value: v.into(),
                        label_key: format!("option.output.{v}"),
                        hint_key: None,
                        illustration: None,
                    })
                    .collect(),
            ),
            _ => None,
        },
        visible_when: match key {
            "iphone" | "rfcommChannel" => Some(VisibleWhen {
                key: "transport".into(),
                equals: vec!["wireless".into()],
            }),
            "mfiI2cPath" => Some(VisibleWhen {
                key: "mfiTarget".into(),
                equals: vec!["i2c".into()],
            }),
            "mfiRemoteServer" | "mfiRemoteToken" => Some(VisibleWhen {
                key: "mfiTarget".into(),
                equals: vec!["remote".into()],
            }),
            "networkRoles"
            | "adminHotspotPassword"
            | "wifiCountry"
            | "wifiInterface"
            | "carlinkInterface" => Some(VisibleWhen {
                key: "networkRoles".into(),
                equals: vec![],
            }),
            "carlinkHeadUnit" | "carlinkLocal" => Some(VisibleWhen {
                key: "carlinkLink".into(),
                equals: vec!["static".into()],
            }),
            "carlinkAddress" => Some(VisibleWhen {
                key: "carlinkLink".into(),
                equals: vec!["ble".into()],
            }),
            "carlinkName"
            | "carlinkModel"
            | "carlinkAutoReconnect"
            | "carlinkReconnectOnResize"
            | "carlinkHevc"
            | "carlinkLink" => Some(VisibleWhen {
                key: "output".into(),
                equals: vec!["carlink".into(), "both".into()],
            }),
            "hotspotChannel" | "hotspotBand" => Some(VisibleWhen {
                key: "networkMode".into(),
                equals: vec!["device-hotspot".into()],
            }),
            _ => None,
        },
        min: match key {
            "hotspotChannel" => Some(0.0),
            "rfcommChannel" => Some(1.0),
            "receiverPort" => Some(1.0),
            _ => None,
        },
        max: match key {
            "hotspotChannel" => Some(177.0),
            "rfcommChannel" => Some(30.0),
            "receiverPort" => Some(65535.0),
            _ => None,
        },
    })
    .collect();
    vec![
        picture_section(),
        Section {
            id: "connection".into(),
            title_key: "section.connection".into(),
            fields,
        },
        runtime_section(
            "output",
            &[
                ("carlinkPictureSize", "choice"),
                ("carlinkPictureSizes", "object"),
                ("carlinkPictureDefault", "object"),
                ("carlinkPictureTry", "object"),
            ],
        ),
        car_section(&Default::default()),
        display_overrides_section(),
        runtime_section(
            "experimental",
            &[
                ("experimentalActive", "bool"),
                ("experimentalIapChannelWhenOffered", "bool"),
                ("experimentalReleaseBluetoothOnRequest", "bool"),
                ("experimentalAwaitIphoneReconnect", "bool"),
                ("experimentalRadioConflictPrompt", "bool"),
                ("experimentalStallWatchdog", "bool"),
                ("experimentalScreenSessionIsolation", "bool"),
                ("experimentalScreenReconnectReset", "bool"),
                ("experimentalScreenDeliverySerialization", "bool"),
                ("experimentalRemotePlaybackProof", "bool"),
                ("experimentalStartupStatus", "bool"),
                ("experimentalStartupDiagnostics", "bool"),
                ("experimentalSendBluetoothAddress", "bool"),
                ("experimentalBluetoothAddress", "text"),
            ],
        ),
        runtime_section("developer", &[("developerActive", "bool")]),
        tesla_section(),
        public_name_section(),
        Section {
            id: "audio".into(),
            title_key: "section.audio".into(),
            fields: vec![Field {
                key: "carBluetoothAudio".into(),
                field_type: "bool".into(),
                label_key: "field.carBluetoothAudio".into(),
                hint_key: Some("hint.field.carBluetoothAudio".into()),
                options: None,
                visible_when: None,
                min: None,
                max: None,
            }],
        },
        Section {
            id: "plugins".into(),
            title_key: "section.plugins".into(),
            fields: [("plugins", "bool"), ("pluginsEnabled", "plugins")]
                .into_iter()
                .map(|(key, kind)| Field {
                    key: key.into(),
                    field_type: kind.into(),
                    label_key: format!("field.{key}"),
                    hint_key: (key == "plugins").then(|| "hint.plugins".into()),
                    options: None,
                    visible_when: None,
                    min: None,
                    max: None,
                })
                .collect(),
        },
    ]
}
fn failure_secrets(settings: &Settings, state_dir: &std::path::Path) -> Vec<String> {
    let mut secrets: Vec<_> = settings
        .shared_wifi_networks
        .iter()
        .map(|n| n.password.clone())
        .collect();
    if let Ok(token) = crate::mfi_sources::token(state_dir) {
        secrets.push(token);
    }
    secrets.extend(settings.carlink_pin.clone());
    secrets.extend(
        settings
            .admin_hotspot_password
            .as_ref()
            .filter(|password| password.as_str() != DEFAULT_HOTSPOT_PASSWORD)
            .cloned(),
    );
    if let Ok((_, password)) = crate::linux_hotspot::hotspot_credentials(state_dir) {
        if password != DEFAULT_HOTSPOT_PASSWORD {
            secrets.push(password);
        }
    }
    secrets
}

/// This list drives both normal settings application and configuration import reports.
fn settings_restart_keys(current: &Settings, next: &Settings) -> Vec<&'static str> {
    let mut keys = Vec::new();
    macro_rules! changed {
        ($field:ident, $key:literal) => {
            if next.$field != current.$field {
                keys.push($key);
            }
        };
    }
    changed!(mfi_target, "mfiTarget");
    changed!(mfi_i2c_path, "mfiI2cPath");
    changed!(mfi_remote_server, "mfiRemoteServer");
    changed!(transport, "transport");
    changed!(
        experimental_iap_channel_when_offered,
        "experimentalIapChannelWhenOffered"
    );
    changed!(
        experimental_release_bluetooth_on_request,
        "experimentalReleaseBluetoothOnRequest"
    );
    changed!(
        experimental_await_iphone_reconnect,
        "experimentalAwaitIphoneReconnect"
    );

    changed!(iphone, "iphone");
    changed!(rfcomm_channel, "rfcommChannel");
    changed!(wifi_interface, "wifiInterface");
    changed!(hotspot_channel, "hotspotChannel");
    changed!(hotspot_band, "hotspotBand");
    changed!(hotspot_security, "hotspotSecurity");
    changed!(device_preset, "devicePreset");
    changed!(developer_active, "developerActive");
    changed!(display_mode, "displayMode");
    changed!(display_width, "displayWidth");
    changed!(display_height, "displayHeight");
    changed!(display_density, "displayDensity");
    changed!(physical_width_enabled, "physicalWidthEnabled");
    changed!(physical_width_override_mm, "physicalWidthOverrideMm");
    changed!(car_bluetooth_audio, "carBluetoothAudio");
    changed!(play_car_play_audio, "playCarPlayAudio");
    changed!(microphone_source, "microphoneSource");
    changed!(microphone_source_enabled, "microphoneSourceEnabled");
    changed!(carlink_picture_default, "carlinkPictureDefault");
    changed!(carlink_picture_size, "carlinkPictureSize");
    keys
}
pub fn update_settings(
    current: &Settings,
    mut values: Values,
) -> std::result::Result<Settings, BTreeMap<String, String>> {
    // A picker change clears its descendants; a complete import can supply them together.
    for (key, previous, descendants) in [
        (
            "carBrand",
            &current.car_brand,
            &[
                "carBrandText",
                "carModel",
                "carModelText",
                "carYear",
                "carYearText",
            ][..],
        ),
        (
            "carModel",
            &current.car_model,
            &["carModelText", "carYear", "carYearText"][..],
        ),
        ("carYear", &current.car_year, &["carYearText"][..]),
    ] {
        if values
            .get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|v| v != previous)
        {
            for key in descendants {
                values.entry((*key).to_owned()).or_insert_with(|| "".into());
            }
        }
    }
    let mut merged = serde_json::to_value(current).unwrap();
    let mut errors = BTreeMap::new();
    for (k, mut v) in values {
        let k = match k.as_str() {
            "autoStartWhenReady" => "autoStart".to_owned(),
            "autoStartOnBoot" => "bootStart".to_owned(),
            _ => k,
        };
        if k == "pluginsEnabled" {
            let ids = serde_json::from_value::<Vec<String>>(v.clone());
            let mut seen = std::collections::BTreeSet::new();
            if !ids.is_ok_and(|ids| {
                ids.iter()
                    .all(|id| crate::plugins::valid_id(id) && seen.insert(id.clone()))
            }) {
                errors.insert(k, "error.plugins".into());
                continue;
            }
        }
        if k == "sharedWifiNetworks" {
            let parsed = (|| -> Option<serde_json::Value> {
                let mut networks = Vec::new();
                for item in v.as_array()? {
                    let item = item.as_object()?;
                    if item.keys().any(|k| k != "ssid" && k != "password") {
                        return None;
                    }
                    let ssid = item.get("ssid")?.as_str()?;
                    let password = match item.get("password") {
                        Some(value) => value.as_str()?,
                        None => {
                            &current
                                .shared_wifi_networks
                                .iter()
                                .find(|n| n.ssid == ssid)?
                                .password
                        }
                    };
                    networks.push(SharedWifiNetwork {
                        ssid: ssid.into(),
                        password: password.into(),
                    });
                }
                let check = Settings {
                    shared_wifi_networks: networks.clone(),
                    ..current.clone()
                };
                check.validate().ok()?;
                serde_json::to_value(networks).ok()
            })();
            match parsed {
                Some(networks) => v = networks,
                None => {
                    errors.insert(k, "error.networks".into());
                    continue;
                }
            }
        }
        if (k == "iphone" && v == "auto") || (k == "carlinkAddress" && v == "") {
            v = serde_json::Value::Null
        }
        if k == "rfcommChannel" && (v == 0 || v == "") {
            v = serde_json::Value::Null
        }
        // Keys kept from a newer relay are not settings of this version.
        if merged.get(&k).is_none() || current.unknown.contains_key(&k) {
            errors.insert(k, "error.setting".into());
        } else {
            merged[&k] = v;
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if let Ok(mode) =
        serde_json::from_value::<crate::tesla_address::Mode>(merged["teslaBrowser"].clone())
    {
        if let Some(key) = crate::tesla_address::address_key(mode) {
            if !merged[key]
                .as_str()
                .is_some_and(crate::tesla_address::valid)
            {
                return Err(BTreeMap::from([(key.into(), "error.teslaAddress".into())]));
            }
        }
    } else {
        return Err(BTreeMap::from([(
            "teslaBrowser".into(),
            "error.invalid".into(),
        )]));
    }
    if merged["physicalWidthEnabled"] == true
        && !current.physical_width_enabled
        && merged["physicalWidthOverrideMm"] == 0
    {
        merged["physicalWidthOverrideMm"] = serde_json::json!(200);
    }
    match serde_json::from_value::<Settings>(merged)
        .map_err(std::io::Error::other)
        .and_then(|mut s| {
            s.normalize_car();
            s.validate().map(|_| s)
        }) {
        Ok(s)
            if s.network_mode == "shared-wifi"
                && current.network_mode != s.network_mode
                && s.shared_wifi_networks.is_empty() =>
        {
            Err(BTreeMap::from([(
                "networkMode".into(),
                "error.noSavedWifi".into(),
            )]))
        }
        Ok(s) => Ok(s),
        Err(e) => Err(BTreeMap::from([("settings".into(), e.to_string())])),
    }
}
impl LinuxRelayAdmin {
    fn config_user_values(
        &self,
        settings: &Settings,
        display: &serde_json::Value,
    ) -> std::result::Result<Values, crate::config_files::Errors> {
        let mut values = crate::config_files::settings_values(settings, display);
        let (ssid, password) = crate::linux_hotspot::hotspot_credentials(&self.options.state_dir)
            .map_err(|_| BTreeMap::from([("file".into(), "io".into())]))?;
        values.insert("hotspotSsid".into(), serde_json::json!(ssid));
        values.insert("hotspotPassphrase".into(), serde_json::json!(password));
        values
            .entry("hotspotSecurity")
            .or_insert_with(|| serde_json::json!("wpa2"));
        Ok(values)
    }
    fn log_export_known_values(
        &self,
        settings: &Settings,
        status: &AdminStatus,
    ) -> std::io::Result<Vec<(String, &'static str)>> {
        let mut known: Vec<(String, &str)> = Vec::new();
        if let Ok(token) = crate::mfi_sources::token(&self.options.state_dir) {
            known.push((token, "secret"));
        }
        for network in &settings.shared_wifi_networks {
            known.push((network.password.clone(), "secret"));
        }
        if let Some(hotspot) = status
            .hotspot
            .as_ref()
            .filter(|hotspot| hotspot.password != DEFAULT_HOTSPOT_PASSWORD)
        {
            known.push((hotspot.password.clone(), "secret"));
        }
        for device in &status.paired_devices {
            known.push((device.name.clone(), "name"));
        }
        if let Some(pin) = settings.carlink_pin.as_ref() {
            known.push((pin.clone(), "secret"));
        }
        if let Some(password) = settings
            .admin_hotspot_password
            .as_ref()
            .filter(|password| password.as_str() != DEFAULT_HOTSPOT_PASSWORD)
        {
            known.push((password.clone(), "secret"));
        }
        if let Some(pin) = self.armed_pin.lock().unwrap().as_ref() {
            known.push((pin.pin.clone(), "secret"));
        }
        if let Some(bytes) =
            crate::logs::read_private(&self.options.state_dir.join("hotspot.credentials"), 4096)?
        {
            let credentials = serde_json::from_slice::<(String, String)>(&bytes)
                .ok()
                .or_else(|| {
                    let text = std::str::from_utf8(&bytes).ok()?;
                    let mut lines = text.lines();
                    Some((lines.next()?.into(), lines.next()?.into()))
                });
            if let Some((ssid, password)) = credentials {
                if password != DEFAULT_HOTSPOT_PASSWORD {
                    known.push((password, "secret"));
                }
            }
        }
        for (key, value) in &self.options.overrides {
            if key.contains("password") || key.contains("token") {
                known.push((value.clone(), "secret"));
            }
        }
        if let Some((public, _)) =
            crate::public_name::config::load(&self.options.state_dir.join("public-name.json"))?
        {
            if let Some(token) = public.token {
                known.push((token, "secret"));
            }
        }
        let providers = self.options.state_dir.join("public-name/providers.json");
        if let Some(bytes) =
            crate::logs::read_private(&providers, crate::public_name::config::MAX_BYTES)?
        {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(providers) = value["providers"].as_object() {
                    for provider in providers.values() {
                        if let Some(value) = provider["token"].as_str() {
                            known.push((value.into(), "secret"));
                        }
                    }
                }
            }
        }
        Ok(known)
    }
    fn build_log_export(&self, range: &str) -> std::io::Result<crate::logs::Export> {
        let range = serde_json::from_value::<crate::logs::Range>(serde_json::json!(range))?;
        let status = self.status();
        let settings = self.settings.lock().unwrap().clone();
        let known = self.log_export_known_values(&settings, &status)?;
        let mut safe = crate::config_files::settings_values(
            &settings,
            &oxp_wire::display_settings(&self.input.display_settings()),
        );
        let count = settings.shared_wifi_networks.len();
        safe.retain(|key, _| {
            let key = key.to_ascii_lowercase();
            ![
                "name",
                "ssid",
                "password",
                "token",
                "pin",
                "address",
                "sharedwifi",
            ]
            .iter()
            .any(|part| key.contains(part))
        });
        let profile = &self.options.host_profile;
        let info = serde_json::json!({
            "hostProfile":{"model":profile.profile.model,"loaded":profile.loaded,"devicePreset":settings.device_preset},
            "wifiCapabilities":status.wireless.map(|w| w.summary),
            "settings":safe,"savedWifiNetworkCount":count
        }).to_string();
        // Process, watchdog and wait facts; the export redacts them like every other line.
        let info = format!(
            "{info}\n{}",
            self.events.diagnostics.export_details().trim_end()
        );
        self.log_exports.export(
            range,
            &info,
            &known,
            &self.recent_connection_timeline_for_log_export(),
        )
    }
}
impl RelayAdmin for LinuxRelayAdmin {
    fn recent_connection_timeline_for_log_export(&self) -> String {
        let facts = self.runtime_path_facts();
        let mut text = facts
            .timeline
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(summary) = facts.carlink_summary {
            text.push('\n');
            text.push_str(&summary);
        }
        text
    }
    fn logs_export(&self, range: &str) -> std::result::Result<serde_json::Value, String> {
        self.build_log_export(range)
            .map(|value| serde_json::to_value(value).unwrap())
            .map_err(|_| "error.request".into())
    }
    fn logs_download(&self, id: &str) -> std::io::Result<(std::fs::File, u64, String)> {
        let download = self.log_exports.download(id)?;
        Ok((download.file, download.bytes, download.name))
    }
    fn logs_upload(&self, id: &str) -> std::result::Result<serde_json::Value, String> {
        self.log_exports
            .upload(id)
            .map(|receipt| serde_json::to_value(receipt).unwrap())
            .map_err(|error| error.code().into())
    }
    fn logs_upload_status(&self, id: &str, cancel: bool) -> Option<serde_json::Value> {
        self.log_exports
            .upload_status(id, cancel)
            .map(|value| serde_json::to_value(value).unwrap())
    }
    fn logs_receipt(&self) -> std::io::Result<Option<serde_json::Value>> {
        self.log_exports
            .receipt()
            .map(|receipt| receipt.map(|receipt| serde_json::to_value(receipt).unwrap()))
    }
    fn update_check(&self, channel: &str) -> std::result::Result<serde_json::Value, &'static str> {
        self.update_check_fresh(channel, false)
    }
    fn update_check_fresh(
        &self,
        channel: &str,
        fresh: bool,
    ) -> std::result::Result<serde_json::Value, &'static str> {
        let selected = crate::update_check::Channel::parse(channel).ok_or("invalid")?;
        Ok(match self.release_checks.check(selected, fresh) {
            Ok(check) => serde_json::to_value(check).unwrap(),
            Err(failure) => serde_json::json!({"channel":channel,"failure":failure}),
        })
    }
    fn identity_status(&self) -> serde_json::Value {
        let mut tls = crate::identity::tls::status(&self.options.state_dir);
        tls["pending"] = serde_json::json!(
            self.receiver_tls.lock().unwrap().is_some()
                && crate::identity::tls::snapshot(&self.options.state_dir)
                    .ok()
                    .flatten()
                    != *self.receiver_default_tls.lock().unwrap()
        );
        serde_json::json!({"mfi":crate::identity::status(&self.options.identity_dir),"tls":tls})
    }
    fn identity_import(
        &self,
        body: &Values,
    ) -> std::result::Result<serde_json::Value, &'static str> {
        let result = self
            .identity_imports
            .lock()
            .unwrap()
            .select(&self.options.identity_dir, body)?;
        self.wake_tick();
        Ok(result)
    }
    fn identity_remove(&self) -> std::result::Result<(), &'static str> {
        let mut imports = self.identity_imports.lock().unwrap();
        crate::identity::remove(&self.options.identity_dir).map_err(|_| "error.request")?;
        imports.clear();
        self.wake_tick();
        Ok(())
    }
    fn tls_import(&self, body: &Values) -> std::result::Result<(), &'static str> {
        use base64ct::{Base64, Encoding};
        let _lock = self.user_tls_lock.lock().unwrap();
        if body.contains_key("chainPem") || body.contains_key("keyPem") {
            if body.contains_key("data") || body.contains_key("password") {
                return Err("identity_error_pem");
            }
            let chain = body
                .get("chainPem")
                .and_then(serde_json::Value::as_str)
                .ok_or("identity_error_pem")?;
            let key = body
                .get("keyPem")
                .and_then(serde_json::Value::as_str)
                .ok_or("identity_error_pem")?;
            let material = crate::identity::tls::validate(chain, key, None)
                .map_err(|_| "identity_error_pem")?;
            crate::identity::tls::install(&self.options.state_dir, &material)
                .map_err(|_| "error.request")?;
        } else {
            let encoded = body
                .get("data")
                .and_then(serde_json::Value::as_str)
                .filter(|s| s.len() <= crate::identity::tls::MAX_FILE * 4 / 3 + 4)
                .ok_or("identity_error_tls")?;
            let bytes = zeroize::Zeroizing::new(
                Base64::decode_vec(encoded).map_err(|_| "identity_error_tls")?,
            );
            let password = body
                .get("password")
                .and_then(serde_json::Value::as_str)
                .ok_or("identity_error_tls")?;
            crate::identity::tls::import_pkcs12(&self.options.state_dir, &bytes, password)?;
        }
        self.queue_user_tls();
        Ok(())
    }
    fn tls_remove(&self) -> std::result::Result<(), &'static str> {
        let _lock = self.user_tls_lock.lock().unwrap();
        crate::identity::tls::remove(&self.options.state_dir).map_err(|_| "error.request")?;
        self.queue_user_tls();
        Ok(())
    }
    fn config_export(
        &self,
        privacy: bool,
    ) -> std::result::Result<serde_json::Value, crate::config_files::Errors> {
        let now = crate::logs::now_millis();
        if privacy {
            let mut sections = self
                .public_name
                .privacy_export()
                .map_err(|_| BTreeMap::from([("file".into(), "io".into())]))?;
            let token = crate::mfi_sources::token(&self.options.state_dir)
                .map_err(|_| BTreeMap::from([("file".into(), "io".into())]))?;
            if !token.is_empty() {
                sections["mfiRemoteToken"] = token.into();
            }
            if let Some(material) = crate::identity::tls::export(&self.options.state_dir)
                .map_err(|_| BTreeMap::from([("importedCertificate".into(), "io".into())]))?
            {
                sections["importedCertificate"] = material;
            }
            crate::config_files::export_privacy(sections, now)
        } else {
            let display = oxp_wire::display_settings(&self.input.display_settings());
            let values = self.config_user_values(&self.settings.lock().unwrap(), &display)?;
            crate::config_files::export_user_values(values, now)
        }
    }
    fn config_import(
        &self,
        bytes: &[u8],
        dry_run: bool,
    ) -> std::result::Result<serde_json::Value, crate::config_files::Errors> {
        let current = self.settings.lock().unwrap().clone();
        let mut display = oxp_wire::display_settings(&self.input.display_settings());
        let envelope = crate::config_files::read_envelope(bytes)?;
        let mut current_values = crate::config_files::settings_values(&current, &display);
        let native_hotspot_security = current_values.contains_key("hotspotSecurity");
        if envelope["format"] == crate::config_files::USER_FORMAT {
            current_values = self.config_user_values(&current, &display)?;
        }
        let mut plan = crate::config_files::plan_envelope(
            &envelope,
            &current_values,
            self.public_name.config().as_ref(),
            crate::platform::wall_clock().as_secs(),
        )?;
        let mut settings = plan.values.clone();
        let hotspot = crate::config_files::take_hotspot(
            &mut settings,
            &current_values,
            native_hotspot_security,
        )?;
        for key in ["resolution", "screenWidth", "videoCodec"] {
            if let Some(value) = settings.remove(key) {
                display[key] = value;
            }
        }
        oxp_wire::decode_display_settings(&display)
            .map_err(|_| BTreeMap::from([("settings".into(), "error.linuxPicture".into())]))?;
        let next = update_settings(&current, settings)?;
        self.validate_tesla_change(&current, &next)?;
        self.validate_hotspot_security_change(&current, &next)?;
        if plan.values.contains_key("pluginsEnabled") {
            let installed = crate::plugins::installed(&self.options.plugins_root);
            if next
                .plugins_enabled
                .iter()
                .any(|id| !installed.iter().any(|plugin| &plugin.id == id))
            {
                return Err(BTreeMap::from([(
                    "pluginsEnabled".into(),
                    "error.plugins".into(),
                )]));
            }
        }
        let restart_keys = settings_restart_keys(&current, &next);
        plan.report.next_start = plan
            .report
            .applied
            .iter()
            .filter(|key| restart_keys.contains(&key.as_str()))
            .cloned()
            .collect();
        plan.report.applied_now = plan
            .report
            .applied
            .iter()
            .filter(|key| !restart_keys.contains(&key.as_str()))
            .cloned()
            .collect();
        plan.report.restart_needed = !plan.report.next_start.is_empty();
        if plan.mfi_remote_token.is_some() {
            plan.report
                .applied_now
                .retain(|key| key != "mfiRemoteToken");
            plan.report.next_start.push("mfiRemoteToken".into());
            plan.report.restart_needed = true;
        }
        if !dry_run {
            if let Some(token) = plan.mfi_remote_token {
                crate::mfi_sources::save_token(&self.options.state_dir, &token)
                    .map_err(|_| BTreeMap::from([("file".into(), "io".into())]))?;
                self.restart_required.store(true, Ordering::Release);
                self.events
                    .failure_secrets(failure_secrets(&current, &self.options.state_dir));
            }
            if let Some(mut privacy) = plan.privacy {
                if let Some(material) = privacy.imported_certificate.take() {
                    let _lock = self.user_tls_lock.lock().unwrap();
                    crate::identity::tls::install(&self.options.state_dir, &material).map_err(
                        |_| BTreeMap::from([("importedCertificate".into(), "io".into())]),
                    )?;
                    self.queue_user_tls();
                }
                self.public_name
                    .import_privacy(privacy)
                    .map_err(|_| BTreeMap::from([("file".into(), "io".into())]))?;
            }
            let mut editor_values = plan.values;
            editor_values.remove("hotspotSsid");
            editor_values.remove("hotspotPassphrase");
            if !native_hotspot_security {
                editor_values.remove("hotspotSecurity");
            }
            if !editor_values.is_empty() {
                match self.update(editor_values) {
                    AdminUpdate::Errors(errors) => return Err(errors),
                    AdminUpdate::NextStart => plan.report.restart_needed = true,
                    AdminUpdate::AppliedNow => plan.report.restart_needed = false,
                }
            }
            if let Some((ssid, password)) = hotspot {
                self.hotspot_update(&ssid, &password, None, None)
                    .map_err(|_| BTreeMap::from([("file".into(), "io".into())]))?;
            }
        }
        Ok(serde_json::to_value(plan.report).unwrap())
    }
    fn carlink_trace(&self) -> Option<(bool, bool, serde_json::Value)> {
        let enabled = self
            .settings
            .try_lock()
            .is_ok_and(|s| s.developer_active && s.carlink_trace == "on");
        let running = self
            .supervisor
            .try_lock()
            .is_ok_and(|s| s.as_ref().is_some_and(|s| s.trace_running()));
        Some((enabled, running, self.trace_status.get()))
    }
    fn carlink_trace_control(&self, clear: bool, action: &str) -> AdminResult {
        let _lifecycle = self
            .lifecycle
            .try_lock()
            .map_err(|_| "error.carlinkTraceBusy".to_string())?;
        let enabled = self
            .settings
            .try_lock()
            .is_ok_and(|s| s.developer_active && s.carlink_trace == "on");
        let supervisor = self
            .supervisor
            .try_lock()
            .map_err(|_| "error.carlinkTraceBusy".to_string())?;
        let clear_files = || {
            crate::admin::server::trace_files::clear(
                &self.options.state_dir.join("carlink-trace"),
            )?;
            self.trace_status.reset();
            Ok(())
        };
        if clear {
            match supervisor.as_ref() {
                Some(s) => s
                    .clear_trace(clear_files)
                    .map_err(|_| "error.carlinkTraceBusy".to_string()),
                None => clear_files().map_err(|_: std::io::Error| "error.request".to_string()),
            }
        } else if let Some(s) = supervisor.as_ref().filter(|s| s.trace_running() && enabled) {
            s.send_control(&serde_json::json!({"type":"clink-capture","action":action}))
                .map_err(|_| "error.carlinkTraceBusy".to_string())
        } else {
            Err("error.carlinkTraceOff".into())
        }
    }

    fn public_name_import(
        &self,
        content: &str,
    ) -> std::result::Result<(String, String), crate::public_name::service::ImportError> {
        self.public_name.import(content)
    }
    fn response_failed(&self) {
        self.pending
            .lock()
            .unwrap()
            .remove(&std::thread::current().id());
    }
    fn response_sent(&self) {
        if let Some(op) = self
            .pending
            .lock()
            .unwrap()
            .remove(&std::thread::current().id())
        {
            if self.operations.try_send(op).is_err() {
                self.events.log("Admin action queue full");
            }
        }
    }
    fn status(&self) -> AdminStatus {
        let options = self
            .effective_options()
            .unwrap_or_else(|_| self.options.clone());
        let wifi = self.wifi_status();
        let devices = self.backends.bluetooth.devices().unwrap_or_default();
        let now = crate::admin::auth::monotonic_seconds();
        let wall_clock = crate::platform::wall_clock();
        let wall_now = wall_clock.as_secs();
        let pairing_remaining = self
            .pairing
            .lock()
            .unwrap()
            .as_ref()
            .map(|(_, deadline)| *deadline)
            .filter(|deadline| *deadline > now)
            .map(|deadline| deadline - now);
        let selection =
            crate::linux_bluetooth::IphoneSelection::select(options.iphone.as_deref(), &devices);
        let following_carlink = self.network_override.lock().unwrap().is_some();
        let head_unit = self.head_unit_address();
        let active_hotspot = self.active_hotspot();
        let host_addresses = self.host_addresses().unwrap_or_default();
        let controller = self.controller.lock().unwrap();
        let stage = controller.as_ref().map(|c| c.stage()).unwrap_or_else(|| {
            if wifi.setup_hotspot {
                "WaitingForWifi".into()
            } else {
                "Idle".into()
            }
        });
        let mut receiver_urls = self
            .supervisor
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.receiver_urls("openxplay-linux.local"))
            .unwrap_or_default();
        let selected_tesla = options.settings.tesla_address();
        let served_tesla = *self.receiver_tesla_tls.lock().unwrap();
        receiver_urls.retain(|u| {
            !selected_tesla.is_some_and(|a| u.contains(&format!("://{a}:")))
                && !served_tesla.is_some_and(|a| u.contains(&format!("://{a}:")))
        });
        let worker_public_name = self.public_name.status();
        let mut admin_public_name = worker_public_name.clone();
        let admin_name = self.admin_certificate.name();
        admin_public_name.trusted_name = admin_public_name
            .trusted_name
            .filter(|name| admin_name.as_ref() == Some(name));
        let child_running = self
            .supervisor
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.running());
        let current = self
            .receiver_tls
            .lock()
            .unwrap()
            .clone()
            .flatten()
            .filter(|_| child_running);
        let public_name = receiver_public_name(
            worker_public_name,
            current.as_ref(),
            crate::public_name::service::trusted_clock(),
        );
        if let Some(url) =
            public_name_url(&public_name, &receiver_urls, options.settings.receiver_port)
        {
            receiver_urls.insert(0, url);
        }
        if let Some(url) = self.tesla_url() {
            receiver_urls.retain(|u| u != &url);
            receiver_urls.insert(0, url);
        }
        let hotspot = crate::linux_hotspot::hotspot_credentials(&options.state_dir)
            .ok()
            .map(|(ssid, password)| {
                let ip = if !following_carlink
                    && options.settings.network_mode == "device-hotspot"
                    && self.desired.load(Ordering::Acquire)
                {
                    controller
                        .as_ref()
                        .and_then(|c| c.hotspot_info())
                        .and_then(|info| info.host_address)
                        .or_else(|| active_hotspot.as_ref().map(|(_, address)| *address))
                        .map(|a| a.to_string())
                } else {
                    None
                };
                AdminHotspot {
                    security: options.settings.hotspot_security.clone(),
                    supported_security: self.supported_hotspot_security(&options.wifi_interface),
                    has_password: options.settings.hotspot_security != "open",
                    ssid,
                    password,
                    active: ip.is_some(),
                    ip,
                }
            });
        let mut status = AdminStatus {
            identity: Some(self.identity_status()),
            path_input: serde_json::Value::Null,
            admin_features: ["logs", "config-files", "update-check", "identity"]
                .map(str::to_owned)
                .to_vec(),
            video: self.input.video_status(),
            hotspot,
            wifi_control: true,
            host_addresses: self
                .runtime
                .network_addresses()
                .unwrap_or_default()
                .into_iter()
                .filter(|(interface, a, _)| {
                    let flags =
                        std::fs::read_to_string(self.net_root.join(interface).join("flags"))
                            .ok()
                            .and_then(|flags| {
                                u32::from_str_radix(flags.trim().trim_start_matches("0x"), 16).ok()
                            });
                    !interface.starts_with("oxpusb")
                        && !a.is_loopback()
                        && flags.is_none_or(|flags| flags & 1 != 0)
                })
                .map(|(interface, a, _)| AdminHostAddress::new(interface, a.to_string()))
                .collect(),
            receiver_port: options.settings.receiver_port,
            admin_port: 6975,
            pairing_available: true,
            pairing_deadline: pairing_remaining.map(|remaining| wall_now.saturating_add(remaining)),
            pairing_remaining,
            pairing_cancelable: true,
            paired_devices: devices
                .iter()
                .filter(|d| d.paired)
                .map(|d| AdminPairedDevice {
                    name: d.name.clone(),
                    connected: d.connected,
                    iphone: likely_iphone(d),
                })
                .collect(),
            receivers: None,
            receiver_history: None,
            screen_data: Some(self.input.display.lock().unwrap().admin_status()),
            bluetooth_guard: crate::bluetooth_guard::read(&options.bluetooth_guard_status),
            bluetooth_secure_connections: self.secure_connections.enabled(),
            platform: "linux".into(),
            name: crate::branding::DEVICE_NAME.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            host: "openxplay-linux.local".into(),
            admin_hotspot: Some(self.admin_hotspot.status()),
            host_capabilities: self.capabilities.get(),
            local_audio: crate::local_audio::devices(&self.runtime.root),
            transcoding: serde_json::to_value(&*self.input.transcoding.lock().unwrap())
                .unwrap_or_default(),
            wireless: Some({
                let wifi = self.capabilities.cached_wifi(&options.wifi_interface);
                let phy = wifi.as_ref().and_then(|w| w.capabilities.as_ref());
                let preset = options.device_preset(self.capabilities.root());
                let mut hotspot = crate::device_preset::resolve_hotspot(
                    &options.settings.hotspot_band,
                    options.wifi_channel,
                    &preset.id,
                    phy,
                );
                if let Some(active) = self.capabilities.hotspot(&options.wifi_interface) {
                    let running_ap = !following_carlink
                        && options.settings.network_mode == "device-hotspot"
                        && controller.as_ref().and_then(|c| c.hotspot_info()).is_some();
                    let failed_start = active.reason.as_deref() == Some("ap-start-failed")
                        && active.configured_band == hotspot.configured_band
                        && active.configured_channel == hotspot.configured_channel;
                    if running_ap || failed_start {
                        hotspot.band = active.band;
                        hotspot.channel = active.channel;
                        hotspot.reason = active.reason;
                    }
                }
                AdminWireless {
                    preset,
                    summary: crate::wifi_capabilities::WifiSummary::from_phy(phy),
                    hotspot,
                }
            }),
            host_profile: Some(AdminHostProfile {
                model: options.host_profile.profile.model.clone(),
                profile_path: options.host_profile.path.clone(),
                profile_loaded: options.host_profile.loaded,
                profile_error: options.host_profile.error.clone(),
                bluetooth_adapter: self.backends.bluetooth.selected_adapter().map(|a| {
                    AdminBluetoothAdapter {
                        name: a.name,
                        address: a.address,
                    }
                }),
                wifi_interface: options.wifi_interface.clone(),
                wifi_present: self.wifi_present(&options.wifi_interface),
                secure_connections: match options.host_profile.profile.secure_connections {
                    crate::host_profile::SecureConnectionsPolicy::PairingOff => "pairing-off",
                    crate::host_profile::SecureConnectionsPolicy::Untouched => "untouched",
                }
                .into(),
            }),
            admin_url: "https://openxplay-linux.local:6975/".into(),
            admin_urls: admin_urls(&admin_public_name, head_unit, &host_addresses),
            receiver_urls,
            state: stage,
            running: controller.as_ref().is_some_and(|c| c.running()),
            auto_start_paused: !self.desired.load(Ordering::Acquire),
            iphone: AdminIphone {
                name: if options.settings.transport == "usb" {
                    self.events.runtime_path_facts().usb.product
                } else {
                    selection.device().map(|d| d.name.clone())
                },
                reason: selection.reason().into(),
            },
            network: AdminNetwork {
                planned: Some(self.network_status(&self.settings.lock().unwrap())),
                following_carlink,
                mode: options.settings.network_mode.clone(),
                wifi: wifi.ssid.clone(),
            },
            output: options.settings.output.clone(),
            carlink: if options.settings.output != "web" {
                let mut status = self.carlink.lock().unwrap().clone().at(now, wall_now);
                status.trusted = has_trusted_carlink_for(
                    &self.options.state_dir,
                    options.settings.carlink_link == "ble",
                );
                status.vehicle = self.vehicle.lock().unwrap().status();
                Some(status)
            } else {
                None
            },
            notices: {
                let mut notices = self
                    .provision
                    .try_lock()
                    .map(|p| p.as_ref().map(|p| p.notices().to_vec()).unwrap_or_default())
                    .unwrap_or_default();
                if self.restart_required.load(Ordering::Acquire)
                    || self.apply_pending.load(Ordering::Acquire)
                {
                    notices.push("notice.nextStart".into());
                }
                notices
            },
            wifi_provisioning: Some(wifi.setup_hotspot),
            wifi_client: Some(wifi),
            public_name: Some(public_name),
        };
        drop(controller);
        let identity_ready = self.options.test_input.is_some()
            || crate::mfi_sources::readiness(
                &options.settings,
                &options.state_dir,
                &options.identity_dir,
            ) == "ready";
        crate::path_input::update(
            &self.runtime_path_facts(),
            &self.settings.lock().unwrap(),
            &mut status,
            wall_clock.as_millis() as u64,
            identity_ready,
            self.backends.bluetooth.check_radio().is_ok(),
        );
        if let Some(enabled) = self.backends.access_point.wifi_enabled() {
            status.path_input["relay"]["wifiOn"] = enabled.into();
        }
        status
    }
    fn network_roles_update(&self, values: Values) -> Option<crate::network_roles::SaveResult> {
        Some(self.propose_roles(values, true))
    }
    fn network_roles_preview(&self, values: Values) -> Option<crate::network_roles::SaveResult> {
        Some(self.propose_roles(values, false))
    }
    fn settings(&self) -> AdminSettings {
        let mut values = serde_json::to_value(self.settings.lock().unwrap().clone())
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        values.remove("adminHotspotPassword");
        if values["experimentalBluetoothAddress"] == "" {
            values.insert(
                "experimentalBluetoothAddress".into(),
                self.backends
                    .bluetooth
                    .selected_adapter()
                    .map(|a| a.address)
                    .unwrap_or_default()
                    .into(),
            );
        }
        values.insert(
            "frameRate".into(),
            crate::carplay_policy::frame_rate_value(self.input.display_settings().frame_rate)
                .into(),
        );
        values.insert(
            "carlinkPictureTry".into(),
            serde_json::to_value({
                let d = self.input.display.lock().unwrap();
                d.pictures.trial.unwrap_or(d.picture_default)
            })
            .unwrap(),
        );
        let current = self
            .provision
            .try_lock()
            .ok()
            .and_then(|p| p.as_ref().and_then(|p| p.current_ssid()));
        let networks = self.settings.lock().unwrap().shared_wifi_networks.iter().map(|n|
            serde_json::json!({"ssid": n.ssid, "hasPassword": !n.password.is_empty(), "current": current.as_ref() == Some(&n.ssid)})
        ).collect::<Vec<_>>();
        values.insert("sharedWifiNetworks".into(), serde_json::json!(networks));
        let display = oxp_wire::display_settings(&self.input.display_settings());
        for key in ["resolution", "screenWidth", "videoCodec"] {
            values.insert(key.into(), display[key].clone());
        }
        let mut schema = settings_schema();
        if let Some(section) = schema.iter_mut().find(|s| s.id == "car") {
            *section = car_section(&self.settings.lock().unwrap().car());
        }
        values.retain(|key, _| {
            schema
                .iter()
                .any(|section| section.fields.iter().any(|field| &field.key == key))
        });
        if values["iphone"].is_null() {
            values.insert("iphone".into(), "auto".into());
        }
        // The token never leaves the relay: `publicNameTokenSet` says whether one is saved.
        values.extend(self.public_name.admin_values());
        values.insert(
            "publicNamePrivacyAvailable".into(),
            values
                .get("privacyConfigAvailable")
                .cloned()
                .unwrap_or(false.into()),
        );
        let token_set =
            crate::mfi_sources::token(&self.options.state_dir).is_ok_and(|v| !v.is_empty());
        values.insert("mfiRemoteTokenSet".into(), token_set.into());
        if token_set
            || crate::identity::tls::snapshot(&self.options.state_dir)
                .is_ok_and(|tls| tls.is_some())
        {
            values.insert("privacyConfigAvailable".into(), true.into());
        }
        values.insert(
            "mfiSourceStatus".into(),
            crate::mfi_sources::readiness(
                &self.settings.lock().unwrap(),
                &self.options.state_dir,
                &self.options.identity_dir,
            )
            .into(),
        );
        AdminSettings {
            schema,
            values,
            installed_plugins: crate::plugins::installed(&self.options.plugins_root),
        }
    }
    fn update(&self, mut values: Values) -> AdminUpdate {
        // Role changes must pass the plan checks through the dedicated endpoint.
        if values
            .keys()
            .any(|k| ["networkRoles", "adminHotspotPassword", "wifiCountry"].contains(&k.as_str()))
        {
            return AdminUpdate::Errors(BTreeMap::from([(
                "settings".into(),
                "error.invalid".into(),
            )]));
        }
        // Remote credentials use a write-only field and an independent private store.
        if let Some(value) = values.remove("mfiRemoteToken") {
            let Some(token) = value
                .as_str()
                .filter(|v| crate::mfi_sources::valid_token(v))
            else {
                return AdminUpdate::Errors(BTreeMap::from([(
                    "mfiRemoteToken".into(),
                    "error.invalid".into(),
                )]));
            };
            if let Err(errors) = update_settings(&self.settings.lock().unwrap(), values.clone()) {
                return AdminUpdate::Errors(errors);
            }
            if crate::mfi_sources::save_token(&self.options.state_dir, token).is_err() {
                return AdminUpdate::Errors(BTreeMap::from([(
                    "mfiRemoteToken".into(),
                    "error.invalid".into(),
                )]));
            }
            self.events.failure_secrets(failure_secrets(
                &self.settings.lock().unwrap(),
                &self.options.state_dir,
            ));
            self.restart_required.store(true, Ordering::Release);
            return match if values.is_empty() {
                AdminUpdate::NextStart
            } else {
                self.update(values)
            } {
                AdminUpdate::Errors(e) => AdminUpdate::Errors(e),
                _ => AdminUpdate::NextStart,
            };
        }
        // Trusted HTTPS lives in its own file and applies at once.
        let public: Vec<String> = values
            .keys()
            .filter(|k| k.starts_with("publicName"))
            .cloned()
            .collect();
        if !public.is_empty() {
            let public: Values = public
                .iter()
                .filter_map(|k| values.remove_entry(k))
                .collect();
            let result = self.public_name.update(&public);
            let rest = if values.is_empty() {
                AdminUpdate::AppliedNow
            } else {
                self.update(values)
            };
            return match (result, rest) {
                (Ok(()), rest) => rest,
                (Err(errors), AdminUpdate::Errors(mut rest)) => {
                    rest.extend(errors);
                    AdminUpdate::Errors(rest)
                }
                (Err(errors), _) => AdminUpdate::Errors(errors),
            };
        }
        if let Some(trial) = values.remove("carlinkPictureTry") {
            if !values.is_empty() {
                if let AdminUpdate::Errors(errors) = self.update(values) {
                    return AdminUpdate::Errors(errors);
                }
            }
            let size = if trial.is_null() {
                None
            } else {
                match serde_json::from_value(trial) {
                    Ok(size) => Some(size),
                    Err(_) => {
                        return AdminUpdate::Errors(BTreeMap::from([(
                            "carlinkPictureTry".into(),
                            "error.invalid".into(),
                        )]))
                    }
                }
            };
            return match self.try_carlink_picture(size) {
                Ok(()) => AdminUpdate::AppliedNow,
                Err(e) => AdminUpdate::Errors(BTreeMap::from([("carlinkPictureTry".into(), e)])),
            };
        }
        let mut display = oxp_wire::display_settings(&self.input.display_settings());
        if let Some(value) = values
            .get("frameRate")
            .and_then(|v| v.as_str())
            .and_then(crate::carplay_policy::frame_rate_setting)
        {
            display["frameRate"] = serde_json::json!(value.wire_name());
        }
        let mut picture_keys = Vec::new();
        for key in ["resolution", "screenWidth", "videoCodec"] {
            if let Some(value) = values.remove(key) {
                display[key] = value;
                picture_keys.push(key);
            }
        }
        let picture_changed = !picture_keys.is_empty() || values.contains_key("frameRate");
        // The admin page shows errors by field key.
        let picture_errors = |keys: &[&str]| {
            AdminUpdate::Errors(
                keys.iter()
                    .map(|k| (k.to_string(), "error.linuxPicture".to_string()))
                    .collect(),
            )
        };
        let picture = match oxp_wire::decode_display_settings(&display) {
            Ok(picture) => picture,
            Err(_) => return picture_errors(&picture_keys),
        };
        let validate_plugins = values.contains_key("pluginsEnabled");
        let mut current = self.settings.lock().unwrap();
        current.frame_rate =
            crate::carplay_policy::frame_rate_value(self.input.display_settings().frame_rate)
                .into();
        match update_settings(&current, values) {
            Ok(next) => {
                if let Err(errors) = self.validate_tesla_change(&current, &next) {
                    return AdminUpdate::Errors(errors);
                }
                if validate_plugins {
                    let installed = crate::plugins::installed(&self.options.plugins_root);
                    if next
                        .plugins_enabled
                        .iter()
                        .any(|id| !installed.iter().any(|p| &p.id == id))
                    {
                        return AdminUpdate::Errors(BTreeMap::from([(
                            "pluginsEnabled".into(),
                            "error.plugins".into(),
                        )]));
                    }
                }
                if let Err(errors) = self.validate_hotspot_security_change(&current, &next) {
                    return AdminUpdate::Errors(errors);
                }
                let mode_changed = next.network_mode != current.network_mode;
                let networks_changed = next.shared_wifi_networks != current.shared_wifi_networks;
                let network_update = mode_changed || networks_changed;
                let effective_next = next.effective_developer();
                let effective_current = current.effective_developer();
                let outputs_changed = picture.video_codec
                    != self.input.display_settings().video_codec
                    || effective_next.web_transcode != effective_current.web_transcode
                    || next.plugins != current.plugins
                    || next.plugins_enabled != current.plugins_enabled
                    || next.output != current.output
                    || next.require_pairing_code != current.require_pairing_code
                    || effective_next.receiver_port != effective_current.receiver_port
                    || next.carlink_interface != current.carlink_interface
                    || effective_next.carlink_link != effective_current.carlink_link
                    || effective_next.carlink_address != effective_current.carlink_address
                    || effective_next.carlink_head_unit != effective_current.carlink_head_unit
                    || effective_next.carlink_local != effective_current.carlink_local
                    || effective_next.carlink_pin != effective_current.carlink_pin
                    || effective_next.carlink_ports != effective_current.carlink_ports
                    || next.carlink_name != current.carlink_name
                    || effective_next.carlink_model != effective_current.carlink_model
                    || effective_next.carlink_trace != effective_current.carlink_trace
                    || next.carlink_auto_reconnect != current.carlink_auto_reconnect;
                let controller_changed = !settings_restart_keys(&current, &next).is_empty();
                let network = if network_update {
                    let ssid = self
                        .provision
                        .try_lock()
                        .ok()
                        .and_then(|p| p.as_ref().and_then(|p| p.current_ssid()));
                    let reconnect = ssid.as_ref().is_some_and(|ssid| {
                        current
                            .shared_wifi_networks
                            .iter()
                            .find(|n| &n.ssid == ssid)
                            != next.shared_wifi_networks.iter().find(|n| &n.ssid == ssid)
                    });
                    Some(Operation::Network {
                        mode_changed,
                        reconnect,
                    })
                } else {
                    None
                };
                let carlink_changed = effective_next.carlink_hevc != effective_current.carlink_hevc
                    || next.vehicle_information != current.vehicle_information;
                let operation = if outputs_changed {
                    Some(Operation::Outputs(network.map(Box::new)))
                } else if carlink_changed {
                    Some(Operation::Carlink)
                } else {
                    network
                };
                if let Some(operation) = operation {
                    if let Err(error) = self.defer(operation) {
                        return AdminUpdate::Errors(BTreeMap::from([("settings".into(), error)]));
                    }
                }
                {
                    let mut display = self.input.display.lock().unwrap();
                    display.resize.set_enabled(next.carlink_reconnect_on_resize);
                    if next.car_play_view_areas != current.car_play_view_areas {
                        display.disable_view_areas = !next.car_play_view_areas;
                        display.settings_since = Some(std::time::Instant::now());
                    }
                }
                if picture_changed {
                    if let Err(e) = self.input.update_display_settings(picture) {
                        self.events.emit("error", &serde_json::json!({"message": format!("picture settings not applied: {e}")}));
                        return picture_errors(&picture_keys);
                    }
                }
                if let Err(e) = next.save(&self.options.state_dir) {
                    self.response_failed();
                    return AdminUpdate::Errors(BTreeMap::from([(
                        "settings".into(),
                        e.to_string(),
                    )]));
                }
                if let Err(e) = self.input.configure_runtime_settings(&next) {
                    self.events
                        .log(&format!("runtime settings not applied: {e}"));
                }
                if !current.auto_start && next.auto_start {
                    if let Err(e) = self.auto_start_gate.lock().unwrap().set_paused(false) {
                        return AdminUpdate::Errors(BTreeMap::from([(
                            "autoStart".into(),
                            e.to_string(),
                        )]));
                    }
                }
                if !next.auto_start {
                    self.arrival.lock().unwrap().cancel();
                }
                self.events
                    .failure_secrets(failure_secrets(&next, &self.options.state_dir));
                self.vehicle
                    .lock()
                    .unwrap()
                    .enable(next.vehicle_information);
                if !next.developer_active {
                    self.vehicle.lock().unwrap().sample(
                        false,
                        std::time::Instant::now(),
                        crate::platform::wall_clock().as_millis() as u64,
                    );
                }
                // The stall watchdog applies at once.
                self.events
                    .diagnostics
                    .set_enabled(next.effective_experimental().stall_watchdog);
                *current = next;
                if outputs_changed {
                    self.apply_pending.store(true, Ordering::Release);
                }
                if controller_changed {
                    self.restart_required.store(true, Ordering::Release);
                    AdminUpdate::NextStart
                } else {
                    AdminUpdate::AppliedNow
                }
            }
            Err(e) => AdminUpdate::Errors(e),
        }
    }
    fn action(&self, name: &str) -> AdminResult {
        match name {
            "pair-iphone" => {
                let mut pairing = self.pairing.lock().unwrap();
                let now = crate::admin::auth::monotonic_seconds();
                if pairing
                    .as_ref()
                    .is_some_and(|(_, deadline)| *deadline > now)
                {
                    return Ok(());
                }
                if let Some((previous, _)) = pairing.take() {
                    let _ = previous.close();
                }
                let mode =
                    crate::linux_bluetooth::PairingMode::new(PAIRING_SECONDS, self.events.clone())
                        .with_backend(self.pairing_backend.clone())
                        .with_secure_connections(self.secure_connections.clone());
                mode.start().map_err(|_| "error.pairing".to_string())?;
                *pairing = Some((
                    mode,
                    crate::admin::auth::monotonic_seconds() + u64::from(PAIRING_SECONDS),
                ));
                Ok(())
            }
            "cancel-pairing" => {
                if let Some((pairing, _)) = self.pairing.lock().unwrap().take() {
                    pairing.end("pairing-cancelled");
                }
                Ok(())
            }
            "enable-bluetooth" => self
                .backends
                .bluetooth
                .power_on()
                .map_err(|_| "error.radioBluetooth".into()),
            "enable-wifi" => crate::platform::command(
                self.runtime.runner.as_ref(),
                &["nmcli", "radio", "wifi", "on"],
                Duration::from_secs(3),
            )
            .map(|_| ())
            .map_err(|_| "error.radioWifi".into()),
            "start" => self.defer(Operation::Start).inspect_err(|_| {
                let detail = serde_json::json!({"action":"start"});
                self.events.diagnostics.refuse(&self.events, "busy", detail);
            }),
            "stop" | "cancel-arrival" => self.defer(Operation::Stop),
            "restart" => self.defer(Operation::Restart).inspect_err(|_| {
                let detail = serde_json::json!({"action":"restart"});
                self.events.diagnostics.refuse(&self.events, "busy", detail);
            }),
            "forget-wifi" => self.defer(Operation::Forget),
            "forget-carlink" => self.defer(Operation::ForgetCarlink),
            "clear-saved-head-unit" => self.defer(Operation::ClearSavedHeadUnit),
            "clear-window-records" => self
                .input
                .display
                .lock()
                .unwrap()
                .clear_records()
                .map_err(|_| "error.request".into()),
            // The child generates a new random browser code on each output start.
            "regenerate-pairing-code" => {
                let settings = self.settings.lock().unwrap();
                if !settings.require_pairing_code || settings.output == "carlink" {
                    return Err("error.action".into());
                }
                self.defer(Operation::RegeneratePairingCode)
            }
            "clear-vehicle-history" => self
                .vehicle
                .lock()
                .unwrap()
                .clear()
                .map_err(|_| "error.request".into()),
            "public-name-renew" => {
                self.public_name.renew();
                Ok(())
            }
            "public-name-remove" => self
                .public_name
                .remove()
                .map_err(|_| "error.request".to_string()),
            _ => Err("error.action".into()),
        }
    }
    fn carlink_connect(&self, pin: Option<&str>) -> AdminResult {
        if self.settings.lock().unwrap().output == "web" {
            return Err("error.action".into());
        }
        if pin
            .is_some_and(|p| !(4..=8).contains(&p.len()) || !p.bytes().all(|b| b.is_ascii_digit()))
            || (pin.is_none()
                && !has_trusted_carlink_for(
                    &self.options.state_dir,
                    self.settings.lock().unwrap().carlink_link == "ble",
                ))
        {
            return Err("error.carlinkPin".into());
        }
        self.defer(Operation::ConnectCarlink(pin.map(|pin| ArmedPin {
            pin: pin.into(),
            expires: std::time::Instant::now() + Duration::from_secs(110),
        })))
    }
    fn vehicle_sample(&self, enabled: bool) -> AdminResult {
        let enabled = enabled && self.settings.lock().unwrap().developer_active;
        self.vehicle.lock().unwrap().sample(
            enabled,
            std::time::Instant::now(),
            crate::platform::wall_clock().as_millis() as u64,
        );
        Ok(())
    }
    fn vehicle(&self) -> serde_json::Value {
        self.vehicle.lock().unwrap().view()
    }
    fn carlink_pairing(
        &self,
        request: u64,
        generation: u64,
        action: &str,
        pin: Option<&str>,
    ) -> AdminResult {
        let status = self.carlink.lock().unwrap();
        if !status.pin_requested
            || status.generation.unwrap_or(0) != generation
            || status.pin_request != Some(request)
            || status
                .pin_expiry
                .is_none_or(|d| d <= crate::admin::auth::monotonic_seconds())
            || !["approve", "pin", "cancel"].contains(&action)
            || (action == "approve" && !status.pin_confirmation)
            || (action == "pin"
                && (status.pin_confirmation
                    || pin.is_none_or(|p| {
                        p.is_empty() || p.len() > 128 || !p.bytes().all(|b| b.is_ascii_digit())
                    })))
        {
            return Err("error.carlinkPinNotRequested".into());
        }
        drop(status);
        let supervisor = self.supervisor.lock().unwrap();
        supervisor
            .as_ref()
            .ok_or("error.request")?
            .pairing(request, action, pin)
            .map_err(|_| "error.request".to_owned())?;
        let mut status = self.carlink.lock().unwrap();
        if status.pin_request == Some(request) {
            status.clear_pin();
            status.pin_incorrect = false;
        }
        Ok(())
    }
    fn carlink_pin(&self, pin: &str) -> AdminResult {
        if self.busy.load(Ordering::Acquire) {
            return Err("error.busy".into());
        }
        let (request, generation) = {
            let status = self.carlink.lock().unwrap();
            (
                status.pin_request.ok_or("error.carlinkPinNotRequested")?,
                status.generation.unwrap_or(0),
            )
        };
        self.carlink_pairing(request, generation, "pin", Some(pin))
    }

    fn phones(&self) -> Vec<AdminPhone> {
        let choice = self.effective_options().ok().and_then(|o| o.iphone);
        self.backends
            .bluetooth
            .devices()
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d.paired)
            .map(|d| AdminPhone {
                id: d.address.clone(),
                name: d.name.clone(),
                kind: "bluetooth".into(),
                connected: d.connected,
                likely_iphone: likely_iphone(&d),
                selected: choice
                    .as_ref()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&d.address)),
            })
            .collect()
    }
    fn hotspot_update(
        &self,
        ssid: &str,
        password: &str,
        band: Option<&str>,
        security: Option<&str>,
    ) -> AdminResult {
        let (_, saved_password) =
            crate::linux_hotspot::hotspot_credentials(&self.options.state_dir)
                .map_err(|_| "error.request".to_string())?;
        let password = if password.is_empty() {
            &saved_password
        } else {
            password
        };
        let selected = security
            .map(str::to_string)
            .unwrap_or_else(|| self.settings.lock().unwrap().hotspot_security.clone());
        if !crate::linux_hotspot::HOTSPOT_SECURITY.contains(&selected.as_str()) {
            return Err("error.invalid".into());
        }
        if !crate::linux_hotspot::security_supported(
            &selected,
            &self.supported_hotspot_security(
                &self
                    .effective_options()
                    .map_err(|_| "error.request".to_string())?
                    .wifi_interface,
            ),
        ) {
            return Err("error.hotspotSecurity".into());
        }
        crate::linux_hotspot::validate_hotspot_credentials(ssid, password)
            .map_err(str::to_owned)?;
        if band.is_some_and(|b| !["auto", "2.4", "5"].contains(&b)) {
            return Err("error.invalid".into());
        }
        self.defer_hotspot()?;
        {
            let mut current = self.settings.lock().unwrap();
            let mut next = current.clone();
            if let Some(band) = band {
                next.hotspot_band = band.into();
            }
            next.hotspot_security = selected;
            if next.save(&self.options.state_dir).is_err() {
                self.response_failed();
                return Err("error.request".into());
            }
            *current = next;
        }
        if crate::linux_hotspot::save_hotspot_credentials(&self.options.state_dir, ssid, password)
            .is_err()
        {
            self.response_failed();
            return Err("error.request".into());
        }
        self.events.failure_secrets(failure_secrets(
            &self.settings.lock().unwrap(),
            &self.options.state_dir,
        ));
        if self.settings.lock().unwrap().network_mode == "device-hotspot" {
            self.apply_pending.store(true, Ordering::Release);
        }
        Ok(())
    }
    fn receivers(&self) -> Vec<serde_json::Value> {
        self.receivers.snapshot(std::time::Instant::now())
    }
    fn show_view_area(&self, index: usize) -> AdminResult {
        if self.input.show_view_area(index) {
            Ok(())
        } else {
            Err("error.viewArea".into())
        }
    }
    fn receiver_history(&self) -> serde_json::Value {
        self.receivers.history()
    }
    fn wifi_status(&self) -> WifiClient {
        self.provision
            .try_lock()
            .map(|p| p.as_ref().map(|p| p.status()).unwrap_or_default())
            .unwrap_or_default()
    }
    fn wifi_networks(&self) -> std::result::Result<Vec<WifiNetwork>, String> {
        self.provision
            .try_lock()
            .map_err(|_| "error.busy".to_string())?
            .as_mut()
            .ok_or_else(|| "Wi-Fi interface unavailable".to_string())?
            .scan()
            .map_err(|e| e.to_string())
    }
    fn wifi_join(&self, ssid: &str, password: Option<&str>) -> AdminResult {
        let settings = self.settings.lock().unwrap();
        let password = password
            .or_else(|| {
                settings
                    .shared_wifi_networks
                    .iter()
                    .find(|n| n.ssid == ssid)
                    .map(|n| n.password.as_str())
            })
            .ok_or("error.networks")?;
        let network = SharedWifiNetwork {
            ssid: ssid.into(),
            password: password.into(),
        };
        if !network.valid()
            || (settings.shared_wifi_networks.len() >= 10
                && !settings.shared_wifi_networks.iter().any(|n| n.ssid == ssid))
        {
            return Err("error.networks".into());
        }
        self.defer(Operation::Join(ssid.into(), password.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("admin_contract_tests.rs");
    include!("identity_admin_tests.rs");
    #[test]
    fn wired_interfaces_never_reach_admin_or_mdns_host_addresses() {
        let (mut admin, dir) = fake();
        Arc::get_mut(&mut admin).unwrap().runtime.addresses = Arc::new(|| {
            Ok(vec![
                ("eth0".into(), "192.0.2.1".parse().unwrap(), 24),
                ("oxpusb0".into(), "192.0.2.2".parse().unwrap(), 24),
                ("oxpusb1".into(), "192.0.2.3".parse().unwrap(), 24),
            ])
        });
        assert_eq!(
            admin.host_addresses().unwrap(),
            ["192.0.2.1".parse::<std::net::Ipv4Addr>().unwrap()]
        );
        let status = admin.status();
        assert_eq!(status.host_addresses.len(), 1);
        assert_eq!(status.host_addresses[0].interface, "eth0");
        assert!(AdminHostAddress::new("oxpusb0".into(), "192.0.2.2".into()).developer_only);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn usb_pair_record_is_never_in_configuration_exports() {
        let (admin, dir) = fake();
        let path = dir.join("lockdown/pair-record.plist");
        crate::platform::private_state_file(
            &path,
            b"RootPrivateKey UNIQUE-LOCKDOWN-PRIVATE-MATERIAL",
        )
        .unwrap();
        for privacy in [false, true] {
            let value = admin.config_export(privacy).unwrap().to_string();
            for forbidden in [
                "lockdown",
                "pair-record",
                "RootPrivateKey",
                "UNIQUE-LOCKDOWN-PRIVATE-MATERIAL",
            ] {
                assert!(!value.contains(forbidden), "{forbidden}");
            }
        }
        assert!(path.exists());
        admin.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn transport_switch_builds_the_matching_controller_on_restart() {
        let (admin, dir) = fake();
        for transport in ["wireless", "usb", "wireless"] {
            admin.stop_controller_for_restart(true);
            admin.events.facts.lock().unwrap().reset_phone();
            admin.settings.lock().unwrap().transport = transport.into();
            admin.ensure_controller().unwrap();
            assert_eq!(
                admin
                    .controller
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .transport(),
                transport
            );
        }
        admin.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn usb_automatic_readiness_needs_identity_and_network_without_bluetooth_or_phone() {
        let (mut admin, dir) = fake();
        Arc::get_mut(&mut admin).unwrap().runtime.root = dir.clone();
        let net = dir.join("sys/class/net/wlan0");
        std::fs::create_dir_all(&net).unwrap();
        std::fs::write(net.join("address"), "02:00:00:00:00:01").unwrap();
        admin.desired.store(false, Ordering::Release);
        {
            let mut settings = admin.settings.lock().unwrap();
            settings.transport = "usb".into();
            settings.auto_start = true;
            settings.mfi_target = "local".into();
        }
        admin.observe_automatic_start().unwrap();
        assert!(
            admin.arrival.lock().unwrap().deadline.is_none(),
            "identity is required"
        );
        {
            let mut settings = admin.settings.lock().unwrap();
            settings.mfi_target = "remote".into();
            settings.mfi_remote_server = "http://127.0.0.1:9".into();
        }
        admin.observe_automatic_start().unwrap();
        assert!(
            admin.arrival.lock().unwrap().deadline.is_some(),
            "USB waits for a cable after automatic start"
        );
        admin.settings.lock().unwrap().transport = "wireless".into();
        admin.observe_automatic_start().unwrap();
        assert!(
            admin.arrival.lock().unwrap().deadline.is_none(),
            "wireless still needs Bluetooth and a selected iPhone"
        );
        admin.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn usb_status_uses_product_name_and_clears_it_on_unplug() {
        let (admin, dir) = fake();
        admin.settings.lock().unwrap().transport = "usb".into();
        admin.events.emit(
            "usb-presence",
            &serde_json::json!({"present":true,"product":"USB iPhone"}),
        );
        assert_eq!(admin.status().iphone.name.as_deref(), Some("USB iPhone"));
        admin
            .events
            .emit("usb-presence", &serde_json::json!({"present":false}));
        assert!(admin.status().iphone.name.is_none());
        admin.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn transport_defaults_cli_validation_and_restart_contract() {
        let (admin, dir) = fake();
        let args = vec!["--state-dir".to_owned(), dir.to_str().unwrap().to_owned()];
        let default = Options::parse(&args).unwrap();
        assert_eq!(default.transport, "wireless");
        assert_eq!(default.usb_charging_ma, 500);
        for ma in [500, 1500, 2400] {
            let mut args = args.clone();
            args.extend([
                "--transport".into(),
                "usb".into(),
                "--usb-charging-ma".into(),
                ma.to_string(),
            ]);
            let options = Options::parse(&args).unwrap();
            assert_eq!(options.transport, "usb");
            assert_eq!(options.settings.transport, "usb");
            assert_eq!(options.usb_charging_ma, ma);
        }
        for (key, value) in [
            ("--transport", "bluetooth"),
            ("--usb-charging-ma", "1000"),
            ("--usb-charging-ma", "-1"),
            ("--usb-charging-ma", "abc"),
        ] {
            let mut args = args.clone();
            args.extend([key.into(), value.into()]);
            assert!(Options::parse(&args).is_err());
        }
        let current = admin.settings.lock().unwrap().clone();
        let next = update_settings(
            &current,
            serde_json::json!({"transport":"usb"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(settings_restart_keys(&current, &next), ["transport"]);
        next.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).unwrap().transport, "usb");
        assert!(!serde_json::to_value(next)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("usbChargingMa"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failure_hotspot_password_redaction_preserves_public_default() {
        let (admin, dir) = fake();
        let line = "/var/lib/openxplay openxplay-linux.local openxplay-relay logs.openxplay.org";
        for source in ["carplay", "admin"] {
            for password in [DEFAULT_HOTSPOT_PASSWORD, "private-hotspot-pass"] {
                let settings = Settings {
                    admin_hotspot_password: Some(
                        if source == "admin" {
                            password
                        } else {
                            DEFAULT_HOTSPOT_PASSWORD
                        }
                        .into(),
                    ),
                    ..Settings::default()
                };
                crate::linux_hotspot::save_hotspot_credentials(
                    &dir,
                    "Relay",
                    if source == "carplay" {
                        password
                    } else {
                        DEFAULT_HOTSPOT_PASSWORD
                    },
                )
                .unwrap();
                let secrets = failure_secrets(&settings, &dir);
                assert!(!secrets
                    .iter()
                    .any(|value| value == DEFAULT_HOTSPOT_PASSWORD));
                assert_eq!(crate::failure_text::redact(line, &secrets), line);
                if password != DEFAULT_HOTSPOT_PASSWORD {
                    assert_eq!(
                        crate::failure_text::redact(&format!("{line} {password}"), &secrets),
                        format!("{line} [redacted]")
                    );
                }
            }
        }
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn log_export_hotspot_password_redaction_preserves_public_default() {
        let (admin, dir) = fake();
        let line = "/var/lib/openxplay openxplay-linux.local openxplay-relay logs.openxplay.org";
        for source in ["status", "saved", "legacy", "admin"] {
            for password in [DEFAULT_HOTSPOT_PASSWORD, "private-hotspot-pass"] {
                let settings = Settings {
                    admin_hotspot_password: Some(
                        if source == "admin" {
                            password
                        } else {
                            DEFAULT_HOTSPOT_PASSWORD
                        }
                        .into(),
                    ),
                    ..Settings::default()
                };
                let saved = if matches!(source, "saved" | "legacy") {
                    password
                } else {
                    DEFAULT_HOTSPOT_PASSWORD
                };
                crate::linux_hotspot::save_hotspot_credentials(&dir, "Relay", saved).unwrap();
                let mut status = admin.status();
                if source == "status" {
                    status.hotspot.as_mut().unwrap().password = password.into();
                } else {
                    status.hotspot = None;
                }
                if source == "legacy" {
                    crate::platform::private_state_file(
                        &dir.join("hotspot.credentials"),
                        format!("Relay\n{saved}\n").as_bytes(),
                    )
                    .unwrap();
                }
                let known = admin.log_export_known_values(&settings, &status).unwrap();
                assert!(!known
                    .iter()
                    .any(|(value, class)| value == DEFAULT_HOTSPOT_PASSWORD && *class == "secret"));
                let mut redactor = crate::logs::redactor::Redactor::new([0; 32]);
                for (value, class) in known {
                    redactor.remember(&value, class);
                }
                assert_eq!(redactor.redact(line), line);
                if password != DEFAULT_HOTSPOT_PASSWORD {
                    assert_eq!(
                        redactor.redact(&format!("{line} {password}")),
                        format!("{line} {}", redactor.token(password, "secret"))
                    );
                }
            }
        }
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn mfi_sources_android_user_envelope_and_restart_contract() {
        let (admin, dir) = fake();
        for (target, path, server) in [
            ("remote", "/dev/i2c-1", "https://mfi.example.org"),
            ("i2c", "/dev/i2c-2", "https://mfi.example.org"),
            ("usb-ch341", "/dev/i2c-2", "https://mfi.example.org"),
            ("local", "/dev/i2c-2", "https://mfi.example.org"),
        ] {
            let envelope = serde_json::json!({"format":"openxplay-user-config","version":1,"settings":{"mfiTarget":target,"mfiI2cPath":path,"mfiRemoteServer":server}});
            let bytes = serde_json::to_vec(&envelope).unwrap();
            let preview = admin.config_import(&bytes, true).unwrap();
            assert_eq!(preview["restartNeeded"], true);
            assert!(preview["ignored"].as_array().unwrap().is_empty());
            assert_eq!(admin.config_import(&bytes, false).unwrap(), preview);
            let exported = admin.config_export(false).unwrap();
            assert_eq!(exported["settings"]["mfiTarget"], target);
            assert_eq!(exported["settings"]["mfiI2cPath"], path);
            assert_eq!(exported["settings"]["mfiRemoteServer"], server);
        }
        for values in [
            serde_json::json!({"mfiTarget":"unknown"}),
            serde_json::json!({"mfiTarget":"i2c","mfiI2cPath":"/tmp/device"}),
            serde_json::json!({"mfiTarget":"remote","mfiRemoteServer":"file:///tmp/server"}),
        ] {
            assert!(matches!(
                admin.update(values.as_object().unwrap().clone()),
                AdminUpdate::Errors(_)
            ));
        }
        assert_eq!(admin.settings.lock().unwrap().mfi_target, "local");
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn mfi_token_is_write_only_private_exportable_and_applied_only_after_preview() {
        use std::os::unix::fs::PermissionsExt;
        let (admin, dir) = fake();
        let bytes=br#"{"format":"openxplay-privacy-config","version":1,"mfiRemoteToken":"mfi-test-secret"}"#;
        let preview = admin.config_import(bytes, true).unwrap();
        assert_eq!(preview["nextStart"], serde_json::json!(["mfiRemoteToken"]));
        assert_eq!(preview["restartNeeded"], true);
        assert_eq!(crate::mfi_sources::token(&dir).unwrap(), "");
        assert_eq!(admin.config_import(bytes, false).unwrap(), preview);
        assert_eq!(
            std::fs::metadata(dir.join("mfi-remote-token"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let settings = admin.settings();
        assert_eq!(settings.values["mfiRemoteTokenSet"], true);
        assert_eq!(settings.values["privacyConfigAvailable"], true);
        assert_eq!(settings.values["publicNamePrivacyAvailable"], false);
        assert!(!serde_json::to_string(&settings)
            .unwrap()
            .contains("mfi-test-secret"));
        assert!(!admin
            .config_export(false)
            .unwrap()
            .to_string()
            .contains("mfi-test-secret"));
        assert!(!std::fs::read_to_string(dir.join("settings.json"))
            .unwrap()
            .contains("mfi-test-secret"));
        assert_eq!(
            admin.config_export(true).unwrap()["mfiRemoteToken"],
            "mfi-test-secret"
        );
        assert!(failure_secrets(&admin.settings.lock().unwrap(), &dir)
            .contains(&"mfi-test-secret".into()));
        assert!(matches!(admin.update(serde_json::json!({"mfiRemoteServer":"https://mfi.example.org","mfiTarget":"remote"}).as_object().unwrap().clone()),AdminUpdate::NextStart));
        assert_eq!(crate::mfi_sources::token(&dir).unwrap(), "mfi-test-secret");
        assert_eq!(admin.settings().values["mfiSourceStatus"], "ready");
        assert!(matches!(
            admin.update(
                serde_json::json!({"mfiRemoteToken":"bad\nsecret"})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::Errors(_)
        ));
        assert_eq!(crate::mfi_sources::token(&dir).unwrap(), "mfi-test-secret");
        assert!(matches!(
            admin.update(
                serde_json::json!({"mfiRemoteToken":""})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::NextStart
        ));
        assert_eq!(crate::mfi_sources::token(&dir).unwrap(), "");
        assert_eq!(admin.settings().values["mfiRemoteTokenSet"], false);
        assert_eq!(admin.settings().values["privacyConfigAvailable"], false);
        assert!(admin
            .config_export(true)
            .unwrap()
            .get("mfiRemoteToken")
            .is_none());
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn admin_contract_mfi_sources() {
        let (admin, dir) = fake();
        crate::mfi_sources::save_token(&dir, "fixture-secret").unwrap();
        for target in ["remote", "i2c", "usb-ch341"] {
            {
                let mut settings = admin.settings.lock().unwrap();
                settings.mfi_target = target.into();
                settings.mfi_remote_server = "https://mfi.example.org".into();
            }
            let mut settings = admin.settings();
            settings
                .values
                .insert("experimentalBluetoothAddress".into(), "".into());
            // Pin host-only capability facts; the device backends have their own fake-driver tests.
            settings.values.insert(
                "mfiSourceStatus".into(),
                if target == "remote" {
                    "ready"
                } else {
                    "missing"
                }
                .into(),
            );
            contract_fixture(
                &format!("settings-mfi-{target}"),
                &serde_json::to_value(settings).unwrap(),
            );
        }
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn config_file_preview_apply_and_validation_match_the_settings_editor() {
        let (admin, dir) = fake();
        let before = std::fs::read(dir.join("settings.json")).unwrap();
        let file = serde_json::json!({"format":"openxplay-user-config","version":1,"platform":"android","settings":{"outputs":["web","carlink"],"autoStartOnBoot":false,"localPreview":true}});
        let bytes = serde_json::to_vec(&file).unwrap();
        let preview = admin.config_import(&bytes, true).unwrap();
        assert_eq!(
            preview,
            serde_json::json!({"applied":["bootStart","output"],"ignored":["localPreview"],"restartNeeded":false,"appliedNow":["bootStart","output"],"nextStart":[]})
        );
        assert_eq!(std::fs::read(dir.join("settings.json")).unwrap(), before);
        assert!(admin.pending.lock().unwrap().is_empty());
        assert_eq!(admin.config_import(&bytes, false).unwrap(), preview);
        assert_eq!(admin.settings.lock().unwrap().output, "both");
        assert!(!admin.settings.lock().unwrap().boot_start);
        assert!(!admin.pending.lock().unwrap().is_empty());
        admin.response_failed();
        let invalid=serde_json::to_vec(&serde_json::json!({"format":"openxplay-user-config","version":1,"settings":{"receiverPort":6975}})).unwrap();
        let preview_errors = admin.config_import(&invalid, true).unwrap_err();
        let AdminUpdate::Errors(editor_errors) =
            admin.update(serde_json::from_value(serde_json::json!({"receiverPort":6975})).unwrap())
        else {
            panic!("invalid port accepted")
        };
        assert_eq!(preview_errors["receiverPort"], "invalid");
        assert!(editor_errors["settings"].contains("receiverPort"));
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn config_hotspot_backup_partial_import_and_restart_report_follow_runtime() {
        let (admin, dir) = fake();
        crate::linux_hotspot::save_hotspot_credentials(&dir, "Original", "openxplay").unwrap();
        let exported = admin.config_export(false).unwrap();
        assert_eq!(exported["settings"]["hotspotSsid"], "Original");
        assert_eq!(exported["settings"]["hotspotPassphrase"], "openxplay");
        assert_eq!(exported["settings"]["hotspotSecurity"], "wpa2");
        let file = serde_json::to_vec(&serde_json::json!({"format":"openxplay-user-config","version":1,"settings":{
            "hotspotSsid":"Imported", "car":{"brand":"other","model":"other","year":"other","yearText":"2027 custom"}, "hotspotBand":"ghz-5", "videoStats":"seconds-15", "experimental":{"version":2,"sendBluetoothAddress":false}
        }})).unwrap();
        let preview = admin.config_import(&file, true).unwrap();
        assert_eq!(preview["nextStart"], serde_json::json!(["hotspotBand"]));
        assert_eq!(preview["restartNeeded"], true);
        assert_eq!(
            preview["appliedNow"],
            serde_json::json!([
                "carBrand",
                "carModel",
                "carYear",
                "carYearText",
                "experimentalSendBluetoothAddress",
                "hotspotSsid",
                "videoStats"
            ])
        );
        assert_eq!(
            crate::linux_hotspot::hotspot_credentials(&dir).unwrap().0,
            "Original"
        );
        assert_eq!(admin.config_import(&file, false).unwrap(), preview);
        assert!(matches!(
            admin.pending.lock().unwrap().values().next(),
            Some(Operation::Hotspot(None))
        ));
        assert_eq!(
            crate::linux_hotspot::hotspot_credentials(&dir).unwrap(),
            ("Imported".into(), "openxplay".into())
        );
        assert_eq!(
            admin.config_export(false).unwrap()["settings"]["car"]["yearText"],
            "2027 custom"
        );
        admin.response_failed();
        let unsupported = serde_json::to_vec(&serde_json::json!({"format":"openxplay-user-config","version":1,"settings":{"hotspotSecurity":"wpa3"}})).unwrap();
        assert_eq!(
            admin.config_import(&unsupported, true).unwrap_err()["hotspotSecurity"],
            "error.hotspotSecurity"
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn privacy_file_dry_run_does_not_write_and_apply_reuses_public_name_import() {
        let (admin, dir) = fake();
        admin.settings.lock().unwrap().unknown.insert(
            "linux-local-extension".into(),
            serde_json::json!({"keep":true}),
        );
        let before = admin.settings.lock().unwrap().clone();
        std::fs::write(
            dir.join("hotspot.credentials"),
            b"unrelated invalid hotspot data",
        )
        .unwrap();
        let public = serde_json::json!({"version":1,"enabled":false,"provider":"dynv6","name":"test.dynv6.net","token":"private-token","agreeLetsEncryptTerms":true});
        let bytes=serde_json::to_vec(&serde_json::json!({"format":"openxplay-privacy-config","version":1,"publicName":public})).unwrap();
        let preview = admin.config_import(&bytes, true).unwrap();
        assert!(!dir.join("public-name.json").exists());
        assert_eq!(admin.config_import(&bytes, false).unwrap(), preview);
        assert_eq!(admin.config_export(true).unwrap()["publicName"], public);
        assert_eq!(
            serde_json::to_value(&*admin.settings.lock().unwrap()).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        std::fs::remove_file(dir.join("hotspot.credentials")).unwrap();
        assert!(!admin
            .config_export(false)
            .unwrap()
            .to_string()
            .contains("private-token"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.join("public-name.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn experimental_master_and_phone_address_park_until_next_connection() {
        let (admin, dir) = fake();
        let file = serde_json::to_vec(&serde_json::json!({"format":"openxplay-user-config","version":1,"settings":{
            "experimental":{"version":2,"active":false,"sendBluetoothAddress":false,"bluetoothAddress":"02:11:22:33:44:55"}
        }})).unwrap();
        let preview = admin.config_import(&file, true).unwrap();
        assert_eq!(preview["restartNeeded"], false);
        assert_eq!(preview["nextStart"], serde_json::json!([]));
        assert_eq!(admin.config_import(&file, false).unwrap(), preview);
        assert!(admin.pending.lock().unwrap().is_empty());
        assert_eq!(
            admin
                .input
                .connection_settings()
                .experimental_bluetooth_address,
            "02:11:22:33:44:55"
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn one_developer_master_parks_calibration_and_resets_vehicle_sample() {
        let (admin, dir) = fake();
        assert_eq!(
            settings_schema()
                .iter()
                .flat_map(|s| &s.fields)
                .filter(|f| f.key == "developerActive")
                .count(),
            1
        );
        admin.vehicle_sample(true).unwrap();
        assert_eq!(admin.vehicle()["sample"], true);
        let values = serde_json::json!({"developerActive":false,"physicalWidthEnabled":true,"physicalWidthOverrideMm":325});
        assert!(matches!(
            admin.update(values.as_object().unwrap().clone()),
            AdminUpdate::NextStart
        ));
        assert_eq!(admin.vehicle()["sample"], false);
        admin.vehicle_sample(true).unwrap();
        assert_eq!(admin.vehicle()["sample"], false);
        let saved = Settings::load(&dir).unwrap();
        assert!(!saved.developer_active);
        assert!(saved.physical_width_enabled);
        assert_eq!(saved.physical_width_override_mm, 325);
        admin.response_failed();
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn experimental_choices_use_the_schema_editor_and_reject_invalid_types_atomically() {
        let (admin, dir) = fake();
        let schema = settings_schema();
        let section = schema.iter().find(|s| s.id == "experimental").unwrap();
        for (key, next_start) in [
            ("experimentalIapChannelWhenOffered", true),
            ("experimentalReleaseBluetoothOnRequest", true),
            ("experimentalAwaitIphoneReconnect", true),
            ("experimentalRadioConflictPrompt", false),
            ("experimentalStallWatchdog", false),
        ] {
            let field = section.fields.iter().find(|f| f.key == key).unwrap();
            assert_eq!(field.field_type, "bool");
            assert_eq!(field.label_key, format!("field.{key}"));
            assert_eq!(field.hint_key, Some(format!("hint.field.{key}")));
            assert_eq!(admin.settings().values[key], true);
            for invalid in [
                serde_json::json!("false"),
                serde_json::json!(0),
                serde_json::Value::Null,
                serde_json::json!([]),
                serde_json::json!({}),
            ] {
                let before = admin.settings.lock().unwrap().clone();
                assert!(matches!(
                    admin.update(
                        serde_json::json!({key:invalid,"bootStart":true})
                            .as_object()
                            .unwrap()
                            .clone()
                    ),
                    AdminUpdate::Errors(_)
                ));
                assert_eq!(*admin.settings.lock().unwrap(), before);
            }
            let result = admin.update(
                serde_json::json!({key:false,"experimentalActive":false})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
            assert!(if next_start {
                matches!(result, AdminUpdate::NextStart)
            } else {
                matches!(result, AdminUpdate::AppliedNow)
            });
            assert_eq!(admin.settings().values[key], false);
            assert_eq!(
                serde_json::to_value(Settings::load(&dir).unwrap()).unwrap()[key],
                false
            );
            assert_eq!(
                serde_json::to_value(admin.input.connection_settings()).unwrap()[key],
                false
            );
            assert!(admin.pending.lock().unwrap().is_empty());
        }
        assert!(matches!(
            admin.update(
                serde_json::json!({"experimentalActive":true})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::AppliedNow
        ));
        assert_eq!(
            admin.settings.lock().unwrap().effective_experimental(),
            crate::settings::ExperimentalChoices {
                iap_channel_when_offered: false,
                release_bluetooth_on_request: false,
                await_iphone_reconnect: false,
                radio_conflict_prompt: false,
                stall_watchdog: false,
            }
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn android_new_experiment_import_reports_connection_choices_and_live_observers() {
        let (admin, dir) = fake();
        for enabled in [false, true] {
            let before = admin.settings.lock().unwrap().clone();
            let bytes = serde_json::to_vec(&serde_json::json!({
                "format":"openxplay-user-config","version":1,"platform":"android",
                "settings":{"experimental":{"version":2,
                    "iapChannelWhenOffered":enabled,"releaseBluetoothOnRequest":enabled,
                    "awaitIphoneReconnect":enabled,"radioConflictPrompt":enabled,"stallWatchdog":enabled}}
            })).unwrap();
            let report = admin.config_import(&bytes, true).unwrap();
            assert_eq!(*admin.settings.lock().unwrap(), before);
            assert_eq!(report["ignored"], serde_json::json!([]));
            assert_eq!(report["applied"].as_array().unwrap().len(), 5);
            assert_eq!(
                report["nextStart"],
                serde_json::json!([
                    "experimentalAwaitIphoneReconnect",
                    "experimentalIapChannelWhenOffered",
                    "experimentalReleaseBluetoothOnRequest"
                ])
            );
            assert_eq!(
                report["appliedNow"],
                serde_json::json!([
                    "experimentalRadioConflictPrompt",
                    "experimentalStallWatchdog"
                ])
            );
            assert_eq!(report["restartNeeded"], true);
            assert_eq!(admin.config_import(&bytes, false).unwrap(), report);
            assert!(admin.pending.lock().unwrap().is_empty());
            let saved = Settings::load(&dir).unwrap();
            assert_eq!(saved, *admin.settings.lock().unwrap());
            assert_eq!(saved, admin.input.connection_settings());
            let output = admin.config_export(false).unwrap();
            for wire in [
                "iapChannelWhenOffered",
                "releaseBluetoothOnRequest",
                "awaitIphoneReconnect",
                "radioConflictPrompt",
                "stallWatchdog",
            ] {
                assert_eq!(output["settings"]["experimental"][wire], enabled);
                let mut invalid = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
                invalid["settings"]["experimental"][wire] = "false".into();
                invalid["settings"]["bootStart"] = true.into();
                assert!(admin
                    .config_import(&serde_json::to_vec(&invalid).unwrap(), false)
                    .is_err());
                assert_eq!(*admin.settings.lock().unwrap(), saved);
                assert_eq!(Settings::load(&dir).unwrap(), saved);
            }
        }
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn android_experiment_import_stages_next_connection_without_restarting_output() {
        let (admin, dir) = fake();
        let before = admin.settings.lock().unwrap().clone();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "format":"openxplay-user-config","version":1,"platform":"android",
            "settings":{"experimental":{"version":2,"screenSessionIsolation":false,
                "screenReconnectReset":false,"screenDeliverySerialization":false,
                "remotePlaybackProof":false,"startupStatus":false,"startupDiagnostics":false}}
        }))
        .unwrap();
        let report = admin.config_import(&bytes, true).unwrap();
        assert_eq!(*admin.settings.lock().unwrap(), before);
        assert_eq!(report["restartNeeded"], false);
        assert_eq!(report["ignored"], serde_json::json!([]));
        assert_eq!(report["applied"].as_array().unwrap().len(), 6);
        assert_eq!(admin.config_import(&bytes, false).unwrap(), report);
        assert!(admin.pending.lock().unwrap().is_empty());
        assert!(
            !admin
                .input
                .connection_settings()
                .experimental_screen_session_isolation
        );
        assert!(
            !crate::settings::Settings::load(&dir)
                .unwrap()
                .experimental_startup_status
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn developer_master_gates_effective_options_and_trace_but_keeps_raw_choices() {
        let (admin, dir) = fake();
        {
            let mut settings = admin.settings.lock().unwrap();
            settings.developer_active = false;
            settings.carlink_trace = "on".into();
            settings.receiver_port = 9443;
            settings.carlink_model = "Parked model".into();
            settings.carlink_link = "static".into();
            settings.carlink_head_unit = "192.168.55.1".into();
        }
        assert_eq!(admin.receiver_port().unwrap(), 8443);
        assert_eq!(
            admin.effective_options().unwrap().settings.carlink_link,
            "ble"
        );
        assert_eq!(
            admin
                .effective_options()
                .unwrap()
                .settings
                .carlink_head_unit,
            "192.168.43.1"
        );
        assert!(!admin.carlink_trace().unwrap().0);
        assert_eq!(
            admin.effective_options().unwrap().settings.carlink_model,
            crate::branding::MODEL
        );
        assert_eq!(admin.settings().values["receiverPort"], 9443);
        assert_eq!(admin.settings().values["carlinkTrace"], "on");
        admin.settings.lock().unwrap().developer_active = true;
        assert_eq!(admin.receiver_port().unwrap(), 9443);
        assert_eq!(
            admin.effective_options().unwrap().settings.carlink_link,
            "static"
        );
        assert_eq!(
            admin
                .effective_options()
                .unwrap()
                .settings
                .carlink_head_unit,
            "192.168.55.1"
        );
        assert!(admin.carlink_trace().unwrap().0);
        assert_eq!(
            admin.effective_options().unwrap().settings.carlink_model,
            "Parked model"
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn hotspot_rename_keeps_password_and_open_security_keeps_it_parked() {
        let (admin, dir) = fake();
        let unsupported = serde_json::to_vec(&serde_json::json!({"format":"openxplay-user-config","version":1,"settings":{"hotspotSecurity":"wpa3"}})).unwrap();
        for dry_run in [true, false] {
            assert_eq!(
                admin.config_import(&unsupported, dry_run).unwrap_err()["hotspotSecurity"],
                "error.hotspotSecurity"
            );
        }

        crate::linux_hotspot::save_hotspot_credentials(&dir, "Before", "saved-password").unwrap();
        admin
            .hotspot_update("Renamed", "", None, Some("open"))
            .unwrap();
        assert_eq!(
            crate::linux_hotspot::hotspot_credentials(&dir).unwrap(),
            ("Renamed".into(), "saved-password".into())
        );
        assert_eq!(admin.settings.lock().unwrap().hotspot_security, "open");
        assert!(!admin.status().hotspot.unwrap().has_password);
        admin.response_failed();
        admin
            .hotspot_update("Renamed again", "", None, Some("wpa2"))
            .unwrap();
        assert_eq!(admin.settings.lock().unwrap().hotspot_security, "wpa2");
        assert_eq!(
            crate::linux_hotspot::hotspot_credentials(&dir).unwrap().1,
            "saved-password"
        );
        admin.response_failed();
        assert_eq!(
            admin.hotspot_update("Unsupported", "", None, Some("wpa3")),
            Err("error.hotspotSecurity".into())
        );
        assert_eq!(
            crate::linux_hotspot::hotspot_credentials(&dir).unwrap().0,
            "Renamed again"
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn network_roles_api_auth_preview_probing_persistence_and_application() {
        use std::os::unix::fs::PermissionsExt;
        let (a, dir) = fake();
        let original = a.effective_options().unwrap();
        let mut auth = AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap();
        let token = auth
            .login("127.0.0.1".parse().unwrap(), "openxplay", 100)
            .ok()
            .unwrap();
        let server = AdminServer::new(
            a.clone(),
            auth,
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        let mut request = Request {
            method: "PUT".into(), path: "/api/v0/network-roles".into(),
            headers: BTreeMap::from([("content-type".into(), "application/json".into()), ("x-openxplay-admin".into(), "1".into())]),
            body: br#"{"networkRoles":{"mode":"auto"},"wifiCountry":"CN","adminHotspotPassword":"private-admin-pass"}"#.to_vec(),
            peer: "127.0.0.1".parse().unwrap(),
        };
        assert_eq!(server.route(&request, 100).status, 401);
        request.method = "POST".into();
        request.path = "/api/v0/network-roles/preview".into();
        assert_eq!(server.route(&request, 100).status, 401);
        request
            .headers
            .insert("cookie".into(), format!("oxp_admin={token}"));
        request.headers.remove("x-openxplay-admin");
        assert_eq!(server.route(&request, 100).status, 403);
        request.method = "PUT".into();
        request.path = "/api/v0/network-roles".into();
        assert_eq!(server.route(&request, 100).status, 403);
        request
            .headers
            .insert("x-openxplay-admin".into(), "1".into());
        let probing = serde_json::to_value(a.status()).unwrap();
        assert_eq!(probing["network"]["probing"], true);
        assert_eq!(probing["network"]["checks"], serde_json::json!([]));
        for (method, path) in [
            ("PUT", "/api/v0/network-roles"),
            ("POST", "/api/v0/network-roles/preview"),
        ] {
            request.method = method.into();
            request.path = path.into();
            let response = server.route(&request, 100);
            assert_eq!(response.status, 503);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&response.body).unwrap(),
                serde_json::json!({"error":"probing"})
            );
        }
        a.inventory.set(crate::net_inventory::Inventory {
            probed_at: Some(1),
            ..Default::default()
        });
        let before_preview = std::fs::read(dir.join("settings.json")).ok();
        let preview = server.route(&request, 100);
        assert_eq!(preview.status, 200);
        let preview: serde_json::Value = serde_json::from_slice(&preview.body).unwrap();
        assert!(preview.get("applied").is_none());
        assert!(preview["changes"].is_array());
        assert_eq!(
            std::fs::read(dir.join("settings.json")).ok(),
            before_preview
        );
        assert!(a.pending.lock().unwrap().is_empty());
        request.method = "PUT".into();
        request.path = "/api/v0/network-roles".into();
        let response = server.route(&request, 100);
        assert_eq!(response.status, 200);
        let response: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(response["applied"], true);
        assert!(!response.to_string().contains("private-admin-pass"));
        let saved = Settings::load(&dir).unwrap();
        assert_eq!(saved.wifi_country, "CN");
        assert_eq!(
            saved.admin_hotspot_password.as_deref(),
            Some("private-admin-pass")
        );
        assert_eq!(
            std::fs::metadata(dir.join("settings.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let status = serde_json::to_value(a.status()).unwrap();
        assert!(status["network"]["interfaces"].is_array());
        assert_eq!(status["network"]["country"]["configured"], "CN");
        assert_eq!(
            status["network"]["plan"]["adminHotspot"]["passwordSet"],
            true
        );
        assert_eq!(status["network"]["applied"], true);
        assert_eq!(status["network"]["mode"], original.settings.network_mode);
        for value in [status, serde_json::to_value(a.settings()).unwrap()] {
            assert!(!value.to_string().contains("private-admin-pass"));
            assert!(!value.to_string().contains("adminHotspotPassword\":"));
        }
        let pending = a
            .pending
            .lock()
            .unwrap()
            .remove(&std::thread::current().id())
            .unwrap();
        assert!(
            matches!(pending, Operation::Roles(ref changes) if changes.contains(&"wifiCountry".into()) && changes.contains(&"adminHotspot".into()))
        );
        request.body = serde_json::to_vec(&serde_json::json!({"networkRoles":{
            "mode":"custom", "iphone":{"link":"saved-network","interface":"02:00:00:00:00:09"},
            "carBrowser":{"network":"iphone","interface":null}, "carlink":{"interface":null},
            "adminHotspot":{"enabled":false,"interface":null,"ssid":"Test-Admin","band":"2.4","channel":0}
        }})).unwrap();
        let custom = server.route(&request, 100);
        assert_eq!(custom.status, 200);
        let custom: serde_json::Value = serde_json::from_slice(&custom.body).unwrap();
        assert_eq!(custom["plan"]["iphone"]["link"], "saved-network");
        assert_eq!(custom["plan"]["iphone"]["missing"], true);
        assert_eq!(Settings::load(&dir).unwrap().network_roles.mode, "custom");
        assert_eq!(
            Settings::load(&dir).unwrap().admin_hotspot_password,
            saved.admin_hotspot_password
        );
        let before = std::fs::read(dir.join("settings.json")).unwrap();
        for body in [
            serde_json::json!({"networkRoles":{"mode":"auto","adminHotspot":{"enabled":true,"interface":null,"ssid":"Test-Admin","band":"5","channel":0}}}),
            serde_json::json!({"networkRoles":{"mode":"auto"},"adminHotspotPassword":"short"}),
            serde_json::json!({"networkRoles":{"mode":"auto"},"wifiCountry":"ZZ"}),
            serde_json::json!({"networkRoles":{"mode":"auto"},"wifiCountry":null}),
            serde_json::json!({"networkRoles":{"mode":"auto"},"adminHotspotPassword":null}),
            serde_json::json!({"networkRoles":{"mode":"auto","extra":true}}),
            serde_json::json!({"networkRoles":{"mode":"auto"},"extra":true}),
        ] {
            request.body = serde_json::to_vec(&body).unwrap();
            if body["networkRoles"]["adminHotspot"].is_object() {
                request.method = "POST".into();
                request.path = "/api/v0/network-roles/preview".into();
                let preview = server.route(&request, 100);
                assert_eq!(preview.status, 200);
                let preview: serde_json::Value = serde_json::from_slice(&preview.body).unwrap();
                assert!(preview["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["level"] == "error"));
                assert_eq!(std::fs::read(dir.join("settings.json")).unwrap(), before);
                request.method = "PUT".into();
                request.path = "/api/v0/network-roles".into();
            }
            let response = server.route(&request, 100);
            assert_eq!(response.status, 400);
            if body["networkRoles"]["adminHotspot"].is_object() {
                let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
                assert!(value["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["level"] == "error" && c["code"] == "no-ap-channel"));
            }
            assert_eq!(std::fs::read(dir.join("settings.json")).unwrap(), before);
        }
        let after = a.effective_options().unwrap();
        assert_eq!(after.wifi_interface, "02:00:00:00:00:09");
        assert_eq!(after.settings.network_mode, "shared-wifi");
        let pending = a
            .pending
            .lock()
            .unwrap()
            .remove(&std::thread::current().id())
            .unwrap();
        assert!(
            matches!(pending, Operation::Roles(ref changes) if changes.contains(&"controller".into()))
        );
        assert!(!a.restart_required.load(Ordering::Acquire));
        assert!(!a.apply_pending.load(Ordering::Acquire));
        std::fs::remove_dir_all(dir).unwrap();
    }
    struct RuntimeRunner;
    impl CommandRunner for RuntimeRunner {
        fn run(&self, _: &[String], _: Duration) -> Result<CommandResult> {
            Ok(CommandResult {
                exit_code: 0,
                output: String::new(),
            })
        }
    }
    fn runtime_paths(dir: &std::path::Path) -> crate::runtime_paths::RuntimePaths {
        let mut paths =
            crate::runtime_paths::RuntimePaths::in_state_dir(dir, Arc::new(RuntimeRunner));
        // Most admin tests supply inventory snapshots, including the initial probing state.
        paths.inventory_refresh = InventoryRefresh::Manual;
        paths.local_dns.hosts = dir.join("run/hotspot-hosts");
        paths
    }
    fn fake() -> (Arc<LinuxRelayAdmin>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "wp4a-admin-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let options =
            Options::parse(&["--state-dir".into(), dir.to_str().unwrap().into()]).unwrap();
        let state = Arc::new(PersistentState::open(&dir).unwrap());
        // Existing-installation lifecycle fixtures explicitly enable boot startup.
        std::fs::write(dir.join("settings.json"), br#"{"bootStart":true}"#).unwrap();
        let events = Arc::new(JsonEvents::new(Arc::new(|_| {})));
        let provision = WifiProvisioner::new(
            ProcessNetworkManagerCli {
                executable: "/bin/false".into(),
            },
            "wlan0".into(),
            "AA:BB:CC:DD:EE:FF",
            &dir,
        )
        .unwrap();
        let mut admin = LinuxRelayAdmin::with_provisioner(
            options,
            state,
            events,
            provision,
            runtime_paths(&dir),
        )
        .unwrap();
        admin.tesla = Mutex::new(
            crate::tesla_address::Session::new(Arc::new(
                crate::tesla_address::tests::FakeBackend::default(),
            ))
            .with_ownership(admin.runtime.tesla_address.clone()),
        );
        // These legacy lifecycle tests bypass the new car waits; dedicated tests exercise them.
        admin.start_policy.lock().unwrap().skip();
        admin.start_policy.lock().unwrap().skip();
        admin.net_root = dir.join("present-net");
        std::fs::create_dir_all(admin.net_root.join("wlan0")).unwrap();
        std::fs::write(admin.net_root.join("wlan0/address"), "02:00:00:00:00:01").unwrap();
        (Arc::new(admin), dir)
    }
    struct TlsSessionControl;
    impl carplay_airplay::SessionControl for TlsSessionControl {
        fn send_touch(&self, _: &[carplay_airplay::AirPlayContact]) -> Result<bool> {
            Ok(false)
        }
        fn send_media_button(&self, _: u8) -> Result<bool> {
            Ok(false)
        }
    }
    fn tls_fake_admin() -> (
        Arc<LinuxRelayAdmin>,
        std::path::PathBuf,
        Arc<crate::supervisor::receiver_tls_tests::FakeChildFactory>,
        std::sync::mpsc::Receiver<crate::supervisor::receiver_tls_tests::Request>,
    ) {
        let (mut admin, dir) = fake();
        let (factory, rx) = crate::supervisor::receiver_tls_tests::FakeChildFactory::new();
        let inner = Arc::get_mut(&mut admin).unwrap();
        inner.child_factory = factory.clone();
        inner.runtime.addresses =
            Arc::new(|| Ok(vec![("wlan0".into(), "10.77.0.1".parse().unwrap(), 24)]));
        admin.start_supervisor().unwrap();
        factory.wait_started();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !admin.supervisor.lock().unwrap().as_ref().unwrap().running() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        (admin, dir, factory, rx)
    }
    fn next_tls_operation(admin: &LinuxRelayAdmin) -> Operation {
        let operation = admin
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .unwrap();
        assert!(matches!(operation, Operation::ReceiverTls));
        operation
    }
    #[test]
    fn child_encoder_event_unlocks_dual_output_hevc_without_replacing_the_browser() {
        let (mut admin, dir) = fake();
        {
            let mut settings = admin.settings.lock().unwrap();
            settings.output = "both".into();
            settings.developer_active = true;
            settings.carlink_picture_size = "mode".into();
        }
        let (factory, rx) = crate::supervisor::receiver_tls_tests::FakeChildFactory::new();
        Arc::get_mut(&mut admin).unwrap().child_factory = factory.clone();
        admin.start_supervisor().unwrap();
        factory.wait_started();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !admin.supervisor.lock().unwrap().as_ref().unwrap().running() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        let event = factory.events.lock().unwrap().clone().unwrap();
        let caps = serde_json::json!({"type":"transcode-capabilities","output":"clink","enabled":true,"available":true,"decode":["H264"],"encode":[{"codec":"H264","maxWidth":1920,"maxHeight":1080},{"codec":"H265","maxWidth":1920,"maxHeight":1080}],"scaling":true,"description":"fake encoder"});
        event(caps.clone());
        let operation = admin
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(matches!(operation, Operation::Carlink));
        admin.operation(operation).unwrap();
        let mut command = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        while command.value["hevc"] != "auto" {
            command = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        assert_eq!(command.value["type"], "clink-restart");
        assert_eq!(
            command.value["picture"],
            serde_json::json!({"width":1280,"height":720})
        );
        assert!(admin.input.display.lock().unwrap().transcode_geometry);
        event(caps);
        assert!(admin
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .is_err());
        assert_eq!(factory.launches.lock().unwrap().len(), 1);
        assert_eq!(factory.stops.load(Ordering::SeqCst), 0);
        assert_eq!(admin.status().transcoding["clink"]["available"], true);
        event(
            serde_json::json!({"type":"receiver-info","client":"browser-1","info":{"refreshHz":60}}),
        );
        event(
            serde_json::json!({"type":"receiver-disconnected","client":"browser-1","code":1000,"reason":"closed"}),
        );
        assert!(admin.receivers().is_empty());
        assert_eq!(admin.receiver_history()["disconnects"][0]["code"], 1000);
        assert_eq!(
            admin.receiver_history()["disconnects"][0]["report"]["info"]["refreshHz"],
            60
        );
        admin.shutdown();
        drop(command);
        drop(event);
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tesla_direct_switch_and_coalesced_updates_hot_swap_during_the_same_session() {
        use crate::tesla_address::Mode;
        let (admin, dir, factory, rx) = tls_fake_admin();
        let session = admin.input.begin(Arc::new(TlsSessionControl)).unwrap();
        admin.settings.lock().unwrap().tesla_browser = Mode::PublicAddress;
        admin.apply_public_name();
        admin.apply_public_name();
        let operation = next_tls_operation(&admin);
        assert!(admin
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .is_err());
        // A later edit supersedes the queued address before any request is sent.
        admin.settings.lock().unwrap().tesla_browser = Mode::TeslaAndroid;
        let settings = admin.settings.lock().unwrap().clone();
        admin
            .tesla
            .lock()
            .unwrap()
            .tick(true, true, &settings, false, || Ok(vec![]), 1);
        {
            let mut facts = admin.events.facts.lock().unwrap();
            facts.web.phase = "LISTENING".into();
            facts.web.port = 8443;
        }
        std::thread::scope(|scope| {
            let update = scope.spawn(|| admin.perform_operation(operation));
            let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(request.value["trusted"], serde_json::Value::Null);
            assert!(request.value["sans"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("3.3.3.1")));
            assert!(!request.value["sans"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("169.254.88.1")));
            assert!(admin.receiver_tesla_tls.lock().unwrap().is_none());
            assert!(admin.tesla_url().is_none());
            // Changes arriving while an acknowledgement is pending are picked up next tick.
            admin.settings.lock().unwrap().tesla_android_address = "3.3.3.2".into();
            admin.apply_public_name();
            assert!(admin
                .operation_rx
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .try_recv()
                .is_err());
            request.applied();
            update.join().unwrap().unwrap();
        });
        assert_eq!(
            *admin.receiver_tesla_tls.lock().unwrap(),
            Some("3.3.3.1".parse().unwrap())
        );
        assert_eq!(admin.tesla_url().as_deref(), Some("https://3.3.3.1:8443/"));
        admin.apply_public_name();
        let operation = next_tls_operation(&admin);
        std::thread::scope(|scope| {
            let update = scope.spawn(|| admin.perform_operation(operation));
            let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            for address in ["3.3.3.1", "3.3.3.2", "10.77.0.1", "openxplay-linux.local"] {
                assert!(request.value["sans"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!(address)));
            }
            request.applied();
            update.join().unwrap().unwrap();
        });
        assert!(admin.input.session_active());
        assert_eq!(factory.stops.load(Ordering::SeqCst), 0);
        assert_eq!(factory.launches.lock().unwrap().len(), 1);
        admin.input.end(session).unwrap();
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tls_error_and_timeout_keep_served_state_until_the_idle_respawn_fallback() {
        use crate::tesla_address::Mode;
        let (admin, dir, factory, rx) = tls_fake_admin();
        let session = admin.input.begin(Arc::new(TlsSessionControl)).unwrap();
        let before = admin.receiver_tls_sans.lock().unwrap().clone();
        admin.settings.lock().unwrap().tesla_browser = Mode::TeslaAndroid;
        for reject in [true, false] {
            let started = std::time::Instant::now();
            std::thread::scope(|scope| {
                let update = scope.spawn(|| admin.update_receiver_tls(Duration::from_millis(80)));
                let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
                if reject {
                    request.rejected();
                }
                update.join().unwrap().unwrap();
                request.applied(); // A late success cannot change the old served state.
            });
            assert!(started.elapsed() < Duration::from_secs(2));
            assert_eq!(*admin.receiver_tls.lock().unwrap(), Some(None));
            assert_eq!(*admin.receiver_tls_sans.lock().unwrap(), before);
            assert!(admin.receiver_tesla_tls.lock().unwrap().is_none());
            assert!(admin.input.session_active());
            assert_eq!(factory.stops.load(Ordering::SeqCst), 0);
        }
        admin.input.end(session).unwrap();
        std::thread::scope(|scope| {
            let update = scope.spawn(|| admin.update_receiver_tls(Duration::from_secs(2)));
            rx.recv_timeout(Duration::from_secs(2)).unwrap().rejected();
            update.join().unwrap().unwrap();
        });
        assert_eq!(factory.stops.load(Ordering::SeqCst), 1);
        assert_eq!(
            *admin.receiver_tesla_tls.lock().unwrap(),
            Some("3.3.3.1".parse().unwrap())
        );
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn certificate_issued_during_a_session_is_advertised_only_after_the_swap() {
        use crate::public_name::{
            http::UreqHttp, privacy, testing::FakeHttp, Network, PublicNameService,
        };
        struct Host;
        impl crate::public_name::Host for Host {
            fn address(&self) -> Option<std::net::Ipv4Addr> {
                Some("10.77.0.1".parse().unwrap())
            }
            fn online(&self) -> bool {
                true
            }
            fn clock(&self) -> Option<u64> {
                Some(crate::platform::wall_clock().as_secs())
            }
        }
        let (mut admin, dir, factory, rx) = tls_fake_admin();
        let provider = FakeHttp::start(|_| (200, vec![], b"OK".to_vec()));
        let mut network = Network::internet(None);
        network.http = Arc::new(UreqHttp::new(false));
        network.endpoints.duckdns = provider.url();
        network.tick = Duration::from_millis(20);
        network.settle = Duration::from_millis(5);
        Arc::get_mut(&mut admin).unwrap().public_name =
            Arc::new(PublicNameService::new(&dir, network));
        let session = admin.input.begin(Arc::new(TlsSessionControl)).unwrap();
        let name = "myrelay.duckdns.org";
        let issued = rcgen::generate_simple_self_signed(vec![name.into()]).unwrap();
        let chain = issued.cert.pem();
        let key = issued.key_pair.serialize_pem();
        let backup = serde_json::json!({"publicName":{"version":1,"enabled":true,"provider":"duckdns",
            "name":name,"token":"12345678-1234-1234-1234-123456789abc","agreeLetsEncryptTerms":true},
            "publicNameCertificate":{"name":name,"chainPem":chain,"keyPem":key}});
        admin
            .public_name
            .import_privacy(
                privacy::decode(&backup, None, crate::platform::wall_clock().as_secs())
                    .unwrap_or_else(|e| panic!("{e:?}")),
            )
            .unwrap();
        admin.public_name.start(Arc::new(Host)).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while admin.public_name.status().trusted_name.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "certificate not ready: {:?}",
                admin.public_name.status()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(admin.status().public_name.unwrap().trusted_name.is_none());
        admin.apply_public_name();
        let operation = next_tls_operation(&admin);
        std::thread::scope(|scope| {
            let update = scope.spawn(|| admin.perform_operation(operation));
            let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(request.value["trusted"]["name"], name);
            use std::os::unix::fs::PermissionsExt;
            for (field, expected) in [("cert", &chain), ("key", &key)] {
                let path = request.value["trusted"][field].as_str().unwrap();
                assert_eq!(std::fs::read_to_string(path).unwrap(), *expected);
                assert_eq!(
                    std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            assert_eq!(*admin.receiver_tls.lock().unwrap(), Some(None));
            let pending = receiver_public_name(admin.public_name.status(), None, None);
            assert!(pending.trusted_name.is_none());
            request.applied();
            update.join().unwrap().unwrap();
        });
        let status = admin.status();
        assert_eq!(
            status.public_name.unwrap().trusted_name.as_deref(),
            Some(name)
        );
        assert_eq!(status.path_input["car"]["web"]["trustedName"], name);
        assert!(status.receiver_urls[0].starts_with(&format!("https://{name}:")));
        assert!(admin.input.session_active());
        assert_eq!(factory.stops.load(Ordering::SeqCst), 0);
        admin.input.end(session).unwrap();
        admin.public_name.stop();
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tesla_settings_apply_live_and_config_previews_share_collision_validation() {
        use crate::tesla_address::Mode;
        let (a, dir) = fake();
        let values = serde_json::json!({"teslaBrowser":"public-address"});
        assert!(matches!(
            a.update(values.as_object().unwrap().clone()),
            AdminUpdate::AppliedNow
        ));
        assert!(!a.restart_required.load(Ordering::Acquire));
        assert!(a.pending.lock().unwrap().is_empty());
        assert_eq!(
            Settings::load(&dir).unwrap().tesla_browser,
            Mode::PublicAddress
        );
        let current = a.settings.lock().unwrap().clone();
        for mode in [
            Mode::VirtualName,
            Mode::PublicAddress,
            Mode::Teslamirror,
            Mode::TeslaAndroid,
        ] {
            let key = crate::tesla_address::address_key(mode).unwrap();
            let mut values = Values::new();
            values.insert("teslaBrowser".into(), serde_json::to_value(mode).unwrap());
            values.insert(key.into(), "192.168.1.1".into());
            assert_eq!(
                update_settings(&current, values).unwrap_err()[key],
                "error.teslaAddress"
            );
        }
        // A different relay interface already owns an otherwise valid address.
        let mut owned = Settings {
            tesla_browser: Mode::TeslaAndroid,
            ..current.clone()
        };
        owned.tesla_android_address = "3.3.3.1".into();
        let mut admin = Arc::try_unwrap(a).ok().unwrap();
        admin.runtime.addresses =
            Arc::new(|| Ok(vec![("eth0".into(), "3.3.3.1".parse().unwrap(), 24)]));
        assert_eq!(
            admin.validate_tesla_change(&current, &owned).unwrap_err()["teslaAndroidAddress"],
            "error.teslaAddress"
        );
        let bytes = serde_json::to_vec(&serde_json::json!({"format":"openxplay-user-config","version":1,"settings":{"teslaBrowser":"tesla-android"}})).unwrap();
        for dry_run in [true, false] {
            assert_eq!(
                admin.config_import(&bytes, dry_run).unwrap_err()["teslaAndroidAddress"],
                "error.teslaAddress"
            );
        }
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tesla_url_requires_a_listening_receiver_with_the_direct_san_and_comes_first() {
        use crate::tesla_address::Mode;
        let (a, dir) = fake();
        let settings = Settings {
            tesla_browser: Mode::PublicAddress,
            ..Default::default()
        };
        *a.settings.lock().unwrap() = settings.clone();
        a.tesla
            .lock()
            .unwrap()
            .tick(true, true, &settings, true, || Ok(vec![]), 1);
        assert_eq!(a.tesla_url(), None);
        {
            let mut facts = a.events.facts.lock().unwrap();
            facts.web.phase = "LISTENING".into();
            facts.web.port = 9443;
        }
        assert_eq!(
            a.tesla_url(),
            None,
            "old certificate does not cover a live edit yet"
        );
        assert!(a.tesla_tls_changed());
        struct Control;
        impl carplay_airplay::SessionControl for Control {
            fn send_touch(&self, _: &[carplay_airplay::AirPlayContact]) -> Result<bool> {
                Ok(false)
            }
            fn send_media_button(&self, _: u8) -> Result<bool> {
                Ok(false)
            }
        }
        *a.receiver_tls.lock().unwrap() = Some(None);
        let session = a.input.begin(Arc::new(Control)).unwrap();
        assert!(
            a.tesla_tls_changed(),
            "live SAN changes are allowed while CarPlay runs"
        );
        a.perform_operation(Operation::ReceiverTls).unwrap();
        assert!(
            a.input.session_active(),
            "queued SAN changes cannot stop a newly started session"
        );
        a.input.end(session).unwrap();
        assert!(a.tesla_tls_changed());
        *a.receiver_tesla_tls.lock().unwrap() = Some("169.254.88.1".parse().unwrap());
        let url = "https://169.254.88.1:9443/";
        assert_eq!(a.tesla_url().as_deref(), Some(url));
        assert_eq!(
            a.status().receiver_urls.first().map(String::as_str),
            Some(url)
        );
        assert_eq!(a.status().path_input["car"]["tesla"]["phase"], "READY");
        assert!(!a.tesla_tls_changed());
        a.stop_controller();
        assert_eq!(a.tesla_url(), None);
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn stop_cancels_arrival_and_survives_shell_recreation_until_boot_changes() {
        let (a, dir) = fake();
        a.arrival
            .lock()
            .unwrap()
            .ready(std::time::Instant::now(), true);
        assert_eq!(a.status().path_input["startup"]["arrival"]["pending"], true);
        a.operation(Operation::Stop).unwrap();
        assert!(a.arrival.lock().unwrap().deadline.is_none());
        let options = a.options.clone();
        let state = a.state.clone();
        drop(a);
        let a = LinuxRelayAdmin::with_optional_provisioner(
            options.clone(),
            state.clone(),
            Arc::new(JsonEvents::new(Arc::new(|_| {}))),
            None,
            runtime_paths(&dir),
        )
        .unwrap();
        assert!(!a.desired.load(Ordering::Acquire));
        assert_eq!(a.status().path_input["startup"]["paused"], true);
        drop(a);
        let paths = runtime_paths(&dir);
        let boot = paths.root.join("proc/sys/kernel/random");
        std::fs::create_dir_all(&boot).unwrap();
        std::fs::write(boot.join("boot_id"), "next-boot").unwrap();
        let a = LinuxRelayAdmin::with_optional_provisioner(
            options,
            state,
            Arc::new(JsonEvents::new(Arc::new(|_| {}))),
            None,
            paths,
        )
        .unwrap();
        assert!(a.desired.load(Ordering::Acquire));
        assert_eq!(a.status().path_input["startup"]["paused"], false);
        assert_eq!(a.status().path_input["iphone"]["armed"], true);
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn carlink_timeout_keeps_tick_alive_and_rearms_when_playing_unless_stopped() {
        for interface in ["wlan0", "wlan1"] {
            let (a, dir) = fake();
            {
                let mut settings = a.settings.lock().unwrap();
                settings.output = "carlink".into();
                settings.carlink_interface = interface.into();
                settings.auto_start = true;
            }
            a.start_policy.lock().unwrap().request(
                std::time::Instant::now() - Duration::from_secs(61),
                true,
                true,
            );
            a.tick().unwrap();
            assert!(a.controller.lock().unwrap().is_none());
            let path = a.status().path_input;
            assert_eq!(path["iphone"]["failure"]["code"], "carlink_not_ready");
            assert_eq!(path["iphone"]["failure"]["retrying"], false);
            assert_eq!(path["iphone"]["armed"], true);
            a.tick().unwrap();
            assert!(
                a.controller.lock().unwrap().is_none(),
                "a timer must not retry the failed start"
            );
            a.carlink.lock().unwrap().state = "playing".into();
            a.observe_automatic_start().unwrap();
            assert!(a.events.runtime_path_facts().iphone.failure.is_none());
            assert!(!a.start_policy.lock().unwrap().requested());
            assert!(a.desired.load(Ordering::Acquire));
            // Stop persists, including when the same readiness edge arrives again.
            a.operation(Operation::Stop).unwrap();
            a.carlink.lock().unwrap().state = "scanning".into();
            a.observe_automatic_start().unwrap();
            a.carlink.lock().unwrap().state = "playing".into();
            a.observe_automatic_start().unwrap();
            assert!(!a.desired.load(Ordering::Acquire));
            assert!(!a.runtime_path_facts().iphone.armed);
            a.shutdown();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn lifecycle_contention_never_starves_tesla_or_public_name_refresh() {
        let (mut admin, dir) = fake();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let output = lines.clone();
        Arc::get_mut(&mut admin).unwrap().events =
            Arc::new(JsonEvents::new(Arc::new(move |line| {
                output.lock().unwrap().push(line)
            })));
        assert!(admin.public_name.import(r#"{"version":1,"enabled":true,"provider":"dynv6","name":"car.dynv6.net","token":"private-d2-token","agreeLetsEncryptTerms":true}"#).is_ok());
        admin.settings.lock().unwrap().tesla_browser = crate::tesla_address::Mode::Teslamirror;
        let address = admin
            .settings
            .lock()
            .unwrap()
            .tesla_address()
            .unwrap()
            .to_owned();
        let hotspot = admin
            .hotspot_lifetime
            .create(|| Arc::new(crate::hotspot_lifetime::tests::Fake::default()))
            .unwrap();
        hotspot.start(Duration::from_secs(1)).unwrap();
        let lifecycle = admin.lifecycle.lock().unwrap();
        admin.tick().unwrap();
        admin.tick().unwrap();
        assert_eq!(
            admin
                .tesla
                .lock()
                .unwrap()
                .active
                .as_ref()
                .unwrap()
                .address
                .to_string(),
            address
        );
        let hosts = std::fs::read_to_string(&admin.runtime.local_dns.hosts).unwrap();
        assert!(
            hosts.contains(&format!("{address} car.dynv6.net")),
            "{hosts}"
        );
        // Removing the named mode also refreshes DNS while the controller lock stays held.
        admin.settings.lock().unwrap().tesla_browser = crate::tesla_address::Mode::TeslaAndroid;
        admin.tick().unwrap();
        let hosts = std::fs::read_to_string(&admin.runtime.local_dns.hosts).unwrap();
        assert!(hosts.contains("10.42.0.1 car.dynv6.net"), "{hosts}");
        admin.settings.lock().unwrap().tesla_android_address = "3.3.3.2".into();
        admin.tick().unwrap();
        admin.tesla.lock().unwrap().failure = Some("nmcli private-d2-token command output".into());
        admin.tick().unwrap();
        admin.tick().unwrap();
        let events: Vec<serde_json::Value> = lines
            .lock()
            .unwrap()
            .iter()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|v: &serde_json::Value| v["event"] == "tesla-address")
            .collect();
        assert_eq!(events.len(), 4, "{events:?}");
        assert_eq!(events[0]["mode"], "teslamirror");
        assert_eq!(events[0]["phase"], "READY");
        assert_eq!(events[1]["mode"], "tesla-android");
        assert_eq!(events[2]["address"], "3.3.3.2");
        assert_eq!(events[3]["phase"], "FAILED");
        assert_eq!(events[3]["failureCode"], "tesla_address_failed");
        assert!(!serde_json::to_string(&events)
            .unwrap()
            .contains("private-d2-token"));
        assert!(admin.controller.lock().unwrap().is_none());
        drop(lifecycle);
        admin.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn owned_tesla_address_does_not_change_network_roles_name_or_receiver_sans() {
        use crate::tesla_address::{tests::FakeBackend, Mode, Session};
        for mode in [
            Mode::TeslaAndroid,
            Mode::PublicAddress,
            Mode::Teslamirror,
            Mode::VirtualName,
        ] {
            let (mut admin, dir) = fake();
            let backend = Arc::new(FakeBackend::default());
            backend
                .rows
                .lock()
                .unwrap()
                .push(("wlan0".into(), "10.42.0.1".parse().unwrap(), 24));
            let inner = Arc::get_mut(&mut admin).unwrap();
            let rows = backend.clone();
            inner.runtime.addresses = Arc::new(move || Ok(rows.rows.lock().unwrap().clone()));
            inner.tesla = Mutex::new(
                Session::new(backend.clone()).with_ownership(inner.runtime.tesla_address.clone()),
            );
            admin.settings.lock().unwrap().tesla_browser = mode;
            assert!(admin.public_name.import(r#"{"version":1,"enabled":true,"provider":"dynv6","name":"car.dynv6.net","token":"private-token","agreeLetsEncryptTerms":true}"#).is_ok());
            let hotspot = admin
                .hotspot_lifetime
                .create(|| Arc::new(crate::hotspot_lifetime::tests::Fake::default()))
                .unwrap();
            hotspot.start(Duration::from_secs(1)).unwrap();
            let mut inventory = crate::net_inventory::Inventory {
                probed_at: Some(1),
                interfaces: vec![crate::net_inventory::Interface {
                    name: "wlan0".into(),
                    kind: "wifi".into(),
                    ipv4: vec!["10.42.0.1".into()],
                    ..Default::default()
                }],
                ..Default::default()
            };
            admin.inventory.set(inventory.clone());
            let (options, status) = admin.effective_options_with_status().unwrap();
            let links = admin.links(&options, &status);
            let address = admin.head_unit_address();
            let sans = admin.receiver_sans().unwrap();
            *admin.receiver_tls.lock().unwrap() = Some(None);
            *admin.receiver_tls_sans.lock().unwrap() = sans.clone();
            *admin.receiver_tesla_tls.lock().unwrap() =
                crate::tesla_address::tls_address(&options.settings);
            admin.refresh_tesla_address();
            assert!(admin.tesla.lock().unwrap().active.is_some());
            // Reproduce NetworkManager returning the auxiliary address before the AP address.
            backend.rows.lock().unwrap().reverse();
            let rows = admin.runtime.network_addresses().unwrap();
            assert_eq!(
                rows,
                vec![("wlan0".into(), "10.42.0.1".parse().unwrap(), 24)]
            );
            crate::net_inventory::refresh_activity(
                admin.runtime.runner.as_ref(),
                &admin.runtime.root,
                &mut inventory,
                &rows,
            );
            assert_eq!(inventory.interfaces[0].ipv4, ["10.42.0.1"]);
            admin.inventory.set(inventory);
            let (options, status) = admin.effective_options_with_status().unwrap();
            assert!(links.changes(&admin.links(&options, &status)).is_empty());
            assert_eq!(admin.head_unit_address(), address);
            assert_eq!(admin.receiver_sans().unwrap(), sans);
            admin.apply_public_name();
            assert!(!admin.receiver_tls_queued.load(Ordering::Acquire));
            admin.tesla.lock().unwrap().stop(200);
            assert_eq!(admin.receiver_sans().unwrap(), sans);
            admin.shutdown();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn tesla_address_uses_the_active_ap_before_controller_hotspot_publication() {
        let (mut a, dir) = fake();
        Arc::get_mut(&mut a).unwrap().runtime.addresses = Arc::new(|| {
            Ok(vec![
                ("wlan0".into(), "10.77.0.1".parse().unwrap(), 24),
                ("wlan1".into(), "10.78.0.1".parse().unwrap(), 24),
            ])
        });
        a.settings.lock().unwrap().tesla_browser = crate::tesla_address::Mode::TeslaAndroid;
        assert!(a.controller.lock().unwrap().is_none());
        a.inventory.set(crate::net_inventory::Inventory {
            interfaces: vec![crate::net_inventory::Interface {
                name: "wlan0".into(),
                activity: crate::net_inventory::Activity {
                    mode: Some("AP".into()),
                    profile: Some("oxp-carplay-n1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        });
        a.refresh_tesla_address();
        assert_eq!(
            a.tesla
                .lock()
                .unwrap()
                .active
                .as_ref()
                .unwrap()
                .address
                .to_string(),
            "3.3.3.1"
        );
        assert_eq!(a.head_unit_address(), Some("10.77.0.1".parse().unwrap()));
        a.operation(Operation::Stop).unwrap();
        a.refresh_tesla_address();
        assert!(a.tesla.lock().unwrap().active.is_none());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn tesla_uses_the_started_hotspot_lease_while_controller_snapshot_is_absent() {
        let (a, dir) = fake();
        a.settings.lock().unwrap().tesla_browser = crate::tesla_address::Mode::TeslaAndroid;
        let hotspot = a
            .hotspot_lifetime
            .create(|| Arc::new(crate::hotspot_lifetime::tests::Fake::default()))
            .unwrap();
        hotspot.start(Duration::from_secs(1)).unwrap();
        assert!(a.controller.lock().unwrap().is_none());
        assert!(a.inventory.get().interfaces.is_empty());
        a.refresh_tesla_address();
        assert!(a.tesla.lock().unwrap().active.is_some());
        assert_eq!(
            a.active_hotspot(),
            Some(("wlan0".into(), "10.42.0.1".parse().unwrap()))
        );
        a.operation(Operation::Stop).unwrap();
        assert!(a.hotspot_lifetime.ready_info().is_none());
        assert!(a.tesla.lock().unwrap().active.is_none());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn runtime_waits_before_concurrent_carlink_and_start_now_skips_once() {
        let (a, dir) = fake();
        a.settings.lock().unwrap().output = "both".into();
        a.settings.lock().unwrap().carlink_interface = "wlan1".into();
        a.start_policy.lock().unwrap().cancel();
        a.ensure_controller().unwrap();
        assert!(a.controller.lock().unwrap().is_some());
        let f = a.runtime_path_facts();
        assert!(!f.iphone.armed);
        assert!(f.iphone.car_wait_until_millis.is_some());
        a.perform_operation(Operation::Start).unwrap();
        assert!(!a.start_policy.lock().unwrap().waiting());
        assert!(a.start_policy.lock().unwrap().requested());
        a.ensure_controller().unwrap();
        assert!(a.controller.lock().unwrap().is_some());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn runtime_schema_settings_and_trial_storage_are_separate() {
        let (a, dir) = fake();
        for (section, keys) in [
            ("picture", vec!["frameRate"]),
            (
                "output",
                vec!["carlinkPictureSizes", "carlinkPictureDefault"],
            ),
            ("car", vec!["carBrand", "carModel", "carYear"]),
            (
                "experimental",
                vec!["experimentalActive", "experimentalBluetoothAddress"],
            ),
        ] {
            let schema = settings_schema();
            let row = schema.iter().find(|s| s.id == section).unwrap();
            for key in keys {
                assert!(row.fields.iter().any(|f| f.key == key));
            }
        }
        let value = serde_json::json!({"frameRate":"45","carBrand":"xpeng","carModel":"g6-2023","carYear":"","carlinkPictureSizes":[{"width":1440,"height":900}],"carlinkPictureDefault":{"width":1440,"height":900}});
        assert!(matches!(
            a.update(value.as_object().unwrap().clone()),
            AdminUpdate::NextStart
        ));
        assert_eq!(a.settings().values["frameRate"], "45");
        let saved = Settings::load(&dir).unwrap();
        assert_eq!(saved.car_model, "g6");
        assert_eq!(saved.car_year, "g6-2023");
        assert_eq!(saved.frame_rate, "45");
        a.settings.lock().unwrap().output = "carlink".into();
        a.try_carlink_picture(None).unwrap();
        assert_eq!(a.input.display.lock().unwrap().pictures.trial, Some(None));
        let saved = serde_json::to_value(Settings::load(&dir).unwrap()).unwrap();
        assert!(saved.get("carlinkPictureTry").is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn runtime_paths_isolate_startup_dns_and_inventory_refresh() {
        struct Runner(Mutex<Vec<Vec<String>>>);
        impl CommandRunner for Runner {
            fn run(&self, args: &[String], _: Duration) -> Result<CommandResult> {
                self.0.lock().unwrap().push(args.to_vec());
                Ok(CommandResult {
                    exit_code: 0,
                    output: String::new(),
                })
            }
        }
        let (old, dir) = fake();
        Settings {
            auto_start: false,
            boot_start: false,
            ..Default::default()
        }
        .save(&dir)
        .unwrap();
        let runner = Arc::new(Runner(Mutex::default()));
        let mut runtime = runtime_paths(&dir);
        runtime.runner = runner.clone();
        runtime.inventory_refresh = InventoryRefresh::Automatic;
        let net = runtime.root.join("sys/class/net");
        let add_interface = |name: &str| {
            let path = net.join(name);
            let device = runtime.root.join("sys/devices").join(name);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::create_dir_all(&device).unwrap();
            std::os::unix::fs::symlink(device, path.join("device")).unwrap();
            std::fs::write(path.join("type"), "1\n").unwrap();
            std::fs::write(path.join("address"), "02:00:00:00:00:09\n").unwrap();
        };
        add_interface("eth9");
        let mut a = Arc::new(
            LinuxRelayAdmin::with_optional_provisioner(
                old.options.clone(),
                old.state.clone(),
                old.events.clone(),
                None,
                runtime.clone(),
            )
            .unwrap(),
        );
        drop(old);
        let _children = link_children(&mut a);
        assert!(a.inventory.wait_for_probe(Duration::from_secs(2)));
        assert_eq!(a.inventory.get().interfaces[0].name, "eth9");
        crate::public_name::local_dns::hotspot(
            &runtime.local_dns,
            "stale",
            Some("10.79.0.1".parse().unwrap()),
        )
        .unwrap();
        a.start_services_with_probe_timeout(Duration::from_secs(2))
            .unwrap();
        await_startup(&a);
        assert_eq!(std::fs::read(&runtime.local_dns.hosts).unwrap(), b"");
        assert!(!crate::public_name::local_dns::path(&runtime.local_dns.dir).exists());
        assert!(runner.0.lock().unwrap().iter().any(|args| args
            == &[
                "nmcli",
                "-t",
                "-f",
                "NAME,UUID,ACTIVE",
                "connection",
                "show"
            ]));
        add_interface("eth8");
        a.tick().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while a.inventory.get().interfaces.len() != 2 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            a.inventory
                .get()
                .interfaces
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            ["eth8", "eth9"]
        );
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn trace_control_clear_off_running_and_busy_supervisor() {
        use std::os::unix::fs::PermissionsExt;
        let (a, dir) = fake();
        let trace_dir = dir.join("carlink-trace");
        std::fs::create_dir_all(&trace_dir).unwrap();
        let name = "media-00000001-00000001-0.ts.part";
        std::fs::write(trace_dir.join(name), b"partial").unwrap();
        assert_eq!(
            a.carlink_trace_control(false, "start"),
            Err("error.carlinkTraceOff".into())
        );
        a.carlink_trace_control(true, "clear").unwrap();
        assert!(!trace_dir.join(name).exists());
        {
            let _busy = a.supervisor.lock().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let admin = a.clone();
            let request = std::thread::spawn(move || {
                tx.send((
                    admin.carlink_trace_control(true, "clear"),
                    admin.carlink_trace().unwrap().1,
                ))
                .unwrap();
            });
            // Both requests must finish while the supervisor lock is still held.
            let (result, running) = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("Trace control blocked on the busy supervisor");
            assert_eq!(result, Err("error.carlinkTraceBusy".into()));
            assert!(!running);
            request.join().unwrap();
        }
        let script = dir.join("child");
        let commands = dir.join("commands");
        // The fake consumer keeps its stdin open and records complete command lines.
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nwhile IFS= read -r line; do printf '%s\\n' \"$line\" >> '{}'; done\n",
                commands.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut options = a.options.clone();
        options.settings.output = "carlink".into();
        options.settings.carlink_trace = "on".into();
        options.oxp_relay = script;
        let settings = crate::supervisor_settings::supervisor_settings(
            &options,
            vec![],
            &[],
            String::new(),
            None,
            false,
        );
        let mut supervisor =
            RelaySupervisor::new(settings, Arc::new(ProcessChildFactory), Arc::new(|_| {}));
        supervisor.start().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        // Tracing is advertised before spawn; running is set only after the control pipe exists.
        while !supervisor.running() || !supervisor.trace_running() {
            assert!(
                std::time::Instant::now() < deadline,
                "Timed out waiting for the trace control pipe"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        *a.supervisor.lock().unwrap() = Some(supervisor);
        a.settings.lock().unwrap().carlink_trace = "on".into();
        a.carlink_trace_control(false, "start").unwrap();
        a.carlink_trace_control(true, "clear").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let data = std::fs::read_to_string(&commands).unwrap_or_default();
            if data.lines().count() == 3 {
                let lines = data
                    .lines()
                    .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    lines,
                    vec![
                        serde_json::json!({"type":"clink-capture","action":"start"}),
                        serde_json::json!({"type":"clink-capture","action":"stop"}),
                        serde_json::json!({"type":"clink-trace","action":"clear"})
                    ]
                );
                break;
            }
            assert!(std::time::Instant::now() < deadline, "{data}");
            std::thread::sleep(Duration::from_millis(5));
        }
        a.supervisor.lock().unwrap().take().unwrap().stop();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn linux_car_settings_schema_and_output_restart() {
        let (a, dir) = fake();
        let fields: Vec<_> = settings_schema()
            .into_iter()
            .flat_map(|s| s.fields)
            .collect();
        for key in [
            "carlinkName",
            "carlinkModel",
            "carlinkAutoReconnect",
            "carlinkReconnectOnResize",
            "carlinkHevc",
            "carlinkTrace",
            "autoStart",
            "videoStats",
            "receiverPort",
            "carPlayViewAreas",
        ] {
            assert!(fields.iter().any(|f| f.key == key), "{key}");
        }
        assert!(fields.iter().any(|f| f.key == "bootStart"));
        assert_eq!(
            fields
                .iter()
                .find(|f| f.key == "autoStart")
                .unwrap()
                .hint_key
                .as_deref(),
            Some("hint.field.linuxAutoStart")
        );
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        for (key, value) in [
            ("carlinkName", serde_json::json!("Car")),
            ("carlinkModel", serde_json::json!("Model")),
            ("carlinkHevc", serde_json::json!("off")),
            ("vehicleInformation", serde_json::json!(true)),
            ("carlinkTrace", serde_json::json!("on")),
            ("carlinkAutoReconnect", serde_json::json!(false)),
            ("receiverPort", serde_json::json!(9443)),
        ] {
            let values = serde_json::Map::from_iter([(key.into(), value)]);
            assert!(matches!(a.update(values), AdminUpdate::AppliedNow));
            a.response_sent();
            let operation = rx.try_recv().unwrap();
            if ["carlinkHevc", "vehicleInformation"].contains(&key) {
                assert!(matches!(operation, Operation::Carlink));
            } else {
                assert!(matches!(operation, Operation::Outputs(None)));
            }
        }
        assert_eq!(a.settings.lock().unwrap().receiver_port, 9443);
        assert!(a
            .status()
            .receiver_urls
            .iter()
            .all(|url| url.contains(":9443/")));
        for port in [0, 6975, 65536] {
            assert!(matches!(
                a.update(
                    serde_json::json!({"receiverPort":port})
                        .as_object()
                        .unwrap()
                        .clone()
                ),
                AdminUpdate::Errors(_)
            ));
        }
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn auto_start_false_waits_for_the_explicit_start_action() {
        let (a, dir) = fake();
        Settings {
            auto_start: false,
            boot_start: false,
            ..Default::default()
        }
        .save(&dir)
        .unwrap();
        let options = a.options.clone();
        let state = a.state.clone();
        let events = a.events.clone();
        drop(a);
        let mut a = Arc::new(
            LinuxRelayAdmin::with_optional_provisioner(
                options,
                state,
                events,
                None,
                runtime_paths(&dir),
            )
            .unwrap(),
        );
        let _children = link_children(&mut a);
        assert!(!a.desired.load(Ordering::Acquire));
        assert!(a.status().auto_start_paused);
        a.tick().unwrap();
        assert!(a.controller.lock().unwrap().is_none());
        a.operation(Operation::Start).unwrap();
        assert!(a.desired.load(Ordering::Acquire));
        assert!(!a.status().auto_start_paused);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn changing_car_levels_clears_descendants_but_complete_imports_keep_year_text() {
        let current = Settings {
            car_brand: "xpeng".into(),
            car_model: "g6".into(),
            car_year: "other".into(),
            car_year_text: "2027".into(),
            ..Default::default()
        };
        let next = update_settings(
            &current,
            serde_json::json!({"carModel":"p7"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(
            (&*next.car_model, &*next.car_year, &*next.car_year_text),
            ("p7", "", "")
        );
        let same = update_settings(
            &current,
            serde_json::json!({"carModel":"g6"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(same.car_year_text, "2027");
        let brand = update_settings(
            &current,
            serde_json::json!({"carBrand":"zeekr"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!((&*brand.car_model, &*brand.car_year_text), ("", ""));
        let imported=update_settings(&current,serde_json::json!({"carBrand":"zeekr","carModel":"001","carYear":"other","carYearText":"2028"}).as_object().unwrap().clone()).unwrap();
        assert_eq!(imported.car_year_text, "2028");
        let calibration = update_settings(
            &Settings::default(),
            serde_json::json!({"physicalWidthEnabled":true})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(calibration.physical_width_override_mm, 200);
    }
    #[test]
    fn screen_actions_route_with_auth_and_code_regeneration_restarts_outputs() {
        let (a, dir) = fake();
        let mut auth = AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap();
        let token = auth
            .login("127.0.0.1".parse().unwrap(), "openxplay", 100)
            .ok()
            .unwrap();
        let server = AdminServer::new(
            a.clone(),
            auth,
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        let mut request = Request {
            method: "POST".into(),
            path: "/api/v0/actions/clear-saved-head-unit".into(),
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("x-openxplay-admin".into(), "1".into()),
            ]),
            body: b"{}".to_vec(),
            peer: "127.0.0.1".parse().unwrap(),
        };
        assert_eq!(server.route(&request, 100).status, 401);
        request
            .headers
            .insert("cookie".into(), format!("oxp_admin={token}"));
        assert_eq!(server.route(&request, 100).status, 200);
        a.response_sent();
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            Operation::ClearSavedHeadUnit
        ));
        a.response_failed();
        request.path = "/api/v0/actions/clear-window-records".into();
        assert_eq!(server.route(&request, 100).status, 200);
        request.path = "/api/v0/view-area".into();
        for body in [
            r#"{"index":-1}"#,
            r#"{"index":7}"#,
            r#"{"index":1.5}"#,
            r#"{"index":"0"}"#,
            r#"{}"#,
        ] {
            request.body = body.as_bytes().to_vec();
            assert_eq!(server.route(&request, 100).status, 422);
        }
        request.body = br#"{"index":0}"#.to_vec();
        assert_eq!(server.route(&request, 100).status, 409);
        assert_eq!(a.input.display.lock().unwrap().area_switches.len(), 1);
        request.path = "/api/v0/actions/regenerate-pairing-code".into();
        request.body = b"{}".to_vec();
        assert_eq!(server.route(&request, 100).status, 409);
        a.settings.lock().unwrap().require_pairing_code = true;
        assert_eq!(server.route(&request, 100).status, 200);
        a.response_sent();
        assert!(matches!(
            rx.try_recv().unwrap(),
            Operation::RegeneratePairingCode
        ));
        a.response_failed();
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn clear_saved_head_unit_preserves_records_during_active_sessions() {
        let (a, dir) = fake();
        {
            let mut d = a.input.display.lock().unwrap();
            d.saved.head_unit = Some(crate::display::HeadUnit {
                screen: crate::display::Screen {
                    width: 1920,
                    height: 1080,
                    density_dpi: 160,
                },
                viewport: None,
                fullscreen: None,
                fullscreen_preferred: None,
            });
            d.saved.max_macroblocks = Some(10000);
            d.saved.canvas = crate::display::Size {
                width: 1920,
                height: 1080,
            };
            let screen = d.saved.head_unit.as_ref().unwrap().screen;
            d.saved.records.push(crate::display::Record {
                screen,
                size: crate::display::Size {
                    width: 1280,
                    height: 720,
                },
                fullscreen: false,
                count: 1,
                last_seen: 1,
            });
            d.persist().unwrap();
        }
        a.input.state.lock().unwrap().current = Some(1);
        a.action("clear-saved-head-unit").unwrap();
        assert!(a.input.display.lock().unwrap().saved.head_unit.is_some());
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        a.response_sent();
        a.operation(rx.try_recv().unwrap()).unwrap();
        let saved = crate::display::DisplayState::load(&dir.join("display.json"));
        assert!(saved.head_unit.is_none());
        assert_eq!(saved.records.len(), 1);
        assert_eq!(a.input.state.lock().unwrap().current, Some(1));
        assert_eq!(saved.max_macroblocks, None);
        assert_eq!(saved.canvas, crate::display::DisplayState::default().canvas);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn carlink_events_expose_view_statistics_and_retryable_incorrect_pin() {
        let (a, dir) = fake();
        let controls = capture_controls(&a);
        let mut status = a.carlink.lock().unwrap();
        status.apply_event(
            &serde_json::json!({"type":"clink-view-size","width":1814,"height":1200}),
            10,
        );
        let stats = serde_json::json!({"type":"media-stats","sent":5,"dropped":4,"liveFrames":2,"placeholderFrames":3,"droppedFrames":4,"liveBytes":5678,"sendStalls":6,"codec":"H265","width":1450,"height":960});
        status.apply_event(&stats, 10);
        assert_eq!(status.stats.as_ref(), Some(&stats));
        assert_eq!(
            status.view_size,
            Some(CarlinkViewSize {
                width: 1814,
                height: 1200
            })
        );
        status.apply_event(
            &serde_json::json!({"type":"clink-pin","result":"incorrect","client":"unit"}),
            crate::admin::auth::monotonic_seconds(),
        );
        assert!(status.pin_incorrect);
        assert!(!status.pin_requested);
        status.apply_event(
            &serde_json::json!({"type":"pin-request","request":17}),
            crate::admin::auth::monotonic_seconds(),
        );
        assert!(status.pin_incorrect && status.pin_requested);
        drop(status);
        a.carlink_pin("5678").unwrap();
        assert!(!a.carlink.lock().unwrap().pin_incorrect);
        assert_eq!(controls.lock().unwrap()[0]["pin"], "5678");
        assert!(!carlink_pin_path(&dir).exists());
        let mut status = a.carlink.lock().unwrap();
        status.apply_event(&serde_json::json!({"type":"receiver-starting"}), 20);
        assert!(status.view_size.is_none() && status.stats.is_none());
        drop(status);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn separate_carlink_radio_ignores_link_events_and_releases_an_old_follow() {
        let (a, dir) = fake();
        let joined = serde_json::json!({"type":"link","state":"joined","network":{"ssid":"DIRECT-Car","interface":"wlan1"}});
        a.follow
            .lock()
            .unwrap()
            .apply(&joined, "wlan0", false, &a.events, &a.hotspot_lifetime);
        assert!(a.follow.lock().unwrap().target.is_none());
        assert!(a.carplay_options().unwrap().shared_carlink_radio());
        assert!(!a.status().network.following_carlink);
        a.settings.lock().unwrap().carlink_interface = "wlan1".into();
        let old = serde_json::json!({"type":"link","state":"joined","network":{"ssid":"DIRECT-Car","interface":"wlan0"}});
        // A previous single-radio follow must also be released when the setting changes.
        a.follow
            .lock()
            .unwrap()
            .apply(&old, "wlan0", true, &a.events, &a.hotspot_lifetime);
        *a.network_override.lock().unwrap() = Some(SharedWifiNetwork {
            ssid: "DIRECT-Car".into(),
            password: "password".into(),
        });
        assert_eq!(
            a.carplay_options().unwrap().settings.network_mode,
            "device-hotspot"
        );
        a.reconcile_carlink_network().unwrap();
        assert!(a.follow.lock().unwrap().target.is_none());
        assert!(a.network_override.lock().unwrap().is_none());
        assert!(!a.status().network.following_carlink);
        // A normal single-radio event still enters the same debounced mailbox.
        a.settings.lock().unwrap().carlink_interface.clear();
        a.follow
            .lock()
            .unwrap()
            .apply(&old, "wlan0", true, &a.events, &a.hotspot_lifetime);
        assert_eq!(
            a.follow.lock().unwrap().target.as_deref(),
            Some("DIRECT-Car")
        );
        assert!(!a.follow.lock().unwrap().ready());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn admin_status_reads_the_capability_cache_without_probing() {
        struct FailedProbe(std::sync::atomic::AtomicUsize);
        impl CommandRunner for FailedProbe {
            fn run(&self, _: &[String], timeout: Duration) -> Result<CommandResult> {
                assert!(timeout <= Duration::from_secs(3));
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(std::io::Error::other("unavailable"))
            }
        }
        let (mut a, dir) = fake();
        assert!(serde_json::to_value(a.status())
            .unwrap()
            .get("hostCapabilities")
            .is_none());
        let runner = Arc::new(FailedProbe(Default::default()));
        Arc::get_mut(&mut a).unwrap().capabilities =
            Arc::new(crate::host_capabilities::CapabilityCache::new(
                runner.clone(),
                dir.clone(),
                Default::default(),
            ));
        a.capabilities.initiated("wlan0", "wlan1");
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while a.capabilities.get().is_none() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let count = runner.0.load(Ordering::SeqCst);
        let status = serde_json::to_value(a.status()).unwrap();
        assert_eq!(status["hostCapabilities"]["wifi"]["present"], false);
        assert_eq!(status["hostCapabilities"]["carlink"]["interface"], "wlan1");
        assert!(status["hostCapabilities"]["bluetooth"].is_null());
        assert_eq!(status["wireless"]["mode"], "unknown");
        assert!(status["wireless"]["ap5"].is_null());
        assert_eq!(status["wireless"]["preset"]["id"], "generic");
        a.status();
        assert_eq!(runner.0.load(Ordering::SeqCst), count);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn wireless_status_uses_wifi_interface_and_device_tree_without_a_second_probe() {
        struct WifiProbe(std::sync::atomic::AtomicUsize);
        impl CommandRunner for WifiProbe {
            fn run(&self, args: &[String], _: Duration) -> Result<CommandResult> {
                self.0.fetch_add(1, Ordering::SeqCst);
                let output = if args.get(1).is_some_and(|a| a == "dev") {
                    "wiphy 0"
                } else {
                    include_str!("../tests/fixtures/iw-phy-n1.txt")
                };
                Ok(CommandResult {
                    exit_code: 0,
                    output: output.into(),
                })
            }
        }
        let (mut a, dir) = fake();
        std::fs::create_dir_all(dir.join("proc/device-tree")).unwrap();
        std::fs::write(dir.join("proc/device-tree/compatible"), b"phicomm,n1\0").unwrap();
        let runner = Arc::new(WifiProbe(Default::default()));
        Arc::get_mut(&mut a).unwrap().capabilities =
            Arc::new(crate::host_capabilities::CapabilityCache::new(
                runner.clone(),
                dir.clone(),
                Default::default(),
            ));
        a.capabilities.wifi("wlan0");
        let status = serde_json::to_value(a.status()).unwrap();
        assert_eq!(
            status["wireless"],
            serde_json::json!({
                "preset":{"id":"phicomm-n1","label":"Phicomm N1","source":"detected","detected":"phicomm-n1"},
                "mode":"concurrent","station24":true,"ap24":true,"station5":true,"ap5":true,
                "apStation":true,"apStationChannels":1,"p2pClient":true,"ap5Channels":[36,40,44,48,149,153,157,161,165],
                "hotspot":{"band":"5","autoBand":"5","channel":36,"configuredBand":"auto","configuredChannel":0,"reason":null}
            })
        );
        a.status();
        assert_eq!(runner.0.load(Ordering::SeqCst), 3);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn preset_band_and_channel_schema_and_next_start_settings() {
        let schema = settings_schema();
        let fields = &schema.iter().find(|s| s.id == "connection").unwrap().fields;
        let preset = fields.iter().find(|f| f.key == "devicePreset").unwrap();
        assert_eq!(preset.field_type, "choice");
        assert!(preset.visible_when.is_none());
        assert_eq!(
            preset
                .options
                .as_ref()
                .unwrap()
                .iter()
                .map(|o| o.value.as_str().unwrap())
                .collect::<Vec<_>>(),
            ["auto", "msm8916", "phicomm-n1", "generic"]
        );
        let band = fields.iter().find(|f| f.key == "hotspotBand").unwrap();
        assert_eq!(
            band.options
                .as_ref()
                .unwrap()
                .iter()
                .map(|o| o.value.as_str().unwrap())
                .collect::<Vec<_>>(),
            ["auto", "2.4", "5"]
        );
        assert_eq!(band.visible_when.as_ref().unwrap().key, "networkMode");
        assert_eq!(
            band.visible_when.as_ref().unwrap().equals,
            vec![serde_json::json!("device-hotspot")]
        );
        let channel = fields.iter().find(|f| f.key == "hotspotChannel").unwrap();
        assert_eq!(channel.min, Some(0.0));
        assert_eq!(
            channel.hint_key.as_deref(),
            Some("hint.field.hotspotChannel")
        );
        let (a, dir) = fake();
        for values in [
            serde_json::json!({"hotspotBand":"2.4"}),
            serde_json::json!({"devicePreset":"msm8916"}),
        ] {
            assert!(matches!(
                a.update(values.as_object().unwrap().clone()),
                AdminUpdate::NextStart
            ));
        }
        let saved = Settings::load(&dir).unwrap();
        assert_eq!(saved.hotspot_band, "2.4");
        assert_eq!(saved.device_preset, "msm8916");
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn record_events(a: &mut Arc<LinuxRelayAdmin>) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let copy = lines.clone();
        Arc::get_mut(a).unwrap().events = Arc::new(JsonEvents::new(Arc::new(move |line| {
            copy.lock()
                .unwrap()
                .push(serde_json::from_str(&line).unwrap())
        })));
        lines
    }
    /// `event for/reason outcome` of the named-wait and refused-start lines.
    fn start_lines(lines: &Mutex<Vec<serde_json::Value>>) -> Vec<String> {
        lines
            .lock()
            .unwrap()
            .iter()
            .filter(|l| {
                l["event"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("carplay_"))
            })
            .map(|l| {
                let name = l["for"].as_str().or(l["reason"].as_str()).unwrap_or("");
                let outcome = l["outcome"].as_str().unwrap_or("");
                format!("{} {name} {outcome}", l["event"].as_str().unwrap())
                    .trim_end()
                    .to_string()
            })
            .collect()
    }
    #[test]
    fn a_held_start_waits_for_its_network_once_and_a_running_session_holds_nothing() {
        let (mut a, dir) = fake();
        let lines = record_events(&mut a);
        {
            let a = Arc::get_mut(&mut a).unwrap();
            a.child_factory = Arc::new(OutputFake::default());
            a.net_root = dir.join("net");
            a.backends.station = crate::host_backends::station_with_cli(
                a.runtime.runner.clone(),
                "/usr/bin/true".into(),
                a.runtime.clone(),
            );
            *a.provision.lock().unwrap() = None;
        }
        a.inventory.set(crate::net_inventory::Inventory {
            probed_at: Some(1),
            ..Default::default()
        });
        a.operation(Operation::Startup).unwrap();
        for _ in 0..3 {
            a.tick().unwrap();
        }
        assert!(a.controller.lock().unwrap().is_none());
        let held = lines
            .lock()
            .unwrap()
            .iter()
            .find(|l| l["event"] == "carplay_wait")
            .cloned()
            .unwrap();
        assert_eq!(
            (
                held["for"].as_str(),
                held["reason"].as_str(),
                held["wifi"].as_str()
            ),
            (Some("network"), Some("radio_unavailable"), Some("absent"))
        );
        std::fs::create_dir_all(dir.join("net/wlan0")).unwrap();
        std::fs::write(dir.join("net/wlan0/address"), "AA:BB:CC:DD:EE:FF").unwrap();
        a.start_policy.lock().unwrap().skip();
        a.tick().unwrap();
        assert!(a.controller.lock().unwrap().is_some());
        for _ in 0..3 {
            a.tick().unwrap();
        }
        let seen = start_lines(&lines);
        assert_eq!(
            seen.iter().filter(|l| *l == "carplay_wait network").count(),
            1,
            "{seen:?}"
        );
        assert!(
            seen.contains(&"carplay_wait_end network ready".to_string()),
            "{seen:?}"
        );
        // Waiting for the network is no refusal, and the running session holds no start.
        assert!(
            !seen.iter().any(|l| l.starts_with("carplay_start_blocked")),
            "{seen:?}"
        );
        let admitted = seen
            .iter()
            .position(|l| l == "carplay_wait_end network ready");
        assert!(!seen[admitted.unwrap()..]
            .iter()
            .any(|l| l.starts_with("carplay_wait network")
                || l.starts_with("carplay_waiting network")));
        // Provisioning is a named wait of its own, ended with what happened.
        let outcome = match a.operation(Operation::Initialize) {
            Ok(()) => "ready",
            Err(_) => "failed",
        };
        let seen = start_lines(&lines);
        assert_eq!(
            seen[seen.len() - 2..],
            [
                "carplay_wait provisioning".to_string(),
                format!("carplay_wait_end provisioning {outcome}")
            ]
        );
        a.operation(Operation::Stop).unwrap();
        let seen = start_lines(&lines);
        assert_eq!(
            seen[seen.len() - 2..],
            ["carplay_wait stop", "carplay_wait_end stop done"]
        );
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn refused_starts_log_once_and_the_stall_watchdog_choice_applies_at_once() {
        let (mut a, dir) = fake();
        // Both switches default on.
        assert!(a.events.diagnostics.enabled());
        let lines = record_events(&mut a);
        for (key, value) in [
            ("experimentalStallWatchdog", false),
            ("experimentalStallWatchdog", true),
            ("experimentalActive", false),
            ("experimentalActive", true),
        ] {
            a.update(serde_json::json!({key:value}).as_object().unwrap().clone());
            assert_eq!(a.events.diagnostics.enabled(), value, "{key}={value}");
        }
        a.action("start").unwrap();
        assert_eq!(a.action("start"), Err("error.busy".into()));
        assert_eq!(a.action("start"), Err("error.busy".into()));
        assert_eq!(a.action("restart"), Err("error.busy".into()));
        assert_eq!(
            start_lines(&lines),
            ["carplay_start_blocked busy", "carplay_start_blocked busy"]
        );
        let actions: Vec<_> = lines
            .lock()
            .unwrap()
            .iter()
            .map(|l| l["action"].clone())
            .collect();
        assert_eq!(
            actions,
            [serde_json::json!("start"), serde_json::json!("restart")]
        );
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn log_export_info_lists_process_watchdog_and_wait_facts() {
        struct Quiet;
        impl crate::logs::Journal for Quiet {
            fn read(&self, _: u64, _: u64) -> std::io::Result<crate::logs::JournalSlice> {
                Ok(crate::logs::JournalSlice {
                    bytes: Vec::new(),
                    truncated: false,
                })
            }
        }
        let (mut a, dir) = fake();
        Arc::get_mut(&mut a).unwrap().log_exports = crate::logs::Service::with_sources(
            dir.clone(),
            Arc::new(Quiet),
            Arc::new(crate::logs::http::Client::new(
                Duration::from_secs(1),
                65536,
                "test",
            )),
        );
        let detail = serde_json::json!({"reason":"radio_unavailable"});
        a.events.diagnostics.begin(&a.events, "network", detail);
        let export = a.build_log_export("10m").unwrap();
        use std::io::Read;
        let mut bytes = Vec::new();
        a.log_exports
            .download(&export.id)
            .unwrap()
            .file
            .read_to_end(&mut bytes)
            .unwrap();
        let entries = crate::logs::unzip(&bytes);
        let info = String::from_utf8_lossy(&entries.iter().find(|e| e.0 == "info.txt").unwrap().1)
            .into_owned();
        for fact in [
            "\"hostProfile\"",
            "\nProcess: ",
            "\nPrevious process: ",
            "\nProcess exit: ",
            "\nStall watchdog: {\"droppedDiagnostics\":0,\"enabled\":true",
            "\nSession: ",
            "\nWaits: {",
            "\"for\":\"network\",\"reason\":\"radio_unavailable\"",
        ] {
            assert!(info.contains(fact), "{fact} in {info}");
        }
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn missing_wifi_keeps_admin_available_and_starts_once_when_it_appears() {
        let (mut a, dir) = fake();
        let outputs = Arc::new(OutputFake::default());
        {
            let a = Arc::get_mut(&mut a).unwrap();
            a.child_factory = outputs.clone();
            a.net_root = dir.join("net");
            a.backends.station = crate::host_backends::station_with_cli(
                a.runtime.runner.clone(),
                "/usr/bin/true".into(),
                a.runtime.clone(),
            );
            *a.provision.lock().unwrap() = None;
        }
        a.inventory.set(crate::net_inventory::Inventory {
            probed_at: Some(1),
            ..Default::default()
        });
        a.start_supervisor().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while outputs.launches.lock().unwrap().is_empty() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        a.operation(Operation::Startup).unwrap();
        a.tick().unwrap();
        assert!(a.supervisor.lock().unwrap().is_some());
        assert!(a.provision.lock().unwrap().is_none());
        assert!(a.controller.lock().unwrap().is_none());
        let status = serde_json::to_value(a.status()).unwrap();
        assert_eq!(status["host"], "openxplay-linux.local");
        let host = &status["hostProfile"];
        assert!(host.get("hostname").is_none());
        assert_eq!(host["profilePath"], "/etc/openxplay/host.json");
        assert!(host.get("bluetoothAdapter").is_some());
        assert_eq!(host["wifiPresent"], false);
        assert_eq!(host["wifiInterface"], "wlan0");
        assert_eq!(host["secureConnections"], "pairing-off");
        assert_eq!(host["profileLoaded"], false);
        assert!(host["model"].is_null() && host["profileError"].is_null());
        let mut auth = AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap();
        let token = auth
            .login("127.0.0.1".parse().unwrap(), "openxplay", 100)
            .ok()
            .unwrap();
        let server = AdminServer::new(
            a.clone(),
            auth,
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        for path in ["/api/v0/status", "/api/v0/settings"] {
            let request = Request {
                method: "GET".into(),
                path: path.into(),
                headers: BTreeMap::from([("cookie".into(), format!("oxp_admin={token}"))]),
                body: Vec::new(),
                peer: "127.0.0.1".parse().unwrap(),
            };
            assert_eq!(server.route(&request, 101).status, 200);
        }
        std::fs::create_dir_all(dir.join("net/wlan0")).unwrap();
        std::fs::write(dir.join("net/wlan0/address"), "AA:BB:CC:DD:EE:FF").unwrap();
        a.start_policy.lock().unwrap().skip();
        a.tick().unwrap();
        assert!(a.provision.lock().unwrap().is_some());
        let controller = a.controller.lock().unwrap().clone().unwrap();
        a.tick().unwrap();
        assert!(Arc::ptr_eq(
            &controller,
            a.controller.lock().unwrap().as_ref().unwrap()
        ));
        assert_eq!(
            serde_json::to_value(a.status()).unwrap()["hostProfile"]["wifiPresent"],
            true
        );
        assert!(matches!(
            a.update(
                serde_json::json!({"wifiInterface":"wlan9"})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::NextStart
        ));
        a.tick().unwrap();
        assert!(a.provision.lock().unwrap().is_none());
        assert!(a.controller.lock().unwrap().is_none());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn profile_defaults_survive_admin_updates_and_gap_includes_zero() {
        for seconds in [0, 15, 60] {
            let (mut a, dir) = fake();
            let options = {
                let a = Arc::get_mut(&mut a).unwrap();
                a.options.host_profile.profile.wifi_interface = "wlan7".into();
                a.options.host_profile.profile.hotspot_channel = 11;
                a.options.host_profile.profile.hotspot_down_gap_seconds = seconds;
                a.options.host_profile.profile.hotspot_down_gap_explicit = true;
                a.options.host_profile.profile.secure_connections =
                    crate::host_profile::SecureConnectionsPolicy::Untouched;
                a.options.clone()
            };
            let state = a.state.clone();
            let events = a.events.clone();
            drop(a);
            let a = LinuxRelayAdmin::with_optional_provisioner(
                options,
                state,
                events,
                None,
                runtime_paths(&dir),
            )
            .unwrap();
            assert_eq!(a.settings().values["wifiInterface"], "wlan7");
            assert_eq!(a.settings().values["hotspotChannel"], 11);
            assert!(matches!(
                a.update(
                    serde_json::json!({"output":"both"})
                        .as_object()
                        .unwrap()
                        .clone()
                ),
                AdminUpdate::AppliedNow
            ));
            let saved = Settings::load(&dir).unwrap();
            assert_eq!(saved.wifi_interface, "wlan7");
            assert_eq!(saved.hotspot_channel, 11);
            a.hotspot_lifetime
                .remove_stale_once(|| Ok(true))
                .unwrap()
                .unwrap();
            let remaining = a.hotspot_lifetime.down_remaining();
            if seconds == 0 {
                assert_eq!(remaining, Duration::ZERO);
            } else {
                assert!(
                    remaining > Duration::from_secs(seconds - 1)
                        && remaining <= Duration::from_secs(seconds)
                );
            }
            let status = serde_json::to_value(a.status()).unwrap();
            assert_eq!(status["hostProfile"]["secureConnections"], "untouched");
            assert!(status.get("bluetoothSecureConnections").is_none());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    struct UnusedPlatform;
    impl WirelessPlatform for UnusedPlatform {
        fn check_bluetooth_radio(&self) -> Result<()> {
            unreachable!()
        }
        fn prepare_mfi(&self) -> Result<MfiResource> {
            unreachable!()
        }
        fn create_hotspot(&self) -> Result<Arc<dyn WirelessHotspotManager>> {
            unreachable!()
        }
        fn start_pairing(&self) -> Result<Option<Arc<dyn CloseResource>>> {
            unreachable!()
        }
        fn select_iphone(&self) -> Result<BondedDevice> {
            unreachable!()
        }
        fn create_bluetooth(&self, _: &BondedDevice) -> Result<Arc<dyn BluetoothConnection>> {
            unreachable!()
        }
        fn create_network(
            &self,
            _: WirelessHotspotInfo,
            _: Arc<dyn carplay_iap2::mfi::MfiAuthenticator>,
            _: NetworkCallbacks,
        ) -> Result<Arc<dyn NetworkAttachment>> {
            unreachable!()
        }
        fn run_bootstrap(
            &self,
            _: Arc<dyn BluetoothConnection>,
            _: Arc<dyn PendingNetwork>,
            _: Arc<dyn carplay_iap2::mfi::MfiAuthenticator>,
            _: BootstrapCallbacks,
        ) -> Result<()> {
            unreachable!()
        }
    }
    fn fake_controller(a: &LinuxRelayAdmin) -> Arc<dyn CarPlayController> {
        let controller: Arc<dyn CarPlayController> = Arc::new(LinuxCarPlayController::new(
            Arc::new(UnusedPlatform),
            a.events.clone(),
            Duration::from_secs(1),
        ));
        *a.controller.lock().unwrap() = Some(controller.clone());
        controller
    }
    /// An admin whose nmcli blocks until `release` exists in the state directory, with a saved
    /// Wi-Fi client profile the startup provisioning takes down.
    fn blocked_provisioning(mode: &str) -> (Arc<LinuxRelayAdmin>, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "wp4a-startup-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let nmcli = dir.join("nmcli");
        std::fs::write(
            &nmcli,
            format!(
                "#!/bin/sh\nwhile [ ! -e '{}' ]; do sleep 0.01; done\n",
                dir.join("release").display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&nmcli, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            dir.join("wifi-owned.json"),
            r#"{"ap":null,"clients":[{"uuid":"00000000-0000-4000-8000-000000000001","ssid":"Home"}]}"#,
        )
        .unwrap();
        Settings {
            boot_start: true,
            network_mode: mode.into(),
            shared_wifi_networks: vec![SharedWifiNetwork {
                ssid: "Home".into(),
                password: "password".into(),
            }],
            ..Default::default()
        }
        .save(&dir)
        .unwrap();
        let options =
            Options::parse(&["--state-dir".into(), dir.to_str().unwrap().into()]).unwrap();
        let state = Arc::new(PersistentState::open(&dir).unwrap());
        let events = Arc::new(JsonEvents::new(Arc::new(|_| {})));
        let provision = WifiProvisioner::new(
            ProcessNetworkManagerCli { executable: nmcli },
            "wlan0".into(),
            "AA:BB:CC:DD:EE:FF",
            &dir,
        )
        .unwrap();
        let mut admin = LinuxRelayAdmin::with_provisioner(
            options,
            state,
            events,
            provision,
            runtime_paths(&dir),
        )
        .unwrap();
        admin.start_policy.lock().unwrap().skip();
        admin.net_root = dir.join("present-net");
        std::fs::create_dir_all(admin.net_root.join("wlan0")).unwrap();
        std::fs::write(admin.net_root.join("wlan0/address"), "02:00:00:00:00:01").unwrap();
        (Arc::new(admin), dir)
    }
    #[test]
    fn startup_starts_bluetooth_before_the_wifi_provisioning_and_holds_only_the_ap() {
        let (a, dir) = blocked_provisioning("device-hotspot");
        let startup = {
            let a = a.clone();
            std::thread::spawn(move || {
                let _lifecycle = a.lifecycle.lock().unwrap();
                a.operation(Operation::Startup)
            })
        };
        // The controller, and with it Bluetooth, runs while nmcli still blocks the provisioning.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while a.controller.lock().unwrap().is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "controller not started"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(50));
        assert!(!startup.is_finished());
        // Its AP may not start yet.
        assert!(!a.provisioning.wait(Duration::ZERO));
        std::fs::write(dir.join("release"), "").unwrap();
        startup.join().unwrap().unwrap();
        assert!(a.provisioning.wait(Duration::ZERO));
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn in_shared_wifi_mode_the_controller_follows_the_startup_provisioning() {
        // There the provisioning brings the network up and decides whether CarPlay runs.
        let (a, dir) = blocked_provisioning("shared-wifi");
        let startup = {
            let a = a.clone();
            std::thread::spawn(move || {
                let _lifecycle = a.lifecycle.lock().unwrap();
                let _ = a.operation(Operation::Startup);
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(!startup.is_finished());
        assert!(a.controller.lock().unwrap().is_none());
        assert!(a.provisioning.wait(Duration::ZERO));
        std::fs::write(dir.join("release"), "").unwrap();
        startup.join().unwrap();
        assert!(a.controller.lock().unwrap().is_none());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn a_finished_operation_ends_the_wait_for_the_next_tick() {
        let (a, dir) = fake();
        let started = std::time::Instant::now();
        a.wait_for_tick(Duration::from_millis(30));
        assert!(started.elapsed() >= Duration::from_millis(30));
        // The operation worker finishes the startup network initialisation: the controller
        // starts on the tick that follows at once.
        a.wake_tick();
        let started = std::time::Instant::now();
        a.wait_for_tick(Duration::from_secs(5));
        assert!(started.elapsed() < Duration::from_secs(1));
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn receiver_restart_closes_only_the_carplay_controller() {
        let (a, dir) = fake();
        let controller = fake_controller(&a);
        {
            let mut input = a.input.state.lock().unwrap();
            input.desired_fps = 45;
        }
        a.input
            .control(&oxp_wire::Message::Json(
                serde_json::json!({"type":"restart"}),
            ))
            .unwrap();
        assert!(a
            .input
            .apply_requested_restart(|| a.stop_controller_for_restart(true)));
        assert!(a.controller.lock().unwrap().is_none());
        assert!(a.desired.load(Ordering::Acquire));
        assert_eq!(a.input.configure_frame_rate(), 45);
        assert!(!a
            .input
            .apply_requested_restart(|| panic!("duplicate controller stop")));
        drop(controller);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn settings_restart_closes_retained_hotspot() {
        let (a, dir) = fake();
        let hotspot = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
        let lease = a.hotspot_lifetime.create(|| hotspot.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        a.hotspot_lifetime.keep_for_restart();
        lease.close().unwrap();
        assert_eq!(hotspot.closes.load(Ordering::SeqCst), 0);
        // A network settings change must close even an AP retained between controllers.
        let _ = a.operation(Operation::Network {
            mode_changed: true,
            reconnect: false,
        });
        assert_eq!(hotspot.closes.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
    fn settle_follow(a: &LinuxRelayAdmin) {
        a.follow.lock().unwrap().changed = Some(std::time::Instant::now() - Duration::from_secs(1));
        a.reconcile_carlink_network().unwrap();
    }
    #[test]
    fn carlink_override_restarts_controller_is_transient_and_debounces_departure() {
        use std::os::unix::fs::PermissionsExt;
        for mode in ["device-hotspot", "shared-wifi"] {
            let (mut a, dir) = fake();
            let captured = Arc::new(Mutex::new(Vec::new()));
            let copy = captured.clone();
            Arc::get_mut(&mut a).unwrap().events =
                Arc::new(JsonEvents::new(Arc::new(move |line| {
                    copy.lock().unwrap().push(line)
                })));
            let script = dir.join("nmcli-fake");
            std::fs::write(&script, r#"#!/bin/sh
printf '%s\n' "$*" >> "$(dirname "$0")/nmcli-calls"
case "$*" in
  *GENERAL.CON-UUID*) echo 00000000-0000-4000-8000-000000000001 ;;
  *'connection add'*) echo "Connection 'Home' (00000000-0000-4000-8000-000000000002) successfully added." ;;
  *802-11-wireless-security.psk*) echo private-car-secret ;;
  *802-11-wireless.ssid*) echo DIRECT-Car ;;
  *SSID,SIGNAL,SECURITY,IN-USE*) echo 'Home:82:WPA2:*' ;;
esac
"#).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
            *a.provision.lock().unwrap() = Some(Box::new(
                WifiProvisioner::new(
                    ProcessNetworkManagerCli { executable: script },
                    "wlan0".into(),
                    "AA:BB:CC:DD:EE:FF",
                    &dir,
                )
                .unwrap()
                .with_local_dns(a.runtime.managed_dns()),
            ));
            {
                let mut settings = a.settings.lock().unwrap();
                settings.network_mode = mode.into();
                settings.shared_wifi_networks = vec![SharedWifiNetwork {
                    ssid: "Home".into(),
                    password: "home-password".into(),
                }];
                settings.save(&dir).unwrap();
            }
            let original = std::fs::read(dir.join("settings.json")).unwrap();
            let first = fake_controller(&a);
            let shell = a.clone();
            let closing = first.clone();
            *a.worker.lock().unwrap() = Some(std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                while closing.stage() != "Stopped" {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::yield_now();
                }
                assert_eq!(
                    shell
                        .carplay_options()
                        .unwrap()
                        .settings
                        .shared_wifi_networks[0]
                        .ssid,
                    "DIRECT-Car"
                );
            }));
            // Receiver negotiation can request a restart while Carlink joins.
            a.input.state.lock().unwrap().desired_fps = 30;
            a.input
                .control(&oxp_wire::Message::Json(
                    serde_json::json!({"type":"restart"}),
                ))
                .unwrap();
            let hotspot = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
            let lease = a.hotspot_lifetime.create(|| hotspot.clone()).unwrap();
            lease.start(Duration::from_secs(1)).unwrap();
            let joined = serde_json::json!({"type":"link","state":"joined","network":{"ssid":"DIRECT-Car","interface":"wlan0"}});
            a.follow.lock().unwrap().observe(&joined, "wlan1");
            settle_follow(&a);
            assert!(a.network_override.lock().unwrap().is_none());
            a.follow.lock().unwrap().observe(&joined, "wlan0");
            a.reconcile_carlink_network().unwrap();
            assert_eq!(first.stage(), "Idle");
            settle_follow(&a);
            assert_eq!(first.stage(), "Stopped");
            assert_eq!(hotspot.closes.load(Ordering::SeqCst), 1);
            // The replacement snapshots pending receiver changes before its worker runs.
            assert_eq!(a.input.configure_session().0, 30);
            let options = a.carplay_options().unwrap();
            assert_eq!(options.settings.network_mode, "shared-wifi");
            assert_eq!(options.settings.shared_wifi_networks[0].ssid, "DIRECT-Car");
            assert_eq!(
                options.settings.shared_wifi_networks[0].password,
                "private-car-secret"
            );
            assert!(a.status().network.following_carlink);
            let second = fake_controller(&a);
            a.tick().unwrap();
            assert_eq!(second.stage(), "Idle");
            a.follow.lock().unwrap().observe(&joined, "wlan0");
            settle_follow(&a);
            a.tick().unwrap();
            assert_eq!(second.stage(), "Idle");
            assert_eq!(hotspot.closes.load(Ordering::SeqCst), 1);
            let lost = serde_json::json!({"type":"link","state":"lost"});
            a.follow.lock().unwrap().observe(&lost, "wlan0");
            a.reconcile_carlink_network().unwrap();
            a.follow.lock().unwrap().observe(&joined, "wlan0");
            settle_follow(&a);
            assert_eq!(second.stage(), "Idle");
            // Initialize cannot bring the home profile up during the override.
            let calls = std::fs::read_to_string(dir.join("nmcli-calls")).unwrap();
            a.operation(Operation::Initialize).unwrap();
            assert_eq!(
                calls,
                std::fs::read_to_string(dir.join("nmcli-calls")).unwrap()
            );
            assert!(a
                .provision
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .start()
                .is_err());
            a.follow.lock().unwrap().observe(
                &serde_json::json!({"type":"state","state":"Ended"}),
                "wlan0",
            );
            settle_follow(&a);
            assert_eq!(second.stage(), "Stopped");
            assert!(a.network_override.lock().unwrap().is_none());
            assert!(!a.status().network.following_carlink);
            assert_eq!(a.carplay_options().unwrap().settings.network_mode, mode);
            if mode == "shared-wifi" {
                assert!(std::fs::read_to_string(dir.join("nmcli-calls"))
                    .unwrap()
                    .contains("connection up uuid"));
            }
            assert_eq!(original, std::fs::read(dir.join("settings.json")).unwrap());
            assert!(!serde_json::to_string(&a.status())
                .unwrap()
                .contains("private-car-secret"));
            assert!(!captured
                .lock()
                .unwrap()
                .join(" ")
                .contains("private-car-secret"));
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn credential_failure_holds_radio_until_link_ends_without_starting_hotspot() {
        let (a, dir) = fake();
        let controller = fake_controller(&a);
        a.follow.lock().unwrap().observe(&serde_json::json!({"type":"link","state":"joined","network":{"ssid":"DIRECT-Car","interface":"wlan0"}}), "wlan0");
        a.follow.lock().unwrap().changed = Some(std::time::Instant::now() - Duration::from_secs(1));
        let error = a.reconcile_carlink_network().unwrap_err();
        assert_eq!(
            error.to_string(),
            "Could not read the Carlink network credentials"
        );
        assert_eq!(controller.stage(), "Stopped");
        assert!(a
            .provision
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .stop()
            .is_err());
        a.tick().unwrap();
        assert!(a.controller.lock().unwrap().is_none());
        a.follow
            .lock()
            .unwrap()
            .observe(&serde_json::json!({"type":"receiver-stopped"}), "wlan0");
        settle_follow(&a);
        a.provision
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .stop()
            .unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn picture_settings_live_outside_rollback_settings() {
        let (a, dir) = fake();
        assert!(a
            .settings()
            .schema
            .iter()
            .any(|s| s.id == "picture" && s.fields.len() == 4));
        let update =
            serde_json::json!({"resolution":"native","screenWidth":250,"videoCodec":"h264"});
        assert!(matches!(
            a.update(update.as_object().unwrap().clone()),
            AdminUpdate::AppliedNow
        ));
        assert_eq!(a.settings().values["resolution"], "native");
        assert_eq!(a.input.display_settings().screen_width, 250);
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
        assert!(stored.get("resolution").is_none());
        Settings::load(&dir).unwrap();
        assert!(matches!(
            a.update(
                serde_json::json!({"screenWidth":150})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::Errors(_)
        ));
        assert_eq!(a.input.display_settings().screen_width, 250);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn network_values_are_secret_free_and_updates_preserve_omitted_passwords() {
        let (a, dir) = fake();
        a.settings.lock().unwrap().shared_wifi_networks = vec![SharedWifiNetwork {
            ssid: "Home".into(),
            password: "private-secret".into(),
        }];
        let settings = a.settings();
        let value = serde_json::to_value(settings).unwrap();
        assert!(!value.to_string().contains("private-secret"));
        assert_eq!(
            value["values"]["sharedWifiNetworks"][0],
            serde_json::json!({"ssid":"Home","hasPassword":true,"current":false})
        );
        let schema = settings_schema();
        let field = schema
            .iter()
            .flat_map(|s| &s.fields)
            .find(|f| f.key == "sharedWifiNetworks")
            .unwrap();
        assert_eq!(field.field_type, "networks");
        assert!(field.visible_when.is_none());
        let current = a.settings.lock().unwrap().clone();
        let next = update_settings(&current, serde_json::json!({"sharedWifiNetworks":[{"ssid":"Home"},{"ssid":"Open","password":""}]}).as_object().unwrap().clone()).unwrap();
        assert_eq!(next.shared_wifi_networks[0].password, "private-secret");
        assert!(next.shared_wifi_networks[1].password.is_empty());
        for list in [
            serde_json::json!([{"ssid":"New"}]),
            serde_json::json!([{"ssid":"Home","password":null}]),
            serde_json::json!([{"ssid":"Home","password":"short"}]),
        ] {
            let error = update_settings(
                &current,
                serde_json::json!({"sharedWifiNetworks":list})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap_err();
            assert_eq!(error["sharedWifiNetworks"], "error.networks");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn mode_switch_is_saved_now_but_radio_work_waits_for_response() {
        let (a, dir) = fake();
        let values = || {
            serde_json::json!({"networkMode":"shared-wifi"})
                .as_object()
                .unwrap()
                .clone()
        };
        match a.update(values()) {
            AdminUpdate::Errors(errors) => assert_eq!(errors["networkMode"], "error.noSavedWifi"),
            _ => panic!("empty shared Wi-Fi accepted"),
        }
        a.settings
            .lock()
            .unwrap()
            .shared_wifi_networks
            .push(SharedWifiNetwork {
                ssid: "Home".into(),
                password: "password".into(),
            });
        assert!(matches!(a.update(values()), AdminUpdate::AppliedNow));
        assert_eq!(Settings::load(&dir).unwrap().network_mode, "shared-wifi");
        let guard = a.operation_rx.lock().unwrap();
        let rx = guard.as_ref().unwrap();
        assert!(rx.try_recv().is_err());
        a.response_sent();
        assert!(matches!(
            rx.try_recv().unwrap(),
            Operation::Network {
                mode_changed: true,
                ..
            }
        ));
        assert!(matches!(
            a.update(
                serde_json::json!({"hotspotChannel":6})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::NextStart
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn wifi_put_uses_saved_password_only_and_defers_radio_changes() {
        let (a, dir) = fake();
        a.settings
            .lock()
            .unwrap()
            .shared_wifi_networks
            .push(SharedWifiNetwork {
                ssid: "Home".into(),
                password: "private-secret".into(),
            });
        let mut auth = AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap();
        let token = auth
            .login("127.0.0.1".parse().unwrap(), "openxplay", 100)
            .ok()
            .unwrap();
        let server = AdminServer::new(
            a.clone(),
            auth,
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        let mut request = Request {
            method: "PUT".into(),
            path: "/api/v0/wifi".into(),
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("x-openxplay-admin".into(), "1".into()),
                ("cookie".into(), format!("oxp_admin={token}")),
            ]),
            body: br#"{"ssid":"Home"}"#.to_vec(),
            peer: "127.0.0.1".parse().unwrap(),
        };
        assert_eq!(server.route(&request, 100).status, 200);
        let guard = a.operation_rx.lock().unwrap();
        let rx = guard.as_ref().unwrap();
        assert!(rx.try_recv().is_err());
        a.response_sent();
        assert!(
            matches!(rx.try_recv().unwrap(), Operation::Join(ssid, password) if ssid == "Home" && password == "private-secret")
        );
        request.body = br#"{"ssid":"New"}"#.to_vec();
        let response = server.route(&request, 102);
        assert!(String::from_utf8(response.body)
            .unwrap()
            .contains("error.networks"));
        request.body = br#"{"ssid":"Home","password":null}"#.to_vec();
        assert_eq!(server.route(&request, 104).status, 422);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn linux_status_has_hotspot_credentials_and_wifi_control() {
        let (a, dir) = fake();
        let json = serde_json::to_value(a.status()).unwrap();
        assert_eq!(json["wifiControl"], true);
        assert_eq!(json["hotspot"]["active"], false);
        assert!(json["hotspot"]["ip"].is_null());
        let (ssid, password) = crate::linux_hotspot::hotspot_credentials(&dir).unwrap();
        assert_eq!(json["hotspot"]["ssid"], ssid);
        assert_eq!(json["hotspot"]["password"], password);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn hotspot_api_requires_auth_validates_and_defers_radio_change() {
        let (a, dir) = fake();
        let mut auth = AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap();
        let token = auth
            .login("127.0.0.1".parse().unwrap(), "openxplay", 100)
            .ok()
            .unwrap();
        let server = AdminServer::new(
            a.clone(),
            auth,
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        let mut request = Request {
            method: "PUT".into(),
            path: "/api/v0/hotspot".into(),
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("x-openxplay-admin".into(), "1".into()),
            ]),
            body: br#"{"ssid":"New Relay","password":"newpass123"}"#.to_vec(),
            peer: "127.0.0.1".parse().unwrap(),
        };
        assert_eq!(server.route(&request, 100).status, 401);
        request.path = "/api/v0/status".into();
        request.method = "GET".into();
        let response = server.route(&request, 100);
        assert_eq!(response.status, 401);
        assert!(!String::from_utf8(response.body)
            .unwrap()
            .contains("password\":"));
        request.path = "/api/v0/hotspot".into();
        request.method = "PUT".into();
        request
            .headers
            .insert("cookie".into(), format!("oxp_admin={token}"));
        request.headers.remove("x-openxplay-admin");
        assert_eq!(server.route(&request, 100).status, 403);
        request
            .headers
            .insert("x-openxplay-admin".into(), "1".into());
        for body in [
            serde_json::json!({"ssid":"", "password":"12345678"}),
            serde_json::json!({"ssid":"Relay", "password":"short"}),
            serde_json::json!({"ssid":null,"password":"12345678"}),
            serde_json::json!({"ssid":"Relay","password":"12345678","band":"6"}),
            serde_json::json!({"ssid":"Relay","password":"12345678","band":null}),
            serde_json::json!({"ssid":"Relay","password":"12345678","band":5}),
        ] {
            request.body = serde_json::to_vec(&body).unwrap();
            assert_eq!(server.route(&request, 100).status, 422);
        }
        request.body = br#"{"ssid":"New Relay","password":"newpass123","band":"2.4"}"#.to_vec();
        assert_eq!(server.route(&request, 100).status, 200);
        assert_eq!(Settings::load(&dir).unwrap().hotspot_band, "2.4");
        assert_eq!(
            crate::linux_hotspot::hotspot_credentials(&dir).unwrap(),
            ("New Relay".into(), "newpass123".into())
        );
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        assert!(rx.try_recv().is_err());
        assert!(a.status().notices.contains(&"notice.nextStart".into()));
        a.response_sent();
        let hotspot = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
        let lease = a.hotspot_lifetime.create(|| hotspot.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        let op = rx.try_recv().unwrap();
        assert!(matches!(op, Operation::Hotspot(None)));
        a.operation(op).unwrap();
        assert_eq!(hotspot.closes.load(Ordering::SeqCst), 1);
        assert!(!a.apply_pending.load(Ordering::Acquire));
        assert!(a.status().hotspot.unwrap().has_password);
        // In shared mode saving credentials leaves the current controller and AP alone.
        a.settings.lock().unwrap().network_mode = "shared-wifi".into();
        let controller = fake_controller(&a);
        a.hotspot_update("Later", "laterpass1", None, None).unwrap();
        a.response_sent();
        a.operation(rx.try_recv().unwrap()).unwrap();
        assert!(Arc::ptr_eq(
            a.controller.lock().unwrap().as_ref().unwrap(),
            &controller
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn plugins_api_and_output_only_apply() {
        let (mut a, dir) = fake();
        let root = dir.join("packages");
        crate::plugins::tests::package(
            &root,
            "org.openxplay.pet",
            "(MIT OR Apache-2.0) AND CC-BY-NC-SA-4.0",
        );
        let outputs = Arc::new(OutputFake::default());
        let admin = Arc::get_mut(&mut a).unwrap();
        admin.options.plugins_root = root;
        admin.child_factory = outputs.clone();
        a.start_supervisor().unwrap();
        let wait = |count| {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while outputs.launches.lock().unwrap().len() < count {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        wait(1);
        let controller = fake_controller(&a);
        let hotspot = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
        let lease = a.hotspot_lifetime.create(|| hotspot.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        let contract = serde_json::to_value(a.settings()).unwrap();
        assert_eq!(contract["values"]["plugins"], false);
        assert_eq!(contract["values"]["pluginsEnabled"], serde_json::json!([]));
        assert_eq!(contract["installedPlugins"][0]["name"]["zh-CN"], "宠物");
        assert_eq!(contract["installedPlugins"][0]["nonCommercialAssets"], true);
        assert!(contract["schema"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "plugins"));
        for invalid in [
            serde_json::json!(["org.missing"]),
            serde_json::json!(["../escape"]),
            serde_json::json!(["org.openxplay.pet", "org.openxplay.pet"]),
            serde_json::json!("org.openxplay.pet"),
        ] {
            assert!(matches!(
                a.update(
                    serde_json::json!({"pluginsEnabled":invalid})
                        .as_object()
                        .unwrap()
                        .clone()
                ),
                AdminUpdate::Errors(_)
            ));
        }
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        for (index, patch) in [
            serde_json::json!({"pluginsEnabled":["org.openxplay.pet"]}),
            serde_json::json!({"plugins":true}),
            serde_json::json!({"plugins":false}),
            serde_json::json!({"pluginsEnabled":[]}),
        ]
        .into_iter()
        .enumerate()
        {
            assert!(matches!(
                a.update(patch.as_object().unwrap().clone()),
                AdminUpdate::AppliedNow
            ));
            a.response_sent();
            let operation = rx.try_recv().unwrap();
            assert!(matches!(operation, Operation::Outputs(None)));
            a.operation(operation).unwrap();
            wait(index + 2);
            assert_eq!(a.input.plugins_enabled(), index == 1);
            assert!(Arc::ptr_eq(
                a.controller.lock().unwrap().as_ref().unwrap(),
                &controller
            ));
            assert_eq!(hotspot.closes.load(Ordering::SeqCst), 0);
            assert!(!a.restart_required.load(Ordering::Acquire));
        }
        let launches = outputs.launches.lock().unwrap();
        assert_eq!(launches[0], launches[1]);
        assert!(launches[2].iter().any(|arg| arg == "--plugins-enabled"));
        assert_eq!(launches[0], launches[3]);
        assert_eq!(launches[0], launches[4]);
        assert_eq!(outputs.stops.load(Ordering::SeqCst), 4);
        drop(launches);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[derive(Default)]
    struct OutputFake {
        launches: Mutex<Vec<Vec<std::ffi::OsString>>>,
        stops: Arc<std::sync::atomic::AtomicUsize>,
    }
    struct OutputChild(Arc<std::sync::atomic::AtomicUsize>);
    impl RelayChild for OutputChild {
        fn exited(&mut self) -> std::io::Result<bool> {
            Ok(false)
        }
        fn stop(&mut self) -> std::io::Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    impl ChildFactory for OutputFake {
        fn spawn(
            &self,
            _: &std::path::Path,
            args: &[std::ffi::OsString],
            _: EventSink,
        ) -> std::io::Result<Box<dyn RelayChild>> {
            self.launches.lock().unwrap().push(args.to_vec());
            Ok(Box::new(OutputChild(self.stops.clone())))
        }
    }
    #[test]
    fn output_apply_replaces_only_child_and_retains_controller_and_hotspot() {
        let (mut a, dir) = fake();
        let outputs = Arc::new(OutputFake::default());
        Arc::get_mut(&mut a).unwrap().child_factory = outputs.clone();
        a.start_supervisor().unwrap();
        let wait_for_launches = |count| {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while outputs.launches.lock().unwrap().len() < count {
                assert!(
                    std::time::Instant::now() < deadline,
                    "output did not launch"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        wait_for_launches(1);
        std::fs::create_dir_all(dir.join("carlink")).unwrap();
        std::fs::write(
            dir.join("carlink/trust.json"),
            serde_json::to_vec(
                &serde_json::json!({"peers":{"[\"ble\",\"unit\"]":format!("{}==", "A".repeat(86))}}),
            )
            .unwrap(),
        )
        .unwrap();
        let controller = fake_controller(&a);
        let hotspot = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
        let lease = a.hotspot_lifetime.create(|| hotspot.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            a.update(
                serde_json::json!({"output":"carlink"})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::AppliedNow
        ));
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        assert_eq!(outputs.launches.lock().unwrap().len(), 1);
        a.response_sent();
        a.operation(rx.try_recv().unwrap()).unwrap();
        wait_for_launches(2);
        assert_eq!(outputs.stops.load(Ordering::SeqCst), 1);
        let launches = outputs.launches.lock().unwrap();
        assert_eq!(launches.len(), 2);
        assert!(launches[0].iter().any(|arg| arg == "--web-port"));
        assert!(!launches[1].iter().any(|arg| arg == "--web-port"));
        assert!(Arc::ptr_eq(
            a.controller.lock().unwrap().as_ref().unwrap(),
            &controller
        ));
        assert_eq!(hotspot.closes.load(Ordering::SeqCst), 0);
        assert!(!a.apply_pending.load(Ordering::Acquire));
        drop(launches);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn outputs_apply_now_and_controller_changes_report_restart_required() {
        let (a, dir) = fake();
        let controller = fake_controller(&a);
        assert!(matches!(
            a.update(
                serde_json::json!({"output":"both"})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::AppliedNow
        ));
        assert!(a
            .pending
            .lock()
            .unwrap()
            .values()
            .any(|op| matches!(op, Operation::Outputs(None))));
        assert!(Arc::ptr_eq(
            a.controller.lock().unwrap().as_ref().unwrap(),
            &controller
        ));
        a.response_failed();
        assert!(a.status().notices.contains(&"notice.nextStart".into()));
        assert!(matches!(
            a.update(
                serde_json::json!({"rfcommChannel":3})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::NextStart
        ));
        assert!(matches!(
            a.update(
                serde_json::json!({"hotspotChannel":6})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::NextStart
        ));
        assert!(matches!(
            a.update(
                serde_json::json!({"carBluetoothAudio":false})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::NextStart
        ));
        assert!(!a.settings.lock().unwrap().car_bluetooth_audio);
        assert_eq!(a.settings().values["carBluetoothAudio"], false);
        assert!(!Settings::load(&dir).unwrap().car_bluetooth_audio);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn pairing_confirmation_cancel_and_stale_responses_use_matching_stdin_requests() {
        let (a, dir) = fake();
        let sent = capture_controls(&a);
        a.carlink.lock().unwrap().apply_event(&serde_json::json!({"type":"pin-request","request":17,"generation":1,"confirmation":true,"client":"unit"}), crate::admin::auth::monotonic_seconds());
        assert!(a.carlink_pairing(16, 1, "approve", None).is_err());
        assert!(a.carlink_pairing(17, 0, "approve", None).is_err());
        assert!(a.carlink_pairing(17, 1, "pin", Some("001234")).is_err());
        a.carlink_pairing(17, 1, "approve", None).unwrap();
        assert_eq!(sent.lock().unwrap()[0]["action"], "approve");
        pin_request(&a);
        a.carlink_pairing(17, 1, "cancel", None).unwrap();
        assert_eq!(sent.lock().unwrap()[1]["action"], "cancel");
        assert!(a.carlink_pairing(17, 1, "cancel", None).is_err());
        assert!(!carlink_pin_path(&dir).exists());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn timeout_rearms_only_after_current_generation_playing_and_stop_wins() {
        let (admin, dir) = fake();
        admin.settings.lock().unwrap().output = "carlink".into();
        admin.settings.lock().unwrap().auto_start = true;
        let event = |value| admin.carlink.lock().unwrap().apply_event(&value, 0);
        event(serde_json::json!({"type":"state","generation":1,"state":"Playing"}));
        event(serde_json::json!({"type":"carlink-starting"}));
        event(serde_json::json!({"type":"clink-restart","generation":2}));
        event(
            serde_json::json!({"type":"clink-view-size","generation":2,"width":1920,"height":1080}),
        );
        admin.start_policy.lock().unwrap().request(
            std::time::Instant::now() - Duration::from_secs(61),
            true,
            true,
        );
        admin.tick().unwrap();
        assert_eq!(
            admin
                .events
                .runtime_path_facts()
                .iphone
                .failure
                .unwrap()
                .code,
            "carlink_not_ready"
        );
        event(serde_json::json!({"type":"state","generation":1,"state":"Playing"}));
        admin.observe_automatic_start().unwrap();
        assert!(admin.events.runtime_path_facts().iphone.failure.is_some());
        event(serde_json::json!({"type":"carlink-starting"}));
        event(serde_json::json!({"type":"state","generation":3,"state":"Playing"}));
        admin.observe_automatic_start().unwrap();
        assert!(
            admin.events.runtime_path_facts().iphone.failure.is_some(),
            "pending replacement must not rearm"
        );
        event(serde_json::json!({"type":"clink-restart","generation":3}));
        admin.observe_automatic_start().unwrap();
        assert!(admin.events.runtime_path_facts().iphone.failure.is_none());
        assert!(admin.desired.load(Ordering::Acquire));
        admin.operation(Operation::Stop).unwrap();
        event(serde_json::json!({"type":"carlink-starting"}));
        event(serde_json::json!({"type":"clink-restart","generation":4}));
        event(serde_json::json!({"type":"state","generation":4,"state":"Playing"}));
        admin.observe_automatic_start().unwrap();
        assert!(!admin.desired.load(Ordering::Acquire));
        assert!(!admin.runtime_path_facts().iphone.armed);
        admin.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn carlink_summary_fields_reach_status_journal_and_attempt_timeline() {
        let (mut admin, dir) = fake();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let output = lines.clone();
        Arc::get_mut(&mut admin).unwrap().events =
            Arc::new(JsonEvents::new(Arc::new(move |line| {
                output.lock().unwrap().push(line)
            })));
        let sink = carlink_event_sink(
            admin.carlink.clone(),
            admin.armed_pin.clone(),
            dir.clone(),
            admin.events.clone(),
            Arc::new(Mutex::new(None)),
        );
        admin.settings.lock().unwrap().output = "both".into();
        let fields = serde_json::json!({"vendor":"Vendor","product":"Car","name":"Technical head unit","ssid":"CarWifi","frequency":"5180","mode":"window","view":"1920x1080","window":"1814x1200","stream":"1280x720","codec":"H264","hevc":"supported","audio":"true","uibc":"HIDC"});
        sink(serde_json::json!({"type":"state","state":"Playing","generation":2}));
        sink(
            serde_json::json!({"type":"clink-summary","generation":2,"atMillis":1234,"fields":fields}),
        );
        let status = serde_json::to_value(admin.status()).unwrap();
        assert_eq!(status["carlink"]["summary"], fields);
        assert_eq!(
            status["carlink"]["appliedPicture"],
            serde_json::json!({"width":1280,"height":720})
        );
        let records: Vec<serde_json::Value> = lines
            .lock()
            .unwrap()
            .iter()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let baseline = records
            .iter()
            .find(|v| v["event"] == "clink-summary")
            .unwrap();
        let timeline = records
            .iter()
            .find(|v| v["event"] == "timeline" && v["mark"] == "summary")
            .unwrap();
        assert_eq!(baseline["fields"], fields);
        assert_eq!(timeline["detail"]["fields"], fields);
        assert_eq!(timeline["detail"]["atMillis"], 1234);
        drop(sink);
        drop(admin);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn start_gate_ignores_saved_view_and_old_generation_playing() {
        let mut status = AdminCarlink::default();
        let now = std::time::Instant::now();
        let mut gate = crate::start_policy::StartPolicy::default();
        gate.request(now, true, true);
        status.apply_event(&serde_json::json!({"type":"clink-view-size","generation":2,"width":1920,"height":1080}),0);
        status.apply_event(
            &serde_json::json!({"type":"state","generation":1,"state":"Playing"}),
            0,
        );
        assert!(!gate
            .ready(now, true, status.state == "playing", true)
            .unwrap());
        status.apply_event(
            &serde_json::json!({"type":"clink-restart","generation":2,"result":"started"}),
            0,
        );
        assert!(!gate
            .ready(now, true, status.state == "playing", true)
            .unwrap());
        status.apply_event(
            &serde_json::json!({"type":"state","generation":2,"state":"Playing"}),
            0,
        );
        assert!(gate
            .ready(now, true, status.state == "playing", true)
            .unwrap());
        status.apply_event(&serde_json::json!({"type":"clink-summary","generation":2,"fields":{"stream":"1280x720","view":"1920x1080","codec":"H264"}}),0);
        assert_eq!(
            status.applied_picture,
            Some(CarlinkViewSize {
                width: 1280,
                height: 720
            })
        );
        status.apply_event(
            &serde_json::json!({"type":"state","generation":2,"state":"Ended"}),
            0,
        );
        assert!(status.applied_picture.is_none());
    }
    #[test]
    fn vehicle_events_reach_private_history_and_never_the_journal_or_timeline() {
        let (mut a, dir) = fake();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let copy = logs.clone();
        Arc::get_mut(&mut a).unwrap().events = Arc::new(JsonEvents::new(Arc::new(move |line| {
            copy.lock().unwrap().push(line)
        })));
        let children = link_children(&mut a);
        a.vehicle.lock().unwrap().enable(true);
        a.settings.lock().unwrap().output = "both".into();
        a.start_supervisor().unwrap();
        let (_, sink) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        sink(
            serde_json::json!({"type":"clink-vehicle","generation":1,"atMillis":123,"data":{"type":"Gps","latitude":45.123456,"timestamp":"18446744073709551615"}}),
        );
        sink(
            serde_json::json!({"type":"clink-vehicle-state","generation":1,"sensorActive":true,"malformedFrames":1}),
        );
        assert_eq!(a.vehicle()["reports"][0]["fields"]["latitude"], 45.123456);
        assert_eq!(a.vehicle()["malformedFrames"], 1);
        assert!(!logs.lock().unwrap().join(" ").contains("45.123456"));
        assert!(!logs.lock().unwrap().join(" ").contains("clink-vehicle"));
        assert!(!a
            .events
            .facts
            .lock()
            .unwrap()
            .timeline
            .iter()
            .any(|s| s.contains("45.123456")));
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn capture_controls(a: &LinuxRelayAdmin) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let controls = Arc::new(Mutex::new(Vec::new()));
        let copy = controls.clone();
        let pipe = ControlPipe::fake(move |v| {
            copy.lock().unwrap().push(v.clone());
            Ok(())
        });
        let settings = crate::supervisor_settings::supervisor_settings(
            &a.options,
            vec![],
            &[],
            String::new(),
            None,
            false,
        );
        *a.supervisor.lock().unwrap() = Some(
            RelaySupervisor::new(settings, Arc::new(ProcessChildFactory), Arc::new(|_| {}))
                .with_control_handle(Arc::new(Mutex::new(Some(pipe)))),
        );
        controls
    }
    fn pin_request(a: &LinuxRelayAdmin) {
        a.carlink.lock().unwrap().apply_event(
            &serde_json::json!({"type":"pin-request","client":"unit","request":17,"generation":1}),
            crate::admin::auth::monotonic_seconds(),
        );
    }
    #[test]
    fn connect_endpoint_validates_auth_csrf_digits_rate_and_optional_trusted_pin() {
        let (a, dir) = fake();
        a.settings.lock().unwrap().output = "both".into();
        let mut auth = AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap();
        let token = auth
            .login("127.0.0.1".parse().unwrap(), "openxplay", 100)
            .ok()
            .unwrap();
        let server = AdminServer::new(
            a.clone(),
            auth,
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        let mut req = Request {
            method: "POST".into(),
            path: "/api/carlink/connect".into(),
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("x-openxplay-admin".into(), "1".into()),
            ]),
            body: br#"{"pin":"012345"}"#.to_vec(),
            peer: "127.0.0.1".parse().unwrap(),
        };
        assert_eq!(server.route(&req, 100).status, 401);
        req.headers
            .insert("cookie".into(), format!("oxp_admin={token}"));
        req.headers.remove("x-openxplay-admin");
        assert_eq!(server.route(&req, 100).status, 403);
        req.headers.insert("x-openxplay-admin".into(), "1".into());
        for (i, pin) in [
            serde_json::json!("123"),
            serde_json::json!("123456789"),
            serde_json::json!("12a4"),
            serde_json::json!("１２３４"),
            serde_json::json!("1234\n"),
            serde_json::json!(1234),
            serde_json::Value::Null,
        ]
        .into_iter()
        .enumerate()
        {
            req.body = serde_json::to_vec(&serde_json::json!({"pin":pin})).unwrap();
            assert_eq!(server.route(&req, 100 + i as u64 * 2).status, 422);
        }
        req.body = br#"{"pin":"012345"}"#.to_vec();
        assert_eq!(server.route(&req, 112).status, 429);
        assert_eq!(server.route(&req, 114).status, 200);
        assert!(!carlink_pin_path(&dir).exists());
        assert!(a.armed_pin.lock().unwrap().is_none());
        a.response_failed();
        assert!(a.pending.lock().unwrap().is_empty());
        assert_eq!(server.route(&req, 116).status, 200);
        a.response_sent();
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        let op = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        if let Operation::ConnectCarlink(Some(pin)) = &op {
            assert_eq!(pin.pin, "012345");
            assert!(
                pin.expires.duration_since(std::time::Instant::now()) <= Duration::from_secs(110)
            );
        } else {
            panic!("missing armed PIN");
        }
        a.operation(op).unwrap();
        assert!(!carlink_pin_path(&dir).exists());
        assert!(!std::fs::read_to_string(dir.join("settings.json"))
            .unwrap_or_default()
            .contains("012345"));
        req.body = b"{}".to_vec();
        assert_eq!(server.route(&req, 118).status, 409);
        std::fs::create_dir_all(dir.join("carlink")).unwrap();
        std::fs::write(
            dir.join("carlink/trust.json"),
            serde_json::to_vec(
                &serde_json::json!({"peers":{"[\"ble\",\"unit\"]":format!("{}==", "A".repeat(86))}}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(server.route(&req, 120).status, 200);
        a.response_failed();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn prearmed_pin_is_single_use_stdin_only_and_confirmation_never_auto_approves() {
        let (a, dir) = fake();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let copy = logs.clone();
        let controls = Arc::new(Mutex::new(Vec::new()));
        let sent = controls.clone();
        let pipe = ControlPipe::fake(move |v| {
            sent.lock().unwrap().push(v.clone());
            Ok(())
        });
        let sink = carlink_event_sink(
            a.carlink.clone(),
            a.armed_pin.clone(),
            dir.clone(),
            Arc::new(JsonEvents::new(Arc::new(move |line| {
                copy.lock().unwrap().push(line)
            }))),
            Arc::new(Mutex::new(Some(pipe))),
        );
        *a.armed_pin.lock().unwrap() = Some(ArmedPin {
            pin: "098765".into(),
            expires: std::time::Instant::now() + Duration::from_secs(110),
        });
        sink(
            serde_json::json!({"type":"pin-request","request":17,"client":"unit","confirmation":true}),
        );
        assert!(controls.lock().unwrap().is_empty());
        assert!(a.carlink.lock().unwrap().pin_confirmation);
        sink(serde_json::json!({"type":"pin-request","request":18,"client":"unit"}));
        assert_eq!(
            controls.lock().unwrap()[0],
            serde_json::json!({"type":"clink-pairing","request":18,"action":"pin","pin":"098765"})
        );
        assert!(a.armed_pin.lock().unwrap().is_none());
        assert!(!a.carlink.lock().unwrap().pin_requested);
        sink(serde_json::json!({"type":"pin-request","request":19,"client":"unit"}));
        assert!(a.carlink.lock().unwrap().pin_requested);
        assert!(!carlink_pin_path(&dir).exists());
        assert!(!logs.lock().unwrap().join(" ").contains("098765"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn expired_and_completed_attempts_clear_prearmed_pin() {
        let (a, dir) = fake();
        let mut status = AdminCarlink::default();
        let mut armed = Some(ArmedPin {
            pin: "1234".into(),
            expires: std::time::Instant::now(),
        });
        apply_carlink_event(
            &mut status,
            &mut armed,
            &dir,
            &serde_json::json!({"type":"pin-request"}),
        );
        assert!(armed.is_none());
        assert!(status.pin_requested);
        assert!(!carlink_pin_path(&dir).exists());
        for event in [
            serde_json::json!({"type":"link","state":"lost","reason":"failed"}),
            serde_json::json!({"type":"state","state":"Playing"}),
            serde_json::json!({"type":"error","message":"failed"}),
        ] {
            armed = Some(ArmedPin {
                pin: "1234".into(),
                expires: std::time::Instant::now() + Duration::from_secs(110),
            });
            apply_carlink_event(&mut status, &mut armed, &dir, &event);
            assert!(armed.is_none());
        }
        status.apply_event(&serde_json::json!({"type":"carlink-unpaired"}), 200);
        assert_eq!(status.state, "unpaired");
        assert_eq!(status.detail, "carlink.unpairedHint");
        assert_eq!(status.last_error.as_deref(), Some("failed"));
        assert!(!status.pin_requested);
        let expired = Operation::ConnectCarlink(Some(ArmedPin {
            pin: "1234".into(),
            expires: std::time::Instant::now(),
        }));
        assert!(a.operation(expired).is_err());
        assert!(a.armed_pin.lock().unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn carlink_schema_and_empty_address_update() {
        let schema = settings_schema();
        let fields: Vec<_> = schema.iter().flat_map(|s| &s.fields).collect();
        for key in [
            "output",
            "carlinkLink",
            "carlinkAddress",
            "carlinkHeadUnit",
            "carlinkLocal",
        ] {
            assert!(fields.iter().any(|f| f.key == key && f.hint_key.is_some()));
        }
        for key in ["carlinkHeadUnit", "carlinkLocal"] {
            let condition = fields
                .iter()
                .find(|f| f.key == key)
                .unwrap()
                .visible_when
                .as_ref()
                .unwrap();
            assert_eq!(condition.key, "carlinkLink");
            assert_eq!(condition.equals, vec![serde_json::json!("static")]);
        }
        let current = Settings {
            carlink_address: Some("AA:BB:CC:DD:EE:FF".into()),
            ..Default::default()
        };
        let next = update_settings(
            &current,
            serde_json::json!({"carlinkAddress":""})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert!(next.carlink_address.is_none());
    }
    #[test]
    fn carlink_pin_endpoint_checks_auth_csrf_request_format_rate_and_atomic_mode() {
        let (a, dir) = fake();
        let controls = capture_controls(&a);
        let mut auth = AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap();
        let token = auth
            .login("127.0.0.1".parse().unwrap(), "openxplay", 100)
            .ok()
            .unwrap();
        let server = AdminServer::new(
            a.clone(),
            auth,
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        let mut request = Request {
            method: "POST".into(),
            path: "/api/carlink/pin".into(),
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("x-openxplay-admin".into(), "1".into()),
            ]),
            body: br#"{"pin":"1234"}"#.to_vec(),
            peer: "127.0.0.1".parse().unwrap(),
        };
        assert_eq!(server.route(&request, 100).status, 401);
        request
            .headers
            .insert("cookie".into(), format!("oxp_admin={token}"));
        request.headers.remove("x-openxplay-admin");
        assert_eq!(server.route(&request, 100).status, 403);
        request
            .headers
            .insert("x-openxplay-admin".into(), "1".into());
        assert_eq!(server.route(&request, 100).status, 409);
        assert!(!carlink_pin_path(&dir).exists());
        pin_request(&a);
        for (i, pin) in ["123", "123456789", "12a4", "１２３４", "1234\n"]
            .iter()
            .enumerate()
        {
            request.body = serde_json::to_vec(&serde_json::json!({"pin":pin})).unwrap();
            assert_eq!(server.route(&request, 102 + i as u64 * 2).status, 422);
        }
        request.body = br#"{"pin":"012345"}"#.to_vec();
        assert_eq!(server.route(&request, 110).status, 429);
        assert_eq!(server.route(&request, 112).status, 200);
        assert_eq!(
            controls.lock().unwrap()[0],
            serde_json::json!({"type":"clink-pairing","request":17,"action":"pin","pin":"012345"})
        );
        assert!(!carlink_pin_path(&dir).exists());
        assert!(!a.carlink.lock().unwrap().pin_requested);
        assert_eq!(server.route(&request, 114).status, 409);
        pin_request(&a);
        a.carlink.lock().unwrap().pin_expiry = Some(1);
        assert_eq!(server.route(&request, 116).status, 409);
        pin_request(&a);
        a.busy.store(true, Ordering::Release);
        assert_eq!(server.route(&request, 118).status, 409);
        drop(server);
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn forget_carlink_is_deferred_and_clears_trust_and_pin_but_keeps_identity() {
        let (a, dir) = fake();
        let identity = a.state.identity().unwrap().pairing_id.clone();
        let trust = dir.join("carlink/trust/nested");
        std::fs::create_dir_all(&trust).unwrap();
        std::fs::write(trust.join("unit"), "trusted").unwrap();
        crate::platform::private_state_file(&carlink_pin_path(&dir), b"1234").unwrap();
        a.settings.lock().unwrap().carlink_address = Some("AA:BB:CC:DD:EE:FF".into());
        pin_request(&a);
        a.action("forget-carlink").unwrap();
        assert!(trust.exists());
        a.response_sent();
        let op = a
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .unwrap();
        assert!(matches!(op, Operation::ForgetCarlink));
        a.operation(op).unwrap();
        assert_eq!(std::fs::read_dir(dir.join("carlink")).unwrap().count(), 0);
        assert!(Settings::load(&dir).unwrap().carlink_address.is_none());
        assert!(!a.carlink.lock().unwrap().pin_requested);
        assert_eq!(a.state.identity().unwrap().pairing_id, identity);
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn trust_cleanup_keeps_pin_path_separate() {
        let (a, dir) = fake();
        crate::platform::private_state_file(&carlink_pin_path(&dir), b"1234").unwrap();
        std::fs::write(dir.join("carlink/trust.json"), "trust").unwrap();
        clear_carlink_trust(&dir).unwrap();
        assert!(carlink_pin_path(&dir).exists());
        assert!(!dir.join("carlink/trust.json").exists());
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn wifi_join_waits_for_response_flush() {
        let (a, dir) = fake();
        a.wifi_join("Car", Some("password")).unwrap();
        assert!(a
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .is_err());
        a.response_sent();
        let op = a
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .unwrap();
        assert!(matches!(op,Operation::Join(ssid,password)if ssid=="Car"&&password=="password"));
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_response_discards_deferred_join() {
        let (a, dir) = fake();
        a.wifi_join("Car", Some("password")).unwrap();
        a.response_failed();
        a.response_sent();
        assert!(a
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .is_err());
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unrelated_response_cannot_release_join() {
        let (a, dir) = fake();
        a.wifi_join("Car", Some("password")).unwrap();
        let other = a.clone();
        std::thread::spawn(move || other.response_sent())
            .join()
            .unwrap();
        assert!(a
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .is_err());
        a.response_sent();
        assert!(matches!(
            a.operation_rx
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .try_recv()
                .unwrap(),
            Operation::Join(..)
        ));
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn actions_are_queued_and_unknown_actions_rejected() {
        let (a, dir) = fake();
        for (name, expected) in [
            ("start", 0),
            ("stop", 1),
            ("restart", 2),
            ("forget-wifi", 3),
        ] {
            a.action(name).unwrap();
            a.response_sent();
            let op = a
                .operation_rx
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .try_recv()
                .unwrap();
            let actual = match op {
                Operation::Start => 0,
                Operation::Stop => 1,
                Operation::Restart => 2,
                Operation::Forget => 3,
                _ => 4,
            };
            assert_eq!(actual, expected);
        }
        assert!(a.action("unknown").is_err());
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    /// `btmgmt` that records each Secure Connections switch and always succeeds.
    #[derive(Default)]
    struct Btmgmt(Mutex<Vec<String>>);
    impl CommandRunner for Btmgmt {
        fn run(&self, args: &[String], _: Duration) -> Result<CommandResult> {
            let on = args[2] == "exec btmgmt --index 0 sc on <&1";
            assert!(on || args[2] == "exec btmgmt --index 0 sc off <&1");
            self.0
                .lock()
                .unwrap()
                .push(if on { "on" } else { "off" }.into());
            Ok(CommandResult {
                exit_code: 0,
                output: format!(
                    "hci0 Set Secure Connections complete, settings: powered ssp br/edr le{}\n",
                    if on { " secure-conn" } else { "" }
                ),
            })
        }
    }
    struct Discoverable;
    struct Hidden;
    impl crate::linux_bluetooth::PairingBackend for Discoverable {
        fn open(
            &self,
            _: u32,
            _: std::time::Instant,
        ) -> Result<Box<dyn crate::linux_bluetooth::PairingSession>> {
            Ok(Box::new(Hidden))
        }
    }
    impl crate::linux_bluetooth::PairingSession for Hidden {
        fn close(self: Box<Self>) {}
    }
    #[test]
    fn admin_pairing_turns_secure_connections_off_and_cancel_or_stop_restore_it() {
        let (mut a, dir) = fake();
        let runner = Arc::new(Btmgmt::default());
        let lines = Arc::new(Mutex::new(Vec::new()));
        let copy = lines.clone();
        {
            let admin = Arc::get_mut(&mut a).unwrap();
            admin.events = Arc::new(JsonEvents::new(Arc::new(move |line| {
                copy.lock().unwrap().push(line)
            })));
            admin.pairing_backend = Arc::new(Discoverable);
            admin.secure_connections = Arc::new(crate::secure_connections::SecureConnections::new(
                runner.clone(),
                admin.events.clone(),
            ));
        }
        let switches = || runner.0.lock().unwrap().clone();
        let reported = |a: &LinuxRelayAdmin| {
            serde_json::to_value(a.status()).unwrap()["bluetoothSecureConnections"].clone()
        };
        assert!(reported(&a).is_null());
        a.action("pair-iphone").unwrap();
        assert_eq!(switches(), ["off"]);
        assert_eq!(reported(&a), false);
        // A second request while the window is open changes nothing.
        a.action("pair-iphone").unwrap();
        assert_eq!(switches(), ["off"]);
        a.action("cancel-pairing").unwrap();
        assert_eq!(switches(), ["off", "on"]);
        assert_eq!(reported(&a), true);
        a.action("pair-iphone").unwrap();
        a.shutdown();
        assert_eq!(switches(), ["off", "on", "off", "on"]);
        let logs: Vec<String> = lines
            .lock()
            .unwrap()
            .iter()
            .filter_map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["message"]
                    .as_str()
                    .filter(|m| m.starts_with("bluetooth-sc "))
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(
            logs,
            [
                "bluetooth-sc off reason=pairing",
                "bluetooth-sc on reason=pairing-cancelled",
                "bluetooth-sc off reason=pairing",
                "bluetooth-sc on reason=relay-stop"
            ]
        );
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn pairing_status_is_cancelable_and_closing_without_a_window_is_harmless() {
        let (a, dir) = fake();
        assert_eq!(PAIRING_SECONDS, 600);
        let json = serde_json::to_value(a.status()).unwrap();
        assert_eq!(json["pairingAvailable"], true);
        assert_eq!(json["pairingCancelable"], true);
        assert!(json["pairingDeadline"].is_null() && json["pairingRemaining"].is_null());
        assert!(json["pairedDevices"].is_array());
        assert!(json.get("receivers").is_none());
        assert!(json["video"]["codec"].is_string());
        a.action("cancel-pairing").unwrap();
        assert!(a.pairing.lock().unwrap().is_none());
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn pairing_window_runs_on_the_monotonic_clock_and_shows_a_wall_clock_deadline() {
        let (mut a, dir) = fake();
        {
            let admin = Arc::get_mut(&mut a).unwrap();
            admin.pairing_backend = Arc::new(Discoverable);
            admin.secure_connections = Arc::new(crate::secure_connections::SecureConnections::new(
                Arc::new(Btmgmt::default()),
                admin.events.clone(),
            ));
        }
        a.action("pair-iphone").unwrap();
        let json = serde_json::to_value(a.status()).unwrap();
        let remaining = json["pairingRemaining"].as_u64().unwrap();
        assert!((u64::from(PAIRING_SECONDS) - 1..=u64::from(PAIRING_SECONDS)).contains(&remaining));
        // The display deadline is the same moment on the wall clock, however wrong that clock is.
        let wall = crate::platform::wall_clock().as_secs();
        let deadline = json["pairingDeadline"].as_u64().unwrap();
        assert!(deadline.abs_diff(wall + remaining) <= 1);
        // The window closes when the monotonic clock reaches its end, and a new one may open.
        a.pairing.lock().unwrap().as_mut().unwrap().1 = crate::admin::auth::monotonic_seconds();
        let json = serde_json::to_value(a.status()).unwrap();
        assert!(json["pairingDeadline"].is_null() && json["pairingRemaining"].is_null());
        a.action("pair-iphone").unwrap();
        assert!(serde_json::to_value(a.status()).unwrap()["pairingRemaining"].is_u64());
        a.action("cancel-pairing").unwrap();
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bluetooth_guard_file_is_optional_and_configurable() {
        let (a, dir) = fake();
        // Release the state directory lock before a second shell opens it.
        drop(a);
        let path = dir.join("bluetooth-guard.json");
        let options = Options::parse(&[
            "--state-dir".into(),
            dir.to_str().unwrap().into(),
            "--bluetooth-guard-status".into(),
            path.to_str().unwrap().into(),
        ])
        .unwrap();
        assert_eq!(
            Options::parse(&["--state-dir".into(), dir.to_str().unwrap().into()])
                .unwrap()
                .bluetooth_guard_status,
            std::path::PathBuf::from(crate::bluetooth_guard::DEFAULT_STATUS_PATH)
        );
        let state = Arc::new(PersistentState::open(&dir).unwrap());
        let events = Arc::new(JsonEvents::new(Arc::new(|_| {})));
        let provision = WifiProvisioner::new(
            ProcessNetworkManagerCli {
                executable: "/bin/false".into(),
            },
            "wlan0".into(),
            "AA:BB:CC:DD:EE:FF",
            &dir,
        )
        .unwrap();
        let guarded = LinuxRelayAdmin::with_provisioner(
            options,
            state,
            events,
            provision,
            runtime_paths(&dir),
        )
        .unwrap();
        assert!(serde_json::to_value(guarded.status())
            .unwrap()
            .get("bluetoothGuard")
            .is_none());
        std::fs::write(
            &path,
            br#"{"state":"failed","attempts":3,"lastRecovery":"2026-10-16T08:00:00Z","lastError":"reset failed","address":null}"#,
        )
        .unwrap();
        let json = serde_json::to_value(guarded.status()).unwrap();
        assert_eq!(
            json["bluetoothGuard"],
            serde_json::json!({"state":"failed","attempts":3,"lastRecovery":"2026-10-16T08:00:00Z","lastError":"reset failed","address":null})
        );
        drop(guarded);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn receiver_events_are_kept_bounded_and_stats_stay_out_of_the_log() {
        let (a, dir) = fake();
        let info = serde_json::json!({"type":"receiver-info","client":"4","info":{"refreshHz":60}});
        assert!(!receiver_event(&a.receivers, &info));
        for fps in 0..40 {
            assert!(receiver_event(
                &a.receivers,
                &serde_json::json!({"type":"stats","client":"4","renderedFps":fps})
            ));
        }
        assert!(!receiver_event(
            &a.receivers,
            &serde_json::json!({"type":"link","state":"joined"})
        ));
        let receivers = a.receivers();
        assert_eq!(receivers.len(), 1);
        assert_eq!(receivers[0]["info"]["refreshHz"], 60);
        assert_eq!(
            receivers[0]["stats"].as_array().unwrap().len(),
            crate::receiver_diagnostics::MAX_SAMPLES
        );
        assert!(!receiver_event(
            &a.receivers,
            &serde_json::json!({"type":"receiver-stopped"})
        ));
        assert!(a.receivers().is_empty());
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn relay_admin_settings_mapping_applies_network_now() {
        let (a, dir) = fake();
        assert!(matches!(a.update(serde_json::json!({"networkMode":"shared-wifi","sharedWifiNetworks":[{"ssid":"Car","password":"password"}],"iphone":"auto","hotspotChannel":6,"output":"both"}).as_object().unwrap().clone()),AdminUpdate::NextStart));
        let settings = a.settings();
        assert_eq!(settings.values["networkMode"], "shared-wifi");
        assert_eq!(settings.values["iphone"], "auto");
        assert_eq!(Settings::load(&dir).unwrap().hotspot_channel, 6);
        drop(a);
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn trust_carlink(dir: &std::path::Path) {
        std::fs::create_dir_all(dir.join("carlink")).unwrap();
        std::fs::write(
            dir.join("carlink/trust.json"),
            serde_json::to_vec(
                &serde_json::json!({"peers":{"[\"ble\",\"unit\"]":format!("{}==", "A".repeat(86))}}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    type Spawned = (Vec<std::ffi::OsString>, EventSink);
    /// Hands each child's arguments and event sink to the test.
    struct LinkFake(Mutex<std::sync::mpsc::Sender<Spawned>>);
    impl ChildFactory for LinkFake {
        fn spawn(
            &self,
            _: &std::path::Path,
            args: &[std::ffi::OsString],
            events: EventSink,
        ) -> std::io::Result<Box<dyn RelayChild>> {
            let _ = self.0.lock().unwrap().send((args.to_vec(), events.clone()));
            let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
            let tx = self.0.lock().unwrap().clone();
            let pipe = ControlPipe::fake(move |v| {
                let tx = tx.clone();
                let events = events.clone();
                let value = v.clone();
                let generation = generation.clone();
                std::thread::spawn(move || {
                    let next = generation.fetch_add(1, Ordering::AcqRel) + 1;
                    let _ = tx.send((
                        vec![serde_json::to_string(&value).unwrap().into()],
                        events.clone(),
                    ));
                    events(
                        serde_json::json!({"type":"clink-restart","request":value["request"],"generation":next,"result":"started"}),
                    );
                });
                Ok(())
            });
            struct ControlledChild(ControlPipe);
            impl RelayChild for ControlledChild {
                fn control(&self) -> Option<ControlPipe> {
                    Some(self.0.clone())
                }
                fn exited(&mut self) -> std::io::Result<bool> {
                    Ok(false)
                }
                fn stop(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            Ok(Box::new(ControlledChild(pipe)))
        }
    }
    fn link_children(a: &mut Arc<LinuxRelayAdmin>) -> std::sync::mpsc::Receiver<Spawned> {
        let (tx, rx) = std::sync::mpsc::channel();
        Arc::get_mut(a).unwrap().child_factory = Arc::new(LinkFake(Mutex::new(tx)));
        rx
    }
    fn role_inventory() -> crate::net_inventory::Inventory {
        crate::net_inventory::Inventory {
            probed_at: Some(1),
            interfaces: [
                (
                    "wlan0",
                    "phy0",
                    include_str!("../tests/fixtures/iw-phy-n1-cn.txt"),
                ),
                (
                    "wlan1",
                    "phy1",
                    include_str!("../tests/fixtures/iw-phy-rtl8822bu-cn.txt"),
                ),
            ]
            .into_iter()
            .map(|(name, phy, capture)| crate::net_inventory::Interface {
                name: name.into(),
                phy: Some(phy.into()),
                kind: "wifi".into(),
                wifi: Some(crate::net_inventory::Radio::from_phy(
                    crate::wifi_capabilities::parse_phy(capture),
                )),
                ..Default::default()
            })
            .collect(),
            ..Default::default()
        }
    }
    fn add_second_radio(a: &LinuxRelayAdmin) {
        std::fs::create_dir_all(a.net_root.join("wlan1")).unwrap();
        std::fs::write(a.net_root.join("wlan1/address"), "02:00:00:00:00:02").unwrap();
    }
    fn await_startup(a: &LinuxRelayAdmin) {
        a.start_policy.lock().unwrap().skip();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while a.busy.load(Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn admin_operations_fit_the_device_worker_stack() {
        let (mut admin, dir) = fake();
        let _children = link_children(&mut admin);
        admin.desired.store(false, Ordering::Release);
        admin.settings.lock().unwrap().output = "both".into();
        add_second_radio(&admin);
        trust_carlink(&dir);
        admin.inventory.set(role_inventory());
        let worker = admin.clone();
        std::thread::Builder::new()
            .name("admin-operations-small-stack".into())
            // Leave headroom below the device worker's 256 KiB for caller and platform frames.
            .stack_size(192 * 1024)
            .spawn(move || {
                // Roles chains output and network changes; startup and restart initialize Wi-Fi.
                for operation in [
                    Operation::Startup,
                    Operation::Roles(vec!["carlink".into(), "controller".into()]),
                    Operation::Restart,
                    Operation::Start,
                ] {
                    worker.operation(operation).unwrap();
                }
                assert!(worker.supervisor.lock().unwrap().is_some());
                assert!(worker.desired.load(Ordering::Acquire));
            })
            .unwrap()
            .join()
            .unwrap();
        admin.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unprobed_options_keep_legacy_inputs_and_tick_does_not_record_roles() {
        let (a, dir) = fake();
        a.desired.store(false, Ordering::Release);
        let before = a.effective_options().unwrap();
        a.settings.lock().unwrap().network_roles = serde_json::from_value(serde_json::json!({
            "mode":"custom", "iphone":{"link":"saved-network","interface":"wlan9"},
            "carBrowser":{"network":"interface","interface":"eth9"}, "carlink":{"interface":"wlan8"},
            "adminHotspot":{"enabled":false,"interface":null,"ssid":"Test Admin","band":"2.4","channel":0}
        })).unwrap();
        let options = a.effective_options().unwrap();
        assert_eq!(options.wifi_interface, before.wifi_interface);
        assert_eq!(options.settings.network_mode, before.settings.network_mode);
        assert_eq!(
            options.settings.carlink_interface,
            before.settings.carlink_interface
        );
        assert_eq!(a.browser_target.lock().unwrap().as_deref(), Some("wlan0"));
        for _ in 0..3 {
            a.tick().unwrap();
        }
        assert!(a.running_links.lock().unwrap().is_none());
        assert!(a
            .operation_rx
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_recv()
            .is_err());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn startup_probe_before_timeout_starts_chosen_outputs_without_a_role_restart() {
        let (mut a, dir) = fake();
        let children = link_children(&mut a);
        a.settings.lock().unwrap().output = "both".into();
        add_second_radio(&a);
        trust_carlink(&dir);
        let starting = a.clone();
        let startup = std::thread::spawn(move || {
            starting
                .start_services_with_probe_timeout(Duration::from_secs(2))
                .unwrap();
        });
        assert!(children.recv_timeout(Duration::from_millis(50)).is_err());
        assert!(a.controller.lock().unwrap().is_none());
        assert!(a.running_links.lock().unwrap().is_none());
        a.inventory.set(role_inventory());
        startup.join().unwrap();
        await_startup(&a);
        let (args, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(args
            .windows(2)
            .any(|a| a[0] == "--clink-interface" && a[1] == "wlan1"));
        let controller = a
            .controller
            .lock()
            .unwrap()
            .clone()
            .expect("controller did not start");
        for _ in 0..3 {
            a.tick().unwrap();
        }
        assert!(Arc::ptr_eq(
            a.controller.lock().unwrap().as_ref().unwrap(),
            &controller
        ));
        assert!(children.recv_timeout(Duration::from_millis(50)).is_err());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn startup_probe_after_timeout_applies_only_the_needed_change_once() {
        for second_radio in [false, true] {
            let (mut a, dir) = fake();
            let children = link_children(&mut a);
            a.settings.lock().unwrap().output = "both".into();
            if second_radio {
                add_second_radio(&a);
            }
            trust_carlink(&dir);
            let started = std::time::Instant::now();
            a.start_services_with_probe_timeout(Duration::from_millis(20))
                .unwrap();
            assert!(started.elapsed() >= Duration::from_millis(20));
            await_startup(&a);
            let (args, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(args
                .windows(2)
                .any(|a| a[0] == "--clink-interface" && a[1] == "wlan0"));
            let controller = a
                .controller
                .lock()
                .unwrap()
                .clone()
                .expect("legacy controller did not start");
            for _ in 0..3 {
                a.tick().unwrap();
            }
            assert!(children.try_recv().is_err());
            let mut inv = role_inventory();
            if !second_radio {
                inv.interfaces.pop();
            }
            a.inventory.set(inv);
            let (options, status) = a.effective_options_with_status().unwrap();
            let changes = a
                .running_links
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .changes(&a.links(&options, &status));
            assert_eq!(
                changes,
                if second_radio {
                    vec!["carlink"]
                } else {
                    vec![]
                }
            );
            for _ in 0..3 {
                a.tick().unwrap();
            }
            if second_radio {
                let (args, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(args
                    .windows(2)
                    .any(|a| a[0] == "--clink-interface" && a[1] == "wlan1"));
            }
            assert!(children.recv_timeout(Duration::from_millis(50)).is_err());
            assert!(Arc::ptr_eq(
                a.controller.lock().unwrap().as_ref().unwrap(),
                &controller
            ));
            a.shutdown();
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn saving_identical_roles_recomputes_automatic_choices_but_preview_does_not_apply() {
        let (a, dir) = fake();
        let mut inv = role_inventory();
        inv.interfaces[1].activity.kind = "other-ap".into();
        a.inventory.set(inv.clone());
        let current = a.network_status(&a.settings.lock().unwrap());
        assert_eq!(current.plan.carlink.role.name(), Some("wlan0"));
        inv.interfaces[1].activity.kind = "idle".into();
        a.inventory.set(inv.clone());
        let values = serde_json::json!({"networkRoles":{"mode":"auto"}})
            .as_object()
            .unwrap()
            .clone();
        let crate::network_roles::SaveResult::Saved(preview) =
            a.propose_roles(values.clone(), false)
        else {
            panic!("preview failed");
        };
        assert_eq!(preview.changes, ["carlink"]);
        assert_eq!(preview.plan.carlink.role.name(), Some("wlan1"));
        assert_eq!(
            a.network_status(&a.settings.lock().unwrap())
                .plan
                .carlink
                .role
                .name(),
            Some("wlan0")
        );
        let crate::network_roles::SaveResult::Saved(saved) = a.propose_roles(values, true) else {
            panic!("save failed");
        };
        assert_eq!(saved.changes, ["carlink"]);
        assert_eq!(
            a.network_status(&a.settings.lock().unwrap())
                .plan
                .carlink
                .role
                .name(),
            Some("wlan1")
        );
        inv.interfaces[1].activity.kind = "other-network".into();
        a.inventory.set(inv);
        assert_eq!(
            a.network_status(&a.settings.lock().unwrap())
                .plan
                .carlink
                .role
                .name(),
            Some("wlan1")
        );
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn video_codec_change_rebuilds_output_and_re_evaluates_hevc() {
        let (mut a, dir) = fake();
        let children = link_children(&mut a);
        a.settings.lock().unwrap().output = "carlink".into();
        a.desired.store(false, Ordering::Release);
        trust_carlink(&dir);
        a.start_supervisor().unwrap();
        let (args, sink) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        let has_hevc = |args: &[std::ffi::OsString], value: &str| {
            args.windows(2)
                .any(|a| a[0] == "--clink-hevc" && a[1] == value)
        };
        assert!(has_hevc(&args, "auto"));
        sink(
            serde_json::json!({"type":"diagnostic","message":"codec mismatch expected=H265 actual=Some(H264)"}),
        );
        let (control, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        let control: serde_json::Value =
            serde_json::from_str(&control[0].to_string_lossy()).unwrap();
        assert_eq!(control["type"], "clink-restart");
        assert_eq!(control["hevc"], "off");
        a.supervisor.lock().unwrap().take().unwrap().stop();
        a.start_supervisor().unwrap();
        let (args, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(has_hevc(&args, "off"));
        let rx = a.operation_rx.lock().unwrap().take().unwrap();
        for (codec, expected) in [("h264", "off"), ("auto", "auto")] {
            assert!(matches!(
                a.update(
                    serde_json::json!({"videoCodec":codec})
                        .as_object()
                        .unwrap()
                        .clone()
                ),
                AdminUpdate::AppliedNow
            ));
            a.response_sent();
            let op = rx.try_recv().unwrap();
            assert!(matches!(op, Operation::Outputs(None)));
            a.operation(op).unwrap();
            let (args, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(has_hevc(&args, expected));
        }
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn window_and_view_events_replace_only_carlink_once_and_blacklist_unchanged_window() {
        let (mut a, dir) = fake();
        let children = link_children(&mut a);
        {
            let mut s = a.settings.lock().unwrap();
            s.output = "both".into();
            s.carlink_reconnect_on_resize = true;
        }
        a.desired.store(false, Ordering::Release);
        trust_carlink(&dir);
        a.start_supervisor().unwrap();
        let (_, first) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut capabilities: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../OpenXPlay-Stream/spec/vectors/wire/capabilities.json"
        ))
        .unwrap();
        capabilities["displays"]["0"]["widthPx"] = 1814.into();
        capabilities["displays"]["0"]["heightPx"] = 1200.into();
        capabilities["displays"]["0"]["viewport"] =
            serde_json::json!({"widthPx":1532,"heightPx":1013});
        a.input
            .control(&oxp_wire::Message::Json(capabilities.clone()))
            .unwrap();
        first(serde_json::json!({"type":"clink-view-size","width":1814,"height":1200}));
        first(serde_json::json!({"type":"state","state":"Playing"}));
        a.tick().unwrap();
        let (args, second) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        let command: serde_json::Value = serde_json::from_str(&args[0].to_string_lossy()).unwrap();
        assert_eq!(command["type"], "clink-restart");
        a.input
            .control(&oxp_wire::Message::Json(capabilities))
            .unwrap();
        second(serde_json::json!({"type":"clink-view-size","width":1814,"height":1200}));
        second(serde_json::json!({"type":"state","state":"Playing"}));
        a.tick().unwrap();
        assert!(children.recv_timeout(Duration::from_millis(150)).is_err());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn role_changes_restart_only_affected_links_and_keep_a_separate_radio_ap() {
        let (mut a, dir) = fake();
        let children = link_children(&mut a);
        a.desired.store(false, Ordering::Release);
        a.settings.lock().unwrap().output = "both".into();
        std::fs::create_dir_all(a.net_root.join("wlan1")).unwrap();
        std::fs::write(a.net_root.join("wlan1/address"), "02:00:00:00:00:02").unwrap();
        a.inventory.set(crate::net_inventory::Inventory {
            probed_at: Some(1),
            interfaces: [
                ("wlan0", "wifi", "phy0"),
                ("wlan1", "wifi", "phy1"),
                ("eth0", "ethernet", ""),
            ]
            .into_iter()
            .map(|(name, kind, phy)| crate::net_inventory::Interface {
                name: name.into(),
                mac: Some(
                    if name == "wlan1" {
                        "02:00:00:00:00:02"
                    } else {
                        "02:00:00:00:00:01"
                    }
                    .into(),
                ),
                kind: kind.into(),
                phy: Some(phy.into()),
                ..Default::default()
            })
            .collect(),
            ..Default::default()
        });
        trust_carlink(&dir);
        a.start_supervisor().unwrap();
        let _ = children.recv_timeout(Duration::from_secs(2)).unwrap();
        let controller = fake_controller(&a);
        let hotspot = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
        let lease = a.hotspot_lifetime.create(|| hotspot.clone()).unwrap();
        lease.start(Duration::from_secs(1)).unwrap();
        let roles: crate::network_roles::NetworkRoles = serde_json::from_value(serde_json::json!({
            "mode":"custom", "iphone":{"link":"hotspot","interface":"wlan0"},
            "carBrowser":{"network":"interface","interface":"eth0"}, "carlink":{"interface":"wlan0"},
            "adminHotspot":{"enabled":false,"interface":null,"ssid":"Test Admin","band":"2.4","channel":0}
        })).unwrap();
        a.settings.lock().unwrap().network_roles = roles;
        a.operation(Operation::Roles(vec!["carBrowserAddress".into()]))
            .unwrap();
        assert!(Arc::ptr_eq(
            a.controller.lock().unwrap().as_ref().unwrap(),
            &controller
        ));
        assert!(children.try_recv().is_err());
        a.settings
            .lock()
            .unwrap()
            .network_roles
            .carlink
            .as_mut()
            .unwrap()
            .interface = Some("02:00:00:00:00:02".into());
        a.operation(Operation::Roles(vec!["carlink".into()]))
            .unwrap();
        let (args, sink) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(args.iter().any(|a| a == "--web-port"));
        assert!(args
            .windows(2)
            .any(|a| a[0] == "--clink-interface" && a[1] == "wlan1"));
        sink(
            serde_json::json!({"type":"link","state":"joined","network":{"ssid":"Synthetic Car","interface":"wlan1"}}),
        );
        assert_eq!(hotspot.closes.load(Ordering::SeqCst), 0);
        assert!(lease.validate_ready().is_ok());
        assert!(Arc::ptr_eq(
            a.controller.lock().unwrap().as_ref().unwrap(),
            &controller
        ));
        let mut snapshot = a.inventory.get();
        let mut radio = snapshot.interfaces.remove(1);
        a.inventory.set(snapshot.clone());
        std::fs::remove_dir_all(a.net_root.join("wlan1")).unwrap();
        a.tick().unwrap();
        let (args, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!args.iter().any(|a| a == "--clink-interface"));
        assert!(
            a.status()
                .network
                .planned
                .unwrap()
                .plan
                .carlink
                .role
                .missing
        );
        radio.name = "wlan2".into();
        snapshot.interfaces.push(radio);
        a.inventory.set(snapshot);
        std::fs::create_dir_all(a.net_root.join("wlan2")).unwrap();
        std::fs::write(a.net_root.join("wlan2/address"), "02:00:00:00:00:02").unwrap();
        a.tick().unwrap();
        let (args, _) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(args
            .windows(2)
            .any(|a| a[0] == "--clink-interface" && a[1] == "wlan2"));
        assert_eq!(hotspot.closes.load(Ordering::SeqCst), 0);
        assert!(Arc::ptr_eq(
            a.controller.lock().unwrap().as_ref().unwrap(),
            &controller
        ));
        a.settings
            .lock()
            .unwrap()
            .network_roles
            .iphone
            .as_mut()
            .unwrap()
            .link = "saved-network".into();
        // Changing the phone link closes its controller even when provisioning cannot join.
        let _ = a.operation(Operation::Roles(vec!["controller".into()]));
        assert!(a.controller.lock().unwrap().is_none());
        assert_eq!(hotspot.closes.load(Ordering::SeqCst), 1);
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn admin_restart_closes_the_hotspot_like_start() {
        let (mut a, dir) = fake();
        let _children = link_children(&mut a);
        for op in [Operation::Restart, Operation::Start] {
            // Even an AP retained between controllers for a soft restart is closed.
            let hotspot = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
            let lease = a.hotspot_lifetime.create(|| hotspot.clone()).unwrap();
            lease.start(Duration::from_secs(1)).unwrap();
            a.hotspot_lifetime.keep_for_restart();
            lease.close().unwrap();
            assert_eq!(hotspot.closes.load(Ordering::SeqCst), 0);
            a.operation(op).unwrap();
            assert_eq!(hotspot.closes.load(Ordering::SeqCst), 1);
        }
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn terminal_loss_keeps_shared_child_and_forget_clears_pairing_state() {
        let (mut a, dir) = fake();
        let children = link_children(&mut a);
        a.settings.lock().unwrap().output = "both".into();
        a.start_supervisor().unwrap();
        let (_, sink) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        sink(
            serde_json::json!({"type":"link","state":"lost","generation":1,"reason":"head_unit_band_unsupported:5180"}),
        );
        assert!(children.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(a.supervisor.lock().unwrap().as_ref().unwrap().running());
        a.operation(Operation::ForgetCarlink).unwrap();
        assert!(a.carlink.lock().unwrap().pin_request.is_none());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn only_link_departure_undoes_the_follow_over() {
        let joined = serde_json::json!({"type":"link","state":"joined","network":{"ssid":"DIRECT-Car","interface":"wlan0"}});
        for departure in [
            serde_json::json!({"type":"link","state":"lost","reason":"network_link_lost"}),
            serde_json::json!({"type":"state","state":"Ended"}),
            serde_json::json!({"type":"receiver-stopped"}),
        ] {
            let mut follow = CarlinkFollow::default();
            follow.observe(&joined, "wlan0");
            let generation = follow.generation;
            // oxp-relay reports these while the link stays on the head unit's network.
            for event in [
                serde_json::json!({"type":"error","message":"Cannot read PIN file"}),
                serde_json::json!({"type":"error","code":"WIRE","message":"Broken pipe"}),
                serde_json::json!({"type":"pin-timeout","client":"unit"}),
                serde_json::json!({"type":"receiver-error","message":"No child"}),
                serde_json::json!({"type":"state","state":"Negotiating"}),
            ] {
                follow.observe(&event, "wlan0");
            }
            assert_eq!(follow.target.as_deref(), Some("DIRECT-Car"));
            assert_eq!(follow.generation, generation);
            follow.observe(&departure, "wlan0");
            assert!(follow.target.is_none());
            assert_ne!(follow.generation, generation);
        }
    }
    struct NoMfi;
    impl carplay_iap2::mfi::MfiAuthenticator for NoMfi {
        fn protocol_major(&self) -> Result<u8> {
            Ok(3)
        }
        fn read_certificate(&self, _: usize) -> Result<Vec<u8>> {
            unreachable!()
        }
        fn sign_challenge(&self, _: &[u8]) -> Result<Vec<u8>> {
            unreachable!()
        }
        fn baa_certificates(&self) -> Result<carplay_iap2::mfi::BaaCertificatePair> {
            unreachable!()
        }
    }
    /// Brings its AP up through the relay's hotspot lifetime, then finds no iPhone: the
    /// controller retries every few milliseconds and keeps the AP for the next attempt.
    struct RetryingPlatform {
        lifetime: Arc<crate::hotspot_lifetime::HotspotLifetime>,
        aps: Mutex<Vec<Arc<crate::hotspot_lifetime::tests::Fake>>>,
    }
    impl WirelessPlatform for RetryingPlatform {
        fn check_bluetooth_radio(&self) -> Result<()> {
            Ok(())
        }
        fn prepare_mfi(&self) -> Result<MfiResource> {
            Ok(MfiResource {
                authenticator: Arc::new(NoMfi),
            })
        }
        fn create_hotspot(&self) -> Result<Arc<dyn WirelessHotspotManager>> {
            self.lifetime.create(|| {
                let ap = Arc::new(crate::hotspot_lifetime::tests::Fake::default());
                self.aps.lock().unwrap().push(ap.clone());
                ap
            })
        }
        fn keep_hotspot_for_retry(&self) {
            self.lifetime.keep_for_retry();
        }
        fn cancel_hotspot_retry(&self) -> Result<()> {
            self.lifetime.cancel_retry()
        }
        fn start_pairing(&self) -> Result<Option<Arc<dyn CloseResource>>> {
            Ok(None)
        }
        fn select_iphone(&self) -> Result<BondedDevice> {
            Err(std::io::Error::other("No iPhone"))
        }
        fn create_bluetooth(&self, _: &BondedDevice) -> Result<Arc<dyn BluetoothConnection>> {
            unreachable!()
        }
        fn create_network(
            &self,
            _: WirelessHotspotInfo,
            _: Arc<dyn carplay_iap2::mfi::MfiAuthenticator>,
            _: NetworkCallbacks,
        ) -> Result<Arc<dyn NetworkAttachment>> {
            Err(std::io::Error::other("No iPhone"))
        }
        fn run_bootstrap(
            &self,
            _: Arc<dyn BluetoothConnection>,
            _: Arc<dyn PendingNetwork>,
            _: Arc<dyn carplay_iap2::mfi::MfiAuthenticator>,
            _: BootstrapCallbacks,
        ) -> Result<()> {
            unreachable!()
        }
    }
    #[test]
    fn pending_follow_over_keeps_old_controller_retries_off_the_radio() {
        let (mut a, dir) = fake();
        let children = link_children(&mut a);
        a.start_supervisor().unwrap();
        let (_, sink) = children.recv_timeout(Duration::from_secs(2)).unwrap();
        let platform = Arc::new(RetryingPlatform {
            lifetime: a.hotspot_lifetime.clone(),
            aps: Mutex::default(),
        });
        let controller = Arc::new(
            LinuxCarPlayController::new(platform.clone(), a.events.clone(), Duration::from_secs(1))
                .with_retry_delay(Duration::from_millis(50), Duration::from_millis(100)),
        );
        *a.controller.lock().unwrap() = Some(controller.clone());
        let running = controller.clone();
        *a.worker.lock().unwrap() = Some(std::thread::spawn(move || {
            let _ = running.run();
        }));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while platform
            .aps
            .lock()
            .unwrap()
            .first()
            .is_none_or(|ap| ap.starts.load(Ordering::SeqCst) == 0)
        {
            assert!(std::time::Instant::now() < deadline, "no AP started");
            std::thread::sleep(Duration::from_millis(1));
        }
        // Retries reuse the retained AP.
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(platform.aps.lock().unwrap().len(), 1);
        let ap = platform.aps.lock().unwrap()[0].clone();
        // Carlink joins the head unit's group on wlan0, which takes the radio from the AP.
        sink(
            serde_json::json!({"type":"link","state":"joined","network":{"ssid":"DIRECT-Car","interface":"wlan0"}}),
        );
        ap.failed.store(true, Ordering::SeqCst);
        // An error that leaves the link up is shown but does not end the follow-over.
        sink(serde_json::json!({"type":"error","message":"Cannot read PIN file"}));
        assert_eq!(
            a.carlink.lock().unwrap().last_error.as_deref(),
            Some("Cannot read PIN file")
        );
        // During the debounce the old controller keeps failing but brings no new AP up.
        std::thread::sleep(Duration::from_millis(100));
        assert_ne!(controller.stage(), "Stopped");
        assert_eq!(platform.aps.lock().unwrap().len(), 1);
        assert_eq!(ap.starts.load(Ordering::SeqCst), 1);
        assert_eq!(ap.closes.load(Ordering::SeqCst), 0);
        // The follow-over stops it; here the network credentials cannot be read.
        a.follow.lock().unwrap().changed = Some(std::time::Instant::now() - Duration::from_secs(1));
        assert!(a.reconcile_carlink_network().is_err());
        assert_eq!(controller.stage(), "Stopped");
        assert!(a.worker.lock().unwrap().is_none());
        assert_eq!(platform.aps.lock().unwrap().len(), 1);
        // Once the link has left, the next controller may bring its AP up again.
        sink(serde_json::json!({"type":"receiver-stopped"}));
        settle_follow(&a);
        assert!(a
            .hotspot_lifetime
            .create(|| Arc::new(crate::hotspot_lifetime::tests::Fake::default()))
            .is_ok());
        a.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn trusted_https_is_off_without_public_name_json_and_changes_nothing() {
        let (a, dir) = fake();
        let status = a.status();
        assert_eq!(status.public_name.as_ref().unwrap().state, "off");
        assert!(a.public_name.served().is_none());
        assert!(status.receiver_urls.iter().all(|u| !u.contains("duckdns")));
        // No file, no directory, no SNI certificate, nothing for the head-unit page.
        a.apply_public_name();
        assert!(a.admin_certificate().name().is_none());
        assert!(!dir.join("public-name.json").exists());
        assert!(!dir.join("public-name").exists());
        assert!(a.receiver_tls.lock().unwrap().is_none());
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["publicName"], serde_json::json!({"state":"off"}));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn public_name_settings_go_to_their_own_file_and_hide_the_token() {
        let (a, dir) = fake();
        let section = settings_schema()
            .into_iter()
            .find(|s| s.id == "publicName")
            .unwrap();
        let keys: Vec<_> = section.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "publicNameEnabled",
                "publicNameProvider",
                "publicName",
                "publicNameToken",
                "publicNameAgreeTerms"
            ]
        );
        let provider = section
            .fields
            .iter()
            .find(|f| f.key == "publicNameProvider")
            .unwrap();
        let choices: Vec<_> = provider
            .options
            .as_ref()
            .unwrap()
            .iter()
            .map(|o| o.value.clone())
            .collect();
        assert_eq!(
            choices,
            [serde_json::json!("duckdns"), serde_json::json!("dynv6")]
        );
        assert_eq!(
            section
                .fields
                .iter()
                .find(|f| f.key == "publicNameToken")
                .unwrap()
                .field_type,
            "secret"
        );
        let values = a.settings().values;
        assert_eq!(values["publicNameEnabled"], false);
        assert_eq!(values["publicNameTokenSet"], false);
        let token = "a1b2c3d4-0000-0000-0000-000000000000";
        let update = serde_json::json!({"publicNameEnabled":true,"publicNameProvider":"duckdns",
            "publicName":"MyRelay.duckdns.org","publicNameToken":token,"output":"web"});
        assert!(matches!(
            a.update(update.as_object().unwrap().clone()),
            AdminUpdate::AppliedNow
        ));
        let settings = serde_json::to_value(a.settings()).unwrap();
        assert!(!settings.to_string().contains(token));
        assert_eq!(settings["values"]["publicName"], "myrelay.duckdns.org");
        assert_eq!(settings["values"]["publicNameTokenSet"], true);
        // settings.json, which must survive a rollback, never sees these keys.
        let stored: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.join("settings.json")).unwrap_or_else(|_| b"{}".to_vec()),
        )
        .unwrap();
        assert!(stored
            .as_object()
            .unwrap()
            .keys()
            .all(|k| !k.starts_with("publicName")));
        Settings::load(&dir).unwrap();
        assert_eq!(a.status().public_name.unwrap().state, "incomplete");
        match a.update(
            serde_json::json!({"publicName":"bad_name","publicNameToken":""})
                .as_object()
                .unwrap()
                .clone(),
        ) {
            AdminUpdate::Errors(errors) => {
                assert_eq!(errors["publicName"], "publicName.nameInvalid")
            }
            _ => panic!("invalid name accepted"),
        }
        assert!(a.action("public-name-renew").is_ok());
        assert!(a.action("public-name-remove").is_ok());
        assert!(!dir.join("public-name.json").exists());
        assert!(!dir.join("public-name/providers.json").exists());
        assert_eq!(a.status().public_name.unwrap().state, "off");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn the_public_name_comes_first_only_while_trusted() {
        let ready = crate::public_name::PublicNameStatus {
            state: "ready".into(),
            name: Some("myrelay.duckdns.org".into()),
            trusted_name: Some("myrelay.duckdns.org".into()),
            ..Default::default()
        };
        let urls = vec!["https://openxplay-linux.local:8443/#pair=1".to_string()];
        assert_eq!(
            public_name_url(&ready, &urls, 8443).as_deref(),
            Some("https://myrelay.duckdns.org:8443/#pair=1")
        );
        assert_eq!(public_name_url(&ready, &[], 8443), None, "no web output");
        // A failing renewal does not hide a name that still works.
        let failing = crate::public_name::PublicNameStatus {
            state: "error".into(),
            ..ready.clone()
        };
        assert!(public_name_url(&failing, &urls, 8443).is_some());
        let untrusted = crate::public_name::PublicNameStatus {
            trusted_name: None,
            ..ready
        };
        assert_eq!(public_name_url(&untrusted, &urls, 8443), None);
    }
    #[test]
    fn advertised_name_and_path_input_wait_for_the_certificate_served_by_the_child() {
        let ready = crate::public_name::PublicNameStatus {
            state: "ready".into(),
            trusted_name: Some("myrelay.duckdns.org".into()),
            not_after: Some(1_900_000_000),
            ..Default::default()
        };
        let certificate = crate::public_name::Served {
            name: "myrelay.duckdns.org".into(),
            cert: "cert".into(),
            key: "key".into(),
            not_after: 1_850_000_000,
            fingerprint: "old".into(),
        };
        let urls = vec!["https://10.77.0.1:8443/#pairing".into()];
        for served in [
            None,
            Some(&crate::public_name::Served {
                name: "other.duckdns.org".into(),
                ..certificate.clone()
            }),
        ] {
            let pending = receiver_public_name(ready.clone(), served, Some(1_800_000_000));
            assert!(pending.trusted_name.is_none());
            assert!(public_name_url(&pending, &urls, 8443).is_none());
        }
        let serving = receiver_public_name(ready.clone(), Some(&certificate), Some(1_800_000_000));
        assert_eq!(serving.not_after, Some(certificate.not_after));
        assert_eq!(
            public_name_url(&serving, &urls, 8443).as_deref(),
            Some("https://myrelay.duckdns.org:8443/#pairing")
        );
        let expired = receiver_public_name(ready, Some(&certificate), Some(certificate.not_after));
        assert!(expired.trusted_name.is_none());
        let (admin, dir) = fake();
        let mut status = admin.status();
        status.public_name = Some(serving);
        let facts = crate::path_facts::RuntimePathFacts::default();
        let settings = Settings::default();
        let path = crate::path_input::build(&facts, &settings, &status, 0, true, true);
        assert_eq!(path["car"]["web"]["trustedName"], "myrelay.duckdns.org");
        status.public_name = Some(expired);
        let path = crate::path_input::build(&facts, &settings, &status, 0, true, true);
        assert!(path["car"]["web"]["trustedName"].is_null());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn receiver_certificate_and_sans_share_one_queued_update() {
        let (admin, dir) = fake();
        *admin.receiver_tls.lock().unwrap() = Some(None);
        let sans = vec![std::net::Ipv4Addr::new(10, 77, 0, 1)];
        admin.apply_receiver_tls(None, sans.clone());
        assert!(admin.receiver_tls_queued.load(Ordering::Acquire));
        assert!(matches!(
            admin
                .operation_rx
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .try_recv(),
            Ok(Operation::ReceiverTls)
        ));
        admin.apply_receiver_tls(None, sans);
        assert!(
            admin
                .operation_rx
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .try_recv()
                .is_err(),
            "coalesced while queued"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn receiver_sans_follow_new_networks_retain_old_ones_and_stay_bounded() {
        let (mut admin, dir) = fake();
        Arc::get_mut(&mut admin).unwrap().runtime.addresses = Arc::new(|| {
            Ok(vec![
                ("lo".into(), std::net::Ipv4Addr::LOCALHOST, 8),
                ("ap0".into(), std::net::Ipv4Addr::new(10, 77, 0, 1), 24),
            ])
        });
        *admin.receiver_tls_sans.lock().unwrap() = (1..=20)
            .map(|n| std::net::Ipv4Addr::new(192, 168, 1, n))
            .collect();
        let sans = admin.receiver_sans().unwrap();
        assert_eq!(sans.len(), 16);
        assert!(sans.contains(&std::net::Ipv4Addr::new(10, 77, 0, 1)));
        assert!(sans.contains(&std::net::Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!sans.contains(&std::net::Ipv4Addr::LOCALHOST));
        *admin.receiver_tls_sans.lock().unwrap() = sans.clone();
        assert_eq!(
            admin.receiver_sans().unwrap(),
            sans,
            "address flapping does not replace the certificate"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn provider_switches_and_empty_tokens_keep_each_saved_setup() {
        let (a, dir) = fake();
        let update = |value: serde_json::Value| {
            assert!(matches!(
                a.update(value.as_object().unwrap().clone()),
                AdminUpdate::AppliedNow
            ))
        };
        let token = "a1b2c3d4-0000-0000-0000-000000000000";
        update(
            serde_json::json!({"publicNameProvider":"duckdns","publicName":"duck.duckdns.org","publicNameToken":token}),
        );
        update(serde_json::json!({"publicNameProvider":"dynv6"}));
        assert_eq!(a.settings().values["publicName"], "");
        assert_eq!(a.settings().values["publicNameTokenSet"], false);
        update(serde_json::json!({"publicName":"dyn.dynv6.net","publicNameToken":"dyn-token"}));
        for (provider, name, saved_token) in [
            ("duckdns", "duck.duckdns.org", token),
            ("dynv6", "dyn.dynv6.net", "dyn-token"),
        ] {
            update(serde_json::json!({"publicNameProvider":provider,"publicNameToken":""}));
            let values = a.settings().values;
            assert_eq!(values["publicName"], name);
            assert_eq!(values["publicNameTokenSet"], true);
            assert_eq!(
                values["publicNameProviders"][provider],
                serde_json::json!({"name":name,"tokenSet":true})
            );
            let (active, _) = crate::public_name::config::load(&dir.join("public-name.json"))
                .unwrap()
                .unwrap();
            assert_eq!(active.provider.unwrap().id(), provider);
            assert_eq!(active.token.as_deref(), Some(saved_token));
            assert!(!serde_json::to_string(&values)
                .unwrap()
                .contains(saved_token));
        }
        assert!(matches!(
            a.update(
                serde_json::json!({"publicNameProviders":{}})
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            AdminUpdate::Errors(_)
        ));
        // Deleting the active file by hand also forgets both providers and the account.
        std::fs::write(dir.join("public-name/account.key"), "key").unwrap();
        std::fs::remove_file(dir.join("public-name.json")).unwrap();
        assert_eq!(
            a.settings().values["publicNameProviders"],
            serde_json::json!({"duckdns":{"name":"","tokenSet":false},"dynv6":{"name":"","tokenSet":false}})
        );
        assert!(!dir.join("public-name").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn admin_addresses_put_the_trusted_name_and_head_unit_first() {
        let head: std::net::Ipv4Addr = "10.77.0.1".parse().unwrap();
        let hosts = [
            "192.168.1.2".parse().unwrap(),
            head,
            "192.168.1.2".parse().unwrap(),
        ];
        let ready = crate::public_name::PublicNameStatus {
            state: "ready".into(),
            trusted_name: Some("a.duckdns.org".into()),
            address: Some(head.to_string()),
            ..Default::default()
        };
        let fallback = vec![
            "https://10.77.0.1:6975/",
            "https://192.168.1.2:6975/",
            "https://openxplay-linux.local:6975/",
        ];
        let mut expected = vec!["https://a.duckdns.org:6975/"];
        expected.extend(fallback.iter().copied());
        assert_eq!(admin_urls(&ready, Some(head), &hosts), expected);
        let not_ready = crate::public_name::PublicNameStatus {
            trusted_name: None,
            ..ready.clone()
        };
        assert_eq!(admin_urls(&not_ready, Some(head), &hosts), fallback);
        let mismatch = crate::public_name::PublicNameStatus {
            address: Some("10.78.0.1".into()),
            ..ready.clone()
        };
        assert_eq!(admin_urls(&mismatch, Some(head), &hosts), fallback);
        let local_name = crate::public_name::PublicNameStatus {
            trusted_name: Some("openxplay-linux.local".into()),
            ..ready.clone()
        };
        assert_eq!(
            admin_urls(&local_name, Some(head), &hosts),
            [
                "https://openxplay-linux.local:6975/",
                "https://10.77.0.1:6975/",
                "https://192.168.1.2:6975/",
            ]
        );
        assert_eq!(
            admin_urls(&ready, None, &[]),
            ["https://openxplay-linux.local:6975/"]
        );
        let renewing = crate::public_name::PublicNameStatus {
            state: "error".into(),
            ..ready
        };
        assert_eq!(admin_urls(&renewing, Some(head), &hosts), expected);
    }
    #[test]
    fn public_name_import_route_validates_auth_errors_size_and_activates_the_provider() {
        use crate::admin::{
            auth::{AdminAuth, FileAuthStore},
            server::{read_request, AdminServer, AdminServerConfig, Request},
        };
        let (a, dir) = fake();
        let server = AdminServer::new(
            a.clone(),
            AdminAuth::new(Box::new(FileAuthStore(dir.join("auth.json")))).unwrap(),
            AdminServerConfig::new(dir.clone(), dir.clone(), vec![]),
        );
        let request = |path: &str, body: serde_json::Value, cookie: &str| Request {
            method: "POST".into(),
            path: path.into(),
            peer: "10.77.0.2".parse().unwrap(),
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("x-openxplay-admin".into(), "1".into()),
                ("cookie".into(), cookie.into()),
            ]),
            body: serde_json::to_vec(&body).unwrap(),
        };
        let path = "/api/v0/public-name/import";
        assert_eq!(
            server
                .route(&request(path, serde_json::json!({"content":"{}"}), ""), 1)
                .status,
            401
        );
        let login = server.route(
            &request(
                "/api/v0/login",
                serde_json::json!({"password":"openxplay"}),
                "",
            ),
            1,
        );
        let cookie = login.headers["Set-Cookie"].split(';').next().unwrap();
        let mut no_csrf = request(path, serde_json::json!({"content":"{}"}), cookie);
        no_csrf.headers.remove("x-openxplay-admin");
        assert_eq!(server.route(&no_csrf, 2).status, 403);
        let token = "a1b2c3d4-0000-0000-0000-000000000000";
        let valid = serde_json::json!({"version":1,"provider":"duckdns","name":"a.duckdns.org","token":token,"agreeLetsEncryptTerms":true}).to_string();
        let import = |content: &str| {
            server.route(
                &request(path, serde_json::json!({"content":content}), cookie),
                2,
            )
        };
        let response = import(&valid);
        assert_eq!(response.status, 200);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&response.body).unwrap(),
            serde_json::json!({"ok":true,"provider":"duckdns","name":"a.duckdns.org"})
        );
        let before = std::fs::read(dir.join("public-name.json")).unwrap();
        for (content, key, code) in [
            ("{}".into(), "version", "missing"),
            ("{\"version\":2}".into(), "version", "bad-version"),
            (
                "{\"version\":1,\"provider\":\"other\"}".into(),
                "provider",
                "invalid",
            ),
            (
                "{\"version\":1,\"nmae\":\"x\"}".into(),
                "nmae",
                "unknown-key",
            ),
            ("[1]".into(), "file", "not-json"),
            ("not json".into(), "file", "not-json"),
            (
                " ".repeat(crate::public_name::config::MAX_BYTES + 1),
                "file",
                "too-large",
            ),
        ] {
            let response = import(&content);
            assert_eq!(response.status, 422);
            let errors: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(errors["errors"][key], code);
            assert_eq!(std::fs::read(dir.join("public-name.json")).unwrap(), before);
        }
        // The JSON wrapper may exceed 16 KiB; the cap is on the decoded UTF-8 file text.
        let padded = format!(
            "{valid}{}",
            " ".repeat(crate::public_name::config::MAX_BYTES - valid.len())
        );
        let wrapped = request(path, serde_json::json!({"content":padded}), cookie);
        assert!(wrapped.body.len() > crate::admin::server::MAX_BODY);
        let mut wire = format!(
            "POST {path} HTTP/1.1\r\nHost: relay\r\nContent-Length: {}\r\n\r\n",
            wrapped.body.len()
        )
        .into_bytes();
        wire.extend_from_slice(&wrapped.body);
        assert_eq!(
            read_request(&mut wire.as_slice(), wrapped.peer)
                .unwrap()
                .body,
            wrapped.body
        );
        assert_eq!(server.route(&wrapped, 2).status, 200);
        let dynv6 = serde_json::json!({"version":1,"provider":"dynv6","name":"b.dynv6.net","token":"dyn-secret"}).to_string();
        let response = import(&dynv6);
        assert_eq!(response.status, 200);
        let values = a.settings().values;
        assert_eq!(values["publicNameProvider"], "dynv6");
        assert_eq!(values["publicName"], "b.dynv6.net");
        assert_eq!(
            values["publicNameProviders"]["duckdns"]["name"],
            "a.duckdns.org"
        );
        assert_eq!(values["publicNameProviders"]["dynv6"]["tokenSet"], true);
        assert!(!String::from_utf8(response.body)
            .unwrap()
            .contains("dyn-secret"));
        let (active, _) = crate::public_name::config::load(&dir.join("public-name.json"))
            .unwrap()
            .unwrap();
        assert_eq!(
            active.provider,
            Some(crate::public_name::config::Provider::Dynv6)
        );
        assert_eq!(active.token.as_deref(), Some("dyn-secret"));
        a.public_name.remove().unwrap();
        assert!(!dir.join("public-name").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn admin_api_defaults_password_without_saving_or_exporting_it() {
        use crate::network_roles::SaveResult;
        let (a, dir) = fake();
        a.inventory.set(role_inventory());
        let values = serde_json::json!({"networkRoles":{"mode":"auto","adminHotspot":{
            "enabled":true,"interface":null,"ssid":"Test-Admin","band":"auto","channel":0
        }}})
        .as_object()
        .unwrap()
        .clone();
        for save in [false, true] {
            let SaveResult::Saved(proposal) = a.propose_roles(values.clone(), save) else {
                panic!("default password is usable");
            };
            assert!(proposal.plan.admin_hotspot.enabled);
            assert!(!proposal.plan.admin_hotspot.password_set);
            assert!(!proposal
                .checks
                .iter()
                .any(|c| c.code == "admin-hotspot-password"));
            assert!(a.settings.lock().unwrap().admin_hotspot_password.is_none());
        }
        assert!(Settings::load(&dir)
            .unwrap()
            .admin_hotspot_password
            .is_none());
        assert!(!a.settings().values.contains_key("adminHotspotPassword"));
        for value in [
            a.config_export(false).unwrap(),
            a.config_export(true).unwrap(),
        ] {
            assert!(!value.to_string().contains("adminHotspotPassword"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn admin_api_rejects_invalid_submitted_passwords() {
        use crate::network_roles::SaveResult;
        let (a, dir) = fake();
        a.inventory.set(role_inventory());
        let before = std::fs::read(dir.join("settings.json")).ok();
        for password in [
            "".into(),
            "short".into(),
            "a".repeat(64),
            "has\nnewline".into(),
            "non-ascii-é".into(),
        ] {
            let values = serde_json::json!({
                "networkRoles":{"mode":"auto"},"adminHotspotPassword":password
            })
            .as_object()
            .unwrap()
            .clone();
            for save in [false, true] {
                let SaveResult::Checks(checks) = a.propose_roles(values.clone(), save) else {
                    panic!("invalid password must be rejected");
                };
                assert!(checks
                    .iter()
                    .any(|c| c.code == "admin-hotspot-password" && c.level == "error"));
                assert!(a.settings.lock().unwrap().admin_hotspot_password.is_none());
                assert_eq!(std::fs::read(dir.join("settings.json")).ok(), before);
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn admin_api_hides_saved_password_and_preserves_other_automatic_links() {
        use crate::network_roles::{AdminHotspot, SaveResult};
        let (a, dir) = fake();
        let mut inv = role_inventory();
        inv.interfaces[1].activity.kind = "other-ap".into();
        a.inventory.set(inv.clone());
        {
            let mut settings = a.settings.lock().unwrap();
            settings.network_roles.admin_hotspot = Some(AdminHotspot {
                enabled: true,
                ..Default::default()
            });
        }
        let values = || {
            serde_json::json!({"networkRoles":a.settings.lock().unwrap().network_roles})
                .as_object()
                .unwrap()
                .clone()
        };
        a.settings.lock().unwrap().admin_hotspot_password =
            Some("synthetic-private-password".into());
        let before = a.network_status(&a.settings.lock().unwrap());
        assert_eq!(before.plan.carlink.role.name(), Some("wlan0"));
        // The foreign profile was removed and the admin AP can use the adapter, but an admin edit
        // must preserve the existing automatic Carlink assignment.
        inv.interfaces[1].activity.kind = "idle".into();
        a.inventory.set(inv);
        let mut update = values();
        // The UI includes resolved role fields even in Automatic mode; those fields are ignored.
        update["networkRoles"]["iphone"] =
            serde_json::json!({"link":"hotspot","interface":"wlan0"});
        update["networkRoles"]["carBrowser"] =
            serde_json::json!({"network":"iphone","interface":null});
        update["networkRoles"]["carlink"] = serde_json::json!({"interface":"wlan0"});
        update["networkRoles"]["adminHotspot"]["ssid"] = "Changed Admin".into();
        update.insert(
            "adminHotspotPassword".into(),
            "new-synthetic-private-password".into(),
        );
        let SaveResult::Saved(preview) = a.propose_roles(update.clone(), false) else {
            panic!("valid preview");
        };
        assert_eq!(preview.changes, ["adminHotspot"]);
        assert_eq!(preview.plan.carlink.role.name(), Some("wlan0"));
        assert!(!serde_json::to_string(&preview)
            .unwrap()
            .contains("private-password"));
        let SaveResult::Saved(saved) = a.propose_roles(update, true) else {
            panic!("valid save");
        };
        assert_eq!(saved.changes, ["adminHotspot"]);
        assert!(saved.plan.admin_hotspot.password_set);
        assert!(!serde_json::to_string(&a.status())
            .unwrap()
            .contains("private-password"));
        assert!(!serde_json::to_string(&a.settings())
            .unwrap()
            .contains("private-password"));
        assert!(a
            .admin_addresses()
            .unwrap()
            .contains(&std::net::Ipv4Addr::new(10, 79, 0, 1)));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// Select the interface shared with CarPlay rather than the first host interface (often usb0).
fn prefer_pairing_address(
    event: &mut serde_json::Value,
    interface: &str,
    addresses: &[(String, std::net::Ipv4Addr, u8)],
) {
    let Some((_, address, _)) = addresses
        .iter()
        .find(|(name, ip, _)| name == interface && !ip.is_loopback() && !ip.is_unspecified())
    else {
        return;
    };
    let Some(url) = event["url"].as_str() else {
        return;
    };
    let Some((scheme, rest)) = url.split_once("://") else {
        return;
    };
    let end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..end];
    let port = authority.rfind(':').map(|i| &authority[i..]).unwrap_or("");
    event["url"] = format!("{scheme}://{address}{port}{}", &rest[end..]).into();
}

#[cfg(test)]
mod pairing_address_tests {
    use super::*;
    #[test]
    fn pairing_url_prefers_wifi_and_preserves_pairing_token() {
        let original = serde_json::json!({"type":"pairing-url","url":"https://192.168.68.1:8443/#pair=secret"});
        let mut event = original.clone();
        let rows = vec![
            ("usb0".into(), "192.168.68.1".parse().unwrap(), 24),
            ("wlan0".into(), "192.168.5.173".parse().unwrap(), 24),
        ];
        prefer_pairing_address(&mut event, "wlan0", &rows);
        assert_eq!(event["url"], "https://192.168.5.173:8443/#pair=secret");
        let mut fallback = original.clone();
        prefer_pairing_address(&mut fallback, "wlan1", &rows);
        assert_eq!(fallback, original);
    }
}
