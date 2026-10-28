// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

#[test]
fn identity_admin_import_remove_status_and_exports_never_return_mfi_material() {
    use base64ct::{Base64, Encoding};
    let (admin, dir) = fake();
    let (key, cert) = crate::identity::tests::pair();
    let files =
        serde_json::json!({"files":[Base64::encode_string(&cert),Base64::encode_string(&key)]});
    admin.identity_import(files.as_object().unwrap()).unwrap();
    assert_eq!(admin.identity_status()["mfi"]["state"], "READY");
    let encoded = Base64::encode_string(&key);
    for value in [
        admin.identity_status(),
        serde_json::to_value(admin.status()).unwrap(),
        admin.config_export(false).unwrap(),
    ] {
        assert!(!value.to_string().contains(&encoded));
        assert!(!value.to_string().contains("PRIVATE KEY"));
    }
    let privacy = admin.config_export(true).unwrap();
    assert!(!privacy.to_string().contains(&encoded));
    assert!(privacy.get("identity").is_none());
    admin.identity_remove().unwrap();
    assert_eq!(admin.identity_status()["mfi"]["state"], "MISSING");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn imported_tls_privacy_preview_replacement_validation_and_removal() {
    let (admin, dir) = fake();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["relay.test".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let envelope = serde_json::json!({"format":"openxplay-privacy-config","version":1,"importedCertificate":{"chainPem":cert.pem(),"keyPem":key.serialize_pem()}});
    let bytes = serde_json::to_vec(&envelope).unwrap();
    let preview = admin.config_import(&bytes, true).unwrap();
    assert_eq!(
        preview["applied"],
        serde_json::json!(["importedCertificate"])
    );
    assert!(crate::identity::tls::material(&dir).unwrap().is_none());
    assert_eq!(admin.config_import(&bytes, false).unwrap(), preview);
    let backup = admin.config_export(true).unwrap();
    assert_eq!(
        backup["importedCertificate"],
        envelope["importedCertificate"]
    );
    assert!(backup.get("identity").is_none());
    let before = admin.identity_status()["tls"]["fingerprint"].clone();
    let mut bad = envelope.clone();
    bad["importedCertificate"]["keyPem"] = "bad".into();
    assert_eq!(
        admin
            .config_import(&serde_json::to_vec(&bad).unwrap(), false)
            .unwrap_err()["importedCertificate"],
        "invalid"
    );
    assert_eq!(admin.identity_status()["tls"]["fingerprint"], before);
    admin.tls_remove().unwrap();
    assert!(crate::identity::tls::material(&dir).unwrap().is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn user_tls_uses_the_coalesced_receiver_path_and_keeps_the_active_session() {
    let (admin, dir, factory, rx) = tls_fake_admin();
    let session = admin.input.begin(Arc::new(TlsSessionControl)).unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["relay.test".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let material = crate::identity::tls::validate(&cert.pem(), &key.serialize_pem(), None).unwrap();
    crate::identity::tls::install(&dir, &material).unwrap();
    admin.queue_user_tls();
    assert_eq!(admin.identity_status()["tls"]["pending"], true);
    let operation = next_tls_operation(&admin);
    std::thread::scope(|scope| {
        let update = scope.spawn(|| admin.perform_operation(operation));
        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(!request.value["default"]["key"].is_null());
        assert_eq!(request.value["trusted"], serde_json::Value::Null);
        (request.events)(
            serde_json::json!({"type":"web-tls","id":request.value["id"],"applied":true,"trustedName":null,"defaultFingerprint":material.fingerprint}),
        );
        update.join().unwrap().unwrap();
    });
    assert_eq!(admin.identity_status()["tls"]["pending"], false);
    admin.tls_remove().unwrap();
    let operation = next_tls_operation(&admin);
    std::thread::scope(|scope| {
        let update = scope.spawn(|| admin.perform_operation(operation));
        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(request.value["default"], serde_json::Value::Null);
        (request.events)(
            serde_json::json!({"type":"web-tls","id":request.value["id"],"applied":true,"trustedName":null,"defaultFingerprint":null}),
        );
        update.join().unwrap().unwrap();
    });
    assert_eq!(admin.identity_status()["tls"]["pending"], false);
    assert!(admin.input.session_active());
    assert_eq!(factory.stops.load(Ordering::SeqCst), 0);
    admin.input.end(session).unwrap();
    drop(admin);
    std::fs::remove_dir_all(dir).unwrap();
}
