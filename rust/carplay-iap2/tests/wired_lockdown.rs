// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

mod wired_common;
use carplay_iap2::{
    transport::{
        lockdown_pair_record::{certificate_from_pem, LockdownPairRecord},
        lockdown_pairing_client::LockdownPairingError,
        lockdown_plist_channel::{self as plist_channel, LockdownPlistChannel},
        lockdown_tls_engine_factory::{RustlsConnector, TlsConnector},
    },
    DuplexStream, Receive,
};
use der::Encode;
use plist::{Dictionary, Value};
use rsa::{
    pkcs1::{DecodeRsaPublicKey, EncodeRsaPublicKey},
    pkcs1v15::{Signature, VerifyingKey},
    pkcs8::{EncodePrivateKey, LineEnding},
    signature::Verifier,
    RsaPrivateKey,
};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::{
    io::{Cursor, Read, Write},
    sync::{Arc, OnceLock},
    time::Duration,
};
use wired_common::*;
struct Fixture {
    device: RsaPrivateKey,
    record: LockdownPairRecord,
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let device = RsaPrivateKey::new(&mut rand_core::OsRng, 2048).unwrap();
        let public = device.to_public_key().to_pkcs1_pem(LineEnding::LF).unwrap();
        let record = LockdownPairRecord::generate(
            public.as_bytes(),
            "02:00:00:00:00:02",
            "host-id",
            "system-buid",
        )
        .unwrap();
        Fixture { device, record }
    })
}
fn dict(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Dictionary {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}
#[test]
fn pair_record_certificate_profile_signatures_and_private_plist_roundtrip() {
    let fixture = fixture();
    let r = &fixture.record;
    let root = certificate_from_pem(&r.root_certificate_pem).unwrap();
    let host = certificate_from_pem(&r.host_certificate_pem).unwrap();
    let device = certificate_from_pem(&r.device_certificate_pem).unwrap();
    let root_key = rsa::RsaPublicKey::from_pkcs1_der(
        root.tbs_certificate
            .subject_public_key_info
            .subject_public_key
            .as_bytes()
            .unwrap(),
    )
    .unwrap();
    for cert in [&root, &host, &device] {
        assert_eq!(cert.tbs_certificate.version, x509_cert::Version::V3);
        assert_eq!(cert.tbs_certificate.serial_number.as_bytes(), [0]);
        assert!(cert.tbs_certificate.issuer.is_empty());
        assert!(cert.tbs_certificate.subject.is_empty());
        assert_eq!(
            cert.tbs_certificate.validity.not_after.to_unix_duration()
                - cert.tbs_certificate.validity.not_before.to_unix_duration(),
            Duration::from_secs(10 * 365 * 24 * 60 * 60)
        );
        VerifyingKey::<sha2::Sha256>::new(root_key.clone())
            .verify(
                &cert.tbs_certificate.to_der().unwrap(),
                &Signature::try_from(cert.signature.as_bytes().unwrap()).unwrap(),
            )
            .unwrap();
    }
    let extensions = root.tbs_certificate.extensions.as_ref().unwrap();
    assert_eq!(extensions.len(), 1);
    assert!(extensions[0].critical);
    assert_eq!(extensions[0].extn_value.as_bytes(), [0x30, 3, 1, 1, 255]);
    for cert in [&host, &device] {
        let exts = cert.tbs_certificate.extensions.as_ref().unwrap();
        assert_eq!(exts[0].extn_value.as_bytes(), [0x30, 0]);
        let ku = exts
            .iter()
            .find(|e| e.extn_id.to_string() == "2.5.29.15")
            .unwrap();
        assert!(ku.critical);
        assert_eq!(ku.extn_value.as_bytes(), [3, 2, 5, 0xa0]);
    }
    assert_eq!(host.tbs_certificate.extensions.as_ref().unwrap().len(), 2);
    let ski = device
        .tbs_certificate
        .extensions
        .as_ref()
        .unwrap()
        .iter()
        .find(|e| e.extn_id.to_string() == "2.5.29.14")
        .unwrap();
    assert!(!ski.critical);
    use sha1::Digest;
    let public = fixture.device.to_public_key().to_pkcs1_der().unwrap();
    assert_eq!(
        &ski.extn_value.as_bytes()[2..],
        &sha1::Sha1::digest(public.as_bytes())[..]
    );
    let mut r = r.clone();
    r.escrow_bag = Some(vec![1, 2, 3]);
    let bytes = r.serialize().unwrap();
    let restored = LockdownPairRecord::parse(&bytes).unwrap();
    assert_eq!(restored.host_id, "host-id");
    assert_eq!(restored.system_buid, "system-buid");
    assert_eq!(restored.escrow_bag, Some(vec![1, 2, 3]));
    assert_eq!(restored.serialize().unwrap(), bytes);
    assert_eq!(format!("{restored:?}"), "LockdownPairRecord(redacted)");
    assert!(!r
        .to_pair_request_dictionary()
        .contains_key("RootPrivateKey"));
    let mut invalid = plist_channel::parse(&bytes).unwrap();
    invalid.insert(
        "RootPrivateKey".into(),
        Value::Data(r.host_private_key_pem.clone()),
    );
    assert_eq!(
        LockdownPairRecord::parse(&plist_channel::encode(&invalid).unwrap())
            .unwrap_err()
            .to_string(),
        "Invalid pair record"
    );
    assert!(LockdownPairRecord::parse(b"garbage").is_err());
}
#[test]
fn plist_fragmentation_coalescing_and_detach() {
    let (a, b) = memory_pair();
    let channel = LockdownPlistChannel::new(a.clone());
    let message = dict([
        ("Text", Value::String("<&\"'日本語".into())),
        ("Data", Value::Data(vec![0, 1, 255])),
        ("Int", Value::Integer((-12i64).into())),
        ("Bool", Value::Boolean(true)),
        (
            "Nested",
            Value::Dictionary(dict([("Empty", Value::String("".into()))])),
        ),
    ]);
    channel.send(&message).unwrap();
    let length = u32::from_be_bytes(read_exact(b.as_ref(), 4).try_into().unwrap()) as usize;
    let xml = read_exact(b.as_ref(), length);
    assert_eq!(plist_channel::parse(&xml).unwrap(), message);
    let mut frame = (xml.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&xml);
    for chunk in frame.chunks(3) {
        b.send(chunk).unwrap();
    }
    b.send(&frame).unwrap();
    assert_eq!(channel.receive(Duration::from_secs(1)).unwrap(), message);
    assert_eq!(channel.receive(Duration::from_secs(1)).unwrap(), message);
    let detached = channel.detach().unwrap();
    assert!(channel.detach().is_err());
    assert!(channel.send(&message).is_err());
    channel.close().unwrap();
    detached.send(b"ok").unwrap();
    assert_eq!(read_exact(b.as_ref(), 2), b"ok");
}
#[test]
fn plist_lengths_eof_timeout_duplicates_nesting_and_unsupported_values() {
    for length in [0u32, 1024 * 1024 + 1] {
        let (a, b) = memory_pair();
        b.send(&length.to_be_bytes()).unwrap();
        let c = LockdownPlistChannel::new(a);
        assert_eq!(
            c.receive(Duration::from_secs(1)).unwrap_err().to_string(),
            format!("Invalid Lockdown plist message length {length} (maximum 1048576)")
        );
    }
    let (a, b) = memory_pair();
    b.send(&[0, 0, 0, 10, 1]).unwrap();
    b.close().unwrap();
    assert!(LockdownPlistChannel::new(a)
        .receive(Duration::from_secs(1))
        .unwrap_err()
        .to_string()
        .contains("ended inside"));
    let (a, _b) = memory_pair();
    assert_eq!(
        LockdownPlistChannel::new(a)
            .receive(Duration::from_millis(1))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    for body in [
        "<dict><key>a</key><string>x</string><key>a</key><string>y</string></dict>",
        "<array/>",
        "<dict><key>x</key><real>1.0</real></dict>",
        "<dict><key>x</key><data>not-base64</data></dict>",
    ] {
        assert!(
            plist_channel::parse(format!("<plist version=\"1.0\">{body}</plist>").as_bytes())
                .is_err()
        );
    }
    let nested = format!(
        "<plist>{}<string>x</string>{}</plist>",
        "<dict><key>x</key>".repeat(65),
        "</dict>".repeat(65)
    );
    assert_eq!(
        plist_channel::parse(nested.as_bytes())
            .unwrap_err()
            .to_string(),
        "Lockdown plist nesting exceeds 64"
    );
}
#[test]
fn pairing_error_mapping_is_typed() {
    for (code, expected) in [
        ("UserDeniedPairing", "The user denied Lockdown pairing"),
        ("PasswordProtected", "The iPhone is password protected"),
        (
            "PairingDialogResponsePending",
            "Lockdown pairing dialog response pending",
        ),
        ("InvalidPairRecord", "InvalidPairRecord"),
        ("InvalidHostID", "InvalidHostID"),
    ] {
        let e = LockdownPairingError::from_code(code);
        assert_eq!(e.to_string(), expected);
        assert_eq!(
            e.invalidates_record(),
            code == "InvalidPairRecord" || code == "InvalidHostID"
        );
    }
    assert!(matches!(
        LockdownPairingError::from(std::io::Error::from(std::io::ErrorKind::TimedOut)),
        LockdownPairingError::Timeout
    ));
}
#[derive(Debug)]
struct ServerIdentity(Arc<rustls::sign::CertifiedKey>);
impl rustls::server::ResolvesServerCert for ServerIdentity {
    fn resolve(
        &self,
        hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        assert!(hello.server_name().is_none());
        Some(self.0.clone())
    }
}
fn pem(label: &str, b: &[u8]) -> Vec<u8> {
    use base64ct::{Base64, Encoding};
    let s = std::str::from_utf8(b).unwrap();
    let body = s
        .trim()
        .strip_prefix(&format!("-----BEGIN {label}-----"))
        .unwrap()
        .strip_suffix(&format!("-----END {label}-----"))
        .unwrap();
    Base64::decode_vec(
        &body
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
    )
    .unwrap()
}
#[test]
fn tls12_and_tls13_in_memory_accept_generated_device_certificate_and_exchange_records() {
    let f = fixture();
    for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let certificate =
            CertificateDer::from(pem("CERTIFICATE", &f.record.device_certificate_pem));
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            f.device.to_pkcs8_der().unwrap().as_bytes().to_vec(),
        ));
        let signer = rustls::crypto::ring::sign::any_supported_type(&key).unwrap();
        let certified = Arc::new(rustls::sign::CertifiedKey::new(vec![certificate], signer));
        let server_config = rustls::ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[version])
            .unwrap()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(ServerIdentity(certified)));
        let (a, b) = memory_pair();
        let server = std::thread::spawn(move || {
            let mut server = rustls::ServerConnection::new(Arc::new(server_config)).unwrap();
            let mut plaintext = vec![];
            loop {
                let bytes = match b.recv(16384, Duration::from_secs(5)).unwrap() {
                    Receive::Data(v) => v,
                    other => panic!("TLS server receive: {other:?}"),
                };
                server.read_tls(&mut Cursor::new(bytes)).unwrap();
                server.process_new_packets().unwrap();
                let mut buf = [0; 512];
                loop {
                    match server.reader().read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => plaintext.extend_from_slice(&buf[..n]),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => panic!("{e}"),
                    }
                }
                let mut out = vec![];
                while server.wants_write() {
                    server.write_tls(&mut out).unwrap();
                }
                if !out.is_empty() {
                    for chunk in out.chunks(7) {
                        b.send(chunk).unwrap();
                    }
                }
                if plaintext == b"hello over carkit" {
                    server.writer().write_all(b"reply over tls").unwrap();
                    server.send_close_notify();
                    let mut out = vec![];
                    while server.wants_write() {
                        server.write_tls(&mut out).unwrap();
                    }
                    b.send(&out).unwrap();
                    break;
                }
            }
        });
        let client = RustlsConnector
            .connect(
                a,
                vec![CertificateDer::from(pem(
                    "CERTIFICATE",
                    &f.record.root_certificate_pem,
                ))],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pem(
                    "PRIVATE KEY",
                    &f.record.root_private_key_pem,
                ))),
            )
            .unwrap();
        client.send(b"hello over carkit").unwrap();
        assert_eq!(read_exact(client.as_ref(), 14), b"reply over tls");
        assert_eq!(
            client.recv(100, Duration::from_secs(1)).unwrap(),
            Receive::Eof
        );
        client.close().unwrap();
        server.join().unwrap();
    }
}

