// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Locally generated RSA pairing material and its private, persistable plist representation.
use super::lockdown_plist_channel::{dictionary, encode, parse};
use crate::{invalid, Result};
use der::{
    asn1::{BitString, GeneralizedTime, ObjectIdentifier, OctetString, UtcTime},
    Decode, Encode,
};
use plist::Value;
use rsa::{
    pkcs1::{DecodeRsaPublicKey, EncodeRsaPublicKey},
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    signature::{SignatureEncoding, Signer, Verifier},
    RsaPrivateKey, RsaPublicKey,
};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use x509_cert::{
    ext::Extension,
    name::Name,
    serial_number::SerialNumber,
    spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned},
    time::{Time, Validity},
    Certificate, TbsCertificate, Version,
};
#[derive(Clone)]
pub struct LockdownPairRecord {
    pub host_id: String,
    pub system_buid: String,
    pub wifi_mac_address: String,
    pub device_public_key_pem: Vec<u8>,
    pub device_certificate_pem: Vec<u8>,
    pub host_private_key_pem: Vec<u8>,
    pub host_certificate_pem: Vec<u8>,
    pub root_private_key_pem: Vec<u8>,
    pub root_certificate_pem: Vec<u8>,
    pub escrow_bag: Option<Vec<u8>>,
}
impl fmt::Debug for LockdownPairRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LockdownPairRecord(redacted)")
    }
}
impl LockdownPairRecord {
    pub fn generate(
        device_public_key_pkcs1_pem: &[u8],
        wifi_address: &str,
        host_id: &str,
        system_buid: &str,
    ) -> Result<Self> {
        if [wifi_address, host_id, system_buid]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err(invalid("Pairing identifiers must not be blank"));
        }
        let device = RsaPublicKey::from_pkcs1_pem(
            std::str::from_utf8(device_public_key_pkcs1_pem)
                .map_err(|_| invalid("Invalid device public key"))?,
        )
        .map_err(|_| invalid("Expected a PKCS#1 RSA public key"))?;
        let root = RsaPrivateKey::new(&mut rand_core::OsRng, 2048).map_err(crypto_error)?;
        let host = RsaPrivateKey::new(&mut rand_core::OsRng, 2048).map_err(crypto_error)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(crypto_error)?;
        let root_cert = certificate(&root.to_public_key(), &root, now, true, false)?;
        let host_cert = certificate(&host.to_public_key(), &root, now, false, false)?;
        let device_cert = certificate(&device, &root, now, false, true)?;
        Ok(Self {
            host_id: host_id.into(),
            system_buid: system_buid.into(),
            wifi_mac_address: wifi_address.into(),
            device_public_key_pem: device_public_key_pkcs1_pem.to_vec(),
            device_certificate_pem: pem(
                "CERTIFICATE",
                &device_cert.to_der().map_err(crypto_error)?,
            ),
            host_private_key_pem: host
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(crypto_error)?
                .as_bytes()
                .to_vec(),
            host_certificate_pem: pem("CERTIFICATE", &host_cert.to_der().map_err(crypto_error)?),
            root_private_key_pem: root
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(crypto_error)?
                .as_bytes()
                .to_vec(),
            root_certificate_pem: pem("CERTIFICATE", &root_cert.to_der().map_err(crypto_error)?),
            escrow_bag: None,
        })
    }
    pub fn to_pair_request_dictionary(&self) -> plist::Dictionary {
        dictionary([
            (
                "DeviceCertificate",
                Value::Data(self.device_certificate_pem.clone()),
            ),
            (
                "HostCertificate",
                Value::Data(self.host_certificate_pem.clone()),
            ),
            ("HostID", Value::String(self.host_id.clone())),
            (
                "RootCertificate",
                Value::Data(self.root_certificate_pem.clone()),
            ),
            ("SystemBUID", Value::String(self.system_buid.clone())),
        ])
    }
    /// Private bytes for relay persistence. Never include these in logs or configuration exports.
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut d = self.to_pair_request_dictionary();
        for (k, v) in [
            ("DevicePublicKey", &self.device_public_key_pem),
            ("HostPrivateKey", &self.host_private_key_pem),
            ("RootPrivateKey", &self.root_private_key_pem),
        ] {
            d.insert(k.into(), Value::Data(v.clone()));
        }
        d.insert(
            "WiFiMACAddress".into(),
            Value::String(self.wifi_mac_address.clone()),
        );
        if let Some(b) = &self.escrow_bag {
            d.insert("EscrowBag".into(), Value::Data(b.clone()));
        }
        encode(&d)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let d = parse(bytes).map_err(|_| invalid("Invalid pair record"))?;
        let text = |k: &str| {
            d.get(k)
                .and_then(Value::as_string)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid("Invalid pair record"))
        };
        let data = |k: &str| {
            d.get(k)
                .and_then(Value::as_data)
                .filter(|b| !b.is_empty())
                .map(<[u8]>::to_vec)
                .ok_or_else(|| invalid("Invalid pair record"))
        };
        let result = Self {
            host_id: text("HostID")?,
            system_buid: text("SystemBUID")?,
            wifi_mac_address: text("WiFiMACAddress")?,
            device_public_key_pem: data("DevicePublicKey")?,
            device_certificate_pem: data("DeviceCertificate")?,
            host_private_key_pem: data("HostPrivateKey")?,
            host_certificate_pem: data("HostCertificate")?,
            root_private_key_pem: data("RootPrivateKey")?,
            root_certificate_pem: data("RootCertificate")?,
            escrow_bag: match d.get("EscrowBag") {
                None => None,
                Some(Value::Data(b)) => Some(b.clone()),
                _ => return Err(invalid("Invalid pair record")),
            },
        };
        result
            .validate()
            .map_err(|_| invalid("Invalid pair record"))?;
        Ok(result)
    }
    fn validate(&self) -> Result<()> {
        let key = |pem: &[u8]| {
            RsaPrivateKey::from_pkcs8_pem(std::str::from_utf8(pem).map_err(crypto_error)?)
                .map_err(crypto_error)
        };
        let root = key(&self.root_private_key_pem)?;
        let host = key(&self.host_private_key_pem)?;
        let root_cert = certificate_from_pem(&self.root_certificate_pem)?;
        let host_cert = certificate_from_pem(&self.host_certificate_pem)?;
        let device_cert = certificate_from_pem(&self.device_certificate_pem)?;
        for (cert, public) in [
            (&root_cert, root.to_public_key()),
            (&host_cert, host.to_public_key()),
            (
                &device_cert,
                RsaPublicKey::from_pkcs1_pem(
                    std::str::from_utf8(&self.device_public_key_pem).map_err(crypto_error)?,
                )
                .map_err(crypto_error)?,
            ),
        ] {
            let encoded = public.to_public_key_der().map_err(crypto_error)?;
            if cert
                .tbs_certificate
                .subject_public_key_info
                .to_der()
                .map_err(crypto_error)?
                != encoded.as_bytes()
            {
                return Err(invalid("Pair record public key mismatch"));
            }
            VerifyingKey::<Sha256>::new(root.to_public_key())
                .verify(
                    &cert.tbs_certificate.to_der().map_err(crypto_error)?,
                    &Signature::try_from(
                        cert.signature
                            .as_bytes()
                            .ok_or_else(|| invalid("Invalid certificate signature"))?,
                    )
                    .map_err(crypto_error)?,
                )
                .map_err(crypto_error)?;
        }
        Ok(())
    }
}
pub fn certificate_from_pem(bytes: &[u8]) -> Result<Certificate> {
    Certificate::from_der(&decode_pem("CERTIFICATE", bytes)?).map_err(crypto_error)
}
pub(crate) fn decode_pem(label: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    use base64ct::{Base64, Encoding};
    let s = std::str::from_utf8(bytes).map_err(crypto_error)?.trim();
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let body = s
        .strip_prefix(&begin)
        .and_then(|s| s.strip_suffix(&end))
        .ok_or_else(|| invalid("Invalid pairing PEM"))?;
    Base64::decode_vec(
        &body
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>(),
    )
    .map_err(crypto_error)
}
fn certificate(
    public: &RsaPublicKey,
    root: &RsaPrivateKey,
    now: Duration,
    ca: bool,
    ski: bool,
) -> Result<Certificate> {
    let algorithm = AlgorithmIdentifierOwned {
        oid: ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11"),
        parameters: Some(der::Any::null()),
    };
    let extension = |oid, critical, value: Vec<u8>| -> Result<Extension> {
        Ok(Extension {
            extn_id: ObjectIdentifier::new_unwrap(oid),
            critical,
            extn_value: OctetString::new(value).map_err(crypto_error)?,
        })
    };
    let mut extensions = vec![extension(
        "2.5.29.19",
        true,
        if ca {
            vec![0x30, 3, 1, 1, 0xff]
        } else {
            vec![0x30, 0]
        },
    )?];
    if ski {
        let public = public.to_pkcs1_der().map_err(crypto_error)?;
        let digest = sha1::Sha1::digest(public.as_bytes());
        let mut value = vec![4, 20];
        value.extend_from_slice(&digest);
        extensions.push(extension("2.5.29.14", false, value)?);
    }
    if !ca {
        extensions.push(extension("2.5.29.15", true, vec![3, 2, 5, 0xa0])?);
    }
    let time = |d: Duration| -> Result<Time> {
        let date = der::DateTime::from_unix_duration(d).map_err(crypto_error)?;
        Ok(if date.year() <= 2049 {
            Time::UtcTime(UtcTime::from_date_time(date).map_err(crypto_error)?)
        } else {
            Time::GeneralTime(GeneralizedTime::from_date_time(date))
        })
    };
    let tbs = TbsCertificate {
        version: Version::V3,
        serial_number: SerialNumber::new(&[0]).map_err(crypto_error)?,
        signature: algorithm.clone(),
        issuer: Name::default(),
        subject: Name::default(),
        validity: Validity {
            not_before: time(now)?,
            not_after: time(now + Duration::from_secs(10 * 365 * 24 * 60 * 60))?,
        },
        subject_public_key_info: SubjectPublicKeyInfoOwned::from_der(
            public.to_public_key_der().map_err(crypto_error)?.as_bytes(),
        )
        .map_err(crypto_error)?,
        issuer_unique_id: None,
        subject_unique_id: None,
        extensions: Some(extensions),
    };
    let bytes = tbs.to_der().map_err(crypto_error)?;
    let signature = SigningKey::<Sha256>::new(root.clone()).sign(&bytes);
    VerifyingKey::<Sha256>::new(root.to_public_key())
        .verify(&bytes, &signature)
        .map_err(crypto_error)?;
    Ok(Certificate {
        tbs_certificate: tbs,
        signature_algorithm: algorithm,
        signature: BitString::from_bytes(&signature.to_vec()).map_err(crypto_error)?,
    })
}
fn pem(label: &str, der: &[u8]) -> Vec<u8> {
    use base64ct::{Base64, Encoding};
    let encoded = Base64::encode_string(der);
    let mut text = format!("-----BEGIN {label}-----\n");
    for line in encoded.as_bytes().chunks(64) {
        text.push_str(std::str::from_utf8(line).unwrap());
        text.push('\n');
    }
    text.push_str(&format!("-----END {label}-----\n"));
    text.into_bytes()
}
fn crypto_error(_: impl fmt::Display) -> std::io::Error {
    invalid("Invalid Lockdown pairing cryptographic material")
}
