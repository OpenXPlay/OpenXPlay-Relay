// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::platform::command;
use crate::secure_connections::{PairingWindow, SecureConnections};
use crate::Result;
use crate::*;
use carplay_iap2::{DuplexStream, Receive};
use std::{
    collections::HashMap,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};
pub const IAP2_UUID: &str = "00000000-deca-fade-deca-deafdecacafe";
pub const IAP2_ACCESSORY_UUID: &str = "00000000-deca-fade-deca-deafdecacaff";
pub const CARPLAY_MARKER_UUID: &str = "d31fbf50-5d57-2797-a240-41cd484388ec";
pub const PAIRING_DEVICE_CLASS: u32 = 0x240408;
fn err(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}
pub fn valid_mac(a: &str) -> bool {
    let p: Vec<_> = a.split(':').collect();
    p.len() == 6
        && p.iter()
            .all(|s| s.len() == 2 && u8::from_str_radix(s, 16).is_ok())
}
/// A real adapter address; an unconfigured controller reports none or all zeros.
pub fn usable_address(a: &str) -> bool {
    valid_mac(a) && a != "00:00:00:00:00:00"
}
#[derive(Clone, Debug, Default)]
pub struct BondedDevice {
    pub address: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
    pub phone_class: bool,
    pub iap2: bool,
}
/// Stable Android selection reasons are also used by the admin status API.
#[derive(Clone, Debug)]
pub enum IphoneSelection {
    Selected(BondedDevice),
    SelectedUnpaired(String),
    Connected(BondedDevice),
    MultipleConnected(Vec<BondedDevice>),
    OnlyCandidate(BondedDevice),
    MultipleCandidates(Vec<BondedDevice>),
    OnlyBonded(BondedDevice),
    NoIphone,
}
impl IphoneSelection {
    pub fn select(choice: Option<&str>, devices: &[BondedDevice]) -> Self {
        let bonded: Vec<_> = devices.iter().filter(|d| d.paired).cloned().collect();
        if let Some(choice) = choice {
            return bonded
                .iter()
                .find(|d| d.address.eq_ignore_ascii_case(choice))
                .cloned()
                .map(Self::Selected)
                .unwrap_or_else(|| Self::SelectedUnpaired(choice.into()));
        }
        let strong: Vec<_> = bonded
            .iter()
            .filter(|d| d.iap2 || d.name.to_lowercase().contains("iphone"))
            .cloned()
            .collect();
        let candidates: Vec<_> = if strong.is_empty() {
            bonded.iter().filter(|d| d.phone_class).cloned().collect()
        } else {
            strong
        };
        let connected: Vec<_> = candidates.iter().filter(|d| d.connected).cloned().collect();
        if connected.len() == 1 {
            Self::Connected(connected[0].clone())
        } else if connected.len() > 1 {
            Self::MultipleConnected(connected)
        } else if candidates.len() == 1 {
            Self::OnlyCandidate(candidates[0].clone())
        } else if candidates.len() > 1 {
            Self::MultipleCandidates(candidates)
        } else if bonded.len() == 1 {
            Self::OnlyBonded(bonded[0].clone())
        } else {
            Self::NoIphone
        }
    }
    pub fn device(&self) -> Option<&BondedDevice> {
        match self {
            Self::Selected(d)
            | Self::Connected(d)
            | Self::OnlyCandidate(d)
            | Self::OnlyBonded(d) => Some(d),
            _ => None,
        }
    }
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Selected(_) => "Selected",
            Self::SelectedUnpaired(_) => "SelectedUnpaired",
            Self::Connected(_) => "Connected",
            Self::MultipleConnected(_) => "MultipleConnected",
            Self::OnlyCandidate(_) => "OnlyCandidate",
            Self::MultipleCandidates(_) => "MultipleCandidates",
            Self::OnlyBonded(_) => "OnlyBonded",
            Self::NoIphone => "NoIphone",
        }
    }
}
pub fn select_iphone(devices: &[BondedDevice], choice: Option<&str>) -> Result<BondedDevice> {
    let selection = IphoneSelection::select(choice, devices);
    selection.device().cloned().ok_or_else(|| {
        err(match selection {
            IphoneSelection::SelectedUnpaired(_) => "The configured iPhone is no longer paired",
            IphoneSelection::MultipleConnected(_) | IphoneSelection::MultipleCandidates(_) => {
                "More than one bonded iPhone candidate; configure --iphone ADDR"
            }
            _ => "No unambiguous bonded iPhone found; pair one iPhone",
        })
    })
}
pub fn rfcomm_channel(text: &str) -> Result<u8> {
    let mut records = Vec::new();
    let mut current = String::new();
    for l in text.lines() {
        if l.starts_with("Service Name:")
            || l.starts_with("Service RecHandle:") && current.contains("Service RecHandle:")
        {
            records.push(std::mem::take(&mut current));
        }
        current.push_str(l);
        current.push('\n');
    }
    records.push(current);
    let mut channels = Vec::new();
    for r in records {
        if !r.to_lowercase().contains(IAP2_UUID) {
            continue;
        }
        let mut rfcomm = false;
        for l in r.lines() {
            if l.contains("RFCOMM") && l.contains("0x0003") {
                rfcomm = true
            } else if rfcomm {
                if let Some(n) = l
                    .trim()
                    .strip_prefix("Channel:")
                    .and_then(|s| s.trim().parse::<u8>().ok())
                {
                    if (1..=30).contains(&n) {
                        channels.push(n)
                    }
                    rfcomm = false
                } else if l.contains("(0x") {
                    rfcomm = false
                }
            }
        }
    }
    channels.sort();
    channels.dedup();
    if channels.len() == 1 {
        Ok(channels[0])
    } else {
        Err(err(
            "SDP did not report one iAP2 RFCOMM channel; configure --rfcomm-channel",
        ))
    }
}
type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
fn objects(c: &Connection) -> Result<Objects> {
    Proxy::new(c, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager")
        .map_err(err)?
        .call("GetManagedObjects", &())
        .map_err(err)
}
#[derive(Clone, Debug)]
pub struct AdapterInfo {
    pub name: String,
    pub address: String,
    pub powered: Option<bool>,
}
fn adapter_entries(objects: &Objects) -> impl Iterator<Item = (&str, AdapterInfo)> {
    objects.iter().filter_map(|(path, interfaces)| {
        let props = interfaces.get("org.bluez.Adapter1")?;
        adapter_path_index(path.as_str())?;
        Some((
            path.as_str(),
            AdapterInfo {
                name: path.rsplit('/').next()?.into(),
                address: text(props.get("Address")).unwrap_or_default(),
                powered: props.get("Powered").and_then(|v| bool::try_from(v).ok()),
            },
        ))
    })
}
/// BlueZ adapter properties, or unknown when the system bus or BlueZ is unavailable.
pub fn adapters() -> Option<Vec<AdapterInfo>> {
    let c = Connection::system().ok()?;
    Some(
        adapter_entries(&objects(&c).ok()?)
            .map(|(_, info)| info)
            .collect(),
    )
}
/// An explicit name or address never falls back; otherwise use the lowest numeric hci index.
pub fn select_adapter<'a>(
    configured: Option<&str>,
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Option<&'a str> {
    entries
        .into_iter()
        .filter_map(|(name, address)| {
            let index = adapter_path_index(name)?;
            if configured.is_some_and(|wanted| {
                !wanted.eq_ignore_ascii_case(name) && !wanted.eq_ignore_ascii_case(address)
            }) {
                return None;
            }
            Some((index, name))
        })
        .min_by_key(|(index, _)| *index)
        .map(|(_, name)| name)
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SelectedAdapter {
    pub name: String,
    pub address: String,
}
#[derive(Clone, Debug, Default)]
pub struct AdapterSelection(pub Option<String>);
impl AdapterSelection {
    fn select(&self, objects: &Objects) -> Option<String> {
        let entries = adapter_entries(objects).collect::<Vec<_>>();
        let name = select_adapter(
            self.0.as_deref(),
            entries
                .iter()
                .map(|(_, a)| (a.name.as_str(), a.address.as_str())),
        )?;
        entries
            .iter()
            .find(|(_, a)| a.name == name)
            .map(|(path, _)| (*path).into())
    }
    fn selected_path(&self, objects: &Objects) -> Result<String> {
        self.select(objects).ok_or_else(|| {
            err(match &self.0 {
                Some(wanted) => format!("Bluetooth adapter {wanted} unavailable"),
                None => "Bluetooth adapter address unavailable".into(),
            })
        })
    }
    fn adapter(&self, c: &Connection) -> Result<String> {
        self.selected_path(&objects(c)?)
    }
    pub fn resolved(&self) -> Option<SelectedAdapter> {
        let c = Connection::system().ok()?;
        let objects = objects(&c).ok()?;
        let path = self.select(&objects)?;
        let address = text(
            objects
                .iter()
                .find(|(p, _)| p.as_str() == path)?
                .1
                .get("org.bluez.Adapter1")?
                .get("Address"),
        )
        .unwrap_or_default();
        Some(SelectedAdapter {
            name: path.rsplit('/').next()?.into(),
            address,
        })
    }
    pub fn index(&self) -> Option<u16> {
        let c = Connection::system().ok()?;
        adapter_path_index(&self.adapter(&c).ok()?)
    }
    pub fn clink_adapter(&self) -> Option<String> {
        self.clink_adapter_for(self.resolved())
    }
    fn clink_adapter_for(&self, resolved: Option<SelectedAdapter>) -> Option<String> {
        resolved.map(|a| a.name).or_else(|| {
            self.0
                .as_deref()
                .and_then(crate::host_profile::adapter_name_index)
                .map(|n| format!("hci{n}"))
        })
    }
}
fn prop<'a>(c: &'a Connection, p: &'a str, interface: &'a str) -> Result<Proxy<'a>> {
    Proxy::new(c, "org.bluez", p, interface).map_err(err)
}
fn text(v: Option<&OwnedValue>) -> Option<String> {
    v.and_then(|x| <&str>::try_from(x).ok()).map(str::to_string)
}
pub struct LinuxBluetooth {
    selection: AdapterSelection,
    runner: Arc<dyn CommandRunner>,
    configured: Option<String>,
    channel_override: Option<u8>,
    registration: Mutex<Option<BluezRegistration>>,
}
impl LinuxBluetooth {
    pub fn new(
        runner: Arc<dyn CommandRunner>,
        configured: Option<String>,
        rfcomm_channel: Option<u8>,
    ) -> Self {
        Self {
            selection: AdapterSelection::default(),
            runner,
            configured,
            channel_override: rfcomm_channel,
            registration: Mutex::new(None),
        }
    }
    pub fn with_adapter_selection(mut self, selection: AdapterSelection) -> Self {
        self.selection = selection;
        self
    }
    pub fn devices(&self) -> Result<Vec<BondedDevice>> {
        let c = Connection::system().map_err(err)?;
        let objects = objects(&c)?;
        let selected = self.selection.selected_path(&objects)?;
        let prefix = format!("{selected}/");
        Ok(objects
            .into_iter()
            .filter(|(path, _)| path.as_str().starts_with(&prefix))
            .map(|(_, interfaces)| interfaces)
            .filter_map(|mut interfaces| {
                let p = interfaces.remove("org.bluez.Device1")?;
                let class = p
                    .get("Class")
                    .and_then(|v| u32::try_from(v).ok())
                    .unwrap_or(0);
                let uuids = p
                    .get("UUIDs")
                    .and_then(|v| v.try_clone().ok())
                    .and_then(|v| Vec::<String>::try_from(v).ok())
                    .unwrap_or_default();
                Some(BondedDevice {
                    address: text(p.get("Address"))?,
                    name: text(p.get("Name"))
                        .or_else(|| text(p.get("Alias")))
                        .unwrap_or_default(),
                    connected: p
                        .get("Connected")
                        .and_then(|v| bool::try_from(v).ok())
                        .unwrap_or(false),
                    paired: p
                        .get("Paired")
                        .and_then(|v| bool::try_from(v).ok())
                        .unwrap_or(false),
                    phone_class: class & 0x1f00 == 0x0200,
                    iap2: uuids.iter().any(|s| s.eq_ignore_ascii_case(IAP2_UUID)),
                })
            })
            .collect())
    }
    /// Whether the adapter can reach the iPhone: present, addressed and powered. The controller
    /// waits for it with the access point already up; BlueZ changes end that wait early.
    pub fn check_radio(&self) -> Result<()> {
        let c = Connection::system().map_err(err)?;
        let path = self.selection.adapter(&c)?;
        {
            let adapter = prop(&c, &path, "org.bluez.Adapter1")?;
            // A controller that has no public address yet cannot page the iPhone.
            let address = adapter.get_property::<String>("Address").map_err(err)?;
            if !usable_address(&address) {
                return Err(err("Bluetooth adapter address unavailable"));
            }
            if !adapter.get_property::<bool>("Powered").map_err(err)? {
                return Err(err(
                    "Bluetooth radio is off; power on the adapter before starting CarPlay",
                ));
            }
        }
        let mut r = self.registration.lock().unwrap();
        if r.is_none() {
            *r = Some(BluezRegistration::new(c, &path)?);
        }
        Ok(())
    }
    pub fn select(&self) -> Result<BondedDevice> {
        select_iphone(&self.devices()?, self.configured.as_deref())
    }
    pub fn local_address(&self) -> Result<String> {
        let c = Connection::system().map_err(err)?;
        let a = self.selection.adapter(&c)?;
        let result = prop(&c, &a, "org.bluez.Adapter1")?
            .get_property("Address")
            .map_err(err);
        result
    }
    pub fn channel(&self, address: &str) -> Result<u8> {
        if !valid_mac(address) {
            return Err(err("Invalid iPhone address"));
        }
        if let Some(n) = self.channel_override {
            return Ok(n);
        }
        let c = Connection::system().map_err(err)?;
        let path = self.selection.adapter(&c)?;
        let name = path.rsplit('/').next().unwrap();
        rfcomm_channel(&command(
            self.runner.as_ref(),
            &sdp_command(name, address),
            Duration::from_secs(20),
        )?)
    }
}
fn sdp_command<'a>(adapter: &'a str, address: &'a str) -> Vec<&'a str> {
    let mut args = vec!["sdptool"];
    if adapter != "hci0" {
        args.extend(["-i", adapter]);
    }
    args.extend(["browse", address]);
    args
}
struct Profile;
#[zbus::interface(name = "org.bluez.Profile1")]
impl Profile {
    fn release(&self) {}
    /// The relay always opens iAP2 to the iPhone, as DiPlay does. An incoming connection must be
    /// rejected, not silently dropped: BlueZ keeps its own copy of the socket, the iPhone then
    /// believes the accessory link is up and refuses the relay's own connection with DM.
    fn new_connection(
        &self,
        _device: OwnedObjectPath,
        _fd: zbus::zvariant::OwnedFd,
        _properties: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::AccessDenied(
            "OpenXPlay opens iAP2 to the iPhone; incoming connections are not used".into(),
        ))
    }
    fn request_disconnection(&self, _device: OwnedObjectPath) {}
}
struct BluezRegistration {
    connection: Connection,
    paths: Vec<String>,
}
impl BluezRegistration {
    fn new(connection: Connection, adapter_path: &str) -> Result<Self> {
        let mut result = Self {
            connection,
            paths: Vec::new(),
        };
        for (path, uuid, channel) in [
            ("/org/openxplay/iap2", IAP2_ACCESSORY_UUID, 1u16),
            ("/org/openxplay/carplay", CARPLAY_MARKER_UUID, 2),
        ] {
            result
                .connection
                .object_server()
                .at(path, Profile)
                .map_err(err)?;
            let manager = prop(
                &result.connection,
                "/org/bluez",
                "org.bluez.ProfileManager1",
            )?;
            let mut options: HashMap<&str, Value<'_>> = HashMap::new();
            options.insert("Name", Value::from(crate::branding::DEVICE_NAME));
            options.insert("Role", Value::from("server"));
            options.insert("RequireAuthentication", Value::from(false));
            options.insert("RequireAuthorization", Value::from(false));
            options.insert("Channel", Value::from(channel));
            let record=format!("<record><attribute id='0x0001'><sequence><uuid value='{uuid}'/></sequence></attribute><attribute id='0x0004'><sequence><sequence><uuid value='0x0100'/></sequence><sequence><uuid value='0x0003'/><uint8 value='0x{channel:02x}'/></sequence></sequence></attribute><attribute id='0x0005'><sequence><uuid value='0x1002'/></sequence></attribute><attribute id='0x0100'><text value='{name}'/></attribute></record>", name = crate::branding::DEVICE_NAME);
            options.insert("ServiceRecord", Value::from(record.as_str()));
            let call: zbus::Result<()> = manager.call(
                "RegisterProfile",
                &(ObjectPath::try_from(path).map_err(err)?, uuid, options),
            );
            match call {
                Ok(()) => result.paths.push(path.into()),
                Err(e) if e.to_string().contains("AlreadyExists") => {
                    eprintln!("Warning: BlueZ profile {uuid} already owned by another process")
                }
                Err(e) => return Err(err(e)),
            }
        }
        prop(&result.connection, adapter_path, "org.bluez.Adapter1")?
            .set_property("Alias", crate::branding::DEVICE_NAME)
            .map_err(err)?;
        Ok(result)
    }
}
impl Drop for BluezRegistration {
    fn drop(&mut self) {
        if let Ok(p) = prop(&self.connection, "/org/bluez", "org.bluez.ProfileManager1") {
            for path in &self.paths {
                if let Ok(path) = ObjectPath::try_from(path.as_str()) {
                    let _: zbus::Result<()> = p.call("UnregisterProfile", &(path,));
                }
            }
        }
    }
}
/// BlueZ changes from the system bus. One thread follows BlueZ's signals for the life of the
/// process and reconnects when the bus connection ends; the bus resolves the `org.bluez`
/// sender at match time, so a restarted bluetoothd is followed too.
pub struct BluezSignals {
    events: Arc<JsonEvents>,
}
impl BluezSignals {
    pub fn new(events: Arc<JsonEvents>) -> Self {
        Self { events }
    }
}
impl crate::bluetooth_wake::BluetoothSignals for BluezSignals {
    fn start(&self, deliver: crate::bluetooth_wake::BluetoothListener) -> Result<()> {
        let events = self.events.clone();
        crate::platform::spawn_worker(
            "bluez-signals",
            Box::new(move || {
                let mut failures = 0u64;
                loop {
                    let started = Instant::now();
                    let e = match follow_bluez(&deliver, &events) {
                        Ok(()) => err("System bus connection closed"),
                        Err(e) => e,
                    };
                    // A connection that served a while starts the reconnect backoff over.
                    if started.elapsed() > Duration::from_secs(60) {
                        failures = 0;
                    }
                    failures += 1;
                    if failures == 1 {
                        events.log(&format!("bluetooth-events unavailable: {e}; waits poll"));
                    }
                    std::thread::sleep(Duration::from_secs(5 * failures.min(12)));
                }
            }),
        )?;
        Ok(())
    }
}
/// Delivers BlueZ changes until the bus connection ends.
fn follow_bluez(
    deliver: &crate::bluetooth_wake::BluetoothListener,
    events: &JsonEvents,
) -> Result<()> {
    let c = Connection::system().map_err(err)?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.bluez")
        .map_err(err)?
        .build();
    let messages =
        zbus::blocking::MessageIterator::for_match_rule(rule, &c, Some(64)).map_err(err)?;
    events.log("bluetooth-events listening");
    for message in messages {
        if let Some(change) = bluez_change(&message.map_err(err)?) {
            deliver(&change);
        }
    }
    Ok(())
}
type Properties = HashMap<String, OwnedValue>;
fn bluez_change(message: &zbus::Message) -> Option<crate::bluetooth_wake::BluetoothChange> {
    let header = message.header();
    let path = header.path()?.as_str();
    let body = message.body();
    match (header.interface()?.as_str(), header.member()?.as_str()) {
        ("org.freedesktop.DBus.Properties", "PropertiesChanged") => {
            let (interface, changed, _): (String, Properties, Vec<String>) =
                body.deserialize().ok()?;
            properties_change(path, &interface, &changed)
        }
        ("org.freedesktop.DBus.ObjectManager", "InterfacesAdded") => {
            let (object, interfaces): (OwnedObjectPath, HashMap<String, Properties>) =
                body.deserialize().ok()?;
            interfaces_added_change(object.as_str(), &interfaces)
        }
        _ => None,
    }
}
fn flag(properties: &Properties, key: &str) -> Option<bool> {
    properties.get(key).and_then(|v| bool::try_from(v).ok())
}
/// The address of the BlueZ device object at `path` (`.../dev_AA_BB_CC_DD_EE_FF`).
fn device_address(path: &str) -> Option<String> {
    let address = path
        .rsplit('/')
        .next()?
        .strip_prefix("dev_")?
        .replace('_', ":")
        .to_ascii_uppercase();
    valid_mac(&address).then_some(address)
}
/// What a BlueZ `PropertiesChanged` signal of `interface` on `path` means for a waiting
/// attempt: an adapter that was powered on or got an address may be usable now, and a device
/// that connected may be the iPhone coming back into range.
pub fn properties_change(
    path: &str,
    interface: &str,
    changed: &Properties,
) -> Option<crate::bluetooth_wake::BluetoothChange> {
    use crate::bluetooth_wake::BluetoothChange;
    match interface {
        "org.bluez.Adapter1" => {
            let powered = flag(changed, "Powered");
            let addressed = text(changed.get("Address")).is_some_and(|a| usable_address(&a));
            (powered == Some(true) || powered.is_none() && addressed)
                .then_some(BluetoothChange::AdapterReady)
        }
        "org.bluez.Device1" if flag(changed, "Connected") == Some(true) => {
            device_address(path).map(BluetoothChange::DeviceConnected)
        }
        _ => None,
    }
}
/// The same for an `InterfacesAdded` signal: a new adapter counts once it is powered.
pub fn interfaces_added_change(
    path: &str,
    interfaces: &HashMap<String, Properties>,
) -> Option<crate::bluetooth_wake::BluetoothChange> {
    use crate::bluetooth_wake::BluetoothChange;
    if let Some(adapter) = interfaces.get("org.bluez.Adapter1") {
        return (flag(adapter, "Powered") == Some(true)).then_some(BluetoothChange::AdapterReady);
    }
    let device = interfaces.get("org.bluez.Device1")?;
    if flag(device, "Connected") != Some(true) {
        return None;
    }
    device_address(path).map(BluetoothChange::DeviceConnected)
}
struct Agent {
    deadline: Instant,
}
impl Agent {
    fn accept(&self) -> zbus::fdo::Result<()> {
        if Instant::now() < self.deadline {
            Ok(())
        } else {
            Err(zbus::fdo::Error::AccessDenied(
                "Pairing window closed".into(),
            ))
        }
    }
}
#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}
    fn cancel(&self) {}
    fn request_confirmation(
        &self,
        _device: OwnedObjectPath,
        _passkey: u32,
    ) -> zbus::fdo::Result<()> {
        self.accept()
    }
    fn request_authorization(&self, _device: OwnedObjectPath) -> zbus::fdo::Result<()> {
        self.accept()
    }
    fn authorize_service(&self, _device: OwnedObjectPath, _uuid: String) -> zbus::fdo::Result<()> {
        self.accept()
    }
    fn request_pin_code(&self, _device: OwnedObjectPath) -> zbus::fdo::Result<String> {
        self.accept()?;
        Err(zbus::fdo::Error::NotSupported("NoInputNoOutput".into()))
    }
    fn request_passkey(&self, _device: OwnedObjectPath) -> zbus::fdo::Result<u32> {
        self.accept()?;
        Err(zbus::fdo::Error::NotSupported("NoInputNoOutput".into()))
    }
}
/// The Bluetooth-stack side of a pairing window: BlueZ on the device, a fake in tests.
pub trait PairingBackend: Send + Sync {
    /// Makes the adapter discoverable and pairable for `seconds`, with an agent that accepts
    /// pairing until `deadline`. Closing the returned session undoes it.
    fn open(&self, seconds: u32, deadline: Instant) -> Result<Box<dyn PairingSession>>;
}
/// What an open pairing window changed in the Bluetooth stack.
pub trait PairingSession: Send {
    fn close(self: Box<Self>);
}
pub fn set_device_class(runner: &dyn CommandRunner, adapter: &str, class: u32) -> Result<()> {
    command(
        runner,
        &["hciconfig", adapter, "class", &format!("0x{class:06x}")],
        Duration::from_secs(3),
    )
    .map(|_| ())
}
/// Pairing through BlueZ on the system bus.
pub struct BluezPairing {
    selection: AdapterSelection,
    events: Arc<JsonEvents>,
}
impl BluezPairing {
    pub fn new(events: Arc<JsonEvents>) -> Self {
        Self {
            events,
            selection: AdapterSelection::default(),
        }
    }
    pub fn with_adapter_selection(mut self, selection: AdapterSelection) -> Self {
        self.selection = selection;
        self
    }
}
impl PairingBackend for BluezPairing {
    fn open(&self, seconds: u32, deadline: Instant) -> Result<Box<dyn PairingSession>> {
        let c = Connection::system().map_err(err)?;
        let a = self.selection.adapter(&c)?;
        let p = prop(&c, &a, "org.bluez.Adapter1")?;
        let previous_class = p.get_property::<u32>("Class").unwrap_or(0);
        p.set_property("Alias", crate::branding::DEVICE_NAME)
            .map_err(err)?;
        c.object_server()
            .at("/org/openxplay/agent", Agent { deadline })
            .map_err(err)?;
        let manager = prop(&c, "/org/bluez", "org.bluez.AgentManager1")?;
        let path = ObjectPath::try_from("/org/openxplay/agent").unwrap();
        manager
            .call::<_, _, ()>("RegisterAgent", &(path.clone(), "NoInputNoOutput"))
            .map_err(err)?;
        manager
            .call::<_, _, ()>("RequestDefaultAgent", &(path,))
            .map_err(err)?;
        p.set_property("Pairable", true).map_err(err)?;
        p.set_property("DiscoverableTimeout", seconds)
            .map_err(err)?;
        p.set_property("Discoverable", true).map_err(err)?;
        let device = a.rsplit('/').next().unwrap_or("hci0");
        if let Err(e) = set_device_class(&ProcessCommandRunner::new(), device, PAIRING_DEVICE_CLASS)
        {
            self.events
                .log(&format!("Setting Bluetooth class failed: {e}"));
        }
        drop(p);
        drop(manager);
        Ok(Box::new(BluezPairingSession {
            connection: c,
            adapter: a,
            previous_class,
        }))
    }
}
struct BluezPairingSession {
    connection: Connection,
    adapter: String,
    previous_class: u32,
}
impl PairingSession for BluezPairingSession {
    fn close(self: Box<Self>) {
        if let Ok(p) = prop(&self.connection, &self.adapter, "org.bluez.Adapter1") {
            let _ = p.set_property("Discoverable", false);
            let _ = p.set_property("Pairable", false);
        }
        if let Ok(p) = prop(&self.connection, "/org/bluez", "org.bluez.AgentManager1") {
            let _: zbus::Result<()> = p.call(
                "UnregisterAgent",
                &(ObjectPath::try_from("/org/openxplay/agent").unwrap(),),
            );
        }
        let _ = set_device_class(
            &ProcessCommandRunner::new(),
            self.adapter.rsplit('/').next().unwrap_or("hci0"),
            self.previous_class,
        );
    }
}
fn adapter_path_index(path: &str) -> Option<u16> {
    path.rsplit('/').next()?.strip_prefix("hci")?.parse().ok()
}
struct OpenPairing {
    session: Box<dyn PairingSession>,
    secure_connections: Option<PairingWindow>,
}
/// A window in which the adapter is discoverable and accepts pairing without confirmation.
/// With a Secure Connections policy, SC is off for the window.
pub struct PairingMode {
    seconds: u32,
    events: Arc<JsonEvents>,
    backend: Arc<dyn PairingBackend>,
    secure_connections: Option<Arc<SecureConnections>>,
    state: Arc<Mutex<Option<OpenPairing>>>,
}
impl PairingMode {
    pub fn new(seconds: u32, events: Arc<JsonEvents>) -> Self {
        Self {
            seconds,
            backend: Arc::new(BluezPairing::new(events.clone())),
            events,
            secure_connections: None,
            state: Arc::new(Mutex::new(None)),
        }
    }
    /// Replaces BlueZ (tests).
    pub fn with_backend(mut self, backend: Arc<dyn PairingBackend>) -> Self {
        self.backend = backend;
        self
    }
    /// Switches Secure Connections off while the window is open.
    pub fn with_secure_connections(mut self, policy: Arc<SecureConnections>) -> Self {
        self.secure_connections = Some(policy);
        self
    }
    pub fn start(&self) -> Result<()> {
        if !(1..=3600).contains(&self.seconds) {
            return Err(err("Pairing window must be in 1..3600 seconds"));
        }
        // Off before the adapter becomes discoverable: an iPhone may start pairing at once.
        let window = self.secure_connections.as_ref().map(|p| p.open_window());
        let deadline = Instant::now() + Duration::from_secs(self.seconds.into());
        let session = match self.backend.open(self.seconds, deadline) {
            Ok(session) => session,
            Err(e) => {
                if let Some(window) = window {
                    window.close("pairing-failed");
                }
                return Err(e);
            }
        };
        *self.state.lock().unwrap() = Some(OpenPairing {
            session,
            secure_connections: window,
        });
        self.events
            .emit("pairing", &serde_json::json!({"seconds":self.seconds}));
        let state = self.state.clone();
        let events = self.events.clone();
        crate::platform::spawn_worker(
            "pairing-window",
            Box::new(move || {
                while Instant::now() < deadline && state.lock().unwrap().is_some() {
                    std::thread::sleep(Duration::from_millis(100));
                }
                end_pairing(&state, &events, "pairing-ended");
            }),
        )?;
        Ok(())
    }
    /// Ends the window now; `reason` is logged when Secure Connections is switched on again.
    pub fn end(&self, reason: &str) {
        end_pairing(&self.state, &self.events, reason);
    }
    pub fn close(&self) -> Result<()> {
        self.end("pairing-closed");
        Ok(())
    }
}
fn end_pairing(state: &Mutex<Option<OpenPairing>>, events: &JsonEvents, reason: &str) {
    let open = state.lock().unwrap().take();
    if let Some(open) = open {
        // Not pairable any more before Secure Connections is back on.
        open.session.close();
        if let Some(window) = open.secure_connections {
            window.close(reason);
        }
        events.emit("pairing-ended", &serde_json::json!({}));
    }
}
impl CloseResource for PairingMode {
    fn close(&self) -> Result<()> {
        PairingMode::close(self)
    }
}
impl Drop for PairingMode {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
pub struct LinuxRfcommStream {
    fd: Mutex<Option<OwnedFd>>,
    send_gate: Mutex<()>,
}
impl Default for LinuxRfcommStream {
    fn default() -> Self {
        Self::new()
    }
}
impl LinuxRfcommStream {
    pub fn new() -> Self {
        Self {
            fd: Mutex::new(None),
            send_gate: Mutex::new(()),
        }
    }
    fn duplicate(&self) -> Result<OwnedFd> {
        self.fd
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| err("RFCOMM is not connected"))?
            .try_clone()
    }
    pub fn connect(&self, address: &str, channel: u8, timeout: Duration) -> Result<()> {
        if !valid_mac(address) || !(1..=30).contains(&channel) {
            return Err(err("Invalid RFCOMM address or channel"));
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = timeout;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "RFCOMM requires Linux",
            ))
        }
        #[cfg(target_os = "linux")]
        {
            #[repr(C)]
            struct SockAddrRc {
                family: libc::sa_family_t,
                addr: [u8; 6],
                channel: u8,
            }
            let raw = unsafe {
                libc::socket(
                    31,
                    libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                    3,
                )
            };
            if raw < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            let octets: Vec<_> = address
                .split(':')
                .map(|s| u8::from_str_radix(s, 16).unwrap())
                .rev()
                .collect();
            let a = SockAddrRc {
                family: 31,
                addr: octets.try_into().unwrap(),
                channel,
            };
            *self.fd.lock().unwrap() = Some(fd.try_clone()?);
            let n = unsafe {
                libc::connect(
                    raw,
                    &a as *const _ as *const libc::sockaddr,
                    std::mem::size_of_val(&a) as _,
                )
            };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.raw_os_error() != Some(libc::EINPROGRESS) {
                    return Err(e);
                }
                if !poll_fd(raw, libc::POLLOUT, timeout)? {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "RFCOMM connect timed out",
                    ));
                }
                let mut code = 0;
                let mut len = 4;
                unsafe {
                    libc::getsockopt(
                        raw,
                        libc::SOL_SOCKET,
                        libc::SO_ERROR,
                        &mut code as *mut _ as *mut _,
                        &mut len,
                    )
                };
                if code != 0 {
                    return Err(std::io::Error::from_raw_os_error(code));
                }
            }
            Ok(())
        }
    }
}
fn poll_fd(fd: i32, events: i16, t: Duration) -> Result<bool> {
    let mut p = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    loop {
        let r = unsafe { libc::poll(&mut p, 1, t.as_millis().min(i32::MAX as u128) as i32) };
        if r >= 0 {
            return Ok(r > 0);
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}
impl DuplexStream for LinuxRfcommStream {
    fn send(&self, b: &[u8]) -> Result<()> {
        let _g = self.send_gate.lock().unwrap();
        let fd = self.duplicate()?;
        let start = Instant::now();
        let mut pos = 0;
        while pos < b.len() {
            if !poll_fd(
                fd.as_raw_fd(),
                libc::POLLOUT,
                Duration::from_secs(15).saturating_sub(start.elapsed()),
            )? {
                return Err(err("RFCOMM write timed out"));
            }
            let n = unsafe {
                libc::send(
                    fd.as_raw_fd(),
                    b[pos..].as_ptr() as _,
                    b.len() - pos,
                    libc::MSG_NOSIGNAL,
                )
            };
            if n <= 0 {
                return Err(std::io::Error::last_os_error());
            }
            pos += n as usize;
        }
        Ok(())
    }
    fn recv(&self, max: usize, t: Duration) -> Result<Receive> {
        let fd = self.duplicate()?;
        if !poll_fd(fd.as_raw_fd(), libc::POLLIN, t)? {
            return Ok(Receive::Timeout);
        }
        let mut b = vec![0; max.min(1024 * 1024)];
        let n = unsafe { libc::recv(fd.as_raw_fd(), b.as_mut_ptr() as _, b.len(), 0) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            return Ok(Receive::Eof);
        }
        b.truncate(n as usize);
        Ok(Receive::Data(b))
    }
    fn close(&self) -> Result<()> {
        if let Some(fd) = self.fd.lock().unwrap().take() {
            unsafe { libc::shutdown(fd.as_raw_fd(), libc::SHUT_RDWR) };
        }
        Ok(())
    }
}

/// Parsers for command-output fixtures; runtime properties come from D-Bus.
pub struct BluetoothOutput;
impl BluetoothOutput {
    pub fn devices(text: &str) -> Vec<(String, String)> {
        text.lines()
            .filter_map(|l| {
                let rest = l.strip_prefix("Device ")?;
                let (addr, name) = rest.split_once(' ')?;
                valid_mac(addr).then(|| (addr.into(), name.trim().into()))
            })
            .collect()
    }
    pub fn info(address: &str, fallback: &str, text: &str) -> BondedDevice {
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.trim().strip_prefix(&format!("{key}:")).map(str::trim))
        };
        BondedDevice {
            address: address.to_uppercase(),
            name: field("Name").unwrap_or(fallback).into(),
            connected: field("Connected") == Some("yes"),
            paired: field("Paired") == Some("yes") || field("Bonded") == Some("yes"),
            phone_class: false,
            iap2: text.to_lowercase().contains(IAP2_UUID),
        }
    }
    pub fn adapter_address(text: &str) -> Result<String> {
        text.lines()
            .find_map(|l| {
                let addr = l.strip_prefix("Controller ")?.split_whitespace().next()?;
                valid_mac(addr).then(|| addr.into())
            })
            .ok_or_else(|| err("Bluetooth adapter address unavailable"))
    }
    pub fn rfcomm_channel(text: &str) -> Result<u8> {
        rfcomm_channel(text)
    }
    pub fn select(devices: &[BondedDevice], configured: Option<&str>) -> Result<BondedDevice> {
        select_iphone(devices, configured)
    }
}

