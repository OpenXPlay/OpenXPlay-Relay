// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! User certificates are independent of automatic and public-name TLS.
use super::{fingerprint, Error, MaterialStatus};
use der::Decode;
use rustls::sign::CertifiedKey;
use rustls_pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};
pub const MAX_FILE: usize = 1024 * 1024;
const INVALID: &str = "identity_error_tls";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultTls {
    pub cert: PathBuf,
    pub key: PathBuf,
    pub fingerprint: String,
}
pub struct Material {
    pub chain: String,
    pub key: String,
    pub fingerprint: String,
}
impl Drop for Material {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
pub fn validate(chain: &str, key: &str, now: Option<u64>) -> Result<Material, Error> {
    if chain.len() > MAX_FILE * 2
        || key.len() > MAX_FILE
        || !crate::public_name::privacy::strict_pem(chain, "CERTIFICATE", false)
        || !["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"]
            .iter()
            .any(|label| crate::public_name::privacy::strict_pem(key, label, true))
    {
        return Err(INVALID);
    }
    let certified = certified(chain.as_bytes(), key.as_bytes()).map_err(|_| INVALID)?;
    for cert in &certified.cert {
        let parsed = x509_cert::Certificate::from_der(cert).map_err(|_| INVALID)?;
        if let Some(now) = now {
            let validity = parsed.tbs_certificate.validity;
            if cert == &certified.cert[0]
                && (now < validity.not_before.to_unix_duration().as_secs()
                    || now > validity.not_after.to_unix_duration().as_secs())
            {
                return Err(INVALID);
            }
        }
    }
    Ok(Material {
        chain: chain.into(),
        key: key.into(),
        fingerprint: fingerprint(&certified.cert[0]),
    })
}
pub fn certified(chain: &[u8], key: &[u8]) -> io::Result<Arc<CertifiedKey>> {
    let certs = CertificateDer::pem_slice_iter(chain)
        .collect::<Result<Vec<_>, _>>()
        .map_err(io::Error::other)?;
    if certs.is_empty() {
        return Err(io::Error::other(INVALID));
    }
    let key = PrivateKeyDer::from_pem_slice(key).map_err(io::Error::other)?;
    let signer = rustls::crypto::ring::sign::any_supported_type(&key).map_err(io::Error::other)?;
    let pair = CertifiedKey::new(certs, signer);
    pair.keys_match().map_err(io::Error::other)?;
    Ok(Arc::new(pair))
}
pub fn directory(state: &Path) -> PathBuf {
    state.join("user-tls")
}
pub fn material(state: &Path) -> io::Result<Option<Material>> {
    let _lock = super::MATERIALS.lock().unwrap();
    material_unlocked(state)
}
fn material_unlocked(state: &Path) -> io::Result<Option<Material>> {
    let root = directory(state);
    if fs::symlink_metadata(root.join("current"))
        .is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
    {
        return Ok(None);
    }
    let chain = super::read(&root, "chain.pem", MAX_FILE * 2)?;
    let key = super::read(&root, "key.pem", MAX_FILE)?;
    validate(
        std::str::from_utf8(&chain).map_err(io::Error::other)?,
        std::str::from_utf8(&key).map_err(io::Error::other)?,
        None,
    )
    .map(Some)
    .map_err(io::Error::other)
}
pub fn snapshot(state: &Path) -> io::Result<Option<DefaultTls>> {
    let _lock = super::MATERIALS.lock().unwrap();
    let Some(material) = material_unlocked(state)? else {
        return Ok(None);
    };
    let root = super::selected(&directory(state))?;
    Ok(Some(DefaultTls {
        cert: root.join("chain.pem"),
        key: root.join("key.pem"),
        fingerprint: material.fingerprint.clone(),
    }))
}
pub fn install(state: &Path, material: &Material) -> io::Result<()> {
    super::replace(
        &directory(state),
        &[
            ("chain.pem", material.chain.as_bytes()),
            ("key.pem", material.key.as_bytes()),
        ],
    )
}
pub fn export(state: &Path) -> io::Result<Option<Value>> {
    Ok(material(state)?.map(|m| json!({"chainPem":m.chain,"keyPem":m.key})))
}
pub fn status(state: &Path) -> Value {
    let imported = fs::symlink_metadata(directory(state).join("current")).is_ok();
    let current = if imported {
        match material(state) {
            Ok(Some(m)) => MaterialStatus {
                state: "READY",
                fingerprint: Some(m.fingerprint.clone()),
            },
            _ => MaterialStatus {
                state: "INVALID",
                fingerprint: None,
            },
        }
    } else {
        let fingerprint = fs::read(state.join("receiver-tls/cert.pem"))
            .ok()
            .and_then(|b| crate::public_name::cert::chain(&b))
            .map(|c| fingerprint(&c[0]))
            .or_else(|| {
                #[derive(Deserialize)]
                struct Generated {
                    cert: Vec<u8>,
                }
                let bytes = Zeroizing::new(fs::read(state.join("admin-tls.json")).ok()?);
                let generated: Generated = serde_json::from_slice(&bytes).ok()?;
                Some(fingerprint(&generated.cert))
            });
        MaterialStatus {
            state: if fingerprint.is_some() {
                "READY"
            } else {
                "MISSING"
            },
            fingerprint,
        }
    };
    json!({"state":current.state,"fingerprint":current.fingerprint,"imported":imported})
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
/// OpenSSL is a bounded PKCS#12 decoder only. Passwords never enter arguments or diagnostics.
pub fn import_pkcs12(state: &Path, bytes: &[u8], password: &str) -> Result<(), Error> {
    import_pkcs12_with_program(state, bytes, password, Path::new("openssl"))
}
fn import_pkcs12_with_program(
    state: &Path,
    bytes: &[u8],
    password: &str,
    program: &Path,
) -> Result<(), Error> {
    if bytes.is_empty()
        || bytes.len() > MAX_FILE
        || password.encode_utf16().count() > 4096
        || password.contains('\0')
    {
        return Err(INVALID);
    }
    let scratch = Temporary(state.join(format!(
        ".tls-import-{}",
        crate::admin::auth::random_hex(12).map_err(|_| "error.request")?
    )));
    crate::admin::auth::private_file(&scratch.0.join("input.p12"), bytes)
        .map_err(|_| "error.request")?;
    let mut decoded = None;
    for legacy in [false, true] {
        match decode_pkcs12(program, &scratch.0.join("input.p12"), password, legacy) {
            Ok(output) => {
                decoded = Some(output);
                break;
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ) =>
            {
                return Err("identity_error_pkcs12_unavailable")
            }
            Err(_) => {}
        }
    }
    let output = decoded.ok_or(INVALID)?;
    let text = Zeroizing::new(String::from_utf8(output.to_vec()).map_err(|_| INVALID)?);
    let mut chain = String::new();
    let mut key = Zeroizing::new(String::new());
    let mut block = None;
    for line in text.lines() {
        if line == "-----BEGIN CERTIFICATE-----" {
            block = Some(false);
        }
        if line == "-----BEGIN PRIVATE KEY-----" {
            if !key.is_empty() {
                return Err(INVALID);
            }
            block = Some(true);
        }
        if let Some(private) = block {
            let target = if private { &mut *key } else { &mut chain };
            target.push_str(line);
            target.push('\n');
        }
        if line.starts_with("-----END ") {
            block = None;
        }
    }
    let material = validate(&chain, &key, None)?;
    install(state, &material).map_err(|_| "error.request")
}
fn decode_pkcs12(
    program: &Path,
    path: &Path,
    password: &str,
    legacy: bool,
) -> io::Result<Zeroizing<Vec<u8>>> {
    let mut command = Command::new(program);
    command
        .args(["pkcs12", "-nodes", "-in"])
        .arg(path)
        .args(["-passin", "env:OXP_PKCS12_PASSWORD"]);
    if legacy {
        command.arg("-legacy");
    }
    let mut child = command
        .env("OXP_PKCS12_PASSWORD", password)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other(INVALID))?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Zeroizing::new(Vec::new());
        output
            .take((MAX_FILE * 3 + 1) as u64)
            .read_to_end(&mut bytes)?;
        Ok::<_, io::Error>(bytes)
    });
    let start = Instant::now();
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(8) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader.join().map_err(|_| io::Error::other(INVALID))??;
    if !success || bytes.len() > MAX_FILE * 3 {
        return Err(io::Error::other(INVALID));
    }
    Ok(bytes)
}

