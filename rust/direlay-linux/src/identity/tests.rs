// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use super::*;
use cms::{
    cert::CertificateChoices,
    content_info::{CmsVersion, ContentInfo},
    signed_data::{CertificateSet, EncapsulatedContentInfo, SignedData, SignerInfos},
};
use der::asn1::{Any, ObjectIdentifier, SetOfVec};
fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "oxp-identity-{}",
        crate::admin::auth::random_hex(12).unwrap()
    ));
    fs::create_dir(&path).unwrap();
    path
}
pub(crate) fn pair() -> (Vec<u8>, Vec<u8>) {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let cert = rcgen::CertificateParams::new(vec!["accessory.test".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let signed = SignedData {
        version: CmsVersion::V1,
        digest_algorithms: SetOfVec::new(),
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1"),
            econtent: None,
        },
        certificates: Some(CertificateSet(
            SetOfVec::try_from(vec![CertificateChoices::Certificate(
                x509_cert::Certificate::from_der(cert.der()).unwrap(),
            )])
            .unwrap(),
        )),
        crls: None,
        signer_infos: SignerInfos(SetOfVec::new()),
    };
    let info = ContentInfo {
        content_type: ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2"),
        content: Any::from_der(&signed.to_der().unwrap()).unwrap(),
    };
    (key.serialize_der(), info.to_der().unwrap())
}
fn body(files: &[&[u8]]) -> crate::admin::Values {
    json!({"files":files.iter().map(|b| Base64::encode_string(b)).collect::<Vec<_>>()})
        .as_object()
        .unwrap()
        .clone()
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    // Independent ZIP fixture writer; archive importing never shares the log export's parser.
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let compressed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let offset = out.len() as u32;
        let crc = data.iter().fold(!0u32, |mut c, b| {
            c ^= *b as u32;
            for _ in 0..8 {
                c = c >> 1 ^ (0xedb88320 & 0u32.wrapping_sub(c & 1));
            }
            c
        }) ^ !0;
        out.extend_from_slice(b"PK\x03\x04");
        for n in [20u16, 0, 8, 0, 0] {
            out.extend(n.to_le_bytes());
        }
        for n in [crc, compressed.len() as u32, data.len() as u32] {
            out.extend(n.to_le_bytes());
        }
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(&compressed);
        central.extend_from_slice(b"PK\x01\x02");
        for n in [20u16, 20, 0, 8, 0, 0] {
            central.extend(n.to_le_bytes());
        }
        for n in [crc, compressed.len() as u32, data.len() as u32] {
            central.extend(n.to_le_bytes());
        }
        for n in [name.len() as u16, 0, 0, 0, 0] {
            central.extend(n.to_le_bytes());
        }
        central.extend(0u32.to_le_bytes());
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let offset = out.len() as u32;
    out.extend(&central);
    out.extend_from_slice(b"PK\x05\x06");
    for n in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        out.extend(n.to_le_bytes());
    }
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}
#[test]
fn any_order_der_pem_and_follow_up_keep_bytes_private() {
    let root = root();
    let (key, cert) = pair();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        Base64::encode_string(&key)
    );
    let mut imports = Imports::default();
    for files in [
        vec![&key[..], &cert[..]],
        vec![&cert[..], &key[..]],
        vec![&cert[..], pem.as_bytes()],
    ] {
        let result = imports.select(&root, &body(&files)).unwrap();
        assert_eq!(result["state"], "READY");
        assert!(result.get("files").is_none());
        assert_eq!(status(&root).state, "READY");
    }
    for first in [&key[..], &cert[..], b"unknown"] {
        let pending = imports.select(&root, &body(&[first])).unwrap();
        assert!(!pending.to_string().contains(&Base64::encode_string(first)));
        let other = if first == &key[..] {
            &cert[..]
        } else {
            &key[..]
        };
        let mut follow = body(&[other]);
        follow.insert("followUp".into(), pending["followUp"].clone());
        if first == b"unknown" {
            assert_eq!(
                imports.select(&root, &follow).unwrap_err(),
                "identity_error_pair"
            );
        } else {
            assert_eq!(imports.select(&root, &follow).unwrap()["state"], "READY");
        }
    }
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_mismatched_same_kind_and_oversize_preserve_the_installed_pair() {
    let root = root();
    let (key, cert) = pair();
    let (other, _) = pair();
    let mut imports = Imports::default();
    imports.select(&root, &body(&[&key, &cert])).unwrap();
    let before = status(&root).fingerprint;
    for (files, code) in [
        (vec![&key[..], &other[..]], "identity_error_same_kind"),
        (vec![&other[..], &cert[..]], "identity_error_pair"),
        (vec![b"bad".as_slice(), &cert[..]], "identity_error_pair"),
        (
            vec![&key[..], &vec![0; MAX_FILE + 1][..]],
            "identity_error_size",
        ),
    ] {
        assert_eq!(imports.select(&root, &body(&files)).unwrap_err(), code);
        assert_eq!(status(&root).fingerprint, before);
    }
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn follow_up_expires_cancels_and_is_bounded() {
    let root = root();
    let (key, cert) = pair();
    let mut imports = Imports::default();
    let pending = imports.select(&root, &body(&[&key])).unwrap();
    let token = pending["followUp"].as_str().unwrap();
    imports.pending.get_mut(token).unwrap().0 = Instant::now() - Duration::from_secs(301);
    let mut follow = body(&[&cert]);
    follow.insert("followUp".into(), token.into());
    assert_eq!(
        imports.select(&root, &follow).unwrap_err(),
        "identity_error_selection"
    );
    for _ in 0..8 {
        imports.select(&root, &body(&[&key])).unwrap();
    }
    assert!(imports.select(&root, &body(&[&key])).is_err());
    let token = imports.pending.keys().next().unwrap().clone();
    imports
        .select(
            &root,
            json!({"cancel":true,"followUp":token}).as_object().unwrap(),
        )
        .unwrap();
    assert_eq!(imports.pending.len(), 7);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn readiness_checks_provisioned_files_and_permissions_and_removal() {
    let root = root();
    assert_eq!(status(&root).state, "MISSING");
    fs::write(root.join("identity.pk8"), b"not empty").unwrap();
    fs::write(root.join("certificate.p7b"), b"not empty").unwrap();
    assert_eq!(status(&root).state, "INVALID");
    let (key, cert) = pair();
    fs::write(root.join("identity.pk8"), key).unwrap();
    fs::write(root.join("certificate.p7b"), cert).unwrap();
    assert_eq!(status(&root).state, "READY");
    assert!(load(&root).is_ok());
    assert_eq!(
        fs::metadata(root.join("identity.pk8"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    remove(&root).unwrap();
    assert_eq!(status(&root).state, "MISSING");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn zip_signature_nested_case_insensitive_names_and_unique_extensions() {
    let root = root();
    let (key, cert) = pair();
    let mut imports = Imports::default();
    for names in [
        ("folder/IDENTITY.PK8", "nested/CERTIFICATE.P7B"),
        ("other/a.pk8", "b.p7b"),
    ] {
        let archive = zip(&[(names.0, &key), (names.1, &cert), ("ignored", b"unrelated")]);
        assert_eq!(
            imports.select(&root, &body(&[&archive])).unwrap()["state"],
            "READY"
        );
    }
    let archive = zip(&[("identity.pk8", &key)]);
    let selected = imports.select(&root, &body(&[&archive])).unwrap();
    assert_eq!(selected["recognized"], "key");
    let mut follow = body(&[&cert]);
    follow.insert("followUp".into(), selected["followUp"].clone());
    assert!(imports.select(&root, &follow).is_ok());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn zip_rejects_ambiguity_oversize_encryption_corruption_and_content_search() {
    let (key, cert) = pair();
    for (archive, code) in [
        (
            zip(&[
                ("identity.pk8", &key),
                ("dir/identity.pk8", &key),
                ("certificate.p7b", &cert),
            ]),
            "identity_error_duplicate_key",
        ),
        (
            zip(&[("a.p7b", &cert), ("b.p7b", &cert)]),
            "identity_error_duplicate_certificate",
        ),
        (
            zip(&[("unrelated", &key), ("other", &cert)]),
            "identity_error_missing",
        ),
        (
            zip(&[("identity.pk8", &vec![0; MAX_FILE + 1])]),
            "identity_error_size",
        ),
    ] {
        assert_eq!(archive::read(&archive).unwrap_err(), code);
    }
    let mut encrypted = zip(&[("identity.pk8", &key)]);
    let c = encrypted
        .windows(4)
        .position(|b| b == b"PK\x01\x02")
        .unwrap();
    encrypted[c + 8] = 1;
    assert_eq!(
        archive::read(&encrypted).unwrap_err(),
        "identity_error_encrypted"
    );
    let mut corrupt = zip(&[("identity.pk8", &key)]);
    corrupt[40] ^= 7;
    assert!(archive::read(&corrupt).is_err());
    let mut too_many = zip(&[]);
    too_many[8..10].copy_from_slice(&20_001u16.to_le_bytes());
    too_many[10..12].copy_from_slice(&20_001u16.to_le_bytes());
    assert_eq!(
        archive::read(&too_many).unwrap_err(),
        "identity_error_limit"
    );
}
#[test]
fn tls_privacy_validates_key_chain_dates_and_round_trips_without_mfi() {
    let root = root();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["relay.test".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let material = tls::validate(&cert.pem(), &key.serialize_pem(), None).unwrap();
    tls::install(&root, &material).unwrap();
    let exported = tls::export(&root).unwrap().unwrap();
    assert_eq!(exported["chainPem"], cert.pem());
    let envelope = json!({"importedCertificate":exported});
    let import = crate::public_name::privacy::decode(
        &envelope,
        None,
        crate::platform::wall_clock().as_secs(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(import.sections(), ["importedCertificate"]);
    let other = rcgen::KeyPair::generate().unwrap();
    assert!(tls::validate(&cert.pem(), &other.serialize_pem(), None).is_err());
    assert!(tls::validate(&cert.pem(), &key.serialize_pem(), Some(0)).is_err());
    // Traditional unencrypted SEC1 PEM works without converting through an external command.
    use p256::pkcs8::DecodePrivateKey;
    let secret = p256::SecretKey::from_pkcs8_der(&key.serialize_der()).unwrap();
    let sec1 = secret.to_sec1_der().unwrap();
    let pem = format!(
        "-----BEGIN EC PRIVATE KEY-----\n{}\n-----END EC PRIVATE KEY-----\n",
        Base64::encode_string(&sec1)
    );
    assert_eq!(
        tls::validate(&cert.pem(), &pem, None).unwrap().fingerprint,
        material.fingerprint
    );
    let old = tls::snapshot(&root).unwrap().unwrap();
    tls::remove(&root).unwrap();
    assert!(old.key.exists());
    assert!(tls::export(&root).unwrap().is_none());
    tls::prune(&root).unwrap();
    assert!(!old.key.exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn pkcs12_password_and_private_key_validation_preserve_the_previous_certificate() {
    let root = root();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["relay.test".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    crate::admin::auth::private_file(&root.join("key.pem"), key.serialize_pem().as_bytes())
        .unwrap();
    fs::write(root.join("cert.pem"), cert.pem()).unwrap();
    let output = std::process::Command::new("openssl")
        .args(["pkcs12", "-export", "-inkey"])
        .arg(root.join("key.pem"))
        .arg("-in")
        .arg(root.join("cert.pem"))
        .args(["-passout", "pass:test-password"])
        .output()
        .unwrap();
    assert!(output.status.success());
    tls::import_pkcs12(&root, &output.stdout, "test-password").unwrap();
    let fp = tls::status(&root)["fingerprint"].clone();
    assert!(tls::import_pkcs12(&root, &output.stdout, "wrong").is_err());
    assert!(tls::import_pkcs12(&root, b"bad", "").is_err());
    assert_eq!(tls::status(&root)["fingerprint"], fp);
    assert!(!fs::read_dir(&root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".tls-import-")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_fields_are_errors_and_do_not_panic() {
    let root = root();
    assert_eq!(
        Imports::default()
            .select(&root, &crate::admin::Values::new())
            .unwrap_err(),
        "identity_error_selection"
    );
    fs::remove_dir_all(root).unwrap();
}