#[cfg(test)]
mod signal_tests {
    use super::*;
    #[test]
    fn adapter_selection_is_deterministic_and_never_falls_back() {
        assert_eq!(sdp_command("hci0", "phone"), ["sdptool", "browse", "phone"]);
        assert_eq!(
            sdp_command("hci2", "phone"),
            ["sdptool", "-i", "hci2", "browse", "phone"]
        );
        let entry = |n: u16, address: &str| {
            (
                OwnedObjectPath::try_from(format!("/org/bluez/hci{n}")).unwrap(),
                HashMap::from([(
                    "org.bluez.Adapter1".into(),
                    HashMap::from([(
                        "Address".into(),
                        OwnedValue::from(zbus::zvariant::Str::from(address)),
                    )]),
                )]),
            )
        };
        for reversed in [false, true] {
            let mut entries = vec![
                entry(10, "AA:BB:CC:DD:EE:10"),
                entry(2, "AA:BB:CC:DD:EE:02"),
            ];
            if reversed {
                entries.reverse();
            }
            let objects: Objects = entries.into_iter().collect();
            assert_eq!(
                AdapterSelection::default().select(&objects).as_deref(),
                Some("/org/bluez/hci2")
            );
            for configured in ["hci10", "HCI10", "aa:bb:cc:dd:ee:10"] {
                assert_eq!(
                    AdapterSelection(Some(configured.into()))
                        .select(&objects)
                        .as_deref(),
                    Some("/org/bluez/hci10")
                );
            }
            for missing in ["hci0", "AA:BB:CC:DD:EE:00"] {
                let selection = AdapterSelection(Some(missing.into()));
                assert!(selection.select(&objects).is_none());
                assert!(selection
                    .selected_path(&objects)
                    .unwrap_err()
                    .to_string()
                    .contains(missing));
            }
        }
        assert_eq!(
            AdapterSelection(Some("hci3".into()))
                .clink_adapter_for(None)
                .as_deref(),
            Some("hci3")
        );
        assert!(AdapterSelection(Some("AA:BB:CC:DD:EE:FF".into()))
            .clink_adapter_for(None)
            .is_none());
        assert!(AdapterSelection::default()
            .clink_adapter_for(None)
            .is_none());
        assert_eq!(
            AdapterSelection::default()
                .clink_adapter_for(Some(SelectedAdapter {
                    name: "hci2".into(),
                    address: String::new()
                }))
                .as_deref(),
            Some("hci2")
        );
        let one = HashMap::from([entry(0, "AA:BB:CC:DD:EE:00")]);
        assert_eq!(
            AdapterSelection::default().select(&one).as_deref(),
            Some("/org/bluez/hci0")
        );
    }

