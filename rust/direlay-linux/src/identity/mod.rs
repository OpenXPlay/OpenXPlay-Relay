// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Private local material. No MFi material is serializable or exported.
mod archive;
#[cfg(test)]
pub(crate) mod tests;
pub mod tls;
use base64ct::{Base64, Encoding};
use carplay_iap2::LocalMfiAuthenticationClient;
use der::{Decode, Encode};
use p256::{pkcs8::DecodePrivateKey, SecretKey};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

pub(crate) static MATERIALS: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub const MAX_FILE: usize = 16 * 1024;
pub const MAX_ARCHIVE: usize = 512 * 1024 * 1024;
pub type Secret = Zeroizing<Vec<u8>>;
pub type Error = &'static str;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialStatus {
    pub state: &'static str,
    pub fingerprint: Option<String>,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Key,
    Certificate,
    Unknown,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Certificate => "certificate",
            Self::Unknown => "unknown",
        }
    }
}
pub fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}
fn normalized(bytes: &[u8]) -> Result<Secret, Error> {
    if bytes.is_empty() || bytes.len() > MAX_FILE {
        return Err("identity_error_size");
    }
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'-') {
        return Ok(Zeroizing::new(bytes.to_vec()));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "identity_error_pair")?;
    let mut lines = text.trim().lines();
    let first = lines.next().ok_or("identity_error_pair")?;
    let label = first
        .strip_prefix("-----BEGIN ")
        .and_then(|s| s.strip_suffix("-----"))
        .ok_or("identity_error_pair")?;
    if !["PRIVATE KEY", "PKCS7", "CERTIFICATE"].contains(&label) {
        return Err("identity_error_pair");
    }
    let end = format!("-----END {label}-----");
    let rest = Zeroizing::new(lines.collect::<Vec<_>>().join("\n"));
    let body = rest.strip_suffix(&end).ok_or("identity_error_pair")?;
    let body = Zeroizing::new(
        body.chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>(),
    );
    Base64::decode_vec(&body)
        .map(Zeroizing::new)
        .map_err(|_| "identity_error_pair")
}
fn classify(bytes: &[u8]) -> Kind {
    let Ok(bytes) = normalized(bytes) else {
        return Kind::Unknown;
    };
    if SecretKey::from_pkcs8_der(&bytes).is_ok() {
        Kind::Key
    } else if x509_cert::Certificate::from_der(&bytes).is_ok()
        || accessory_certificate(&bytes).is_some()
    {
        Kind::Certificate
    } else {
        Kind::Unknown
    }
}
fn accessory_certificate(bytes: &[u8]) -> Option<Secret> {
    let info = cms::content_info::ContentInfo::from_der(bytes).ok()?;
    if info.content_type.to_string() != "1.2.840.113549.1.7.2" {
        return None;
    }
    let signed = info
        .content
        .decode_as::<cms::signed_data::SignedData>()
        .ok()?;
    let certs = signed.certificates?;
    if certs.0.len() != 1 {
        return None;
    }
    match certs.0.iter().next()? {
        cms::cert::CertificateChoices::Certificate(c) => c.to_der().ok().map(Zeroizing::new),
        _ => None,
    }
}
fn validate(key: &[u8], certificate: &[u8]) -> Result<String, Error> {
    LocalMfiAuthenticationClient::from_identity(key, certificate)
        .map_err(|_| "identity_error_pair")?;
    Ok(fingerprint(
        &accessory_certificate(certificate).ok_or("identity_error_pair")?,
    ))
}
/// Follow-up selections are relay-owned, bounded, expire on a monotonic clock and never return bytes.
#[derive(Default)]
pub struct Imports {
    pending: BTreeMap<String, (Instant, Secret)>,
}
impl Imports {
    pub fn expire(&mut self) {
        self.pending
            .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(300));
    }
    pub fn clear(&mut self) {
        self.pending.clear();
    }
    pub fn select(
        &mut self,
        directory: &Path,
        body: &crate::admin::Values,
    ) -> Result<Value, Error> {
        self.expire();
        let follow = body.get("followUp").and_then(Value::as_str);
        if body.get("cancel") == Some(&Value::Bool(true)) {
            if let Some(token) = follow {
                self.pending.remove(token);
            }
            return Ok(json!({"ok":true}));
        }
        let selected = body
            .get("files")
            .and_then(Value::as_array)
            .filter(|f| !f.is_empty() && f.len() <= if follow.is_some() { 1 } else { 2 })
            .ok_or("identity_error_selection")?;
        let mut all = Vec::new();
        for value in selected {
            let encoded = value.as_str().ok_or("identity_error_selection")?;
            if encoded.len() > MAX_ARCHIVE * 4 / 3 + 4 {
                return Err("identity_error_limit");
            }
            let bytes = Zeroizing::new(
                Base64::decode_vec(encoded).map_err(|_| "identity_error_selection")?,
            );
            let zip = archive::is_zip(&bytes);
            if zip && selected.len() != 1 {
                return Err("identity_error_selection");
            }
            if zip {
                all.extend(archive::read(&bytes)?);
            } else {
                if bytes.is_empty() || bytes.len() > MAX_FILE {
                    return Err("identity_error_size");
                }
                all.push(bytes);
            }
        }
        if let Some(token) = follow {
            if all.len() != 1 {
                return Err("identity_error_selection");
            }
            let previous = self.pending.get(token).ok_or("identity_error_selection")?;
            all.push(previous.1.clone());
        }
        if all.len() == 1 {
            if self.pending.len() >= 8 {
                return Err("identity_error_selection");
            }
            let token = crate::admin::auth::random_hex(24).map_err(|_| "error.request")?;
            let kind = classify(&all[0]);
            self.pending
                .insert(token.clone(), (Instant::now(), all.remove(0)));
            return Ok(json!({"ok":true,"followUp":token,"recognized":kind.name()}));
        }
        if all.len() != 2 {
            return Err("identity_error_selection");
        }
        let kinds = [classify(&all[0]), classify(&all[1])];
        if kinds[0] != Kind::Unknown && kinds[0] == kinds[1] {
            return Err("identity_error_same_kind");
        }
        let mut pair = None;
        for index in [0, 1] {
            if let (Ok(key), Ok(cert)) = (normalized(&all[index]), normalized(&all[1 - index])) {
                if let Ok(fingerprint) = validate(&key, &cert) {
                    pair = Some((key, cert, fingerprint));
                    break;
                }
            }
        }
        let (key, cert, fingerprint) = pair.ok_or("identity_error_pair")?;
        replace(
            directory,
            &[("identity.pk8", &key), ("certificate.p7b", &cert)],
        )
        .map_err(|_| "error.request")?;
        if let Some(token) = follow {
            self.pending.remove(token);
        }
        Ok(json!({"ok":true,"state":"READY","fingerprint":fingerprint}))
    }
}
/// One atomic pointer selects both files. Legacy provisioned pairs remain readable.
pub fn selected(directory: &Path) -> io::Result<PathBuf> {
    match fs::read_link(directory.join("current")) {
        Ok(target)
            if target.components().count() == 1
                && target.to_str().is_some_and(|s| s.starts_with("material-")) =>
        {
            Ok(directory.join(target))
        }
        Ok(_) => Err(io::Error::other("Invalid private material generation")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(directory.to_owned()),
        Err(e) => Err(e),
    }
}
pub fn read(directory: &Path, name: &str, max: usize) -> io::Result<Secret> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(selected(directory)?.join(name))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Invalid private material file"));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err(io::Error::other("Invalid private material size"));
    }
    Ok(bytes)
}
pub fn replace(directory: &Path, files: &[(&str, &[u8])]) -> io::Result<()> {
    let _lock = MATERIALS.lock().unwrap();
    fs::create_dir_all(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let generation = format!("material-{}", crate::admin::auth::random_hex(12)?);
    let staging = directory.join(&generation);
    fs::create_dir(&staging)?;
    let pointer = directory.join(format!(".current-{}", crate::admin::auth::random_hex(8)?));
    let mut published = false;
    let result = (|| {
        for (name, bytes) in files {
            crate::admin::auth::private_file(&staging.join(name), bytes)?;
        }
        fs::File::open(&staging)?.sync_all()?;
        std::os::unix::fs::symlink(&generation, &pointer)?;
        fs::rename(&pointer, directory.join("current"))?;
        published = true;
        fs::File::open(directory)?.sync_all()
    })();
    let _ = fs::remove_file(pointer);
    if result.is_err() && !published {
        let _ = fs::remove_dir_all(staging);
    } else if files.iter().any(|(name, _)| *name == "identity.pk8") {
        cleanup(directory, Some(&generation))?;
    }
    result
}
pub(crate) fn cleanup(directory: &Path, keep: Option<&str>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("material-")
            && Some(name.as_ref()) != keep
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
        if ["identity.pk8", "certificate.p7b"].contains(&name.as_ref()) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
pub fn remove(directory: &Path) -> io::Result<()> {
    let _lock = MATERIALS.lock().unwrap();
    if !directory.exists() {
        return Ok(());
    }
    match fs::remove_file(directory.join("current")) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    cleanup(directory, None)
}
pub fn status(directory: &Path) -> MaterialStatus {
    let _lock = MATERIALS.lock().unwrap();
    let files = (
        read(directory, "identity.pk8", MAX_FILE),
        read(directory, "certificate.p7b", MAX_FILE),
    );
    let missing = [&files.0, &files.1].iter().all(|r| {
        r.as_ref()
            .is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
    });
    let fingerprint = match files {
        (Ok(key), Ok(cert)) => validate(&key, &cert).ok(),
        _ => None,
    };
    MaterialStatus {
        state: if fingerprint.is_some() {
            "READY"
        } else if missing {
            "MISSING"
        } else {
            "INVALID"
        },
        fingerprint,
    }
}
pub fn load(directory: &Path) -> io::Result<LocalMfiAuthenticationClient> {
    let _lock = MATERIALS.lock().unwrap();
    let key = read(directory, "identity.pk8", MAX_FILE)?;
    let cert = read(directory, "certificate.p7b", MAX_FILE)?;
    LocalMfiAuthenticationClient::from_identity(&key, &cert)
}
