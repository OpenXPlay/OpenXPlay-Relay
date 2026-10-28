// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-27
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

//! Remote certificate and signing protocol. The host supplies a bounded, redirect-free HTTP client.
use super::{BaaCertificatePair, MfiAuthenticator, MfiCertificateType};
use crate::{invalid, Iap2AuthenticationMessages, Result};
use base64ct::{Base64, Encoding};
use rand_core::{OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub const MAX_HTTP_BODY: usize = 2 * 1024 * 1024;
const MAX_OUTPUT: usize = 65_525;
/// Request values can contain secrets and deliberately do not implement Debug.
pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Vec<u8>>,
}
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}
/// The host must enforce a 5 s connect / 10 s read timeout, a 2 MiB body limit,
/// certificate verification for HTTPS, and no redirects. Errors must contain no request data.
pub trait Http: Send + Sync {
    fn send(&self, request: Request) -> Result<Response>;
}
struct Certificate {
    kind: MfiCertificateType,
    major: u8,
    bytes: Vec<u8>,
    baa: Option<BaaCertificatePair>,
}
pub struct RemoteAuthenticator {
    base: String,
    token: String,
    http: Arc<dyn Http>,
    certificate: Certificate,
    operation: Mutex<()>,
}
impl RemoteAuthenticator {
    /// Start a fresh server session, then validate and cache its certificate.
    /// Reset is never retried. Certificate and signing requests each have two attempts.
    pub fn connect(base: &str, token: String, http: Arc<dyn Http>) -> Result<Self> {
        let base = base.trim().trim_end_matches('/');
        if !(base.starts_with("https://") || base.starts_with("http://"))
            || token.contains(['\0', '\r', '\n'])
        {
            return Err(invalid("Invalid remote MFi configuration"));
        }
        let mut client = Self {
            base: base.into(),
            token,
            http,
            certificate: Certificate {
                kind: MfiCertificateType::Mfi,
                major: 0,
                bytes: vec![],
                baa: None,
            },
            operation: Mutex::new(()),
        };
        client.request("POST", "/mfi/reset", Some(b"{}".to_vec()), false)?;
        let value = client.request("GET", "/mfi/certificate", None, true)?;
        client.certificate = decode_certificate(&value)?;
        Ok(client)
    }
    fn request(
        &self,
        method: &'static str,
        path: &str,
        body: Option<Vec<u8>>,
        retry: bool,
    ) -> Result<Value> {
        let attempts = if retry { 2 } else { 1 };
        for attempt in 0..attempts {
            let mut headers = vec![("Accept", "application/json; charset=utf-8".into())];
            if !self.token.is_empty() {
                headers.push(("Authorization", format!("Bearer {}", self.token)));
            }
            if body.is_some() {
                headers.push(("Content-Type", "application/json; charset=utf-8".into()));
            }
            let response = self.http.send(Request {
                method,
                url: format!("{}{path}", self.base),
                headers,
                body: body.clone(),
            });
            match response {
                Ok(response) => {
                    if response.body.len() > MAX_HTTP_BODY {
                        return Err(invalid("Remote MFi response exceeds 2097152 bytes"));
                    }
                    if (200..300).contains(&response.status) {
                        if path == "/mfi/reset" {
                            return Ok(Value::Null);
                        }
                        return serde_json::from_slice(&response.body)
                            .map_err(|_| invalid("Invalid remote MFi JSON"));
                    }
                    if attempt + 1 == attempts
                        || !(matches!(response.status, 408 | 429)
                            || (500..600).contains(&response.status))
                    {
                        // Server detail and transport errors may echo the bearer token.
                        return Err(std::io::Error::other(format!(
                            "Remote MFi request failed with HTTP {}",
                            response.status
                        )));
                    }
                }
                Err(_) if attempt + 1 < attempts => (),
                Err(_) => return Err(std::io::Error::other("Remote MFi request failed")),
            }
        }
        unreachable!()
    }
}
fn decoded(value: &Value, key: &str) -> Result<Vec<u8>> {
    let bytes = value[key]
        .as_str()
        .and_then(|v| Base64::decode_vec(v).ok())
        .ok_or_else(|| invalid("Invalid remote MFi base64"))?;
    if bytes.is_empty() || bytes.len() > MAX_OUTPUT {
        return Err(invalid("Invalid remote MFi output length"));
    }
    Ok(bytes)
}
fn decode_certificate(value: &Value) -> Result<Certificate> {
    let major = value["protocolMajor"]
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .ok_or_else(|| invalid("Invalid remote MFi protocol major"))?;
    let kind = match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("mfi")
        .to_ascii_lowercase()
        .as_str()
    {
        "mfi" => MfiCertificateType::Mfi,
        "baa" => MfiCertificateType::Baa,
        _ => return Err(invalid("Unsupported remote MFi certificate type")),
    };
    let mut bytes = decoded(value, "certificate")?;
    let expected = value["certificateSha256"]
        .as_str()
        .ok_or_else(|| invalid("Missing remote MFi certificate SHA-256"))?;
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if expected.len() != 64 || !expected.eq_ignore_ascii_case(&actual) {
        return Err(invalid(
            "Remote certificate SHA-256 does not match certificateSha256",
        ));
    }
    let baa = if kind == MfiCertificateType::Baa {
        if bytes.len() < 8 {
            return Err(invalid("BAA certificate package is too short"));
        }
        let leaf = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
        let intermediate = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        if leaf == 0
            || intermediate == 0
            || 8u64 + leaf as u64 + intermediate as u64 != bytes.len() as u64
        {
            return Err(invalid("BAA certificate package has invalid lengths"));
        }
        let pair = BaaCertificatePair {
            leaf: bytes[8..8 + leaf].to_vec(),
            intermediate: bytes[8 + leaf..].to_vec(),
        };
        bytes =
            Iap2AuthenticationMessages::baa_certificate_package(&pair.leaf, &pair.intermediate)?
                .payload;
        if bytes.len() > MAX_OUTPUT {
            return Err(invalid("Remote iAP2 certificate payload is too large"));
        }
        Some(pair)
    } else {
        None
    };
    Ok(Certificate {
        kind,
        major,
        bytes,
        baa,
    })
}
impl MfiAuthenticator for RemoteAuthenticator {
    fn certificate_type(&self) -> MfiCertificateType {
        self.certificate.kind
    }
    fn protocol_major(&self) -> Result<u8> {
        Ok(self.certificate.major)
    }
    fn read_certificate(&self, maximum: usize) -> Result<Vec<u8>> {
        if !(1..=MAX_OUTPUT).contains(&maximum) || self.certificate.bytes.len() > maximum {
            return Err(invalid("Invalid remote certificate length"));
        }
        Ok(self.certificate.bytes.clone())
    }
    fn sign_challenge(&self, challenge: &[u8]) -> Result<Vec<u8>> {
        if !(1..=128).contains(&challenge.len()) {
            return Err(invalid("challenge must be 1..128 bytes"));
        }
        let _guard = self.operation.lock().unwrap();
        let mut id = [0u8; 16];
        OsRng.fill_bytes(&mut id);
        id[6] = (id[6] & 0x0f) | 0x40;
        id[8] = (id[8] & 0x3f) | 0x80;
        let id = id
            .iter()
            .enumerate()
            .map(|(i, b)| {
                format!(
                    "{}{b:02x}",
                    if [4, 6, 8, 10].contains(&i) { "-" } else { "" }
                )
            })
            .collect::<String>();
        let body = serde_json::to_vec(
            &json!({"challenge":Base64::encode_string(challenge),"requestId":id}),
        )
        .unwrap();
        decoded(
            &self.request("POST", "/mfi/sign", Some(body), true)?,
            "signature",
        )
    }
    fn baa_certificates(&self) -> Result<BaaCertificatePair> {
        self.certificate
            .baa
            .clone()
            .ok_or_else(|| invalid("Remote MFi service is not using BAA certificates"))
    }
}
