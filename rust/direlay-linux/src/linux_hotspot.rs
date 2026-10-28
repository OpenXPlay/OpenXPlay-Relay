// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::platform::{command, private_state_file};
use crate::Result;
use crate::*;
use carplay_iap2::Iap2WirelessSecurity;
use std::{
    net::Ipv4Addr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Radio {
    pub bssid: Option<String>,
    pub channel: Option<u8>,
    pub frequency_mhz: Option<u32>,
}
pub struct WifiOutput;
impl WifiOutput {
    pub fn ipv4_networks(text: &str) -> Vec<(Ipv4Addr, u8)> {
        text.lines()
            .filter_map(|line| {
                let (ip, prefix) = line.trim().rsplit(':').next()?.trim().split_once('/')?;
                let ip: Ipv4Addr = ip.parse().ok()?;
                let prefix: u8 = prefix.parse().ok()?;
                (!ip.is_unspecified() && !ip.is_loopback() && prefix <= 32).then_some((ip, prefix))
            })
            .collect()
    }
    pub fn ipv4(text: &str) -> Option<Ipv4Addr> {
        text.lines()
            .filter_map(|s| {
                s.trim()
                    .rsplit(':')
                    .next()?
                    .trim()
                    .split('/')
                    .next()?
                    .parse::<Ipv4Addr>()
                    .ok()
            })
            .find(|a| !a.is_unspecified() && !a.is_loopback())
    }
    pub fn unescape(text: &str) -> String {
        crate::provision::terse_fields(text.trim()).join(":")
    }
    pub fn iw_info(text: &str) -> Radio {
        let mut r = Radio::default();
        for l in text.lines() {
            let w: Vec<_> = l.split_whitespace().collect();
            if w.first() == Some(&"addr") {
                r.bssid = w
                    .get(1)
                    .filter(|s| crate::linux_bluetooth::valid_mac(s))
                    .map(|s| s.to_string())
            }
            if let Some(i) = w.iter().position(|s| *s == "channel") {
                r.channel = w.get(i + 1).and_then(|s| s.parse().ok());
                r.frequency_mhz = w
                    .get(i + 2)
                    .and_then(|s| s.trim_start_matches('(').parse().ok());
            }
        }
        r
    }
    pub fn nmcli_radio(channel: &str, bssid: &str) -> Radio {
        let bssid = Self::unescape(bssid);
        Radio {
            bssid: crate::linux_bluetooth::valid_mac(&bssid).then_some(bssid),
            channel: channel.trim().parse::<u8>().ok().filter(|n| *n > 0),
            frequency_mhz: None,
        }
    }
    pub fn ip_networks(text: &str) -> Vec<String> {
        text.split_whitespace()
            .filter_map(|s| {
                let (a, p) = s.split_once('/')?;
                a.parse::<Ipv4Addr>().ok()?;
                if p.parse::<u8>().ok()? > 32 {
                    return None;
                }
                Some(s.to_string())
            })
            .collect()
    }
    pub fn free_hotspot_address(occupied: &[String]) -> Result<String> {
        Ok(format!(
            "{}/24",
            HotspotAddressPolicy::select_subnet(occupied)?
        ))
    }
    /// Lowercase station addresses from `iw dev <if> station dump` (`Station <mac> (on wlan0)`).
    pub fn stations(text: &str) -> Vec<String> {
        text.lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                if words.next() != Some("Station") {
                    return None;
                }
                let mac = words.next()?;
                crate::linux_bluetooth::valid_mac(mac).then(|| mac.to_ascii_lowercase())
            })
            .collect()
    }
}
/// The stations associated with the AP on `interface`.
pub fn associated_stations(runner: &dyn CommandRunner, interface: &str) -> Result<Vec<String>> {
    command(
        runner,
        &["iw", "dev", interface, "station", "dump"],
        Duration::from_secs(3),
    )
    .map(|dump| WifiOutput::stations(&dump))
}
/// Name prefix of the relay's AP connections; every AP gets `oxp-carplay-<random>`.
pub const HOTSPOT_CONNECTION_PREFIX: &str = "oxp-carplay-";
/// The relay's AP connections in `nmcli -t -f NAME,UUID,ACTIVE connection show` output, as
/// `(uuid, active)`.
pub fn hotspot_connections(rows: &str) -> Vec<(String, bool)> {
    prefixed_connections(rows, HOTSPOT_CONNECTION_PREFIX)
}
fn prefixed_connections(rows: &str, prefix: &str) -> Vec<(String, bool)> {
    rows.lines()
        .filter_map(|line| {
            let fields = crate::provision::terse_fields(line);
            (fields.len() == 3 && fields[0].starts_with(prefix))
                .then(|| (fields[1].clone(), fields[2] == "yes"))
        })
        .collect()
}
/// Deletes the AP connections a previous run left in NetworkManager because it was killed
/// before its teardown. Only one relay holds the state directory, so before this process brings
/// its first AP up every such connection is stale. Returns whether one of them was active, that
/// is its AP was up until now; inactive leftovers are deleted on a best-effort basis.
pub fn remove_stale_hotspots(runner: &dyn CommandRunner) -> Result<bool> {
    remove_stale_prefix(runner, HOTSPOT_CONNECTION_PREFIX)
}
pub fn remove_stale_prefix(runner: &dyn CommandRunner, prefix: &str) -> Result<bool> {
    let rows = command(
        runner,
        &[
            "nmcli",
            "-t",
            "-f",
            "NAME,UUID,ACTIVE",
            "connection",
            "show",
        ],
        Duration::from_secs(10),
    )?;
    let mut was_up = false;
    for (uuid, active) in prefixed_connections(&rows, prefix) {
        // Deleting an active connection also takes its AP down.
        let deleted = command(
            runner,
            &["nmcli", "connection", "delete", "uuid", &uuid],
            Duration::from_secs(10),
        );
        if active {
            deleted?;
            was_up = true;
        }
    }
    Ok(was_up)
}
/// Whether the interface's radio offers an enabled 5 GHz channel. Undetectable counts as 2.4 GHz
/// only, which every Carlink head unit can serve; the UFI003's wcn36xx has no 5 GHz band.
pub fn phy_info(
    runner: &dyn CommandRunner,
    interface: &str,
) -> Option<crate::wifi_capabilities::PhyCapabilities> {
    let Ok(phy) = std::fs::read_to_string(format!("/sys/class/net/{interface}/phy80211/name"))
    else {
        return None;
    };
    command(
        runner,
        &["iw", "phy", phy.trim(), "info"],
        Duration::from_secs(3),
    )
    .ok()
    .map(|info| crate::wifi_capabilities::parse_phy(&info))
}
pub fn supports_5ghz(runner: &dyn CommandRunner, interface: &str) -> bool {
    phy_info(runner, interface).is_some_and(|info| info.station_5ghz())
}
/// Parse `iw phy <phy> info` frequency lines such as `* 5180 MHz [36] (20.0 dBm)`.
pub fn lists_5ghz(iw_phy_info: &str) -> bool {
    crate::wifi_capabilities::parse_phy(iw_phy_info).station_5ghz()
}
/// Enumerate IPv4 addresses and masks directly; OpenStick has no iproute2 binary.
pub fn interface_addresses() -> Result<Vec<(String, Ipv4Addr, u8)>> {
    let mut head = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut result = Vec::new();
    let mut p = head;
    while !p.is_null() {
        let row = unsafe { &*p };
        if !row.ifa_addr.is_null() && unsafe { (*row.ifa_addr).sa_family as i32 } == libc::AF_INET {
            let name = unsafe { std::ffi::CStr::from_ptr(row.ifa_name) }
                .to_string_lossy()
                .into_owned();
            let addr = unsafe { &*(row.ifa_addr as *const libc::sockaddr_in) };
            let a = Ipv4Addr::from(addr.sin_addr.s_addr.to_ne_bytes());
            let prefix = if row.ifa_netmask.is_null() {
                32
            } else {
                unsafe {
                    (*(row.ifa_netmask as *const libc::sockaddr_in))
                        .sin_addr
                        .s_addr
                }
                .count_ones() as u8
            };
            result.push((name, a, prefix));
        }
        p = row.ifa_next;
    }
    unsafe { libc::freeifaddrs(head) };
    Ok(result)
}
static CREDENTIALS_LOCK: Mutex<()> = Mutex::new(());

