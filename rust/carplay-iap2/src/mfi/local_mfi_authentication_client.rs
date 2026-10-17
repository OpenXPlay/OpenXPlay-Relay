// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use der::Decode;
use p256::ecdsa::signature::hazmat::RandomizedPrehashSigner;
use p256::{
    ecdsa::{Signature, SigningKey},
    pkcs8::DecodePrivateKey,
};

/// State owned by `LocalMfiAuthenticationClient`; construct through its factory.
pub struct LocalMfiAuthenticationClient {
    key: SigningKey,
    certificate: Vec<u8>,
}
impl LocalMfiAuthenticationClient {
    pub fn from_identity(private_key_pkcs8: &[u8], certificate_p7b: &[u8]) -> Result<Self> {
        for bytes in [private_key_pkcs8, certificate_p7b] {
            if bytes.is_empty() {
                return Err(invalid("Local identity file is empty"));
            }
            if bytes.len() > 16384 {
                return Err(invalid("Local identity file is too large"));
            }
        }
        let key = SigningKey::from_pkcs8_der(private_key_pkcs8)
            .map_err(|_| invalid("Expected a P-256 private key"))?;
        let info = cms::content_info::ContentInfo::from_der(certificate_p7b)
            .map_err(|_| invalid("Invalid PKCS#7 accessory certificate"))?;
        if info.content_type.to_string() != "1.2.840.113549.1.7.2" {
            return Err(invalid("Expected PKCS#7 SignedData"));
        }
        let signed = info
            .content
            .decode_as::<cms::signed_data::SignedData>()
            .map_err(|_| invalid("Invalid PKCS#7 accessory certificate"))?;
        let certs = signed
            .certificates
            .ok_or_else(|| invalid("Expected one accessory certificate"))?;
        if certs.0.len() != 1 {
            return Err(invalid("Expected one accessory certificate"));
        }
        let cert = match certs.0.iter().next().unwrap() {
            cms::cert::CertificateChoices::Certificate(c) => c,
            _ => return Err(invalid("Expected an EC accessory certificate")),
        };
        let spki = &cert.tbs_certificate.subject_public_key_info;
        if spki.algorithm.oid.to_string() != "1.2.840.10045.2.1" {
            return Err(invalid("Expected an EC accessory certificate"));
        }
        let curve = spki
            .algorithm
            .parameters
            .as_ref()
            .and_then(|v| v.decode_as::<der::asn1::ObjectIdentifier>().ok());
        if curve.map(|v| v.to_string()).as_deref() != Some("1.2.840.10045.3.1.7") {
            return Err(invalid("Expected a P-256 accessory certificate"));
        }
        let public = spki
            .subject_public_key
            .as_bytes()
            .ok_or_else(|| invalid("Invalid EC accessory certificate"))?;
        let public = p256::ecdsa::VerifyingKey::from_sec1_bytes(public)
            .map_err(|_| invalid("Invalid EC accessory certificate"))?;
        if &public != key.verifying_key() {
            return Err(invalid("Local private key does not match certificate"));
        }
        Ok(Self {
            key,
            certificate: certificate_p7b.to_vec(),
        })
    }

    pub fn der_to_raw(der: &[u8]) -> Result<[u8; 64]> {
        // Decode integers rather than scalar values to accept all positive 256-bit integers.
        let sequence = der::asn1::SequenceOf::<der::asn1::Uint, 2>::from_der(der)
            .map_err(|_| invalid("Invalid ECDSA signature"))?;
        if sequence.len() != 2 {
            return Err(invalid("Invalid ECDSA signature"));
        }
        let mut raw = [0u8; 64];
        for (i, value) in sequence.iter().enumerate() {
            let b = value.as_bytes();
            if b.len() > 32 || b.iter().all(|b| *b == 0) {
                return Err(invalid("Invalid ECDSA integer"));
            }
            raw[i * 32 + 32 - b.len()..i * 32 + 32].copy_from_slice(b);
        }
        Ok(raw)
    }
}
impl MfiAuthenticator for LocalMfiAuthenticationClient {
    fn protocol_major(&self) -> Result<u8> {
        Ok(3)
    }
    fn read_certificate(&self, maximum_output_length: usize) -> Result<Vec<u8>> {
        if maximum_output_length == 0 || self.certificate.len() > maximum_output_length {
            return Err(invalid(
                "Local certificate exceeds the requested output limit",
            ));
        }
        Ok(self.certificate.clone())
    }
    fn sign_challenge(&self, challenge: &[u8]) -> Result<Vec<u8>> {
        if challenge.len() != 32 {
            return Err(invalid("Local MFi v3 expects a 32-byte digest"));
        }
        let signature: Signature = self
            .key
            .sign_prehash_with_rng(&mut rand_core::OsRng, challenge)
            .map_err(|_| invalid("Invalid ECDSA digest"))?;
        Ok(signature.to_bytes().to_vec())
    }
    fn baa_certificates(&self) -> Result<BaaCertificatePair> {
        Err(invalid(
            "BAA certificates are not supported by the local MFi provider",
        ))
    }
}

/// Protocol message, field, or framing constant.
pub const DIRECTORY: &str = "offline-mfi";

/// Constant expression: `16 * 1024`.
pub const MAX_FILE_BYTES: u64 = 16384;
