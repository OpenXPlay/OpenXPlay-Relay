// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
mod common;
use carplay_iap2::*;
use common::*;
use der::{Decode, Encode};
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use p256::{
    ecdsa::{Signature, SigningKey},
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
};
use sha2::{Digest, Sha256};
// Public, synthetic certificate template only; each test generates a fresh private key.
const CERTIFICATE_TEMPLATE:&str="3082017d30820124a003020102021478bf1e31e2d82dd6bb42ab33e5d526fcad2fd3de300a06082a8648ce3d04030230253123302106035504030c1a4469506c61792073796e7468657469632074657374206f6e6c79301e170d3236313031353037323030365a170d3236313031363037323030365a30253123302106035504030c1a4469506c61792073796e7468657469632074657374206f6e6c793059301306072a8648ce3d020106082a8648ce3d030107034200043e5144291431c52c9dead42e5ae9ff6f84a9d3b209ed8752c7c2cf8034458e62a0be2f9e36914f29b231d3c83a0245cd4a8179593df974e16393730668a7a2a8a3323030301d0603551d0e041604143996bff492fcc414eac9f9a921d270c899f9e43b300f0603551d130101ff040530030101ff300a06082a8648ce3d0403020347003044022035c911190160e1a5b06d63bdc89702071e3638b228703ca9b76c13a3848ee54102203e17e4f9b3df870303b3bcbd0ba7eddd3fcd5fe2d097364688c57cfcebd9172e";
fn identity() -> (Vec<u8>, Vec<u8>) {
    let key = SigningKey::random(&mut rand_core::OsRng);
    let mut cert = x509_cert::Certificate::from_der(&unhex(CERTIFICATE_TEMPLATE)).unwrap();
    cert.tbs_certificate
        .subject_public_key_info
        .subject_public_key =
        der::asn1::BitString::from_bytes(key.verifying_key().to_encoded_point(false).as_bytes())
            .unwrap();
    let p7b = cms::content_info::ContentInfo::try_from(cert)
        .unwrap()
        .to_der()
        .unwrap();
    (key.to_pkcs8_der().unwrap().as_bytes().to_vec(), p7b)
}
#[test]
fn fresh_digests_verify_without_double_hashing_and_reject_replay() {
    let (key, cert) = identity();
    let client = LocalMfiAuthenticationClient::from_identity(&key, &cert).unwrap();
    let public = *SigningKey::from_pkcs8_der(&key).unwrap().verifying_key();
    for i in 0..8 {
        let digest = Sha256::digest(format!("challenge-{i}").as_bytes());
        let sig = client.sign_challenge(&digest).unwrap();
        assert_eq!(sig.len(), 64);
        let signature = Signature::from_slice(&sig).unwrap();
        public.verify_prehash(&digest, &signature).unwrap();
        let mut wrong = digest;
        wrong[0] ^= 1;
        assert!(public.verify_prehash(&wrong, &signature).is_err());
    }
    assert_eq!(client.protocol_major().unwrap(), 3);
    assert_eq!(client.certificate_type(), MfiCertificateType::Mfi);
}
#[test]
fn rejects_different_private_key() {
    let (_, cert) = identity();
    let (key, _) = identity();
    assert!(LocalMfiAuthenticationClient::from_identity(&key, &cert)
        .err()
        .unwrap()
        .to_string()
        .contains("does not match"));
}
#[test]
fn rejects_incomplete_identity_and_wrong_challenge_lengths() {
    let (key, cert) = identity();
    assert!(LocalMfiAuthenticationClient::from_identity(&[], &cert).is_err());
    assert!(LocalMfiAuthenticationClient::from_identity(&key, &[]).is_err());
    let c = LocalMfiAuthenticationClient::from_identity(&key, &cert).unwrap();
    for n in [0, 20, 31, 33, 64] {
        assert!(c.sign_challenge(&vec![0; n]).is_err());
    }
    assert!(c.read_certificate(1).is_err());
    assert!(c.read_certificate(0).is_err());
}
#[test]
fn certificate_is_independent_copy_and_survives_reload() {
    let (key, cert) = identity();
    let first = LocalMfiAuthenticationClient::from_identity(&key, &cert).unwrap();
    let second = LocalMfiAuthenticationClient::from_identity(&key, &cert).unwrap();
    let mut copy = first.read_certificate(65525).unwrap();
    assert_eq!(copy, second.read_certificate(65525).unwrap());
    copy.fill(0);
    assert_eq!(first.read_certificate(65525).unwrap(), cert);
    assert_eq!(second.sign_challenge(&[0; 32]).unwrap().len(), 64);
}
#[test]
fn identity_file_bounds_are_enforced() {
    let (key, cert) = identity();
    assert!(LocalMfiAuthenticationClient::from_identity(&vec![1; 16385], &cert).is_err());
    assert!(LocalMfiAuthenticationClient::from_identity(&key, &vec![1; 16385]).is_err());
}
#[test]
fn rejects_non_pkcs7_and_multiple_certificates() {
    let (key, p7) = identity();
    assert!(
        LocalMfiAuthenticationClient::from_identity(&key, &unhex(CERTIFICATE_TEMPLATE)).is_err()
    );
    let mut info = cms::content_info::ContentInfo::from_der(&p7).unwrap();
    let mut signed = info
        .content
        .decode_as::<cms::signed_data::SignedData>()
        .unwrap();
    let extra = x509_cert::Certificate::from_der(&unhex(CERTIFICATE_TEMPLATE)).unwrap();
    signed
        .certificates
        .as_mut()
        .unwrap()
        .0
        .insert(cms::cert::CertificateChoices::Certificate(extra))
        .unwrap();
    info.content = der::Any::from_der(&signed.to_der().unwrap()).unwrap();
    assert!(LocalMfiAuthenticationClient::from_identity(&key, &info.to_der().unwrap()).is_err());
}
#[test]
fn rejects_wrong_curve_and_algorithm() {
    let (key, p7) = identity();
    for oid in ["1.3.132.0.34", "1.2.840.10045.3.1.1"] {
        let mut info = cms::content_info::ContentInfo::from_der(&p7).unwrap();
        let mut signed = info
            .content
            .decode_as::<cms::signed_data::SignedData>()
            .unwrap();
        let cert = match signed
            .certificates
            .as_ref()
            .unwrap()
            .0
            .iter()
            .next()
            .unwrap()
        {
            cms::cert::CertificateChoices::Certificate(c) => c.clone(),
            _ => unreachable!(),
        };
        let mut cert = cert;
        cert.tbs_certificate
            .subject_public_key_info
            .algorithm
            .parameters = Some(
            der::Any::from_der(
                &oid.parse::<der::asn1::ObjectIdentifier>()
                    .unwrap()
                    .to_der()
                    .unwrap(),
            )
            .unwrap(),
        );
        let mut certs = cms::signed_data::CertificateSet(der::asn1::SetOfVec::new());
        certs
            .0
            .insert(cms::cert::CertificateChoices::Certificate(cert))
            .unwrap();
        signed.certificates = Some(certs);
        info.content = der::Any::from_der(&signed.to_der().unwrap()).unwrap();
        assert!(
            LocalMfiAuthenticationClient::from_identity(&key, &info.to_der().unwrap()).is_err()
        );
    }
}
#[test]
fn der_to_raw_pads_signed_integer_values() {
    let der = unhex("300702020080020101");
    let raw = LocalMfiAuthenticationClient::der_to_raw(&der).unwrap();
    assert_eq!(raw[31], 128);
    assert_eq!(raw[63], 1);
    assert!(raw[..31].iter().all(|b| *b == 0));
}
#[test]
fn der_to_raw_rejects_zero_negative_oversized_or_extra_integers() {
    for hex in [
        "3006020100020101",
        "3006020180020101",
        "3009020101020101020101",
        "300602010102010100",
    ] {
        assert!(LocalMfiAuthenticationClient::der_to_raw(&unhex(hex)).is_err());
    }
    let mut der = vec![0x30, 38, 2, 33, 1];
    der.extend([0u8; 32]);
    der.extend([2, 1, 1]);
    assert!(LocalMfiAuthenticationClient::der_to_raw(&der).is_err());
}

#[test]
fn der_conversion_accepts_positive_256_bit_integers_without_scalar_reduction() {
    // Convert ASN.1 integers independently of the P-256 group order.
    let mut encoded = vec![0x30, 38, 2, 33, 0];
    encoded.extend([255; 32]);
    encoded.extend([2, 1, 1]);
    let raw = LocalMfiAuthenticationClient::der_to_raw(&encoded).unwrap();
    assert_eq!(&raw[..32], &[255; 32]);
    assert_eq!(raw[63], 1);
}
