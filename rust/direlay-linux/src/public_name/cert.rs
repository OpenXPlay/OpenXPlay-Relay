// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! The certificate key and its CSR, and what the relay checks in an issued chain.
use super::jws::b64;
use der::{oid::AssociatedOid, Decode};
use rustls_pki_types::{pem::PemObject, CertificateDer};
use x509_cert::{
    ext::pkix::{name::GeneralName, AuthorityKeyIdentifier, SubjectAltName},
    Certificate,
};

/// A new EC P-256 certificate key (separate from the account key) and its CSR: SAN dNSName
/// `name`, signed SHA256withECDSA.
pub fn new_key_and_csr(name: &str) -> std::io::Result<(rcgen::KeyPair, Vec<u8>)> {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(std::io::Error::other)?;
    let mut params =
        rcgen::CertificateParams::new(vec![name.to_string()]).map_err(std::io::Error::other)?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    let csr = params
        .serialize_request(&key)
        .map_err(std::io::Error::other)?;
    Ok((key, csr.der().to_vec()))
}

/// The leaf of an issued chain, as far as the relay cares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    pub names: Vec<String>,
    pub not_before: u64,
    pub not_after: u64,
    /// The uncompressed EC point (or other raw key bits) of its subject public key.
    pub public_key: Vec<u8>,
    /// The ARI certificate identifier, when the leaf names its issuer's key.
    pub cert_id: Option<String>,
}
impl Leaf {
    pub fn lifetime(&self) -> u64 {
        self.not_after.saturating_sub(self.not_before)
    }
}

/// The chain's certificates in order, `None` unless it holds at least one.
pub fn chain(pem: &[u8]) -> Option<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (!certs.is_empty()).then_some(certs)
}

pub fn leaf(chain_pem: &[u8]) -> Option<Leaf> {
    let first = chain(chain_pem)?.into_iter().next()?;
    leaf_der(&first)
}
pub fn leaf_der(der: &[u8]) -> Option<Leaf> {
    let cert = Certificate::from_der(der).ok()?;
    let tbs = &cert.tbs_certificate;
    let mut names = Vec::new();
    let mut key_id = None;
    for extension in tbs.extensions.iter().flatten() {
        if extension.extn_id == SubjectAltName::OID {
            let san = SubjectAltName::from_der(extension.extn_value.as_bytes()).ok()?;
            for general in san.0 {
                if let GeneralName::DnsName(dns) = general {
                    names.push(dns.as_str().to_ascii_lowercase());
                }
            }
        } else if extension.extn_id == AuthorityKeyIdentifier::OID {
            let aki = AuthorityKeyIdentifier::from_der(extension.extn_value.as_bytes()).ok()?;
            key_id = aki.key_identifier.map(|id| id.as_bytes().to_vec());
        }
    }
    Some(Leaf {
        names,
        not_before: tbs.validity.not_before.to_unix_duration().as_secs(),
        not_after: tbs.validity.not_after.to_unix_duration().as_secs(),
        public_key: tbs
            .subject_public_key_info
            .subject_public_key
            .raw_bytes()
            .to_vec(),
        cert_id: key_id.map(|id| cert_id(&id, tbs.serial_number.as_bytes())),
    })
}

/// RFC 9773 certificate identifier: base64url(AKI keyIdentifier) `.` base64url(the serial
/// number's DER content bytes).
pub fn cert_id(key_identifier: &[u8], serial: &[u8]) -> String {
    format!("{}.{}", b64(key_identifier), b64(serial))
}

/// The dNSNames a CSR asks for and its raw public key, for the tests' fake CA.
#[cfg(test)]
pub fn csr_request(csr: &[u8]) -> Option<(Vec<String>, Vec<u8>)> {
    use x509_cert::request::{CertReq, ExtensionReq};
    let request = CertReq::from_der(csr).ok()?;
    let mut names = Vec::new();
    for attribute in request.info.attributes.iter() {
        if attribute.oid != ExtensionReq::OID {
            continue;
        }
        for value in attribute.values.iter() {
            for extension in value.decode_as::<ExtensionReq>().ok()?.0 {
                if extension.extn_id == SubjectAltName::OID {
                    for general in SubjectAltName::from_der(extension.extn_value.as_bytes())
                        .ok()?
                        .0
                    {
                        if let GeneralName::DnsName(dns) = general {
                            names.push(dns.as_str().to_string());
                        }
                    }
                }
            }
        }
    }
    let key = request
        .info
        .public_key
        .subject_public_key
        .raw_bytes()
        .to_vec();
    Some((names, key))
}

/// The raw public key of a PKCS #8 PEM key.
pub fn public_key(key_pem: &str) -> Option<Vec<u8>> {
    rcgen::KeyPair::from_pem(key_pem)
        .ok()
        .map(|k| k.public_key_raw().to_vec())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Unreadable,
    KeyMismatch,
    NameMismatch,
    NotYetValid,
    Expired,
}

