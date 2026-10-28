// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! TLS factory restricted to the already-paired USB Lockdown link.
use super::tls_duplex_channel::TlsDuplexChannel;
use crate::{invalid, DuplexStream, Result};
use der::{Decode, Encode};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
    DigitallySignedStruct, SignatureScheme,
};
use std::sync::Arc;
/// Creates a TLS client stream from DER certificates and a PKCS#8 private key.
/// Implementations own/close the input on failure; connect completes a bounded five-second handshake.
pub trait TlsConnector: Send + Sync {
    fn connect(
        &self,
        stream: Arc<dyn DuplexStream>,
        certificate_chain: Vec<CertificateDer<'static>>,
        private_key: PrivateKeyDer<'static>,
    ) -> Result<Arc<dyn DuplexStream>>;
}
#[derive(Default)]
pub struct RustlsConnector;
impl TlsConnector for RustlsConnector {
    fn connect(
        &self,
        stream: Arc<dyn DuplexStream>,
        certificate_chain: Vec<CertificateDer<'static>>,
        private_key: PrivateKeyDer<'static>,
    ) -> Result<Arc<dyn DuplexStream>> {
        let result = (|| {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let mut config = rustls::ClientConfig::builder_with_provider(provider)
                .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
                .map_err(|_| invalid("TLS versions unavailable"))?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(LockdownVerifier))
                .with_client_auth_cert(certificate_chain, private_key)
                .map_err(|_| invalid("Invalid Lockdown TLS identity"))?;
            config.enable_sni = false;
            let connection = rustls::ClientConnection::new(
                Arc::new(config),
                ServerName::try_from("Device").unwrap().to_owned(),
            )
            .map_err(|_| invalid("Could not create Lockdown TLS client"))?;
            TlsDuplexChannel::open(stream.clone(), connection)
        })();
        match result {
            Ok(c) => Ok(c),
            Err(e) => {
                let _ = stream.close();
                Err(e)
            }
        }
    }
}
/// Lockdown trusts its USB peer's certificate but verifies proof of its private key.
#[derive(Debug)]
struct LockdownVerifier;
impl LockdownVerifier {
    fn verify(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
        tls13: bool,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        let algorithms = rustls::crypto::ring::default_provider().signature_verification_algorithms;
        let ordinary = if tls13 {
            rustls::crypto::verify_tls13_signature(message, certificate, signature, &algorithms)
        } else {
            rustls::crypto::verify_tls12_signature(message, certificate, signature, &algorithms)
        };
        if ordinary.is_ok() {
            return ordinary;
        }
        // webpki rejects the source profile's serial zero. Parse only the SPKI here;
        // ring still checks the negotiated handshake signature against that key.
        let bad = || rustls::Error::InvalidCertificate(rustls::CertificateError::BadSignature);
        let cert = x509_cert::Certificate::from_der(certificate.as_ref()).map_err(|_| bad())?;
        let spki = cert.tbs_certificate.subject_public_key_info;
        let key = spki.subject_public_key.as_bytes().ok_or_else(bad)?;
        let algorithm = spki.algorithm.to_der().map_err(|_| bad())?;
        let algorithm = der::AnyRef::from_der(&algorithm).map_err(|_| bad())?;
        let candidates = algorithms
            .mapping
            .iter()
            .find(|(scheme, _)| *scheme == signature.scheme)
            .map(|(_, a)| *a)
            .ok_or_else(bad)?;
        if tls13
            && !matches!(
                signature.scheme,
                SignatureScheme::RSA_PSS_SHA256
                    | SignatureScheme::RSA_PSS_SHA384
                    | SignatureScheme::RSA_PSS_SHA512
                    | SignatureScheme::ECDSA_NISTP256_SHA256
                    | SignatureScheme::ECDSA_NISTP384_SHA384
                    | SignatureScheme::ED25519
            )
        {
            return Err(bad());
        }
        for verifier in candidates
            .iter()
            .take(if tls13 { 1 } else { candidates.len() })
        {
            if verifier.public_key_alg_id().as_ref() == algorithm.value()
                && verifier
                    .verify_signature(key, message, signature.signature())
                    .is_ok()
            {
                return Ok(HandshakeSignatureValid::assertion());
            }
        }
        Err(bad())
    }
}
impl ServerCertVerifier for LockdownVerifier {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.verify(m, c, s, false)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.verify(m, c, s, true)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
