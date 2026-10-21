// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! The ACME account key and the JSON Web Signatures that carry every ACME request (RFC 7515,
//! RFC 7518 ES256, RFC 7638 thumbprints, RFC 8555 section 6.2).
use base64ct::{Base64UrlUnpadded, Encoding};
use p256::{
    ecdsa::{signature::Signer, Signature, SigningKey},
    pkcs8::DecodePrivateKey,
};
use rustls_pki_types::{pem::PemObject, PrivatePkcs8KeyDer};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn b64(bytes: &[u8]) -> String {
    Base64UrlUnpadded::encode_string(bytes)
}
pub fn unb64(text: &str) -> Option<Vec<u8>> {
    Base64UrlUnpadded::decode_vec(text).ok()
}

/// An EC P-256 key, created once and kept with the configuration.
pub struct AccountKey {
    key: SigningKey,
    pem: String,
}
impl AccountKey {
    pub fn generate() -> std::io::Result<Self> {
        let pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(std::io::Error::other)?;
        Self::from_pem(&pair.serialize_pem())
            .ok_or_else(|| std::io::Error::other("generated account key unreadable"))
    }
    /// A PKCS #8 PEM P-256 key.
    pub fn from_pem(pem: &str) -> Option<Self> {
        let der = PrivatePkcs8KeyDer::from_pem_slice(pem.as_bytes()).ok()?;
        let key = SigningKey::from_pkcs8_der(der.secret_pkcs8_der()).ok()?;
        Some(Self {
            key,
            pem: pem.to_string(),
        })
    }
    pub fn pem(&self) -> &str {
        &self.pem
    }
    /// The public key's coordinates, each exactly 32 bytes big-endian.
    fn coordinates(&self) -> ([u8; 32], [u8; 32]) {
        let point = self.key.verifying_key().to_encoded_point(false);
        (
            coordinate(point.x().map_or(&[][..], |x| x.as_slice())),
            coordinate(point.y().map_or(&[][..], |y| y.as_slice())),
        )
    }
    pub fn jwk(&self) -> Value {
        let (x, y) = self.coordinates();
        json!({"crv": "P-256", "kty": "EC", "x": b64(&x), "y": b64(&y)})
    }
    /// RFC 7638: SHA-256 of the required members in lexicographic order, without whitespace.
    pub fn thumbprint(&self) -> String {
        let (x, y) = self.coordinates();
        let canonical = format!(
            r#"{{"crv":"P-256","kty":"EC","x":"{}","y":"{}"}}"#,
            b64(&x),
            b64(&y)
        );
        b64(&Sha256::digest(canonical.as_bytes()))
    }
    /// The key authorization of a challenge token (RFC 8555 section 8.1).
    pub fn key_authorization(&self, token: &str) -> String {
        format!("{token}.{}", self.thumbprint())
    }
    /// A flattened JWS: `protected` is the header, `payload` `None` the empty POST-as-GET body.
    pub fn sign(&self, protected: &Value, payload: Option<&Value>) -> Value {
        let protected = b64(protected.to_string().as_bytes());
        let payload = payload.map_or(String::new(), |p| b64(p.to_string().as_bytes()));
        // ES256 signatures are the raw 64-byte r‖s, not DER.
        let signature: Signature = self.key.sign(format!("{protected}.{payload}").as_bytes());
        json!({"protected": protected, "payload": payload, "signature": b64(&signature.to_bytes())})
    }
}

/// A big-endian coordinate as exactly 32 bytes: a leading sign byte stripped, short values padded.
fn coordinate(bytes: &[u8]) -> [u8; 32] {
    let bytes = if bytes.len() > 32 {
        &bytes[bytes.len() - 32..]
    } else {
        bytes
    };
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(bytes);
    out
}