pub use crate::admin::validate_hotspot_credentials;

fn random_characters(length: usize, alphabet: &[u8]) -> Result<String> {
    use std::io::Read;
    let mut random = std::fs::File::open("/dev/urandom")?;
    let mut value = String::with_capacity(length);
    // Rejection sampling avoids bias when the alphabet does not divide 256.
    while value.len() < length {
        let mut byte = [0];
        random.read_exact(&mut byte)?;
        if (byte[0] as usize) < 256 / alphabet.len() * alphabet.len() {
            value.push(alphabet[byte[0] as usize % alphabet.len()] as char);
        }
    }
    Ok(value)
}

pub fn save_hotspot_credentials(
    state_dir: &std::path::Path,
    ssid: &str,
    password: &str,
) -> Result<()> {
    validate_hotspot_credentials(ssid, password).map_err(std::io::Error::other)?;
    let _guard = CREDENTIALS_LOCK.lock().unwrap();
    // JSON permits every UTF-8 SSID, including names containing line breaks.
    private_state_file(
        &state_dir.join("hotspot.credentials"),
        &serde_json::to_vec(&(ssid, password))?,
    )
}

pub fn hotspot_credentials(state_dir: &std::path::Path) -> Result<(String, String)> {
    let _guard = CREDENTIALS_LOCK.lock().unwrap();
    let p = state_dir.join("hotspot.credentials");
    let s = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let ssid = format!(
                "{}{}",
                crate::branding::HOTSPOT_SSID_PREFIX,
                random_characters(4, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")?
            );
            let password = "openxplay";
            let s = format!("{ssid}\n{password}\n");
            private_state_file(&p, s.as_bytes())?;
            s
        }
        Err(e) => return Err(e),
    };
    let credentials: (String, String) = serde_json::from_str(&s).or_else(|_| {
        let lines: Vec<_> = s.lines().collect();
        if lines.len() == 2 {
            Ok((lines[0].into(), lines[1].into()))
        } else {
            Err(std::io::Error::other("Invalid saved hotspot credentials"))
        }
    })?;
    validate_hotspot_credentials(&credentials.0, &credentials.1).map_err(std::io::Error::other)?;
    Ok(credentials)
}
/// How often the Wi-Fi device's NetworkManager state is read while it is not yet usable.
pub const DEVICE_POLL: Duration = Duration::from_millis(100);
/// The longest wait for NetworkManager to make the Wi-Fi device usable for the access point.
pub const DEVICE_WAIT: Duration = Duration::from_secs(20);
/// The state number in `nmcli -g GENERAL.STATE device show <if>` output such as
/// `30 (disconnected)`.
pub fn device_state(text: &str) -> Option<u32> {
    text.split_whitespace().next()?.parse().ok()
}
/// Whether NetworkManager can activate a connection on a device in `state`: disconnected (30)
/// or later. Unmanaged (10) and unavailable (20) devices are still being set up at boot.
pub fn device_available(state: u32) -> bool {
    state >= 30
}
/// NM keyfile strings use GLib escapes, including spaces at either edge and list separators.
fn keyfile_string(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".into(),
            '\n' => "\\n".into(),
            '\r' => "\\r".into(),
            '\t' => "\\t".into(),
            ' ' => "\\s".into(),
            _ => c.to_string(),
        })
        .collect()
}
/// Security values shared with Android user configuration and the admin API.
pub const HOTSPOT_SECURITY: [&str; 4] = ["open", "wpa2", "wpa3-transition", "wpa3"];
pub fn security_supported(security: &str, supported: &[String]) -> bool {
    matches!(security, "open" | "wpa2") || supported.iter().any(|s| s == security)
}
/// SAE needs both AP management-frame protection and supplicant support. Unknown stays WPA2.
pub fn hotspot_security_capabilities(phy: &str, key_management: &str) -> Vec<String> {
    let mut values = vec!["open".into(), "wpa2".into()];
    let ap = crate::wifi_capabilities::parse_phy(phy).ap;
    let pmf = phy.contains("CMAC") || phy.contains("00-0f-ac:6");
    let sae = key_management.split_whitespace().any(|s| s == "SAE");
    if ap && pmf && sae {
        values.extend(["wpa3-transition".into(), "wpa3".into()]);
    }
    values
}
pub fn probe_hotspot_security(
    runner: &dyn CommandRunner,
    interface: &str,
    phy: &str,
) -> Vec<String> {
    if hotspot_security_capabilities(phy, "SAE").len() == 2 {
        return vec!["open".into(), "wpa2".into()];
    }
    let keys = command(
        runner,
        &["wpa_cli", "-i", interface, "get_capability", "key_mgmt"],
        Duration::from_secs(3),
    )
    .unwrap_or_default();
    hotspot_security_capabilities(phy, &keys)
}
fn keyfile_security(security: &str, password: &str) -> Result<String> {
    if security == "open" {
        return Ok(String::new());
    }
    let (key, pmf) = match security {
        "wpa2" => ("wpa-psk", 1),
        "wpa3-transition" => ("wpa-psk", 2),
        "wpa3" => ("sae", 3),
        _ => return Err(std::io::Error::other("Unsupported hotspot security")),
    };
    Ok(format!("[wifi-security]\nkey-mgmt={key}\nproto=rsn;\npairwise=ccmp;\ngroup=ccmp;\npmf={pmf}\npsk={}\n\n", keyfile_string(password)))
}
fn wireless_security(value: &str) -> Iap2WirelessSecurity {
    match value {
        "open" => Iap2WirelessSecurity::None,
        "wpa3-transition" => Iap2WirelessSecurity::Wpa3Transition,
        "wpa3" => Iap2WirelessSecurity::Wpa3Only,
        _ => Iap2WirelessSecurity::WpaWpa2,
    }
}
struct ApKeyfile<'a> {
    id: &'a str,
    interface: &'a str,
    mac: Option<&'a str>,
    ssid: &'a str,
    password: &'a str,
    security: &'a str,
    band: &'a str,
    channel: &'a str,
    subnet: &'a str,
}
impl ApKeyfile<'_> {
    fn text(&self) -> Result<String> {
        let random = crate::admin::auth::random_hex(16)?;
        let uuid = format!(
            "{}-{}-4{}-8{}-{}",
            &random[..8],
            &random[8..12],
            &random[13..16],
            &random[17..20],
            &random[20..]
        );
        let binding = if self.mac.is_some() {
            ""
        } else {
            self.interface
        };
        let mac = self
            .mac
            .map_or_else(String::new, |mac| format!("mac-address={mac}\n"));
        let security = keyfile_security(self.security, self.password)?;
        Ok(format!("[connection]\nid={}\nuuid={uuid}\ntype=wifi\nautoconnect=false\ninterface-name={binding}\n\n[wifi]\n{mac}mode=ap\nssid={}\nband={}\nchannel={}\npowersave=2\n\n{security}[ipv4]\nmethod=shared\naddress1={}\n\n[ipv6]\nmethod=disabled\n",
            self.id, self.ssid.bytes().map(|b| format!("{b};")).collect::<String>(), self.band, self.channel, self.subnet))
    }
}
struct HotspotState {
    connection: Option<String>,
    profile_loaded: bool,
    profile_channel: u8,
    ready: Option<WirelessHotspotInfo>,
    ready_prefix: u8,
}
pub struct LinuxHotspot {
    state_dir: PathBuf,
    runner: Arc<dyn CommandRunner>,
    interface: String,
    channel: u8,
    state: Mutex<HotspotState>,
    closed: AtomicBool,
    provisioning: Option<Arc<crate::provision::ProvisioningGate>>,
    before_activation: Option<crate::host_backends::BeforeActivation>,
    admin: Option<(String, crate::host_backends::AccessPointRequest)>,
    keyfile_dir: PathBuf,
    dns: crate::public_name::local_dns::Paths,
    resolution: Option<(
        Arc<crate::host_capabilities::CapabilityCache>,
        crate::device_preset::HotspotResolution,
    )>,
}
impl LinuxHotspot {
    pub fn new(
        state_dir: PathBuf,
        runner: Arc<dyn CommandRunner>,
        interface: String,
        channel: u8,
    ) -> Self {
        let paths = crate::runtime_paths::RuntimePaths::new(&state_dir);
        Self {
            keyfile_dir: paths.keyfiles,
            dns: paths.local_dns,
            admin: None,
            state_dir,
            runner,
            interface,
            channel,
            state: Mutex::new(HotspotState {
                connection: None,
                profile_loaded: false,
                profile_channel: 0,
                ready: None,
                ready_prefix: 32,
            }),
            closed: AtomicBool::new(false),
            provisioning: None,
            before_activation: None,
            resolution: None,
        }
    }
    /// Admin APs keep their fixed subnet, MAC binding and requested band on activation failure.
    pub fn with_admin(
        mut self,
        mac: String,
        request: crate::host_backends::AccessPointRequest,
    ) -> Self {
        self.admin = Some((mac, request));
        self
    }
    pub fn with_runtime_dirs(
        mut self,
        keyfiles: PathBuf,
        dns: crate::public_name::local_dns::Paths,
    ) -> Self {
        self.keyfile_dir = keyfiles;
        self.dns = dns;
        self
    }
    pub fn with_resolution(
        mut self,
        cache: Arc<crate::host_capabilities::CapabilityCache>,
        resolution: crate::device_preset::HotspotResolution,
    ) -> Self {
        self.resolution = Some((cache, resolution));
        self
    }
    /// Runs with the AP's address just before its activation, e.g. to give NetworkManager's
    /// shared-mode dnsmasq configuration it reads only when it starts.
    pub fn with_before_activation(mut self, hook: crate::host_backends::BeforeActivation) -> Self {
        self.before_activation = Some(hook);
        self
    }
    /// The relay's Wi-Fi provisioning, which the AP's activation waits for: at startup it takes
    /// the saved client connections on the interface down.
    pub fn with_provisioning(mut self, gate: Arc<crate::provision::ProvisioningGate>) -> Self {
        self.provisioning = Some(gate);
        self
    }
    pub fn start(&self, timeout: Duration) -> Result<WirelessHotspotInfo> {
        if let Some((_, request)) = &self.admin {
            return self.start_requested(request, timeout);
        }
        let hotspot = self
            .resolution
            .as_ref()
            .map(|(_, hotspot)| hotspot.clone())
            .unwrap_or_else(|| {
                crate::device_preset::resolve_hotspot("auto", self.channel, "generic", None)
            });
        self.start_requested(
            &crate::host_backends::AccessPointRequest::saved(
                self.state_dir.clone(),
                if hotspot.band == "5" {
                    crate::host_backends::WifiBand::Ghz5
                } else {
                    crate::host_backends::WifiBand::Ghz2
                },
                hotspot.channel,
            ),
            timeout,
        )
    }
    pub fn start_requested(
        &self,
        request: &crate::host_backends::AccessPointRequest,
        timeout: Duration,
    ) -> Result<WirelessHotspotInfo> {
        if self.closed.load(Ordering::Acquire) {
            return Err(std::io::Error::other("Hotspot cancelled"));
        }
        if let Some(info) = &self.state.lock().unwrap().ready {
            return Ok(info.clone());
        }
        let deadline = Instant::now() + timeout;
        let cmd = |args: &[&str]| -> Result<String> {
            if self.closed.load(Ordering::Acquire) {
                return Err(std::io::Error::other("Hotspot cancelled"));
            }
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(20));
            if remaining.is_zero() {
                return Err(std::io::Error::other("Hotspot startup timed out"));
            }
            command(self.runner.as_ref(), args, remaining)
        };
        if self.admin.is_some() {
            let info = cmd(&["iw", "dev", &self.interface, "info"])?;
            if info.lines().any(|line| line.trim() == "type AP") {
                let profile = cmd(&[
                    "nmcli",
                    "-g",
                    "GENERAL.CONNECTION",
                    "device",
                    "show",
                    &self.interface,
                ])?;
                if !profile.trim().starts_with("oxp-admin-") {
                    return Err(std::io::Error::other(
                        "Admin hotspot interface is held by a foreign AP",
                    ));
                }
            }
        }
        let (ssid, password) = match &request.credentials {
            crate::host_backends::AccessPointCredentials::Saved(dir) => hotspot_credentials(dir)?,
            crate::host_backends::AccessPointCredentials::Supplied { ssid, password } => {
                validate_hotspot_credentials(ssid, password).map_err(std::io::Error::other)?;
                (ssid.clone(), password.clone())
            }
        };
        let security = match &request.credentials {
            crate::host_backends::AccessPointCredentials::Saved(dir) => {
                crate::settings::Settings::load(dir)?.hotspot_security
            }
            crate::host_backends::AccessPointCredentials::Supplied { .. } => "wpa2".into(),
        };
        if matches!(security.as_str(), "wpa3-transition" | "wpa3") {
            let capability = crate::host_capabilities::probe_wifi(
                self.runner.as_ref(),
                std::path::Path::new("/"),
                &self.interface,
            );
            if !security_supported(&security, &capability.hotspot_security) {
                return Err(std::io::Error::other(
                    "Hotspot security is unsupported by this radio",
                ));
            }
        }
        let occupied = interface_addresses()?
            .iter()
            .map(|(_, a, p)| format!("{a}/{p}"))
            .collect::<Vec<_>>();
        let subnet = if self.admin.is_some() {
            "10.79.0.1/24".into()
        } else {
            WifiOutput::free_hotspot_address(&occupied)?
        };
        if let (Some(hook), Some(address)) = (
            &self.before_activation,
            subnet.split('/').next().and_then(|a| a.parse().ok()),
        ) {
            hook(address);
        }
        let id = format!(
            "{}{}",
            if self.admin.is_some() {
                "oxp-admin-"
            } else {
                HOTSPOT_CONNECTION_PREFIX
            },
            crate::admin::auth::random_hex(8)?
        );
        self.state.lock().unwrap().connection = Some(id.clone());
        let result = (|| {
            if let Some((cache, hotspot)) = &self.resolution {
                cache.record_hotspot(&self.interface, hotspot.clone());
            }
            let channel = request.channel.to_string();
            let band = if request.band == crate::host_backends::WifiBand::Ghz5 {
                "a"
            } else {
                "bg"
            };
            let keyfile = self.keyfile_dir.join(format!("{id}.nmconnection"));
            self.state.lock().unwrap().profile_channel = channel.parse().unwrap_or(0);
            let profile = ApKeyfile {
                id: &id,
                interface: &self.interface,
                mac: self.admin.as_ref().map(|(mac, _)| mac.as_str()),
                ssid: &ssid,
                password: &password,
                security: &security,
                band,
                channel: &channel,
                subnet: &subnet,
            };
            // NM ignores dotfiles, including the atomic writer's temporary file.
            let staging = self.keyfile_dir.join(format!(".{id}.nmconnection"));
            private_state_file(&staging, profile.text()?.as_bytes())?;
            if let Err(error) = std::fs::rename(&staging, &keyfile) {
                let _ = std::fs::remove_file(&staging);
                return Err(error);
            }
            // The PSK stays in the 0600 keyfile; neither argv nor command errors contain it.
            cmd(&[
                "nmcli",
                "connection",
                "load",
                keyfile
                    .to_str()
                    .ok_or_else(|| std::io::Error::other("Invalid keyfile path"))?,
            ])?;
            self.state.lock().unwrap().profile_loaded = true;
            self.wait_for_provisioning(deadline);
            self.wait_for_device(deadline);
            crate::public_name::local_dns::hotspot(
                &self.dns,
                &self.interface,
                subnet.split('/').next().and_then(|s| s.parse().ok()),
            )?;
            if let Err(error) = cmd(&["nmcli", "connection", "up", "id", &id]) {
                if self.admin.is_some() {
                    return Err(error);
                }
                if let Some((cache, hotspot)) = &self.resolution {
                    let mut fallback = hotspot.clone();
                    fallback.band = "2.4".into();
                    fallback.channel = 6;
                    fallback.reason = Some("ap-start-failed".into());
                    cache.record_hotspot(&self.interface, fallback);
                }
                cmd(&[
                    "nmcli",
                    "connection",
                    "modify",
                    &id,
                    "802-11-wireless.band",
                    "bg",
                    "802-11-wireless.channel",
                    "6",
                ])?;
                cmd(&["nmcli", "connection", "up", "id", &id])?;
                self.state.lock().unwrap().profile_channel = 6;
            }
            let (address, prefix) = loop {
                let output = cmd(&[
                    "nmcli",
                    "-g",
                    "IP4.ADDRESS",
                    "device",
                    "show",
                    &self.interface,
                ])?;
                // An auxiliary /32 must never become the AP's advertised gateway.
                if let Some((a, prefix)) = WifiOutput::ipv4_networks(&output)
                    .into_iter()
                    .find(|(_, prefix)| *prefix != 32)
                {
                    if self.admin.is_none() || a == Ipv4Addr::new(10, 79, 0, 1) {
                        break (a, prefix);
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            };
            let radio = self.radio(&cmd)?;
            let bssid = radio
                .bssid
                .ok_or_else(|| std::io::Error::other("AP BSSID unavailable"))?;
            let info = WirelessHotspotInfo {
                ssid,
                passphrase: if security == "open" {
                    String::new()
                } else {
                    password
                },
                channel: radio
                    .channel
                    .ok_or_else(|| std::io::Error::other("AP channel unavailable"))?,
                security: wireless_security(&security),
                host_address: Some(address),
                bssid: Some(bssid.clone()),
                access_point_bssid: Some(bssid),
                interface_name: Some(self.interface.clone()),
            };
            if let Some((cache, hotspot)) = &self.resolution {
                let mut actual = cache
                    .hotspot(&self.interface)
                    .unwrap_or_else(|| hotspot.clone());
                actual.channel = info.channel;
                actual.band = if info.channel > 14 { "5" } else { "2.4" }.into();
                cache.record_hotspot(&self.interface, actual);
            }
            let mut state = self.state.lock().unwrap();
            state.ready = Some(info.clone());
            state.ready_prefix = prefix;
            Ok(info)
        })();
        if result.is_err() {
            let _ = self.cleanup();
        }
        result
    }
    /// Waits until the relay's Wi-Fi provisioning lets the AP take the interface, at most until
    /// `deadline`. Everything before the activation, the profile included, is ready meanwhile.
    fn wait_for_provisioning(&self, deadline: Instant) {
        let Some(gate) = &self.provisioning else {
            return;
        };
        let started = Instant::now();
        while !gate.wait(DEVICE_POLL) {
            if self.closed.load(Ordering::Acquire) || Instant::now() >= deadline {
                break;
            }
        }
        let waited = started.elapsed();
        if waited >= DEVICE_POLL {
            eprintln!(
                "{} waited {} ms for the Wi-Fi provisioning before the access point",
                self.interface,
                waited.as_millis()
            );
        }
    }
    /// Waits until NetworkManager can activate a connection on the interface. After power-on it
    /// first sets the Wi-Fi device up, and an access point started before that fails with "No
    /// suitable device found". The relay starts earlier, so the access point waits here for
    /// exactly what its activation needs, at most [`DEVICE_WAIT`]; then the activation reports
    /// what is wrong. Output it cannot read lets the activation go ahead at once.
    fn wait_for_device(&self, deadline: Instant) {
        let started = Instant::now();
        let limit = deadline.min(started + DEVICE_WAIT);
        loop {
            let state = command(
                self.runner.as_ref(),
                &[
                    "nmcli",
                    "-g",
                    "GENERAL.STATE",
                    "device",
                    "show",
                    &self.interface,
                ],
                Duration::from_secs(5),
            );
            // An error means NetworkManager does not list the interface yet.
            let usable = state.map(|text| device_state(&text).is_none_or(device_available));
            if usable.unwrap_or(false)
                || self.closed.load(Ordering::Acquire)
                || Instant::now() + DEVICE_POLL >= limit
            {
                break;
            }
            std::thread::sleep(DEVICE_POLL);
        }
        let waited = started.elapsed();
        if waited >= DEVICE_POLL {
            eprintln!(
                "{} ready for the access point after {} ms",
                self.interface,
                waited.as_millis()
            );
        }
    }
    fn radio(&self, cmd: &dyn Fn(&[&str]) -> Result<String>) -> Result<Radio> {
        let radio = cmd(&["iw", "dev", &self.interface, "info"])
            .map(|s| WifiOutput::iw_info(&s))
            .unwrap_or_default();
        if radio.bssid.is_some() && radio.channel.is_some() {
            return Ok(radio);
        }
        let text = cmd(&[
            "nmcli",
            "-t",
            "-f",
            "IN-USE,CHAN,BSSID",
            "device",
            "wifi",
            "list",
            "--rescan",
            "no",
            "ifname",
            &self.interface,
        ])?;
        for line in text.lines() {
            let f = crate::provision::terse_fields(line);
            if f.len() == 3 && f[0] == "*" {
                return Ok(WifiOutput::nmcli_radio(&f[1], &f[2]));
            }
        }
        Err(std::io::Error::other("Active AP radio details unavailable"))
    }
    pub fn associated_stations(&self) -> Result<Vec<String>> {
        associated_stations(self.runner.as_ref(), &self.interface)
    }
    pub fn radio_info(&self) -> Result<Radio> {
        self.radio(&|args| command(self.runner.as_ref(), args, Duration::from_secs(5)))
    }
    pub fn validate_ready(&self) -> Result<()> {
        let (expected, prefix, connection, profile_channel) = {
            let state = self.state.lock().unwrap();
            (
                state
                    .ready
                    .clone()
                    .ok_or_else(|| std::io::Error::other("Hotspot is not ready"))?,
                state.ready_prefix,
                state
                    .connection
                    .clone()
                    .ok_or_else(|| std::io::Error::other("Hotspot profile is unavailable"))?,
                state.profile_channel,
            )
        };
        let now = WifiOutput::ipv4_networks(&command(
            self.runner.as_ref(),
            &[
                "nmcli",
                "-g",
                "IP4.ADDRESS",
                "device",
                "show",
                &self.interface,
            ],
            Duration::from_secs(5),
        )?);
        // NetworkManager can report the live Tesla /32 first. Validate the AP's own
        // address and prefix, independent of auxiliary addresses and their ordering.
        if !now
            .iter()
            .any(|(ip, p)| Some(*ip) == expected.host_address && *p == prefix)
        {
            return Err(std::io::Error::other("Hotspot address changed"));
        }
        let active = command(
            self.runner.as_ref(),
            &[
                "nmcli",
                "-g",
                "GENERAL.CONNECTION",
                "device",
                "show",
                &self.interface,
            ],
            Duration::from_secs(5),
        )?;
        if crate::provision::terse_fields(active.trim_end()).join(":") != connection {
            return Err(std::io::Error::other("Hotspot connection changed"));
        }
        let profile = command(
            self.runner.as_ref(),
            &[
                "nmcli",
                "-g",
                "802-11-wireless.ssid,802-11-wireless.channel,802-11-wireless-security.key-mgmt",
                "connection",
                "show",
                "id",
                &connection,
            ],
            Duration::from_secs(5),
        )?;
        let rows: Vec<_> = profile.lines().collect();
        let key_matches = match expected.security {
            Iap2WirelessSecurity::None => matches!(rows.get(2).copied(), None | Some("" | "--")),
            Iap2WirelessSecurity::Wpa3Only => rows.get(2) == Some(&"sae"),
            _ => rows.get(2) == Some(&"wpa-psk"),
        };
        if !(2..=3).contains(&rows.len())
            || crate::provision::terse_fields(rows[0]).join(":") != expected.ssid
            || rows[1].parse::<u8>().ok() != Some(profile_channel)
            || !key_matches
        {
            return Err(std::io::Error::other("Hotspot configuration changed"));
        }
        let radio = self.radio_info()?;
        if radio.channel != Some(expected.channel) {
            return Err(std::io::Error::other("Hotspot channel changed"));
        }
        Ok(())
    }
    pub fn on_car_play_confirmed(&self) -> Result<()> {
        Ok(())
    }
    fn cleanup(&self) -> Result<()> {
        let id = self.state.lock().unwrap().connection.clone();
        if let Some(id) = id {
            let _ = command(
                self.runner.as_ref(),
                &["nmcli", "connection", "down", "id", &id],
                Duration::from_secs(10),
            );
            let deleted = command(
                self.runner.as_ref(),
                &["nmcli", "connection", "delete", "id", &id],
                Duration::from_secs(10),
            );
            let _ = std::fs::remove_file(self.keyfile_dir.join(format!("{id}.nmconnection")));
            if self.state.lock().unwrap().profile_loaded {
                deleted?;
            }
            let mut state = self.state.lock().unwrap();
            state.connection = None;
            state.profile_loaded = false;
            state.ready = None;
            crate::public_name::local_dns::hotspot(&self.dns, &self.interface, None)?;
        }
        Ok(())
    }
    pub fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        self.cleanup()
    }
}
impl WirelessHotspotManager for LinuxHotspot {
    fn start(&self, t: Duration) -> Result<WirelessHotspotInfo> {
        self.start(t)
    }
    fn validate_ready(&self) -> Result<()> {
        self.validate_ready()
    }
    fn on_car_play_confirmed(&self) -> Result<()> {
        self.on_car_play_confirmed()
    }
    fn close(&self) -> Result<()> {
        self.close()
    }
}
pub struct ExistingWifiManager {
    pub runner: Arc<dyn CommandRunner>,
    pub interface: String,
    pub ssid: String,
    pub password: String,
    addresses: crate::runtime_paths::InterfaceAddresses,
    ready: Mutex<Option<WirelessHotspotInfo>>,
    closed: AtomicBool,
}
impl ExistingWifiManager {
    pub fn new(
        runner: Arc<dyn CommandRunner>,
        interface: String,
        ssid: String,
        password: String,
    ) -> Self {
        Self {
            runner,
            interface,
            ssid,
            password,
            addresses: Arc::new(interface_addresses),
            ready: Mutex::new(None),
            closed: AtomicBool::new(false),
        }
    }
    pub fn with_addresses(mut self, addresses: crate::runtime_paths::InterfaceAddresses) -> Self {
        self.addresses = addresses;
        self
    }
}
impl ExistingWifiManager {
    fn snapshot(&self) -> Result<WirelessHotspotInfo> {
        if self.closed.load(Ordering::Acquire) {
            return Err(std::io::Error::other(
                "Existing Wi-Fi attachment was cancelled",
            ));
        }
        let c = |a: &[&str]| command(self.runner.as_ref(), a, Duration::from_secs(5));
        let rows = c(&[
            "nmcli",
            "-t",
            "-f",
            "IN-USE,SSID,CHAN,BSSID,SECURITY",
            "device",
            "wifi",
            "list",
            "--rescan",
            "no",
            "ifname",
            &self.interface,
        ])?;
        let fields = rows
            .lines()
            .map(crate::provision::terse_fields)
            .find(|f| f.len() == 5 && f[0] == "*")
            .ok_or_else(|| std::io::Error::other("Existing Wi-Fi is not connected"))?;
        if fields[1] != self.ssid {
            return Err(std::io::Error::other(
                "Connected Wi-Fi does not match configured SSID",
            ));
        }
        let addr = (self.addresses)()?
            .into_iter()
            .find(|(i, a, _)| i == &self.interface && !a.is_loopback())
            .map(|(_, a, _)| a)
            .ok_or_else(|| std::io::Error::other("Existing Wi-Fi has no IPv4 address"))?;
        let security = existing_wifi_security(&fields[4], &self.password)?;
        // AirPlay deviceid belongs to this receiver, never the router.
        let local_mac =
            std::fs::read_to_string(format!("/sys/class/net/{}/address", self.interface))?;
        let info = WirelessHotspotInfo {
            ssid: self.ssid.clone(),
            passphrase: self.password.clone(),
            channel: fields[2].parse().map_err(std::io::Error::other)?,
            security,
            host_address: Some(addr),
            bssid: Some(local_mac.trim().to_ascii_uppercase()),
            access_point_bssid: Some(fields[3].clone()),
            interface_name: Some(self.interface.clone()),
        };
        Ok(info)
    }
}
impl WirelessHotspotManager for ExistingWifiManager {
    fn start(&self, timeout: Duration) -> Result<WirelessHotspotInfo> {
        let started = Instant::now();
        loop {
            match self.snapshot() {
                Ok(info) => {
                    *self.ready.lock().unwrap() = Some(info.clone());
                    return Ok(info);
                }
                Err(e)
                    if matches!(
                        e.to_string().as_str(),
                        "Existing Wi-Fi is not connected" | "Existing Wi-Fi has no IPv4 address"
                    ) =>
                {
                    if started.elapsed() >= timeout {
                        return Err(std::io::Error::other("Existing Wi-Fi is not connected or has no usable address. Connect both devices to the same Wi-Fi in system settings"));
                    }
                    std::thread::sleep(
                        Duration::from_millis(200).min(timeout.saturating_sub(started.elapsed())),
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }
    fn validate_ready(&self) -> Result<()> {
        let before = self.ready.lock().unwrap().clone();
        let now = self.snapshot()?;
        if before.as_ref().is_some_and(|b| {
            b.host_address != now.host_address
                || b.access_point_bssid != now.access_point_bssid
                || b.ssid != now.ssid
                || b.channel != now.channel
                || b.security != now.security
        }) {
            return Err(std::io::Error::other("Existing Wi-Fi changed"));
        }
        Ok(())
    }
    fn on_car_play_confirmed(&self) -> Result<()> {
        Ok(())
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}

pub fn existing_wifi_security(observed: &str, password: &str) -> Result<Iap2WirelessSecurity> {
    if observed.is_empty() || observed == "--" {
        if !password.is_empty() {
            return Err(std::io::Error::other(
                "Connected Wi-Fi is open; clear the saved Wi-Fi password",
            ));
        }
        return Ok(Iap2WirelessSecurity::None);
    }
    if observed.contains("802.1X")
        || observed.contains("EAP")
        || observed.contains("WEP")
        || !observed.contains("WPA")
    {
        return Err(std::io::Error::other(
            "Existing Wi-Fi requires an open or WPA2-Personal network",
        ));
    }
    if password.is_empty() {
        return Err(std::io::Error::other(
            "Connected Wi-Fi is secured; enter its WPA2 password",
        ));
    }
    if !(8..=63).contains(&password.len()) {
        return Err(std::io::Error::other("Existing Wi-Fi password unavailable"));
    }
    Ok(Iap2WirelessSecurity::WpaWpa2)
}

#[cfg(test)]
mod credential_tests {
    use super::*;
    #[test]
    fn security_keyfiles_and_iap2_values_match_the_selected_mode() {
        for (value, key, pmf, wire) in [
            ("wpa2", "wpa-psk", 1, Iap2WirelessSecurity::WpaWpa2),
            (
                "wpa3-transition",
                "wpa-psk",
                2,
                Iap2WirelessSecurity::Wpa3Transition,
            ),
            ("wpa3", "sae", 3, Iap2WirelessSecurity::Wpa3Only),
        ] {
            let profile = ApKeyfile {
                id: "test",
                interface: "wlan0",
                mac: None,
                ssid: "Relay",
                password: "a password",
                security: value,
                band: "bg",
                channel: "6",
                subnet: "10.78.0.1/24",
            }
            .text()
            .unwrap();
            assert!(profile.contains(&format!("key-mgmt={key}\n")));
            assert!(profile.contains(&format!("pmf={pmf}\n")));
            assert!(profile.contains("psk=a\\spassword\n"));
            assert_eq!(wireless_security(value), wire);
        }
        assert_eq!(keyfile_security("open", "retained-password").unwrap(), "");
        assert_eq!(wireless_security("open"), Iap2WirelessSecurity::None);
        assert!(keyfile_security("unknown", "password").is_err());
    }
    #[test]
    fn sae_capability_requires_ap_pmf_and_supplicant_support() {
        let phy =
            "Supported interface modes:\n\t * AP\nSupported Ciphers:\n\t * CMAC (00-0f-ac:6)\n";
        assert_eq!(
            hotspot_security_capabilities(phy, "WPA-PSK SAE"),
            HOTSPOT_SECURITY
        );
        for (radio, keys) in [
            (phy, "WPA-PSK"),
            ("", "SAE"),
            ("Supported interface modes:\n * AP\n", "SAE"),
        ] {
            assert_eq!(hotspot_security_capabilities(radio, keys), ["open", "wpa2"]);
        }
        assert!(!security_supported("wpa3", &[]));
    }

    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    #[test]
    fn fresh_credentials_use_default_password_and_existing_files_are_unchanged() {
        let dir = std::env::temp_dir().join(format!(
            "hotspot-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let (ssid, password) = hotspot_credentials(&dir).unwrap();
        let suffix = ssid.strip_prefix("OpenXPlay-Linux-").unwrap();
        assert_eq!(suffix.len(), 4);
        assert!(suffix
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()));
        assert_eq!(password, "openxplay");
        assert_eq!(hotspot_credentials(&dir).unwrap(), (ssid, password));
        let path = dir.join("hotspot.credentials");
        let legacy = b"Old-Hotspot-12345678\nlong-legacy-password-1234567890\n";
        std::fs::write(&path, legacy).unwrap();
        assert_eq!(
            hotspot_credentials(&dir).unwrap(),
            (
                "Old-Hotspot-12345678".into(),
                "long-legacy-password-1234567890".into()
            )
        );
        assert_eq!(std::fs::read(&path).unwrap(), legacy);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn replacement_is_atomic_private_and_supports_utf8_names() {
        let dir = std::env::temp_dir().join(format!(
            "hotspot-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        hotspot_credentials(&dir).unwrap();
        let path = dir.join("hotspot.credentials");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let old = std::fs::File::open(&path).unwrap();
        save_hotspot_credentials(&dir, "中继\n车机", "new password").unwrap();
        assert_eq!(
            hotspot_credentials(&dir).unwrap(),
            ("中继\n车机".into(), "new password".into())
        );
        assert_ne!(
            old.metadata().unwrap().ino(),
            std::fs::metadata(&path).unwrap().ino()
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let saved = std::fs::read(&path).unwrap();
        assert!(save_hotspot_credentials(&dir, "bad", "short").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn validates_bytes_and_printable_ascii_boundaries() {
        for ssid in ["".into(), "a".repeat(33), "中".repeat(11)] {
            assert_eq!(
                validate_hotspot_credentials(&ssid, "12345678"),
                Err("error.hotspotSsid")
            );
        }
        for password in [
            "a".repeat(7),
            "a".repeat(64),
            "1234567\n".into(),
            "1234567é".into(),
            "1234567\u{7f}".into(),
        ] {
            assert_eq!(
                validate_hotspot_credentials("Relay", &password),
                Err("error.hotspotPassword")
            );
        }
        assert!(validate_hotspot_credentials(&"中".repeat(10), &" ".repeat(8)).is_ok());
        assert!(validate_hotspot_credentials(&"a".repeat(32), &"~".repeat(63)).is_ok());
    }
}
