// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Follows linux_hotspot.rs Wi-Fi output escaping semantics.
// Provisioning is new Linux functionality; it deliberately uses NM 1.30 and no ip binary.
use crate::admin::{
    auth::{private_file, random_hex},
    WifiClient, WifiNetwork,
};
use crate::settings::SharedWifiNetwork;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const ACTIVATE_TIMEOUT: Duration = Duration::from_secs(45);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
/// Lets the CarPlay access point wait for a Wi-Fi provisioning step, such as taking the saved
/// client connections down at startup, while the rest of the controller, Bluetooth included,
/// goes ahead. Open unless a step holds it.
#[derive(Default)]
pub struct ProvisioningGate {
    holds: std::sync::Mutex<usize>,
    released: std::sync::Condvar,
}
impl ProvisioningGate {
    /// Holds the gate until the returned hold drops.
    pub fn hold(self: &std::sync::Arc<Self>) -> ProvisioningHold {
        *self.holds.lock().unwrap() += 1;
        ProvisioningHold(self.clone())
    }
    /// Waits up to `timeout` for the gate to open; whether it is open.
    pub fn wait(&self, timeout: Duration) -> bool {
        let holds = self.holds.lock().unwrap();
        let (holds, _) = self
            .released
            .wait_timeout_while(holds, timeout, |holds| *holds > 0)
            .unwrap();
        *holds == 0
    }
}
/// A provisioning step in progress; the gate opens when the last hold drops.
pub struct ProvisioningHold(std::sync::Arc<ProvisioningGate>);
impl Drop for ProvisioningHold {
    fn drop(&mut self) {
        *self.0.holds.lock().unwrap() -= 1;
        self.0.released.notify_all();
    }
}
pub trait NetworkManagerCli: Send + Sync {
    fn run(&self, args: &[String], timeout: Duration) -> io::Result<String>;
}
pub struct ProcessNetworkManagerCli {
    pub executable: PathBuf,
}
impl Default for ProcessNetworkManagerCli {
    fn default() -> Self {
        Self {
            executable: "nmcli".into(),
        }
    }
}
impl NetworkManagerCli for ProcessNetworkManagerCli {
    fn run(&self, args: &[String], timeout: Duration) -> io::Result<String> {
        let mut child = Command::new(&self.executable)
            .args(args)
            // C.UTF-8 keeps tool messages stable but preserves non-ASCII SSIDs (C prints them as "?").
            .env("LC_ALL", "C.UTF-8")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        fn capture(
            mut pipe: impl Read + Send + 'static,
        ) -> thread::JoinHandle<io::Result<Vec<u8>>> {
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let n = pipe.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    if bytes.len() < 1024 * 1024 {
                        let room = 1024 * 1024 - bytes.len();
                        bytes.extend_from_slice(&buffer[..n.min(room)]);
                    }
                }
                Ok(bytes)
            })
        }
        let out = capture(child.stdout.take().unwrap());
        let err = capture(child.stderr.take().unwrap());
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break Ok(s),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(io::Error::new(io::ErrorKind::TimedOut, "nmcli timed out"));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(e);
                }
            }
        };
        let stdout = out
            .join()
            .map_err(|_| io::Error::other("nmcli stdout worker failed"))??;
        let stderr = err
            .join()
            .map_err(|_| io::Error::other("nmcli stderr worker failed"))??;
        let status = status?;
        if !status.success() {
            let mut message = String::from_utf8_lossy(&stderr).trim().to_owned();
            for pair in args.windows(2) {
                if pair[0].ends_with(".psk") && !pair[1].is_empty() {
                    message = message.replace(&pair[1], "[redacted]");
                }
            }
            return Err(io::Error::other(message));
        }
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    }
}
/// Split NM terse fields without breaking escaped colons or backslashes.
pub fn terse_fields(line: &str) -> Vec<String> {
    escaped_fields(line, ':')
}
/// Decode a single `nmcli -g` field, whose values may be pipe-separated or one per line.
/// Split before unescaping so literal `\|`, `\:` and `\\` stay inside their value.
pub fn nmcli_values(text: &str) -> Vec<String> {
    text.lines()
        .flat_map(|line| escaped_fields(line, '|'))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}
