// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::settings::Settings;
use crate::Result;
use crate::*;
use carplay_airplay::{AirPlayIdentity, PairingStore};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
#[derive(Clone, Debug)]
pub struct Options {
    pub host_profile: crate::host_profile::LoadedHostProfile,
    pub state_dir: PathBuf,
    pub wifi_interface: String,
    pub wifi_channel: u8,
    pub transport: String,
    pub usb_charging_ma: u16,
    pub iphone: Option<String>,
    pub rfcomm_channel: Option<u8>,
    pub pair_seconds: Option<u32>,
    pub identity_dir: PathBuf,
    pub stream_socket: PathBuf,
    pub record_dir: Option<PathBuf>,
    pub oxp_relay: PathBuf,
    pub web_root: PathBuf,
    pub plugins_root: PathBuf,
    pub admin_root: PathBuf,
    /// Optional device-support status file of a Bluetooth guard service.
    pub bluetooth_guard_status: PathBuf,
    pub settings: Settings,
    pub overrides: BTreeMap<String, String>,
    /// Test mode: a simulated iPhone replaces the CarPlay connection.
    pub test_input: Option<crate::replay_input::TestInput>,
}
impl Options {
    pub fn device_preset(&self, root: &Path) -> crate::device_preset::PresetSelection {
        let settings_selects = std::fs::read(self.state_dir.join("settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|value| value.get("devicePreset").is_some());
        let source = if self.overrides.contains_key("--device-preset") {
            "command-line"
        } else if settings_selects {
            "settings"
        } else if self.host_profile.loaded {
            "host-profile"
        } else {
            "default"
        };
        crate::device_preset::select(root, &self.settings.device_preset, source)
    }
    pub fn hotspot_down_gap_seconds(&self) -> u64 {
        let profile = &self.host_profile.profile;
        if profile.hotspot_down_gap_explicit {
            profile.hotspot_down_gap_seconds
        } else {
            crate::device_preset::preset(&self.device_preset(Path::new("/")).id)
                .hotspot_down_gap_seconds
        }
    }
    pub fn carlink_interface(&self) -> &str {
        if self.settings.carlink_interface.is_empty() {
            &self.wifi_interface
        } else {
            &self.settings.carlink_interface
        }
    }
    pub fn shared_carlink_radio(&self) -> bool {
        self.carlink_interface() == self.wifi_interface
    }
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut i = 0;
        while i < args.len() {
            let option = &args[i];
            i += 1;
            if option == "--pairing"
                || option == "--reset-admin-password"
                || option == "--enable-admin"
            {
                if values.insert(option.clone(), "true".into()).is_some() {
                    return Err(invalid(format!("Missing or duplicate value for {option}")));
                }
                continue;
            }
            if ![
                "--host-profile",
                "--device-preset",
                "--state-dir",
                "--wifi-interface",
                "--carlink-interface",
                "--wifi-channel",
                "--identity-dir",
                "--transport",
                "--usb-charging-ma",
                "--iphone",
                "--rfcomm-channel",
                "--pair",
                "--record",
                "--stream-socket",
                "--network-mode",
                "--wifi-ssid",
                "--wifi-password",
                "--receiver-port",
                "--output",
                "--oxp-relay",
                "--web-root",
                "--plugins-root",
                "--admin-root",
                "--bluetooth-guard-status",
                "--test-input",
                "--test-keyframe",
                "--test-context",
            ]
            .contains(&option.as_str())
            {
                return Err(invalid(format!("Unknown option: {option}")));
            }
            if values.contains_key(option) || i == args.len() || args[i].starts_with("--") {
                return Err(invalid(format!("Missing or duplicate value for {option}")));
            }
            values.insert(option.clone(), args[i].clone());
            i += 1;
        }
        let path = |value: &str| -> Result<PathBuf> {
            let p = PathBuf::from(value);
            Ok(if p.is_absolute() {
                p
            } else {
                std::env::current_dir()?.join(p)
            })
        };
        let state = path(
            values
                .get("--state-dir")
                .map(String::as_str)
                .unwrap_or("/var/lib/openxplay"),
        )?;
        let host_profile = crate::host_profile::LoadedHostProfile::load(&path(
            values
                .get("--host-profile")
                .map(String::as_str)
                .unwrap_or(crate::host_profile::DEFAULT_PATH),
        )?);
        let settings =
            Settings::load_with_defaults(&state, &host_profile.profile.settings_defaults())?;
        let mut options = Self {
            state_dir: state.clone(),
            wifi_interface: settings.wifi_interface.clone(),
            wifi_channel: settings.hotspot_channel,
            transport: settings.transport.clone(),
            usb_charging_ma: 500,
            iphone: settings.iphone.clone(),
            rfcomm_channel: settings.rfcomm_channel,
            pair_seconds: None,
            identity_dir: values
                .get("--identity-dir")
                .map(|value| path(value))
                .transpose()?
                .unwrap_or_else(|| state.join("mfi")),
            stream_socket: path(
                values
                    .get("--stream-socket")
                    .map(String::as_str)
                    .unwrap_or("/tmp/oxp-relay.sock"),
            )?,
            record_dir: values.get("--record").map(|s| path(s)).transpose()?,
            oxp_relay: path(
                values
                    .get("--oxp-relay")
                    .map(String::as_str)
                    .unwrap_or("/opt/openxplay/bin/oxp-relay"),
            )?,
            web_root: path(
                values
                    .get("--web-root")
                    .map(String::as_str)
                    .unwrap_or("/opt/openxplay/share/web"),
            )?,
            plugins_root: path(
                values
                    .get("--plugins-root")
                    .map(String::as_str)
                    .unwrap_or("/opt/openxplay/share/plugins"),
            )?,
            admin_root: path(
                values
                    .get("--admin-root")
                    .map(String::as_str)
                    .unwrap_or("/opt/openxplay/share/admin"),
            )?,
            bluetooth_guard_status: path(
                values
                    .get("--bluetooth-guard-status")
                    .map(String::as_str)
                    .unwrap_or(
                        host_profile
                            .profile
                            .bluetooth_guard_status
                            .to_str()
                            .unwrap(),
                    ),
            )?,
            host_profile,
            settings,
            test_input: crate::replay_input::TestInput::from_options(&values)?,
            overrides: values,
        };
        if let Some(n) = options.overrides.get("--pair") {
            let n = n
                .parse::<u32>()
                .map_err(|_| invalid("Pairing window must be in 1..3600 seconds"))?;
            if !(1..=3600).contains(&n) {
                return Err(invalid("Pairing window must be in 1..3600 seconds"));
            }
            options.pair_seconds = Some(n);
        }
        options.apply_overrides()?;
        Ok(options)
    }
    pub fn apply_overrides(&mut self) -> Result<()> {
        let s = &mut self.settings;
        for (k, v) in &self.overrides {
            match k.as_str() {
                "--device-preset" => s.device_preset = v.clone(),
                "--wifi-interface" => s.wifi_interface = v.clone(),
                "--carlink-interface" => s.carlink_interface = v.clone(),
                "--wifi-channel" => {
                    s.hotspot_channel = v.parse().map_err(|_| invalid("Invalid Wi-Fi channel"))?
                }
                "--transport" => s.transport = v.clone(),
                "--usb-charging-ma" => {
                    self.usb_charging_ma = v
                        .parse()
                        .map_err(|_| invalid("USB charging must be 500, 1500 or 2400 mA"))?;
                    if ![500, 1500, 2400].contains(&self.usb_charging_ma) {
                        return Err(invalid("USB charging must be 500, 1500 or 2400 mA"));
                    }
                }
                "--iphone" => s.iphone = Some(v.clone()),
                "--rfcomm-channel" => {
                    s.rfcomm_channel = Some(
                        v.parse()
                            .map_err(|_| invalid("RFCOMM channel must be in 1..30"))?,
                    )
                }
                "--network-mode" => s.network_mode = v.clone(),
                "--receiver-port" => {
                    s.receiver_port = v.parse().map_err(|_| invalid("Invalid receiverPort"))?
                }
                "--output" => s.output = v.clone(),
                "--pairing" => s.require_pairing_code = true,
                _ => {}
            }
        }
        if self.overrides.contains_key("--wifi-ssid")
            || self.overrides.contains_key("--wifi-password")
        {
            let previous = s.shared_wifi_networks.first();
            s.shared_wifi_networks = vec![crate::settings::SharedWifiNetwork {
                ssid: self
                    .overrides
                    .get("--wifi-ssid")
                    .cloned()
                    .or_else(|| previous.map(|n| n.ssid.clone()))
                    .unwrap_or_default(),
                password: self
                    .overrides
                    .get("--wifi-password")
                    .cloned()
                    .or_else(|| previous.map(|n| n.password.clone()))
                    .unwrap_or_default(),
            }];
        }
        s.validate()?;
        self.wifi_interface = s.wifi_interface.clone();
        self.wifi_channel = s.hotspot_channel;
        self.transport = s.transport.clone();
        self.iphone = s.iphone.clone();
        self.rfcomm_channel = s.rfcomm_channel;
        Ok(())
    }
}
fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}
pub struct PersistentState {
    directory: PathBuf,
    lock: Mutex<Option<File>>,
}
impl PersistentState {
    pub fn open(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)?;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
        let f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join("receiver.lock"))?;
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(std::io::Error::other(
                "Another OpenXPlay Relay process owns this state directory",
            ));
        }
        Ok(Self {
            directory: directory.into(),
            lock: Mutex::new(Some(f)),
        })
    }
    pub fn identity(&self) -> Result<Arc<AirPlayIdentity>> {
        use crate::stream_bridge_sink::{base64, unbase64};
        let path = self.directory.join("airplay.identity");
        if !path.exists() {
            let i = AirPlayIdentity::generate()?;
            crate::platform::private_state_file(
                &path,
                format!(
                    "{}\n{}\n{}\n",
                    i.pairing_id,
                    base64(&i.private_key),
                    base64(&i.public_key)
                )
                .as_bytes(),
            )?;
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let s = std::fs::read_to_string(path)?;
        let f = s.lines().collect::<Vec<_>>();
        if f.len() != 3 || f[0].trim().is_empty() {
            return Err(invalid("Invalid saved AirPlay identity"));
        }
        let private_key = unbase64(f[1])?
            .try_into()
            .map_err(|_| invalid("Invalid AirPlay identity keys"))?;
        let public_key = unbase64(f[2])?
            .try_into()
            .map_err(|_| invalid("Invalid AirPlay identity keys"))?;
        Ok(Arc::new(AirPlayIdentity {
            private_key,
            public_key,
            pairing_id: f[0].into(),
        }))
    }
    pub fn pairings(&self) -> Result<Arc<dyn PairingStore>> {
        let path = self.directory.join("airplay.pairings");
        let mut saved = BTreeMap::new();
        if path.exists() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            for line in std::fs::read_to_string(&path)?
                .lines()
                .filter(|s| !s.is_empty())
            {
                let (a, b) = line
                    .split_once('\t')
                    .ok_or_else(|| invalid("Invalid saved pairing"))?;
                let id = String::from_utf8(crate::stream_bridge_sink::unbase64(a)?)
                    .map_err(|_| invalid("Invalid pairing identifier"))?;
                let key = crate::stream_bridge_sink::unbase64(b)?
                    .try_into()
                    .map_err(|_| invalid("Invalid pairing key"))?;
                saved.insert(id, key);
            }
        }
        Ok(Arc::new(FilePairings {
            path,
            saved: Mutex::new(saved),
        }))
    }
    pub fn close(&self) -> Result<()> {
        if let Some(file) = self.lock.lock().unwrap().take() {
            // Release explicitly: a concurrent fork may briefly inherit this open
            // file description before exec closes its CLOEXEC descriptor.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }
}
struct FilePairings {
    path: PathBuf,
    saved: Mutex<BTreeMap<String, [u8; 32]>>,
}
impl FilePairings {
    fn write(&self, s: &BTreeMap<String, [u8; 32]>) -> Result<()> {
        let mut bytes = String::new();
        for (id, key) in s {
            bytes.push_str(&format!(
                "{}\t{}\n",
                crate::stream_bridge_sink::base64(id.as_bytes()),
                crate::stream_bridge_sink::base64(key)
            ));
        }
        crate::platform::private_state_file(&self.path, bytes.as_bytes())
    }
}
impl PairingStore for FilePairings {
    fn save(&self, id: &str, key: &[u8; 32]) -> Result<()> {
        let mut s = self.saved.lock().unwrap();
        let mut next = s.clone();
        next.insert(id.into(), *key);
        self.write(&next)?;
        *s = next;
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<[u8; 32]>> {
        Ok(self.saved.lock().unwrap().get(id).copied())
    }
    fn clear(&self) -> Result<()> {
        let mut s = self.saved.lock().unwrap();
        self.write(&BTreeMap::new())?;
        s.clear();
        Ok(())
    }
}
impl Drop for PersistentState {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Release);
}
struct ServicesGuard(Arc<crate::relay_admin::LinuxRelayAdmin>);
impl Drop for ServicesGuard {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}
pub fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] {
        println!("{USAGE}");
        return Ok(());
    }
    let events = Arc::new(JsonEvents::new(Arc::new(|line| println!("{line}"))));
    events.milestone(crate::boot_timing::Milestone::RelayStart);
    let options = match Options::parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{USAGE}");
            return Err(e);
        }
    };
    let state = Arc::new(PersistentState::open(&options.state_dir)?);
    if options.overrides.contains_key("--reset-admin-password")
        || options.overrides.contains_key("--enable-admin")
    {
        let mut auth = crate::admin::AdminAuth::new(Box::new(crate::admin::FileAuthStore(
            options.state_dir.join("admin-auth.json"),
        )))?;
        if options.overrides.contains_key("--enable-admin") {
            auth.set_access_enabled(true)?;
            events.emit("admin-access-enabled", &serde_json::json!({}));
        }
        if options.overrides.contains_key("--reset-admin-password") {
            auth.reset()?;
            events.emit("admin-password-reset", &serde_json::json!({}));
        }
        return Ok(());
    }
    let admin = Arc::new(crate::relay_admin::LinuxRelayAdmin::new(
        options.clone(),
        state.clone(),
        events.clone(),
    )?);
    let _services = ServicesGuard(admin.clone());
    let addresses = crate::linux_hotspot::interface_addresses()?
        .into_iter()
        .filter(|(interface, a, _)| !interface.starts_with("oxpusb") && !a.is_loopback())
        .map(|(_, a, _)| a)
        .collect::<Vec<_>>();
    let auth = crate::admin::AdminAuth::new(Box::new(crate::admin::FileAuthStore(
        options.state_dir.join("admin-auth.json"),
    )))?;
    let mut admin_config = crate::admin::AdminServerConfig::new(
        options.state_dir.clone(),
        options.admin_root.clone(),
        admin.admin_addresses()?,
    );
    // Trusted HTTPS: the admin page also answers its public name with that certificate.
    admin_config.named = admin.admin_certificate();
    let mut previous_admin = admin_config.addresses.clone();
    let server = Arc::new(crate::admin::AdminServer::new(
        admin.clone(),
        auth,
        admin_config,
    ));
    let mut http = server.start()?;
    admin.start_services()?;
    let mut receiver_port = admin.receiver_port()?;
    let mut mdns = if addresses.is_empty() {
        None
    } else {
        Some(crate::mdns_host::MdnsHost::new(
            &addresses,
            6975,
            receiver_port,
        )?)
    };
    unsafe {
        libc::signal(libc::SIGINT, stop_signal as *const () as libc::sighandler_t);
        libc::signal(
            libc::SIGTERM,
            stop_signal as *const () as libc::sighandler_t,
        );
    }
    let mut previous = addresses;
    while !STOP.load(Ordering::Acquire) {
        // A finished admin operation, such as the startup network initialisation, ends the wait
        // early, so the controller starts without waiting for the next tick.
        admin.wait_for_tick(Duration::from_millis(250));
        admin.tick()?;
        let admin_addresses = admin.admin_addresses()?;
        if admin_addresses != previous_admin {
            http.update_addresses(&admin_addresses)?;
            previous_admin = admin_addresses;
        }
        let next_receiver_port = admin.receiver_port()?;
        let port_changed = next_receiver_port != receiver_port;
        if port_changed {
            mdns.take();
            receiver_port = next_receiver_port;
        }
        let addresses = admin.host_addresses()?;
        if addresses != previous || port_changed {
            if addresses.is_empty() {
                mdns.take();
            } else if let Some(host) = &mdns {
                host.update_addresses(&addresses)?;
            } else {
                mdns = Some(crate::mdns_host::MdnsHost::new(
                    &addresses,
                    6975,
                    receiver_port,
                )?);
            }
            previous = addresses;
        }
    }
    admin.shutdown();
    http.stop();
    if let Some(mut host) = mdns {
        host.stop();
    }
    state.close()
}
pub const USAGE:&str="Usage: openxplay-relay [options]\n  --identity-dir DIR    Local identity.pk8 and certificate.p7b (default /var/lib/openxplay/mfi)\n  --state-dir DIR       Private state (default /var/lib/openxplay)\n  --host-profile PATH  Device profile (default /etc/openxplay/host.json)\n  --device-preset ID   Device preset: auto, msm8916, phicomm-n1, generic (default auto)\n  --network-mode MODE   device-hotspot or shared-wifi\n  --wifi-interface IF   Wi-Fi interface (profile default, otherwise wlan0)\n  --carlink-interface IF Carlink station interface (empty = Wi-Fi interface)\n  --wifi-channel N      AP channel (0 = automatic; 2.4 GHz fallback 6)\n  --wifi-ssid SSID      Shared Wi-Fi SSID\n  --wifi-password PASS  Shared Wi-Fi password\n  --transport MODE      wireless (default) or usb\n  --usb-charging-ma MA  USB charging offer: 500 (default), 1500 or 2400\n  --iphone ADDR         Select bonded iPhone\n  --rfcomm-channel N    iAP2 RFCOMM channel override (1..30)\n  --pair SECONDS        Auto-accept pairing window (1..3600)\n  --stream-socket PATH  oxp-relay socket (default /tmp/oxp-relay.sock)\n  --oxp-relay PATH      Output binary\n  --web-root DIR        Receiver web assets\n  --plugins-root DIR   Installed plugins (default /opt/openxplay/share/plugins)\n  --admin-root DIR      Admin assets (default /opt/openxplay/share/admin)\n  --bluetooth-guard-status PATH Bluetooth guard state (default /run/openxplay/bluetooth-guard.json)\n  --receiver-port PORT  Receiver HTTPS port (default 8443)\n  --pairing             Require receiver pairing code\n  --output MODE        web, carlink or both\n  --record DIR          Record decrypted media\n  --test-input replay:PATH[,PATH]  Test mode: replay Annex B clips (files or directories; a <file>.jsonl timing index from --record is used when present) instead of an iPhone\n  --test-keyframe RULES Test-mode key-frame answers: immediate (default), delay:MS, ignore-first, ignore:N, ignore-all, stream-restart; comma-separated, an h264: or h265: prefix limits a rule to that codec\n  --test-context FILE   Test-mode context script (JSON) or demo; needs the plugins setting\n  --enable-admin        Enable head-unit admin access and exit (stop the service first)\n  --reset-admin-password Reset admin password and exit\n  --help                Show this help";