/// The leaf of `chain_pem` if it belongs to `key_pem`, names `name` and, when `now` is known, is
/// valid at `now`.
pub fn check(
    chain_pem: &[u8],
    key_pem: &str,
    name: &str,
    now: Option<u64>,
) -> Result<Leaf, Problem> {
    let leaf = leaf(chain_pem).ok_or(Problem::Unreadable)?;
    if public_key(key_pem).ok_or(Problem::Unreadable)? != leaf.public_key {
        return Err(Problem::KeyMismatch);
    }
    if !leaf.names.iter().any(|n| n == name) {
        return Err(Problem::NameMismatch);
    }
    match now {
        Some(now) if now < leaf.not_before => Err(Problem::NotYetValid),
        Some(now) if now >= leaf.not_after => Err(Problem::Expired),
        _ => Ok(leaf),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::public_name::jws::tests::PAD_KEY;
    /// Made with `openssl x509 -req` for the PAD key: serial 0x87654321 (so its DER content
    /// starts with a zero byte), SAN myrelay.duckdns.org, AKI A8329B0A…C186, valid
    /// 2026-10-21T05:53:00Z to 2027-01-19T05:53:00Z. Its certID, computed with openssl and
    /// base64url by hand, is below.
    pub const LEAF: &str = "-----BEGIN CERTIFICATE-----
MIIBSjCB8KADAgECAgUAh2VDITAKBggqhkjOPQQDAjAUMRIwEAYDVQQDDAlWZWN0
b3IgQ0EwHhcNMjYxMDIxMDU1MzAwWhcNMjcwMTE5MDU1MzAwWjAAMFkwEwYHKoZI
zj0CAQYIKoZIzj0DAQcDQgAEAG5TRCAAoJRgmUhxgU4ycm4d3x0TlwczcGHEux6k
6mA5PgHdnR8lDnjo48mgvOXx8AUKDIWau+TgAeissJ6SL6NDMEEwHgYDVR0RBBcw
FYITbXlyZWxheS5kdWNrZG5zLm9yZzAfBgNVHSMEGDAWgBSoMpsKIGaE2neG97Pw
RwaDAlrBhjAKBggqhkjOPQQDAgNJADBGAiEAr2fMXcU3TqpTSgrsryAuQWvS6jjQ
LZFOLbVPSXrvwL4CIQCDGiLKcgJpIZeFHTgYbIaUyqkkvBFIX5qptqNJ49w4lw==
-----END CERTIFICATE-----
";
    #[test]
    fn reads_the_leaf_and_its_ari_identifier() {
        let leaf = leaf(LEAF.as_bytes()).unwrap();
        assert_eq!(leaf.names, ["myrelay.duckdns.org"]);
        assert_eq!(leaf.not_before, 1_792_561_980);
        assert_eq!(leaf.not_after, 1_800_337_980);
        assert_eq!(leaf.lifetime(), 90 * 86_400);
        assert_eq!(
            leaf.cert_id.as_deref(),
            Some("qDKbCiBmhNp3hvez8EcGgwJawYY.AIdlQyE")
        );
        // The example of RFC 9773 section 4.1.
        let aki = [
            0x69, 0x88, 0x5b, 0x6b, 0x87, 0x46, 0x40, 0x41, 0xe1, 0xb3, 0x7b, 0x84, 0x7b, 0xa0,
            0xae, 0x2c, 0xde, 0x01, 0xc8, 0xd4,
        ];
        assert_eq!(
            cert_id(&aki, &[0x00, 0x87, 0x65, 0x43, 0x21]),
            "aYhba4dGQEHhs3uEe6CuLN4ByNQ.AIdlQyE"
        );
        assert!(super::leaf(b"garbage").is_none());
    }
    #[test]
    fn checks_key_name_and_validity() {
        let name = "myrelay.duckdns.org";
        let inside = Some(1_795_000_000);
        assert!(check(LEAF.as_bytes(), PAD_KEY, name, inside).is_ok());
        assert!(check(LEAF.as_bytes(), PAD_KEY, name, None).is_ok());
        let (other, csr) = new_key_and_csr(name).unwrap();
        assert!(!csr.is_empty());
        assert_eq!(
            check(LEAF.as_bytes(), &other.serialize_pem(), name, inside),
            Err(Problem::KeyMismatch)
        );
        assert_eq!(
            check(LEAF.as_bytes(), PAD_KEY, "other.duckdns.org", inside),
            Err(Problem::NameMismatch)
        );
        assert_eq!(
            check(LEAF.as_bytes(), PAD_KEY, name, Some(1_800_337_980)),
            Err(Problem::Expired)
        );
        assert_eq!(
            check(LEAF.as_bytes(), PAD_KEY, name, Some(1_792_561_979)),
            Err(Problem::NotYetValid)
        );
        assert_eq!(check(b"", PAD_KEY, name, None), Err(Problem::Unreadable));
    }
    #[test]
    fn csr_names_only_the_domain_and_carries_the_new_key() {
        let (key, csr) = new_key_and_csr("car.dynv6.net").unwrap();
        let (names, public) = csr_request(&csr).unwrap();
        assert_eq!(names, ["car.dynv6.net"]);
        assert_eq!(public, key.public_key_raw());
        assert_eq!(public_key(&key.serialize_pem()).unwrap(), public);
    }
}
