// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

use crate::{
    platform::{command, spawn_worker, CommandRunner},
    wifi_capabilities::{parse_phy, PhyCapabilities},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(3);
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiCapabilities {
    pub interface: String,
    pub present: Option<bool>,
    pub phy: Option<String>,
    pub capabilities: Option<PhyCapabilities>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothAdapter {
    pub name: String,
    pub address: String,
    pub powered: Option<bool>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothFeatures {
    pub br_edr: bool,
    pub le: bool,
    pub secure_connections: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub present: Option<bool>,
    pub version: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCapabilities {
    pub wifi: WifiCapabilities,
    pub carlink: WifiCapabilities,
    pub regulatory: BTreeMap<String, String>,
    pub adapters: Option<Vec<BluetoothAdapter>>,
    pub selected_adapter: Option<String>,
    pub bluetooth: Option<BluetoothFeatures>,
    pub rfcomm: Option<bool>,
    pub tools: BTreeMap<String, Tool>,
}
/// An unavailable probe stays unknown; only the kernel's explicit unsupported errors mean no.
pub fn rfcomm_support(probe: &dyn Fn() -> std::io::Result<()>) -> Option<bool> {
    match probe() {
        Ok(()) => Some(true),
        Err(e)
            if matches!(
                e.raw_os_error(),
                Some(libc::EAFNOSUPPORT | libc::EPROTONOSUPPORT)
            ) =>
        {
            Some(false)
        }
        Err(_) => None,
    }
}
fn rfcomm_socket() -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let fd = unsafe {
            libc::socket(
                libc::AF_BLUETOOTH,
                libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
                3,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        unsafe {
            libc::close(fd);
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err(std::io::Error::from_raw_os_error(libc::EAFNOSUPPORT))
}
pub fn parse_btmgmt(text: &str) -> Option<BluetoothFeatures> {
    let settings = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("supported settings:"))?;
    let words = settings.split_whitespace().collect::<Vec<_>>();
    Some(BluetoothFeatures {
        br_edr: words.contains(&"br/edr"),
        le: words.contains(&"le"),
        secure_connections: words.contains(&"secure-conn"),
    })
}
pub fn parse_regulatory(text: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut scope = "global".to_string();
    for line in text.lines().map(str::trim) {
        if line == "global" {
            scope = "global".into();
        }
        if let Some(tail) = line.strip_prefix("phy#") {
            scope = format!("phy{}", tail.split_whitespace().next().unwrap_or(""));
        }
        if let Some(country) = line
            .strip_prefix("country ")
            .and_then(|s| s.split_once(':').map(|(s, _)| s))
        {
            result.insert(scope.clone(), country.into());
        }
    }
    result
}
fn run(runner: &dyn CommandRunner, args: &[&str]) -> Option<String> {
    command(runner, args, TIMEOUT).ok()
}
pub fn probe_wifi(runner: &dyn CommandRunner, root: &Path, interface: &str) -> WifiCapabilities {
    let present = match std::fs::metadata(root.join("sys/class/net").join(interface)) {
        Ok(_) => Some(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    };
    let phy = run(runner, &["iw", "dev", interface, "info"]).and_then(|s| {
        s.lines().find_map(|l| {
            l.trim()
                .strip_prefix("wiphy ")
                .and_then(|s| s.parse::<u16>().ok())
                .map(|n| format!("phy{n}"))
        })
    });
    let capabilities = phy
        .as_ref()
        .and_then(|p| run(runner, &["iw", "phy", p, "info"]))
        .map(|s| parse_phy(&s))
        .filter(|p| !p.channels.is_empty() || !p.modes.is_empty() || !p.combinations.is_empty());
    WifiCapabilities {
        interface: interface.into(),
        present,
        phy,
        capabilities,
    }
}
pub fn detect_tools(
    runner: &dyn CommandRunner,
    root: &Path,
    path: &std::ffi::OsStr,
) -> BTreeMap<String, Tool> {
    use std::os::unix::fs::PermissionsExt;
    [
        "nmcli",
        "iw",
        "dnsmasq",
        "sdptool",
        "hciconfig",
        "btmgmt",
        "bluetoothctl",
    ]
    .into_iter()
    .map(|name| {
        let mut present = Some(false);
        for dir in std::env::split_paths(path) {
            let file = root.join(dir.strip_prefix("/").unwrap_or(&dir)).join(name);
            match std::fs::metadata(file) {
                Ok(m) if m.is_file() && m.permissions().mode() & 0o111 != 0 => {
                    present = Some(true);
                    break;
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) => {}
                Err(_) => {
                    present = None;
                }
            }
        }
        let version = (present == Some(true) && name == "nmcli")
            .then(|| run(runner, &[name, "--version"]))
            .flatten()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().into());
        (name.into(), Tool { present, version })
    })
    .collect()
}
pub fn probe(
    runner: &dyn CommandRunner,
    root: &Path,
    wifi: &str,
    carlink: &str,
    selection: &crate::linux_bluetooth::AdapterSelection,
    socket: &dyn Fn() -> std::io::Result<()>,
) -> HostCapabilities {
    let wifi = probe_wifi(runner, root, wifi);
    let carlink = if carlink == wifi.interface {
        wifi.clone()
    } else {
        probe_wifi(runner, root, carlink)
    };
    // bluetoothctl talks to BlueZ; hci names come from its addresses in sysfs.
    let adapters = run(runner, &["bluetoothctl", "list"]).map(|list| {
        list.lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                if words.next()? != "Controller" {
                    return None;
                }
                let address = words.next()?.to_string();
                let alias = words
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim_end_matches(" [default]")
                    .to_string();
                let name = std::fs::read_dir(root.join("sys/class/bluetooth"))
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .find_map(|e| {
                        let a = std::fs::read_to_string(e.path().join("address")).ok()?;
                        a.trim()
                            .eq_ignore_ascii_case(&address)
                            .then(|| e.file_name().to_string_lossy().into_owned())
                    })
                    .unwrap_or(alias);
                let powered = run(runner, &["bluetoothctl", "show", &address]).and_then(|s| {
                    s.lines().find_map(|l| {
                        l.trim().strip_prefix("Powered: ").and_then(|s| match s {
                            "yes" => Some(true),
                            "no" => Some(false),
                            _ => None,
                        })
                    })
                });
                Some(BluetoothAdapter {
                    name,
                    address,
                    powered,
                })
            })
            .collect::<Vec<_>>()
    });
    let selected_adapter = adapters.as_ref().and_then(|adapters| {
        adapters
            .iter()
            .filter(|a| {
                crate::host_profile::adapter_name_index(&a.name).is_some()
                    && selection.0.as_ref().is_none_or(|wanted| {
                        wanted.eq_ignore_ascii_case(&a.name)
                            || wanted.eq_ignore_ascii_case(&a.address)
                    })
            })
            .min_by_key(|a| crate::host_profile::adapter_name_index(&a.name))
            .map(|a| a.name.clone())
    });
    let bluetooth = selected_adapter
        .as_deref()
        .and_then(crate::host_profile::adapter_name_index)
        .and_then(|n| {
            command(
                runner,
                &crate::secure_connections::info_command(n)
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                TIMEOUT,
            )
            .ok()
        })
        .and_then(|s| parse_btmgmt(&s));
    HostCapabilities {
        wifi,
        carlink,
        regulatory: run(runner, &["iw", "reg", "get"])
            .map(|s| parse_regulatory(&s))
            .unwrap_or_default(),
        adapters,
        selected_adapter,
        bluetooth,
        rfcomm: rfcomm_support(socket),
        tools: detect_tools(runner, root, &std::env::var_os("PATH").unwrap_or_default()),
    }
}
#[derive(Default)]
struct CacheState {
    started: bool,
    running: bool,
    requested: Option<(String, String)>,
    result: Option<HostCapabilities>,
    fingerprint: String,
}
/// Starts only when a hotspot start is initiated. Later interface changes queue a fresh snapshot.
pub struct CapabilityCache {
    state: Mutex<CacheState>,
    runner: Arc<dyn CommandRunner>,
    root: PathBuf,
    selection: crate::linux_bluetooth::AdapterSelection,
}
impl CapabilityCache {
    pub fn new(
        runner: Arc<dyn CommandRunner>,
        root: PathBuf,
        selection: crate::linux_bluetooth::AdapterSelection,
    ) -> Self {
        Self {
            state: Mutex::default(),
            runner,
            root,
            selection,
        }
    }
    pub fn is_started(&self) -> bool {
        self.state.lock().unwrap().started
    }
    pub fn get(&self) -> Option<HostCapabilities> {
        self.state.lock().unwrap().result.clone()
    }
    pub fn initiated(self: &Arc<Self>, wifi: &str, carlink: &str) {
        self.state.lock().unwrap().started = true;
        self.refresh(wifi, carlink);
    }
    pub fn refresh(self: &Arc<Self>, wifi: &str, carlink: &str) {
        if !self.is_started() {
            return;
        }
        let fingerprint = [wifi, carlink]
            .iter()
            .map(|name| {
                let path = self.root.join("sys/class/net").join(name);
                format!(
                    "{name}:{:?}:{:?}:{:?}:{:?}",
                    std::fs::read_to_string(path.join("address")),
                    std::fs::read_link(path.join("phy80211")),
                    std::fs::read_to_string(path.join("operstate")),
                    std::fs::metadata(path).map(|m| {
                        use std::os::unix::fs::MetadataExt;
                        m.ino()
                    })
                )
            })
            .collect::<String>();
        let mut state = self.state.lock().unwrap();
        if !state.started || state.fingerprint == fingerprint {
            return;
        }
        state.fingerprint = fingerprint;
        state.requested = Some((wifi.into(), carlink.into()));
        if state.running {
            return;
        }
        state.running = true;
        drop(state);
        let cache = self.clone();
        if spawn_worker(
            "host-capabilities",
            Box::new(move || loop {
                let request = cache.state.lock().unwrap().requested.take();
                let Some((wifi, carlink)) = request else {
                    let mut state = cache.state.lock().unwrap();
                    if state.requested.is_some() {
                        continue;
                    }
                    state.running = false;
                    break;
                };
                let result = probe(
                    cache.runner.as_ref(),
                    &cache.root,
                    &wifi,
                    &carlink,
                    &cache.selection,
                    &rfcomm_socket,
                );
                cache.state.lock().unwrap().result = Some(result);
            }),
        )
        .is_err()
        {
            let mut state = self.state.lock().unwrap();
            state.running = false;
            state.fingerprint.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommandResult;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Fake(AtomicUsize);
    impl CommandRunner for Fake {
        fn run(&self, args: &[String], timeout: Duration) -> std::io::Result<CommandResult> {
            assert!(timeout <= TIMEOUT);
            self.0.fetch_add(1, Ordering::SeqCst);
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            let output = match args.as_slice() {
                ["iw", "dev", "wlan0", "info"] => "Interface wlan0\n wiphy 0",
                ["iw", "dev", "wlan1", "info"] => "Interface wlan1\n wiphy 1",
                ["iw", "phy", "phy0", "info"] => "Supported interface modes:\n * AP\n * managed\nBand 1:\n * 2437 MHz [6] (20.0 dBm)",
                ["iw", "phy", "phy1", "info"] => " * 5180 MHz [36] (no IR)",
                ["iw", "reg", "get"] => "global\ncountry 00: DFS-UNSET\nphy#1 (self-managed)\ncountry US: DFS-FCC",
                ["bluetoothctl", "list"] => "Controller AA:BB:CC:DD:EE:FF Relay [default]",
                ["bluetoothctl", "show", _] => " Powered: yes",
                ["sh", "-c", "exec btmgmt --index 1 info <&1"] => "hci1: Primary controller\n supported settings: powered br/edr le secure-conn\n current settings: powered le",
                ["nmcli", "--version"] => "nmcli tool, version 1.30.0\n",
                _ => return Err(std::io::Error::other("unavailable")),
            };
            Ok(CommandResult {
                exit_code: 0,
                output: output.into(),
            })
        }
    }
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "capabilities-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        for dir in ["sys/class/net/wlan0", "sys/class/bluetooth/hci1", "usr/bin"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        std::fs::write(
            root.join("sys/class/bluetooth/hci1/address"),
            "AA:BB:CC:DD:EE:FF\n",
        )
        .unwrap();
        root
    }
    #[test]
    fn regulatory_features_and_rfcomm_unknowns() {
        let reg = parse_regulatory("global\ncountry 00: DFS-UNSET\nphy#0 (self-managed)\ncountry CN: DFS-FCC\nphy#2\ncountry US: DFS-FCC\n");
        assert_eq!(reg["global"], "00");
        assert_eq!(reg["phy0"], "CN");
        assert_eq!(reg["phy2"], "US");
        let features = parse_btmgmt(
            "supported settings: powered br/edr le secure-conn\ncurrent settings: powered\n",
        )
        .unwrap();
        assert!(features.br_edr && features.le && features.secure_connections);
        assert!(
            !parse_btmgmt("supported settings: powered br/edr\n")
                .unwrap()
                .le
        );
        assert!(parse_btmgmt("").is_none());
        assert_eq!(rfcomm_support(&|| Ok(())), Some(true));
        for errno in [libc::EAFNOSUPPORT, libc::EPROTONOSUPPORT] {
            assert_eq!(
                rfcomm_support(&|| Err(std::io::Error::from_raw_os_error(errno))),
                Some(false)
            );
        }
        assert_eq!(
            rfcomm_support(&|| Err(std::io::Error::from_raw_os_error(libc::EPERM))),
            None
        );
    }
    #[test]
    fn bounded_probes_separate_radios_and_missing_tools() {
        use std::os::unix::fs::PermissionsExt;
        let root = root();
        let runner = Fake(AtomicUsize::new(0));
        let nmcli = root.join("usr/bin/nmcli");
        std::fs::write(&nmcli, "fake").unwrap();
        std::fs::set_permissions(&nmcli, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(root.join("usr/bin/iw"), "not executable").unwrap();
        let tools = detect_tools(&runner, &root, std::ffi::OsStr::new("/usr/bin"));
        assert_eq!(tools["nmcli"].present, Some(true));
        assert_eq!(
            tools["nmcli"].version.as_deref(),
            Some("nmcli tool, version 1.30.0")
        );
        assert_eq!(tools["iw"].present, Some(false));
        assert_eq!(tools["dnsmasq"].present, Some(false));
        let snapshot = probe(
            &runner,
            &root,
            "wlan0",
            "wlan1",
            &Default::default(),
            &|| Ok(()),
        );
        assert_eq!(snapshot.wifi.present, Some(true));
        assert_eq!(snapshot.carlink.present, Some(false));
        assert!(!snapshot.wifi.capabilities.unwrap().station_5ghz());
        let station = snapshot.carlink.capabilities.unwrap();
        assert!(station.station_5ghz() && !station.ap_channel(36));
        assert_eq!(snapshot.selected_adapter.as_deref(), Some("hci1"));
        assert!(snapshot.bluetooth.unwrap().secure_connections);
        assert_eq!(snapshot.adapters.unwrap()[0].powered, Some(true));
        assert_eq!(snapshot.regulatory["phy1"], "US");
        let unknown = probe_wifi(&runner, &root, "missing");
        assert!(unknown.phy.is_none() && unknown.capabilities.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cache_waits_for_hotspot_and_refreshes_on_interface_changes() {
        let root = root();
        let runner = Arc::new(Fake(AtomicUsize::new(0)));
        let cache = Arc::new(CapabilityCache::new(
            runner.clone(),
            root.clone(),
            Default::default(),
        ));
        cache.refresh("wlan0", "wlan1");
        assert_eq!(runner.0.load(Ordering::SeqCst), 0);
        assert!(cache.get().is_none());
        cache.initiated("wlan0", "wlan1");
        let wait = || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while cache.state.lock().unwrap().running {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        wait();
        assert_eq!(cache.get().unwrap().carlink.present, Some(false));
        let count = runner.0.load(Ordering::SeqCst);
        cache.refresh("wlan0", "wlan1");
        assert_eq!(runner.0.load(Ordering::SeqCst), count);
        std::fs::create_dir(root.join("sys/class/net/wlan1")).unwrap();
        cache.refresh("wlan0", "wlan1");
        wait();
        assert_eq!(cache.get().unwrap().carlink.present, Some(true));
        std::fs::remove_dir_all(root).unwrap();
    }
}
