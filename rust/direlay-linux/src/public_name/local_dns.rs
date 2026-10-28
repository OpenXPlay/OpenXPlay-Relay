// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! Every NM shared dnsmasq reads the same hosts file. `localise-queries` selects only
//! addresses on the requesting interface's subnet (unlike an `address=` override).
use std::{
    collections::BTreeMap,
    io::{self, Write},
    net::Ipv4Addr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
pub const DIR: &str = "/etc/NetworkManager/dnsmasq-shared.d";
pub const FILE: &str = "openxplay-public-name.conf";
pub const HOSTS: &str = "/run/openxplay/hotspot-hosts";
#[derive(Clone)]
pub struct Paths {
    pub dir: PathBuf,
    pub hosts: PathBuf,
    pub reload: bool,
}
impl Paths {
    pub fn in_dir(dir: PathBuf) -> Self {
        Self {
            hosts: dir.join("hotspot-hosts"),
            dir,
            reload: false,
        }
    }
}
#[derive(Default)]
struct Names {
    hotspots: BTreeMap<String, Ipv4Addr>,
    public: Option<(String, Option<Ipv4Addr>)>,
    address_override: Option<Ipv4Addr>,
}
static NAMES: LazyLock<Mutex<BTreeMap<PathBuf, Names>>> = LazyLock::new(Mutex::default);
pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}
pub fn text(hosts: &Path) -> String {
    format!("# openxplay-relay: subnet-local names on every relay hotspot.\naddn-hosts={}\nlocalise-queries\n", hosts.display())
}
fn hosts_text(names: &Names) -> String {
    let mut rows: BTreeMap<Ipv4Addr, Vec<&str>> = BTreeMap::new();
    for ip in names.hotspots.values() {
        let row = rows.entry(*ip).or_default();
        if !row.contains(&"openxplay-linux.local") {
            row.push("openxplay-linux.local");
        }
        if let Some((name, _)) = &names.public {
            if names.address_override.is_some() {
                continue;
            }
            if !row.contains(&name.as_str()) {
                row.push(name);
            }
        }
    }
    if let Some((name, _)) = &names.public {
        if let Some(ip) = names.address_override {
            rows.entry(ip).or_default().push(name);
        }
    }
    // Preserve the selected browser address when there is no AP yet (also shared-Wi-Fi mode).
    if names.hotspots.is_empty() && names.address_override.is_none() {
        if let Some((name, Some(ip))) = &names.public {
            rows.entry(*ip).or_default().push(name);
        }
    }
    rows.into_iter()
        .map(|(ip, names)| format!("{ip} {}\n", names.join(" ")))
        .collect()
}
fn atomic_write(target: &Path, text: &str) -> io::Result<bool> {
    if std::fs::read_to_string(target).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    let dir = target.parent().unwrap();
    if !dir.is_dir() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))?;
    }
    let tmp = target.with_extension("tmp");
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(tmp);
    }
    result.map(|()| true)
}
fn sync(paths: &Paths, names: &Names) -> io::Result<()> {
    let dir = &paths.dir;
    let hosts = &paths.hosts;
    // Install hosts first: a new dnsmasq must never see a missing addn-hosts file.
    let changed = atomic_write(hosts, &hosts_text(names))?;
    if names.hotspots.is_empty()
        && names.address_override.is_none()
        && names
            .public
            .as_ref()
            .is_none_or(|(_, address)| address.is_none())
    {
        match std::fs::remove_file(path(dir)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    } else {
        atomic_write(&path(dir), &text(hosts))?;
    }
    if changed && paths.reload {
        reload_shared_dnsmasq();
    }
    Ok(())
}
/// Reload only NM's shared dnsmasq instances; addn-hosts is reread on SIGHUP.
fn reload_shared_dnsmasq() {
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<i32>().ok())
            else {
                continue;
            };
            let Ok(args) = std::fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            let args: Vec<_> = args.split(|b| *b == 0).collect();
            let dnsmasq = args
                .first()
                .is_some_and(|arg| arg.ends_with(b"/dnsmasq") || *arg == b"dnsmasq");
            let shared = args
                .iter()
                .any(|arg| arg.starts_with(b"--conf-dir=/etc/NetworkManager/dnsmasq-shared.d"));
            if dnsmasq && shared {
                unsafe {
                    libc::kill(pid, libc::SIGHUP);
                }
            }
        }
    }
}
/// Set or remove an AP before activation / after teardown. Interface keys keep APs independent.
pub fn hotspot(paths: &Paths, interface: &str, address: Option<Ipv4Addr>) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    let names = all.entry(paths.dir.clone()).or_default();
    if let Some(address) = address {
        names.hotspots.insert(interface.into(), address);
    } else {
        names.hotspots.remove(interface);
    }
    sync(paths, names)
}
pub fn reset(paths: &Paths) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    all.insert(paths.dir.clone(), Names::default());
    sync(paths, &all[&paths.dir])
}
pub fn write(paths: &Paths, name: &str, address: Ipv4Addr) -> io::Result<()> {
    public_name(paths, name, Some(address))
}
pub fn public_name(paths: &Paths, name: &str, address: Option<Ipv4Addr>) -> io::Result<()> {
    public_name_override(paths, name, address, None)
}
/// Tesla name modes answer only with the established /32, regardless of the querying AP.
pub fn public_name_override(
    paths: &Paths,
    name: &str,
    address: Option<Ipv4Addr>,
    address_override: Option<Ipv4Addr>,
) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    let names = all.entry(paths.dir.clone()).or_default();
    names.public = Some((name.into(), address));
    names.address_override = address_override;
    sync(paths, names)
}
pub fn remove(paths: &Paths) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    let names = all.entry(paths.dir.clone()).or_default();
    names.public = None;
    names.address_override = None;
    sync(paths, names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::testing::TempDir;
    #[test]
    fn tesla_override_answers_only_with_the_established_address_on_every_hotspot() {
        let root = TempDir::new("tesla-dns");
        let dns = Paths::in_dir(root.0.join("dns"));
        hotspot(&dns, "wlan0", Some("10.77.0.1".parse().unwrap())).unwrap();
        hotspot(&dns, "wlan1", Some("10.79.0.1".parse().unwrap())).unwrap();
        for address in ["169.254.88.1", "100.88.0.1"] {
            public_name_override(
                &dns,
                "tesla.example.org",
                Some("10.77.0.1".parse().unwrap()),
                Some(address.parse().unwrap()),
            )
            .unwrap();
            let text = std::fs::read_to_string(&dns.hosts).unwrap();
            assert!(text.contains(&format!("{address} tesla.example.org\n")));
            assert_eq!(text.matches("tesla.example.org").count(), 1);
            assert!(text.contains("10.77.0.1 openxplay-linux.local\n"));
            assert!(text.contains("10.79.0.1 openxplay-linux.local\n"));
        }
        public_name_override(
            &dns,
            "tesla.example.org",
            Some("10.77.0.1".parse().unwrap()),
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&dns.hosts).unwrap(), "10.77.0.1 openxplay-linux.local tesla.example.org\n10.79.0.1 openxplay-linux.local tesla.example.org\n");
    }
    #[test]
    fn writes_one_address_line_and_removes_it() {
        let root = TempDir::new("local-dns");
        let dir = root.0.join("dnsmasq-shared.d");
        let dns = Paths {
            hosts: root.0.join("run/hotspot-hosts"),
            ..Paths::in_dir(dir.clone())
        };
        remove(&dns).unwrap();
        write(&dns, "myrelay.duckdns.org", Ipv4Addr::new(10, 77, 0, 1)).unwrap();
        let written = std::fs::read_to_string(path(&dir)).unwrap();
        assert_eq!(written, text(&dns.hosts));
        assert_eq!(
            std::fs::read_to_string(&dns.hosts).unwrap(),
            "10.77.0.1 myrelay.duckdns.org\n"
        );
        assert_eq!(written.lines().filter(|l| !l.starts_with('#')).count(), 2);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path(&dir)), 0o644);
        assert_eq!(mode(&dir), 0o755);
        write(&dns, "myrelay.duckdns.org", Ipv4Addr::new(10, 77, 1, 1)).unwrap();
        assert!(std::fs::read_to_string(&dns.hosts)
            .unwrap()
            .contains("10.77.1.1 myrelay.duckdns.org\n"));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary file left"
        );
        remove(&dns).unwrap();
        assert!(!path(&dir).exists());
    }
    #[test]
    fn every_hotspot_has_local_names_and_public_name_survives_on_admin_alone() {
        let root = TempDir::new("hotspot-dns");
        let dir = root.0.join("dnsmasq-shared.d");
        let dns = Paths {
            hosts: root.0.join("run/hotspot-hosts"),
            ..Paths::in_dir(dir.clone())
        };
        reset(&dns).unwrap();
        let admin = Ipv4Addr::new(10, 79, 0, 1);
        let carplay = Ipv4Addr::new(10, 77, 0, 1);
        hotspot(&dns, "wlan9", Some(admin)).unwrap();
        hotspot(&dns, "wlan8", Some(carplay)).unwrap();
        public_name(&dns, "synthetic.example.org", None).unwrap();
        let content = std::fs::read_to_string(&dns.hosts).unwrap();
        assert_eq!(content, "10.77.0.1 openxplay-linux.local synthetic.example.org\n10.79.0.1 openxplay-linux.local synthetic.example.org\n");
        let conf = std::fs::read_to_string(path(&dir)).unwrap();
        assert_eq!(conf, text(&dns.hosts));
        assert!(conf.contains("localise-queries\n"));
        assert!(!conf.contains("address="));
        hotspot(&dns, "wlan8", None).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dns.hosts).unwrap(),
            "10.79.0.1 openxplay-linux.local synthetic.example.org\n"
        );
        remove(&dns).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dns.hosts).unwrap(),
            "10.79.0.1 openxplay-linux.local\n"
        );
        hotspot(&dns, "wlan9", None).unwrap();
        assert!(std::fs::read_to_string(&dns.hosts).unwrap().is_empty());
    }
}