/// The `_acme-challenge` TXT value: base64url(SHA-256(key authorization)).
pub fn txt_value(key_authorization: &str) -> String {
    b64(&Sha256::digest(key_authorization.as_bytes()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use p256::ecdsa::{signature::Verifier, VerifyingKey};
    /// Vectors computed once with OpenSSL 4.0.3 (`openssl ecparam -name prime256v1 -genkey`,
    /// `openssl pkcs8 -topk8`, then `openssl dgst -sha256 -binary | base64url` over the
    /// canonical JWK and over `<token>.<thumbprint>`). The x coordinate of PAD starts with a zero
    /// byte, so its base64url form keeps the leading `A`.
    pub const PAD_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgyxH4rmmN9Lg63oQA
1Dnr7PTwVDvC80SaegJPCo85546hRANCAAQAblNEIACglGCZSHGBTjJybh3fHROX
BzNwYcS7HqTqYDk+Ad2dHyUOeOjjyaC85fHwBQoMhZq75OAB6KywnpIv
-----END PRIVATE KEY-----
";
    const PLAIN_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgDoJFOH3wrmoBeJiI
8FtWur4gbYCUYmwpgaP/xCPGtYWhRANCAAQCibTNTaU2myVOCvF7mWa9VLx3N2Xm
vJtz7/H28ZHtCB0CIIFqRrWeEuV2CKXBhxpBcT8lIe3VI+hpqQsJRhH/
-----END PRIVATE KEY-----
";
    const TOKEN: &str = "evaGxfADs6pSRb2LAv9IZf17Dt3juxGJ-PCt92wr-oA";
    #[test]
    fn thumbprints_and_key_authorizations_match_openssl() {
        for (pem, x, y, thumbprint, txt) in [
            (
                PAD_KEY,
                "AG5TRCAAoJRgmUhxgU4ycm4d3x0TlwczcGHEux6k6mA",
                "OT4B3Z0fJQ546OPJoLzl8fAFCgyFmrvk4AHorLCeki8",
                "kwoEDrKwGaFVBvmJJgJs_xCnVzJQx8UYoWl5reFEwvw",
                "bk-Ne98TIlsEXefbn6COwqwJB7iTV_0APWCNDmeNy_Y",
            ),
            (
                PLAIN_KEY,
                "Aom0zU2lNpslTgrxe5lmvVS8dzdl5rybc-_x9vGR7Qg",
                "HQIggWpGtZ4S5XYIpcGHGkFxPyUh7dUj6GmpCwlGEf8",
                "tQ-baEkxU8fRpMacdsdXdnFjnwwOwOozUXVSHnG4En8",
                "V_OCYXBG0k6xQ2WJuUG_85XSJTrLrxhrHg4KOLePAS0",
            ),
        ] {
            let key = AccountKey::from_pem(pem).unwrap();
            assert_eq!(key.jwk(), json!({"crv":"P-256","kty":"EC","x":x,"y":y}));
            assert_eq!(key.thumbprint(), thumbprint);
            let authorization = key.key_authorization(TOKEN);
            assert_eq!(authorization, format!("{TOKEN}.{thumbprint}"));
            assert_eq!(txt_value(&authorization), txt);
        }
    }
    #[test]
    fn coordinates_are_exactly_32_bytes() {
        let mut signed = vec![0u8];
        signed.extend_from_slice(&[0xff; 32]);
        assert_eq!(coordinate(&signed), [0xff; 32]);
        let short = coordinate(&[1, 2]);
        assert_eq!(short[..30], [0; 30]);
        assert_eq!(short[30..], [1, 2]);
    }
    #[test]
    fn signatures_are_raw_es256_over_protected_and_payload() {
        let key = AccountKey::generate().unwrap();
        assert!(AccountKey::from_pem(key.pem()).is_some());
        assert!(AccountKey::from_pem("not a key").is_none());
        let protected = json!({"alg":"ES256","nonce":"n","url":"https://ca/x","jwk":key.jwk()});
        for payload in [Some(json!({"termsOfServiceAgreed":true})), None] {
            let jws = key.sign(&protected, payload.as_ref());
            let text = |k: &str| jws[k].as_str().unwrap().to_string();
            assert_eq!(
                serde_json::from_slice::<Value>(&unb64(&text("protected")).unwrap()).unwrap(),
                protected
            );
            match &payload {
                Some(p) => assert_eq!(
                    serde_json::from_slice::<Value>(&unb64(&text("payload")).unwrap()).unwrap(),
                    *p
                ),
                None => assert_eq!(text("payload"), ""),
            }
            let raw = unb64(&text("signature")).unwrap();
            assert_eq!(raw.len(), 64);
            let jwk = key.jwk();
            let mut point = vec![4u8];
            point.extend(unb64(jwk["x"].as_str().unwrap()).unwrap());
            point.extend(unb64(jwk["y"].as_str().unwrap()).unwrap());
            let verifying = VerifyingKey::from_sec1_bytes(&point).unwrap();
            let input = format!("{}.{}", text("protected"), text("payload"));
            verifying
                .verify(input.as_bytes(), &Signature::from_slice(&raw).unwrap())
                .unwrap();
        }
    }
}