pub fn remove(state: &Path) -> io::Result<()> {
    let _lock = super::MATERIALS.lock().unwrap();
    match fs::remove_file(directory(state).join("current")) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
/// Retain acknowledged paths until the child's automatic retries no longer need them.
pub fn prune(state: &Path) -> io::Result<()> {
    let _lock = super::MATERIALS.lock().unwrap();
    let directory = directory(state);
    if !directory.exists() {
        return Ok(());
    }
    let selected = super::selected(&directory)?;
    let keep = if selected == directory {
        None
    } else {
        selected.file_name().and_then(|s| s.to_str())
    };
    super::cleanup(&directory, keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_openssl_preserves_tls_and_pem_install_remains_available() {
        let root = std::env::temp_dir().join(format!(
            "tls-no-openssl-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["relay.test".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let material = validate(&cert.pem(), &key.serialize_pem(), None).unwrap();
        install(&root, &material).unwrap();
        let before = snapshot(&root).unwrap();
        assert_eq!(
            import_pkcs12_with_program(&root, b"pkcs12", "private", &root.join("missing-openssl")),
            Err("identity_error_pkcs12_unavailable")
        );
        assert_eq!(snapshot(&root).unwrap(), before);
        remove(&root).unwrap();
        install(&root, &material).unwrap();
        assert_eq!(
            snapshot(&root).unwrap().unwrap().fingerprint,
            material.fingerprint
        );
        assert!(!fs::read_dir(&root).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".tls-import-")));
        fs::remove_dir_all(root).unwrap();
    }
}
