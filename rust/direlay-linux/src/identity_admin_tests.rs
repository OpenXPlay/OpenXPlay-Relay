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

#[test]
fn imported_default_startup_flags_acknowledgement_and_live_pem_replacement() {
    let (mut admin, dir) = fake();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["relay.test".into()]).unwrap().self_signed(&key).unwrap();
    let material = crate::identity::tls::validate(&cert.pem(), &key.serialize_pem(), None).unwrap();
    crate::identity::tls::install(&dir, &material).unwrap();
    let default = crate::identity::tls::snapshot(&dir).unwrap().unwrap();
    let (factory, rx) = crate::supervisor::receiver_tls_tests::FakeChildFactory::new();
    Arc::get_mut(&mut admin).unwrap().child_factory = factory.clone();
    admin.start_supervisor().unwrap();
    factory.wait_started();
    let args = factory.launches.lock().unwrap()[0].clone();
    for (flag, path) in [("--tls-default-cert", default.cert.as_os_str()), ("--tls-default-key", default.key.as_os_str()), ("--tls-self-signed", dir.join("receiver-tls").as_os_str())] {
        assert_eq!(args.iter().filter(|arg| *arg == flag).count(), 1);
        assert!(args.windows(2).any(|pair| pair[0] == flag && pair[1] == path));
    }
    let events = factory.events.lock().unwrap().as_ref().unwrap().clone();
    events(serde_json::json!({"type":"tls-fingerprint","fingerprint":material.fingerprint,"defaultFingerprint":"wrong"}));
    assert_eq!(admin.identity_status()["tls"]["pending"], true);
    events(serde_json::json!({"type":"tls-fingerprint","fingerprint":material.fingerprint,"defaultFingerprint":material.fingerprint,"trustedFingerprint":null}));
    assert_eq!(admin.identity_status()["tls"]["pending"], false);
    let session = admin.input.begin(Arc::new(TlsSessionControl)).unwrap();
    let new_key = rcgen::KeyPair::generate().unwrap();
    let new_cert = rcgen::CertificateParams::new(vec!["replacement.test".into()]).unwrap().self_signed(&new_key).unwrap();
    let body = serde_json::json!({"chainPem":new_cert.pem(),"keyPem":new_key.serialize_pem()});
    admin.tls_import(body.as_object().unwrap()).unwrap();
    assert_eq!(admin.settings().values["privacyConfigAvailable"], true);
    assert_eq!(admin.settings().values["publicNamePrivacyAvailable"], false);
    let fingerprint = admin.identity_status()["tls"]["fingerprint"].clone();
    for invalid in [serde_json::json!({"chainPem":new_cert.pem()}), serde_json::json!({"chainPem":new_cert.pem(),"keyPem":key.serialize_pem()}), serde_json::json!({"chainPem":new_cert.pem(),"keyPem":new_key.serialize_pem(),"data":"p12"})] {
        assert_eq!(admin.tls_import(invalid.as_object().unwrap()), Err("identity_error_pem"));
        assert_eq!(admin.identity_status()["tls"]["fingerprint"], fingerprint);
    }
    for remove in [false, true] {
        if remove { admin.tls_remove().unwrap(); }
        let operation = next_tls_operation(&admin);
        std::thread::scope(|scope| {
            let update = scope.spawn(|| admin.perform_operation(operation));
            let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(request.value["type"], "web-tls");
            assert_eq!(request.value["trusted"], serde_json::Value::Null);
            assert!(request.value["sans"].as_array().unwrap().contains(&serde_json::json!("openxplay-linux.local")));
            assert_eq!(request.value["default"].is_null(), remove);
            (request.events)(serde_json::json!({"type":"web-tls","id":request.value["id"],"applied":true,"trustedName":null,"defaultFingerprint":if remove { serde_json::Value::Null } else { fingerprint.clone() }}));
            update.join().unwrap().unwrap();
        });
        assert_eq!(admin.identity_status()["tls"]["pending"], false);
        assert!(admin.input.session_active());
        assert_eq!(factory.launches.lock().unwrap().len(), 1);
        assert_eq!(factory.stops.load(Ordering::SeqCst), 0);
    }
    assert_eq!(admin.settings().values["privacyConfigAvailable"], false);
    admin.input.end(session).unwrap();
    drop(admin);
    std::fs::remove_dir_all(dir).unwrap();
}