#[test]
fn tls_rejects_a_handshake_signed_by_a_different_private_key() {
    let f = fixture();
    let certificate = CertificateDer::from(pem("CERTIFICATE", &f.record.device_certificate_pem));
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pem(
        "PRIVATE KEY",
        &f.record.host_private_key_pem,
    )));
    let signer = rustls::crypto::ring::sign::any_supported_type(&key).unwrap();
    let identity = ServerIdentity(Arc::new(rustls::sign::CertifiedKey::new(
        vec![certificate],
        signer,
    )));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_no_client_auth()
    .with_cert_resolver(Arc::new(identity));
    let (a, b) = memory_pair();
    let server = std::thread::spawn(move || {
        let mut server = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        while let Receive::Data(bytes) = b.recv(16384, Duration::from_secs(5)).unwrap() {
            server.read_tls(&mut Cursor::new(bytes)).unwrap();
            if server.process_new_packets().is_err() {
                break;
            }
            let mut out = vec![];
            while server.wants_write() {
                server.write_tls(&mut out).unwrap();
            }
            if b.send(&out).is_err() {
                break;
            }
        }
    });
    assert!(RustlsConnector
        .connect(
            a,
            vec![CertificateDer::from(pem(
                "CERTIFICATE",
                &f.record.root_certificate_pem
            ))],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pem(
                "PRIVATE KEY",
                &f.record.root_private_key_pem
            )))
        )
        .is_err());
    server.join().unwrap();
}