fn escaped_fields(line: &str, separator: char) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            fields.last_mut().unwrap().push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == separator {
            fields.push(String::new());
        } else {
            fields.last_mut().unwrap().push(c);
        }
    }
    if escaped {
        fields.last_mut().unwrap().push('\\');
    }
    fields
}
pub fn parse_networks(output: &str, known: &BTreeSet<String>) -> Vec<WifiNetwork> {
    let mut rows = BTreeMap::<String, WifiNetwork>::new();
    for line in output.lines() {
        let f = terse_fields(line);
        if f.len() != 4 || f[0].is_empty() {
            continue;
        }
        let Ok(signal) = f[1].parse::<u8>() else {
            continue;
        };
        if signal > 100 {
            continue;
        }
        let network = WifiNetwork {
            ssid: f[0].clone(),
            signal,
            secure: !f[2].is_empty() && f[2] != "--",
            known: known.contains(&f[0]),
        };
        if rows.get(&f[0]).is_none_or(|old| old.signal < signal) {
            rows.insert(f[0].clone(), network);
        }
    }
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by(|a, b| b.signal.cmp(&a.signal).then(a.ssid.cmp(&b.ssid)));
    rows
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Profile {
    uuid: String,
    ssid: String,
    #[serde(default)]
    interface: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
struct OwnedProfiles {
    ap: Option<Profile>,
    clients: Vec<Profile>,
}
/// Only UUIDs in this private manifest are eligible for down/delete operations.
pub struct WifiProvisioner<C: NetworkManagerCli> {
    cli: C,
    interface: String,
    setup_ssid: String,
    state_file: PathBuf,
    owned: OwnedProfiles,
    status: WifiClient,
    notices: Vec<String>,
    /// Single-radio adapters cannot scan while the setup hotspot runs; keep the last station scan.
    cached_networks: Vec<WifiNetwork>,
    saved: Vec<SharedWifiNetwork>,
    carlink_held: bool,
    local_dns: Option<crate::public_name::local_dns::Paths>,
}
impl<C: NetworkManagerCli> WifiProvisioner<C> {
    pub fn new(cli: C, interface: String, mac: &str, state_dir: &Path) -> io::Result<Self> {
        if interface.is_empty()
            || !interface
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Wi-Fi interface",
            ));
        }
        let octets: Vec<_> = mac.split(':').collect();
        if octets.len() != 6
            || octets
                .iter()
                .any(|s| s.len() != 2 || u8::from_str_radix(s, 16).is_err())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid WLAN MAC",
            ));
        }
        let state_file = state_dir.join("wifi-owned.json");
        let owned: OwnedProfiles = match fs::read(&state_file) {
            Ok(b) => serde_json::from_slice(&b)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => OwnedProfiles::default(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            local_dns: crate::runtime_paths::RuntimePaths::new(state_dir).managed_dns(),
            carlink_held: false,
            cli,
            interface,
            setup_ssid: owned
                .ap
                .as_ref()
                .map(|ap| ap.ssid.clone())
                .unwrap_or_else(|| {
                    format!(
                        "{}{}{}",
                        crate::branding::SETUP_SSID_PREFIX,
                        octets[4].to_uppercase(),
                        octets[5].to_uppercase()
                    )
                }),
            state_file,
            owned,
            status: WifiClient::default(),
            notices: Vec::new(),
            cached_networks: Vec::new(),
            saved: Vec::new(),
        })
    }
    pub fn with_local_dns(mut self, paths: Option<crate::public_name::local_dns::Paths>) -> Self {
        self.local_dns = paths;
        self
    }
    /// Only manifest UUIDs belong to the relay. Persist each successful move so retries are safe.
    fn repoint_owned(&mut self) -> io::Result<()> {
        let profiles: Vec<_> = self
            .owned
            .clients
            .iter()
            .chain(self.owned.ap.iter())
            .cloned()
            .collect();
        for profile in profiles {
            if profile.interface.as_deref() == Some(&self.interface) {
                continue;
            }
            // Missing NM profiles are recreated by sync/start_ap, never adopted by name.
            if self
                .command(
                    &["-t", "connection", "show", "uuid", &profile.uuid],
                    COMMAND_TIMEOUT,
                )
                .is_err()
            {
                continue;
            }
            self.command(
                &[
                    "-t",
                    "connection",
                    "modify",
                    "uuid",
                    &profile.uuid,
                    "connection.interface-name",
                    &self.interface,
                ],
                COMMAND_TIMEOUT,
            )?;
            for owned in self
                .owned
                .clients
                .iter_mut()
                .chain(self.owned.ap.iter_mut())
            {
                if owned.uuid == profile.uuid {
                    owned.interface = Some(self.interface.clone());
                }
            }
            self.save()?;
        }
        Ok(())
    }
    fn command(&self, args: &[&str], timeout: Duration) -> io::Result<String> {
        self.cli.run(
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            timeout,
        )
    }
    fn save(&self) -> io::Result<()> {
        private_file(&self.state_file, &serde_json::to_vec(&self.owned)?)
    }
    fn uuid() -> io::Result<String> {
        let s = random_hex(16)?;
        Ok(format!(
            "{}-{}-4{}-8{}-{}",
            &s[..8],
            &s[8..12],
            &s[13..16],
            &s[17..20],
            &s[20..]
        ))
    }
    fn up(&self, uuid: &str) -> io::Result<String> {
        self.command(
            &[
                "-t",
                "--wait",
                "45",
                "connection",
                "up",
                "uuid",
                uuid,
                "ifname",
                &self.interface,
            ],
            ACTIVATE_TIMEOUT,
        )
    }
    fn down(&self, uuid: &str) -> io::Result<String> {
        self.command(&["-t", "connection", "down", "uuid", uuid], COMMAND_TIMEOUT)
    }
    fn delete(&self, uuid: &str) -> io::Result<String> {
        self.command(
            &["-t", "connection", "delete", "uuid", uuid],
            COMMAND_TIMEOUT,
        )
    }
    pub fn hold_for_carlink(&mut self, held: bool) -> bool {
        std::mem::replace(&mut self.carlink_held, held)
    }
    fn check_radio_available(&self) -> io::Result<()> {
        if self.carlink_held {
            Err(io::Error::other("Carlink is using the Wi-Fi interface"))
        } else {
            Ok(())
        }
    }
    /// Read the active profile's secret without forwarding NM output or errors to events.
    pub fn carlink_network(&self, ssid: &str) -> io::Result<SharedWifiNetwork> {
        let read = || -> io::Result<SharedWifiNetwork> {
            let uuid = self.command(
                &["-g", "GENERAL.CON-UUID", "device", "show", &self.interface],
                COMMAND_TIMEOUT,
            )?;
            let uuid = uuid.trim();
            if uuid.is_empty() || uuid == "--" {
                return Err(io::Error::other("No active connection"));
            }
            let active_ssid = self.command(
                &[
                    "-e",
                    "no",
                    "-g",
                    "802-11-wireless.ssid",
                    "connection",
                    "show",
                    uuid,
                ],
                COMMAND_TIMEOUT,
            )?;
            if active_ssid.trim_end_matches(['\r', '\n']) != ssid {
                return Err(io::Error::other("Active network changed"));
            }
            let password = self.command(
                &[
                    "-e",
                    "no",
                    "-s",
                    "-g",
                    "802-11-wireless-security.psk",
                    "connection",
                    "show",
                    uuid,
                ],
                COMMAND_TIMEOUT,
            )?;
            Ok(SharedWifiNetwork {
                ssid: ssid.into(),
                password: password.trim_end_matches(['\r', '\n']).into(),
            })
        };
        read().map_err(|_| io::Error::other("Could not read the Carlink network credentials"))
    }
    pub fn stop_setup(&mut self) -> io::Result<()> {
        self.check_radio_available()?;
        if self.status.setup_hotspot {
            if let Some(ap) = &self.owned.ap {
                self.down(&ap.uuid)?;
            }
        }
        self.status.setup_hotspot = false;
        if let Some(paths) = &self.local_dns {
            crate::public_name::local_dns::hotspot(
                paths,
                &format!("setup-{}", self.interface),
                None,
            )?;
        }
        Ok(())
    }
    pub fn stop(&mut self) -> io::Result<()> {
        self.check_radio_available()?;
        self.stop_setup()?;
        // Only an owned client profile is taken down, so without one there is nothing to ask
        // NetworkManager: at boot an nmcli query lasts until NetworkManager has set the Wi-Fi
        // device up, and the CarPlay AP waits for this step.
        let current = if self.owned.clients.is_empty() {
            None
        } else {
            self.current_ssid()
        };
        if let Some(profile) = self
            .owned
            .clients
            .iter()
            .find(|p| Some(&p.ssid) == current.as_ref())
        {
            self.down(&profile.uuid)?;
        }
        self.status = WifiClient::default();
        Ok(())
    }
    pub fn start_with_network(&mut self, ssid: &str, password: &str) -> io::Result<()> {
        self.sync(&[SharedWifiNetwork {
            ssid: ssid.into(),
            password: password.into(),
        }])?;
        self.start()
    }
    pub fn sync(&mut self, networks: &[SharedWifiNetwork]) -> io::Result<()> {
        self.check_radio_available()?;
        let settings = crate::settings::Settings {
            shared_wifi_networks: networks.to_vec(),
            ..Default::default()
        };
        settings.validate()?;
        self.repoint_owned()?;
        let mut seen = BTreeSet::new();
        for profile in self.owned.clients.clone() {
            if !networks.iter().any(|n| n.ssid == profile.ssid)
                || !seen.insert(profile.ssid.clone())
            {
                let _ = self.down(&profile.uuid);
                self.delete(&profile.uuid)?;
                self.owned.clients.retain(|p| p.uuid != profile.uuid);
                self.save()?;
                if self.status.ssid.as_ref() == Some(&profile.ssid) {
                    self.status = WifiClient::default();
                }
            }
        }
        for (index, network) in networks.iter().enumerate() {
            let priority = (10 - index).to_string();
            let profile = self
                .owned
                .clients
                .iter()
                .find(|p| p.ssid == network.ssid)
                .cloned();
            let profile = profile.filter(|p| {
                self.command(
                    &["-t", "connection", "show", "uuid", &p.uuid],
                    COMMAND_TIMEOUT,
                )
                .is_ok()
            });
            if profile.is_none() {
                self.owned.clients.retain(|p| p.ssid != network.ssid);
                self.save()?;
            }
            let created = profile.is_none();
            let profile = match profile {
                Some(profile) => profile,
                None => {
                    let name = format!("oxp-client-{}", Self::uuid()?);
                    let mut args = vec![
                        "-t",
                        "connection",
                        "add",
                        "type",
                        "wifi",
                        "ifname",
                        &self.interface,
                        "con-name",
                        &name,
                        "ssid",
                        &network.ssid,
                        "connection.autoconnect",
                        "yes",
                        "connection.autoconnect-priority",
                        &priority,
                        "ipv4.method",
                        "auto",
                        "ipv6.method",
                        "auto",
                    ];
                    if !network.password.is_empty() {
                        args.extend([
                            "802-11-wireless-security.key-mgmt",
                            "wpa-psk",
                            "802-11-wireless-security.psk",
                            &network.password,
                        ]);
                    }
                    let output = self.command(&args, COMMAND_TIMEOUT)?;
                    let profile = Profile {
                        uuid: added_uuid(&output).ok_or_else(|| {
                            io::Error::other("nmcli did not report the new connection UUID")
                        })?,
                        ssid: network.ssid.clone(),
                        interface: Some(self.interface.clone()),
                    };
                    self.owned.clients.push(profile.clone());
                    self.save()?;
                    profile
                }
            };
            self.command(
                &[
                    "-t",
                    "connection",
                    "modify",
                    "uuid",
                    &profile.uuid,
                    "connection.autoconnect",
                    "yes",
                    "connection.autoconnect-priority",
                    &priority,
                ],
                COMMAND_TIMEOUT,
            )?;
            if created || self.saved.iter().find(|n| n.ssid == network.ssid) != Some(network) {
                if network.password.is_empty() {
                    // NM 1.30 supports removing a setting; empty key-mgmt leaves an invalid security setting.
                    self.command(
                        &[
                            "-t",
                            "connection",
                            "modify",
                            "uuid",
                            &profile.uuid,
                            "remove",
                            "802-11-wireless-security",
                        ],
                        COMMAND_TIMEOUT,
                    )?;
                } else {
                    self.command(
                        &[
                            "-t",
                            "connection",
                            "modify",
                            "uuid",
                            &profile.uuid,
                            "802-11-wireless-security.key-mgmt",
                            "wpa-psk",
                            "802-11-wireless-security.psk",
                            &network.password,
                        ],
                        COMMAND_TIMEOUT,
                    )?;
                }
            }
        }
        self.owned.clients.sort_by_key(|p| {
            networks
                .iter()
                .position(|n| n.ssid == p.ssid)
                .unwrap_or(usize::MAX)
        });
        self.saved = networks.to_vec();
        for network in &mut self.cached_networks {
            network.known = networks.iter().any(|n| n.ssid == network.ssid);
        }
        self.save()
    }
    pub fn start(&mut self) -> io::Result<()> {
        self.check_radio_available()?;
        self.stop_setup()?;
        let scan = self.scan();
        let mut profiles = self.owned.clients.clone();
        if let Ok(networks) = scan {
            profiles.retain(|p| networks.iter().any(|n| n.ssid == p.ssid));
            profiles.sort_by_key(|p| {
                std::cmp::Reverse(networks.iter().find(|n| n.ssid == p.ssid).unwrap().signal)
            });
        }
        for profile in profiles {
            match self.up(&profile.uuid) {
                Ok(_) => {
                    self.notices.clear();
                    self.client_ready(profile.ssid);
                    return Ok(());
                }
                Err(e) => self.notices = vec![classify_join_error(&e).into()],
            }
        }
        self.start_ap()
    }
    fn start_ap(&mut self) -> io::Result<()> {
        self.repoint_owned()?;
        // Scan while still a station: once the AP is up, the radio only reports the AP itself.
        if !self.status.setup_hotspot {
            let _ = self.scan();
        }
        let existing = self.owned.ap.as_ref().is_some_and(|p| {
            self.command(
                &["-t", "connection", "show", "uuid", &p.uuid],
                COMMAND_TIMEOUT,
            )
            .is_ok()
        });
        if !existing {
            // NetworkManager 1.30 rejects connection.uuid on add; read the assigned UUID back.
            let name = format!("oxp-setup-{}", Self::uuid()?);
            let ssid = self.setup_ssid.clone();
            let output = self.command(
                &[
                    "-t",
                    "connection",
                    "add",
                    "type",
                    "wifi",
                    "ifname",
                    &self.interface,
                    "con-name",
                    &name,
                    "ssid",
                    &ssid,
                    "connection.autoconnect",
                    "no",
                    "802-11-wireless.mode",
                    "ap",
                    "802-11-wireless.band",
                    "bg",
                    "802-11-wireless.channel",
                    "6",
                    "ipv4.method",
                    "shared",
                    "ipv4.addresses",
                    "10.78.0.1/24",
                    "ipv6.method",
                    "disabled",
                ],
                COMMAND_TIMEOUT,
            )?;
            let uuid = added_uuid(&output)
                .ok_or_else(|| io::Error::other("nmcli did not report the new connection UUID"))?;
            self.owned.ap = Some(Profile {
                uuid,
                ssid,
                interface: Some(self.interface.clone()),
            });
            self.save()?;
        }
        if let Some(paths) = &self.local_dns {
            crate::public_name::local_dns::hotspot(
                paths,
                &format!("setup-{}", self.interface),
                Some(std::net::Ipv4Addr::new(10, 78, 0, 1)),
            )?;
        }
        let activation = self.up(&self.owned.ap.as_ref().unwrap().uuid);
        if activation.is_err() {
            if let Some(paths) = &self.local_dns {
                let _ = crate::public_name::local_dns::hotspot(
                    paths,
                    &format!("setup-{}", self.interface),
                    None,
                );
            }
        }
        activation?;
        self.status = WifiClient {
            ssid: None,
            ip: Some("10.78.0.1".into()),
            signal: None,
            setup_hotspot: true,
            ..WifiClient::default()
        };
        Ok(())
    }
    fn client_ready(&mut self, ssid: String) {
        let ip = self
            .command(
                &["-t", "-g", "IP4.ADDRESS", "device", "show", &self.interface],
                COMMAND_TIMEOUT,
            )
            .ok()
            .and_then(|s| {
                nmcli_values(&s)
                    .iter()
                    .find_map(|value| value.split('/').next()?.parse::<std::net::Ipv4Addr>().ok())
            })
            .map(|ip| ip.to_string());
        self.status = WifiClient {
            ssid: Some(ssid),
            ip,
            signal: None,
            setup_hotspot: false,
            ..WifiClient::default()
        };
    }
    pub fn scan(&mut self) -> io::Result<Vec<WifiNetwork>> {
        if self.carlink_held || self.status.setup_hotspot {
            return Ok(self.cached_networks.clone());
        }
        let output = self.command(
            &[
                "-t",
                "-f",
                "SSID,SIGNAL,SECURITY,IN-USE",
                "dev",
                "wifi",
                "list",
                "--rescan",
                "yes",
                "ifname",
                &self.interface,
            ],
            Duration::from_secs(20),
        )?;
        let known = self.saved.iter().map(|n| n.ssid.clone()).collect();
        let rows = parse_networks(&output, &known);
        if let Some(ssid) = &self.status.ssid {
            self.status.signal = rows.iter().find(|n| &n.ssid == ssid).map(|n| n.signal);
        }
        let rows: Vec<WifiNetwork> = rows
            .into_iter()
            .filter(|n| n.ssid != self.setup_ssid)
            .collect();
        self.cached_networks = rows.clone();
        Ok(rows)
    }
    /// Call on a worker after acknowledging the browser's join request.
    pub fn join(&mut self, ssid: &str, password: &str) -> io::Result<()> {
        self.check_radio_available()?;
        let network = SharedWifiNetwork {
            ssid: ssid.into(),
            password: password.into(),
        };
        let mut networks = self.saved.clone();
        if let Some(saved) = networks.iter_mut().find(|n| n.ssid == ssid) {
            *saved = network;
        } else {
            networks.push(network);
        }
        self.sync(&networks)?;
        let uuid = self
            .owned
            .clients
            .iter()
            .find(|p| p.ssid == ssid)
            .unwrap()
            .uuid
            .clone();
        self.stop_setup()?;
        if let Err(error) = self.up(&uuid) {
            self.notices = vec![classify_join_error(&error).into()];
            self.start_ap()?;
            return Err(io::Error::other(classify_join_error(&error)));
        }
        self.notices.clear();
        self.client_ready(ssid.into());
        Ok(())
    }
    pub fn forget(&mut self) -> io::Result<()> {
        self.check_radio_available()?;
        let mut failure = None;
        for profile in self.owned.clients.clone() {
            let _ = self.down(&profile.uuid);
            match self.delete(&profile.uuid) {
                Ok(_) => {
                    self.owned.clients.retain(|p| p.uuid != profile.uuid);
                    if let Err(error) = self.save() {
                        failure.get_or_insert(error);
                    }
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        self.saved.clear();
        self.notices.clear();
        self.start_ap()?;
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    /// Observe NM autoconnect changes without scanning or changing the radio.
    pub fn current_ssid(&self) -> Option<String> {
        if self.status.setup_hotspot {
            return None;
        }
        match self.command(
            &[
                "-t",
                "-f",
                "ACTIVE,SSID",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
                "ifname",
                &self.interface,
            ],
            COMMAND_TIMEOUT,
        ) {
            Ok(rows) => rows
                .lines()
                .map(terse_fields)
                .find(|f| f.len() == 2 && f[0] == "yes")
                .map(|f| f[1].clone()),
            Err(_) => self.status.ssid.clone(),
        }
    }
    pub fn status(&self) -> WifiClient {
        let mut status = self.status.clone();
        status.setup_ssid = Some(self.setup_ssid.clone());
        if !status.setup_hotspot && status.ssid.is_some() {
            let current = self.current_ssid();
            if current != status.ssid {
                status.ssid = current;
                status.signal = None;
                status.ip = None;
                if status.ssid.is_some() {
                    status.ip = self
                        .command(
                            &["-t", "-g", "IP4.ADDRESS", "device", "show", &self.interface],
                            COMMAND_TIMEOUT,
                        )
                        .ok()
                        .and_then(|s| {
                            nmcli_values(&s).iter().find_map(|value| {
                                value.split('/').next()?.parse::<std::net::Ipv4Addr>().ok()
                            })
                        })
                        .map(|ip| ip.to_string());
                }
            }
        }
        status
    }
    pub fn notices(&self) -> &[String] {
        &self.notices
    }
    pub fn setup_ssid(&self) -> &str {
        &self.setup_ssid
    }
}
pub fn classify_join_error(error: &io::Error) -> &'static str {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("secret")
        || text.contains("password")
        || text.contains("authentication")
        || text.contains("supplicant")
    {
        "wifi.joinFailed.auth"
    } else if text.contains("not found")
        || text.contains("could not be found")
        || text.contains("no access point")
        || text.contains("no network")
        || text.contains("ssid") && text.contains("unavailable")
    {
        "wifi.joinFailed.notFound"
    } else {
        "wifi.joinFailed.timeout"
    }
}

/// The UUID in nmcli's "Connection 'name' (UUID) successfully added." confirmation.
fn added_uuid(output: &str) -> Option<String> {
    let start = output.rfind('(')? + 1;
    let end = start + output[start..].find(')')?;
    let uuid = &output[start..end];
    (uuid.len() == 36 && uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        .then(|| uuid.to_string())
}

#[cfg(test)]
mod role_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Vec<Vec<String>>>>);
    impl NetworkManagerCli for Fake {
        fn run(&self, args: &[String], _: Duration) -> io::Result<String> {
            self.0.lock().unwrap().push(args.to_vec());
            Ok(String::new())
        }
    }
    #[test]
    fn repoints_only_manifest_profiles_and_migrates_old_format() {
        let dir = std::env::temp_dir().join(format!("wifi-repoint-{}", random_hex(8).unwrap()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("wifi-owned.json");
        fs::write(&file, br#"{"ap":{"uuid":"setup-owned","ssid":"Test Setup"},"clients":[{"uuid":"client-owned","ssid":"Test Saved"}]}"#).unwrap();
        let cli = Fake::default();
        let mut provisioner =
            WifiProvisioner::new(cli.clone(), "wlan2".into(), "02:00:00:00:00:02", &dir).unwrap();
        provisioner.repoint_owned().unwrap();
        for uuid in ["setup-owned", "client-owned"] {
            assert!(cli.0.lock().unwrap().iter().any(|args| args
                == &[
                    "-t",
                    "connection",
                    "modify",
                    "uuid",
                    uuid,
                    "connection.interface-name",
                    "wlan2"
                ]));
        }
        assert_eq!(cli.0.lock().unwrap().len(), 4);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(manifest["ap"]["interface"], "wlan2");
        assert_eq!(manifest["clients"][0]["interface"], "wlan2");
        drop(provisioner);
        cli.0.lock().unwrap().clear();
        let mut same =
            WifiProvisioner::new(cli.clone(), "wlan2".into(), "02:00:00:00:00:02", &dir).unwrap();
        same.repoint_owned().unwrap();
        assert!(cli.0.lock().unwrap().is_empty());
        let mut moved =
            WifiProvisioner::new(cli.clone(), "wlan3".into(), "02:00:00:00:00:03", &dir).unwrap();
        moved.repoint_owned().unwrap();
        assert_eq!(cli.0.lock().unwrap().len(), 4);
        assert!(!cli
            .0
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|arg| arg == "delete" || arg == "down"));
        fs::remove_dir_all(dir).unwrap();
    }
}