    use crate::bluetooth_wake::BluetoothChange;
    const DEVICE: &str = "/org/bluez/hci0/dev_aa_bb_cc_dd_ee_01";
    fn props(entries: &[(&str, OwnedValue)]) -> Properties {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.try_clone().unwrap()))
            .collect()
    }
    fn address(a: &str) -> OwnedValue {
        OwnedValue::try_from(Value::from(a)).unwrap()
    }
    #[test]
    fn adapter_entries_use_bluez_paths_and_optional_properties() {
        let objects = HashMap::from([
            (
                OwnedObjectPath::try_from("/org/bluez/hci0").unwrap(),
                HashMap::from([(
                    "org.bluez.Adapter1".into(),
                    props(&[
                        ("Address", address("AA:BB:CC:DD:EE:00")),
                        ("Alias", address("OpenXPlay Linux")),
                        ("Powered", true.into()),
                    ]),
                )]),
            ),
            (
                OwnedObjectPath::try_from("/org/bluez/hci2").unwrap(),
                HashMap::from([("org.bluez.Adapter1".into(), props(&[]))]),
            ),
            (
                OwnedObjectPath::try_from("/org/bluez/hcix").unwrap(),
                HashMap::from([("org.bluez.Adapter1".into(), props(&[]))]),
            ),
            (
                OwnedObjectPath::try_from(DEVICE).unwrap(),
                HashMap::from([("org.bluez.Device1".into(), props(&[]))]),
            ),
        ]);
        let mut entries = adapter_entries(&objects).collect::<Vec<_>>();
        entries.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));
        assert_eq!(entries.len(), 2);
        let (path, info) = &entries[0];
        assert_eq!(*path, "/org/bluez/hci0");
        assert_eq!(info.name, "hci0");
        assert_eq!(info.address, "AA:BB:CC:DD:EE:00");
        assert_eq!(info.powered, Some(true));
        assert_eq!(entries[1].1.name, "hci2");
        assert_eq!(entries[1].1.address, "");
        assert_eq!(entries[1].1.powered, None);
    }
    #[test]
    fn adapter_changes_wake_only_when_it_may_have_become_usable() {
        let adapter = |entries: &[(&str, OwnedValue)]| {
            properties_change("/org/bluez/hci0", "org.bluez.Adapter1", &props(entries))
        };
        let ready = Some(BluetoothChange::AdapterReady);
        assert_eq!(adapter(&[("Powered", true.into())]), ready);
        assert_eq!(adapter(&[("Address", address("02:11:22:33:44:55"))]), ready);
        assert_eq!(adapter(&[("Powered", false.into())]), None);
        // Powered off in the same signal outweighs a new address.
        assert_eq!(
            adapter(&[
                ("Powered", false.into()),
                ("Address", address("02:11:22:33:44:55"))
            ]),
            None
        );
        assert_eq!(adapter(&[("Address", address("00:00:00:00:00:00"))]), None);
        // The relay's own Alias, class and profile changes never wake a wait.
        assert_eq!(adapter(&[("Alias", address("OpenXPlay"))]), None);
        assert_eq!(adapter(&[("Class", 0x240408u32.into())]), None);
    }
    #[test]
    fn a_device_that_connects_wakes_with_its_address() {
        let device = |path: &str, connected: bool| {
            properties_change(
                path,
                "org.bluez.Device1",
                &props(&[("Connected", connected.into())]),
            )
        };
        assert_eq!(
            device(DEVICE, true),
            Some(BluetoothChange::DeviceConnected("AA:BB:CC:DD:EE:01".into()))
        );
        assert_eq!(device(DEVICE, false), None);
        assert_eq!(device("/org/bluez/hci0/dev_invalid", true), None);
        assert_eq!(
            properties_change(
                DEVICE,
                "org.bluez.MediaControl1",
                &props(&[("Connected", true.into())])
            ),
            None
        );
    }
    #[test]
    fn added_objects_wake_when_powered_or_connected() {
        let added = |path: &str, interface: &str, entries: &[(&str, OwnedValue)]| {
            interfaces_added_change(
                path,
                &HashMap::from([(interface.to_string(), props(entries))]),
            )
        };
        assert_eq!(
            added(
                "/org/bluez/hci0",
                "org.bluez.Adapter1",
                &[
                    ("Powered", true.into()),
                    ("Address", address("02:11:22:33:44:55"))
                ]
            ),
            Some(BluetoothChange::AdapterReady)
        );
        // An adapter that appears unpowered wakes the wait when it is powered on.
        assert_eq!(
            added(
                "/org/bluez/hci0",
                "org.bluez.Adapter1",
                &[("Powered", false.into())]
            ),
            None
        );
        assert_eq!(
            added(DEVICE, "org.bluez.Device1", &[("Connected", true.into())]),
            Some(BluetoothChange::DeviceConnected("AA:BB:CC:DD:EE:01".into()))
        );
        assert_eq!(
            added(DEVICE, "org.bluez.Device1", &[("Connected", false.into())]),
            None
        );
        assert!(usable_address("02:11:22:33:44:55"));
        assert!(!usable_address("00:00:00:00:00:00"));
        assert!(!usable_address(""));
    }
    #[test]
    fn the_adapter_index_comes_from_its_object_path() {
        assert_eq!(adapter_path_index("/org/bluez/hci0"), Some(0));
        assert_eq!(adapter_path_index("/org/bluez/hci12"), Some(12));
        assert_eq!(adapter_path_index("/org/bluez"), None);
        assert_eq!(adapter_path_index("/org/bluez/hcix"), None);
    }
}
