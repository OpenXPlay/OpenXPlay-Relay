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
#[derive(Default)]
struct Names {
    hotspots: BTreeMap<String, Ipv4Addr>,
    public: Option<(String, Option<Ipv4Addr>)>,
}
static NAMES: LazyLock<Mutex<BTreeMap<PathBuf, Names>>> = LazyLock::new(Mutex::default);
pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}
pub fn hosts_path(dir: &Path) -> PathBuf {
    if dir == Path::new(DIR) {
        HOSTS.into()
    } else {
        dir.join("hotspot-hosts")
    }
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
            if !row.contains(&name.as_str()) {
                row.push(name);
            }
        }
    }
    // Preserve the selected browser address when there is no AP yet (also shared-Wi-Fi mode).
    if names.hotspots.is_empty() {
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
fn sync(dir: &Path, names: &Names) -> io::Result<()> {
    let hosts = hosts_path(dir);
    // Install hosts first: a new dnsmasq must never see a missing addn-hosts file.
    let changed = atomic_write(&hosts, &hosts_text(names))?;
    if names.hotspots.is_empty()
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
        atomic_write(&path(dir), &text(&hosts))?;
    }
    if changed && dir == Path::new(DIR) {
        reload_shared_dnsmasq();
    }
    Ok(())
}
/// Reload only NM's shared dnsmasq instances; addn-hosts is reread on SIGHUP.
fn reload_shared_dnsmasq() {
    #[cfg(target_os = "linux")]
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
pub fn hotspot(dir: &Path, interface: &str, address: Option<Ipv4Addr>) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    let names = all.entry(dir.into()).or_default();
    if let Some(address) = address {
        names.hotspots.insert(interface.into(), address);
    } else {
        names.hotspots.remove(interface);
    }
    sync(dir, names)
}
pub fn reset(dir: &Path) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    all.insert(dir.into(), Names::default());
    sync(dir, &all[dir])
}
pub fn write(dir: &Path, name: &str, address: Ipv4Addr) -> io::Result<()> {
    public_name(dir, name, Some(address))
}
pub fn public_name(dir: &Path, name: &str, address: Option<Ipv4Addr>) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    let names = all.entry(dir.into()).or_default();
    names.public = Some((name.into(), address));
    sync(dir, names)
}
pub fn remove(dir: &Path) -> io::Result<()> {
    let mut all = NAMES.lock().unwrap();
    let names = all.entry(dir.into()).or_default();
    names.public = None;
    sync(dir, names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::testing::TempDir;
    #[test]
    fn writes_one_address_line_and_removes_it() {
        let root = TempDir::new("local-dns");
        let dir = root.0.join("dnsmasq-shared.d");
        remove(&dir).unwrap();
        write(&dir, "myrelay.duckdns.org", Ipv4Addr::new(10, 77, 0, 1)).unwrap();
        let written = std::fs::read_to_string(path(&dir)).unwrap();
        assert_eq!(written, text(&hosts_path(&dir)));
        assert_eq!(
            std::fs::read_to_string(hosts_path(&dir)).unwrap(),
            "10.77.0.1 myrelay.duckdns.org\n"
        );
        assert_eq!(written.lines().filter(|l| !l.starts_with('#')).count(), 2);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path(&dir)), 0o644);
        assert_eq!(mode(&dir), 0o755);
        write(&dir, "myrelay.duckdns.org", Ipv4Addr::new(10, 77, 1, 1)).unwrap();
        assert!(std::fs::read_to_string(hosts_path(&dir))
            .unwrap()
            .contains("10.77.1.1 myrelay.duckdns.org\n"));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            2,
            "no temporary file left"
        );
        remove(&dir).unwrap();
        assert!(!path(&dir).exists());
    }
    #[test]
    fn every_hotspot_has_local_names_and_public_name_survives_on_admin_alone() {
        let root = TempDir::new("hotspot-dns");
        let dir = root.0.join("dnsmasq-shared.d");
        reset(&dir).unwrap();
        let admin = Ipv4Addr::new(10, 79, 0, 1);
        let carplay = Ipv4Addr::new(10, 77, 0, 1);
        hotspot(&dir, "wlan9", Some(admin)).unwrap();
        hotspot(&dir, "wlan8", Some(carplay)).unwrap();
        public_name(&dir, "synthetic.example.org", None).unwrap();
        let content = std::fs::read_to_string(hosts_path(&dir)).unwrap();
        assert_eq!(content, "10.77.0.1 openxplay-linux.local synthetic.example.org\n10.79.0.1 openxplay-linux.local synthetic.example.org\n");
        let conf = std::fs::read_to_string(path(&dir)).unwrap();
        assert_eq!(conf, text(&hosts_path(&dir)));
        assert!(conf.contains("localise-queries\n"));
        assert!(!conf.contains("address="));
        hotspot(&dir, "wlan8", None).unwrap();
        assert_eq!(
            std::fs::read_to_string(hosts_path(&dir)).unwrap(),
            "10.79.0.1 openxplay-linux.local synthetic.example.org\n"
        );
        remove(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(hosts_path(&dir)).unwrap(),
            "10.79.0.1 openxplay-linux.local\n"
        );
        hotspot(&dir, "wlan9", None).unwrap();
        assert!(std::fs::read_to_string(hosts_path(&dir))
            .unwrap()
            .is_empty());
    }
}
