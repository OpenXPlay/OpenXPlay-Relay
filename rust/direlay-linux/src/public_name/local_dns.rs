// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! The local DNS answer for device-hotspot mode. NetworkManager's shared-mode dnsmasq (its own
//! instance, bound to the hotspot address; not the system dnsmasq) reads
//! `/etc/NetworkManager/dnsmasq-shared.d/` when the hotspot is activated, so one line there lets
//! the head unit resolve the name to the hotspot address even while the relay has no internet.
//! dnsmasq reads it only at that start: the relay writes it just before each activation.
use std::{
    io::{self, Write},
    net::Ipv4Addr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub const DIR: &str = "/etc/NetworkManager/dnsmasq-shared.d";
pub const FILE: &str = "openxplay-public-name.conf";

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}
/// The configuration text; `name` is a validated LDH name, so it cannot break the syntax.
pub fn text(name: &str, address: Ipv4Addr) -> String {
    format!(
        "# openxplay-relay, trusted HTTPS: the head unit resolves the relay's name locally.\n\
         # Removed when the feature is off; dnsmasq reads it when the hotspot starts.\n\
         address=/{name}/{address}\n"
    )
}
/// Writes the answer atomically (mode 0644: nothing secret, the name is public anyway). Leaves
/// an identical file alone.
pub fn write(dir: &Path, name: &str, address: Ipv4Addr) -> io::Result<()> {
    let target = path(dir);
    let text = text(name, address);
    if std::fs::read_to_string(&target).is_ok_and(|current| current == text) {
        return Ok(());
    }
    if !dir.is_dir() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))?;
    }
    let tmp = dir.join(format!(".{FILE}.tmp"));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
pub fn remove(dir: &Path) -> io::Result<()> {
    match std::fs::remove_file(path(dir)) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
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
        assert!(written.ends_with("\naddress=/myrelay.duckdns.org/10.77.0.1\n"));
        assert_eq!(written.lines().filter(|l| !l.starts_with('#')).count(), 1);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path(&dir)), 0o644);
        assert_eq!(mode(&dir), 0o755);
        write(&dir, "myrelay.duckdns.org", Ipv4Addr::new(10, 77, 1, 1)).unwrap();
        assert!(std::fs::read_to_string(path(&dir))
            .unwrap()
            .contains("/10.77.1.1\n"));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary file left"
        );
        remove(&dir).unwrap();
        assert!(!path(&dir).exists());
    }
}
