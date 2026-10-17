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
    rows.lines()
        .filter_map(|line| {
            let fields = crate::provision::terse_fields(line);
            (fields.len() == 3 && fields[0].starts_with(HOTSPOT_CONNECTION_PREFIX))
                .then(|| (fields[1].clone(), fields[2] == "yes"))
        })
        .collect()
}
/// Deletes the AP connections a previous run left in NetworkManager because it was killed
/// before its teardown. Only one relay holds the state directory, so before this process brings
/// its first AP up every such connection is stale. Returns whether one of them was active, that
/// is its AP was up until now; inactive leftovers are deleted on a best-effort basis.
pub fn remove_stale_hotspots(runner: &dyn CommandRunner) -> Result<bool> {
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
    for (uuid, active) in hotspot_connections(&rows) {
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
pub fn supports_5ghz(runner: &dyn CommandRunner, interface: &str) -> bool {
    let Ok(phy) = std::fs::read_to_string(format!("/sys/class/net/{interface}/phy80211/name"))
    else {
        return false;
    };
    command(
        runner,
        &["iw", "phy", phy.trim(), "info"],
        Duration::from_secs(3),
    )
    .is_ok_and(|info| lists_5ghz(&info))
}
/// Parse `iw phy <phy> info` frequency lines such as `* 5180 MHz [36] (20.0 dBm)`.
pub fn lists_5ghz(iw_phy_info: &str) -> bool {
    iw_phy_info.lines().any(|line| {
        let Some(rest) = line.trim().strip_prefix("* ") else {
            return false;
        };
        let mhz = rest
            .split(" MHz")
            .next()
            .and_then(|f| f.trim().parse::<f64>().ok());
        mhz.is_some_and(|f| (4900.0..5900.0).contains(&f)) && !rest.contains("disabled")
    })
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
            let password = random_characters(10, b"abcdefghijklmnopqrstuvwxyz0123456789")?;
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
/// Choose a supported band before asking NetworkManager to create the AP.
pub fn hotspot_channel(configured: u8, supports_5ghz: bool) -> u8 {
    if configured > 14 && !supports_5ghz {
        6
    } else {
        configured
    }
}
struct HotspotState {
    connection: Option<String>,
    ready: Option<WirelessHotspotInfo>,
}
pub struct LinuxHotspot {
    state_dir: PathBuf,
    runner: Arc<dyn CommandRunner>,
    interface: String,
    channel: u8,
    state: Mutex<HotspotState>,
    closed: AtomicBool,
    provisioning: Option<Arc<crate::provision::ProvisioningGate>>,
}
impl LinuxHotspot {
    pub fn new(
        state_dir: PathBuf,
        runner: Arc<dyn CommandRunner>,
        interface: String,
        channel: u8,
    ) -> Self {
        Self {
            state_dir,
            runner,
            interface,
            channel,
            state: Mutex::new(HotspotState {
                connection: None,
                ready: None,
            }),
            closed: AtomicBool::new(false),
            provisioning: None,
        }
    }
    /// The relay's Wi-Fi provisioning, which the AP's activation waits for: at startup it takes
    /// the saved client connections on the interface down.
    pub fn with_provisioning(mut self, gate: Arc<crate::provision::ProvisioningGate>) -> Self {
        self.provisioning = Some(gate);
        self
    }
    pub fn start(&self, timeout: Duration) -> Result<WirelessHotspotInfo> {
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
        let (ssid, password) = hotspot_credentials(&self.state_dir)?;
        let occupied = interface_addresses()?
            .iter()
            .map(|(_, a, p)| format!("{a}/{p}"))
            .collect::<Vec<_>>();
        let subnet = WifiOutput::free_hotspot_address(&occupied)?;
        let id = format!(
            "{HOTSPOT_CONNECTION_PREFIX}{}",
            crate::admin::auth::random_hex(16)?
        );
        self.state.lock().unwrap().connection = Some(id.clone());
        let result = (|| {
            let selected = hotspot_channel(
                self.channel,
                supports_5ghz(self.runner.as_ref(), &self.interface),
            );
            static FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);
            if selected != self.channel && !FALLBACK_LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!("Wi-Fi radio has no 5 GHz support; using hotspot channel 6");
            }
            let channel = selected.to_string();
            let band = if selected > 14 { "a" } else { "bg" };
            cmd(&[
                "nmcli",
                "connection",
                "add",
                "type",
                "wifi",
                "ifname",
                &self.interface,
                "con-name",
                &id,
                "ssid",
                &ssid,
                "connection.autoconnect",
                "no",
                "802-11-wireless.mode",
                "ap",
                "802-11-wireless.band",
                band,
                "802-11-wireless.channel",
                &channel,
                "802-11-wireless-security.key-mgmt",
                "wpa-psk",
                "802-11-wireless-security.proto",
                "rsn",
                "802-11-wireless-security.pairwise",
                "ccmp",
                "802-11-wireless-security.group",
                "ccmp",
                "802-11-wireless-security.psk",
                &password,
                "ipv4.method",
                "shared",
                "ipv4.addresses",
                &subnet,
                "ipv6.method",
                "disabled",
            ])?;
            self.wait_for_provisioning(deadline);
            self.wait_for_device(deadline);
            if cmd(&["nmcli", "connection", "up", "id", &id]).is_err() {
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
            }
            let address = loop {
                let output = cmd(&[
                    "nmcli",
                    "-g",
                    "IP4.ADDRESS",
                    "device",
                    "show",
                    &self.interface,
                ])?;
                if let Some(a) = WifiOutput::ipv4(&output) {
                    break a;
                }
                std::thread::sleep(Duration::from_millis(250));
            };
            let radio = self.radio(&cmd)?;
            let bssid = radio
                .bssid
                .ok_or_else(|| std::io::Error::other("AP BSSID unavailable"))?;
            let info = WirelessHotspotInfo {
                ssid,
                passphrase: password,
                channel: radio
                    .channel
                    .ok_or_else(|| std::io::Error::other("AP channel unavailable"))?,
                security: carplay_iap2::Iap2WirelessSecurity::WpaWpa2,
                host_address: Some(address),
                bssid: Some(bssid.clone()),
                access_point_bssid: Some(bssid),
                interface_name: Some(self.interface.clone()),
            };
            self.state.lock().unwrap().ready = Some(info.clone());
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
    pub fn validate_ready(&self) -> Result<()> {
        let expected = self
            .state
            .lock()
            .unwrap()
            .ready
            .as_ref()
            .and_then(|r| r.host_address)
            .ok_or_else(|| std::io::Error::other("Hotspot is not ready"))?;
        let now = WifiOutput::ipv4(&command(
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
        if now != Some(expected) {
            return Err(std::io::Error::other("Hotspot address changed"));
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
            command(
                self.runner.as_ref(),
                &["nmcli", "connection", "delete", "id", &id],
                Duration::from_secs(10),
            )?;
            let mut state = self.state.lock().unwrap();
            state.connection = None;
            state.ready = None;
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
            ready: Mutex::new(None),
            closed: AtomicBool::new(false),
        }
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
        let addr = interface_addresses()?
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
            b.host_address != now.host_address || b.access_point_bssid != now.access_point_bssid
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
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    #[test]
    fn fresh_credentials_are_short_and_existing_files_are_unchanged() {
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
        assert_eq!(password.len(), 10);
        assert!(password
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
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
