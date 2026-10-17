// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
use chacha20poly1305::{
    aead::{Aead, Payload},
    ChaCha20Poly1305, KeyInit,
};
use ed25519_dalek::{Signer, Verifier};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha512};
#[derive(Clone, Debug)]
pub struct X25519KeyPair {
    pub private_key: [u8; 32],
    pub public_key: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct Ed25519KeyPair {
    pub private_key: [u8; 32],
    pub public_key: [u8; 32],
}
pub struct AirPlayCrypto;
impl AirPlayCrypto {
    pub fn x25519_generate() -> Result<X25519KeyPair> {
        let mut private_key = [0; 32];
        OsRng
            .try_fill_bytes(&mut private_key)
            .map_err(std::io::Error::other)?;
        let secret = x25519_dalek::StaticSecret::from(private_key);
        Ok(X25519KeyPair {
            private_key,
            public_key: x25519_dalek::PublicKey::from(&secret).to_bytes(),
        })
    }
    pub fn x25519_shared(
        private_key_raw: &[u8; 32],
        peer_public_key_raw: &[u8; 32],
    ) -> Result<[u8; 32]> {
        let shared = x25519_dalek::StaticSecret::from(*private_key_raw)
            .diffie_hellman(&x25519_dalek::PublicKey::from(*peer_public_key_raw));
        if !shared.was_contributory() {
            return Err(std::io::Error::other("X25519 agreement failed"));
        }
        Ok(shared.to_bytes())
    }
    pub fn ed25519_generate() -> Result<Ed25519KeyPair> {
        let mut private_key = [0; 32];
        OsRng
            .try_fill_bytes(&mut private_key)
            .map_err(std::io::Error::other)?;
        Ok(Ed25519KeyPair {
            private_key,
            public_key: ed25519_dalek::SigningKey::from_bytes(&private_key)
                .verifying_key()
                .to_bytes(),
        })
    }
    pub fn ed25519_sign(private_key_raw: &[u8; 32], data: &[u8]) -> Result<[u8; 64]> {
        Ok(ed25519_dalek::SigningKey::from_bytes(private_key_raw)
            .sign(data)
            .to_bytes())
    }
    pub fn ed25519_verify(public_key_raw: &[u8; 32], data: &[u8], signature: &[u8]) -> bool {
        let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(public_key_raw) else {
            return false;
        };
        let Ok(sig) = ed25519_dalek::Signature::from_slice(signature) else {
            return false;
        };
        key.verify(data, &sig).is_ok()
    }
    pub fn hkdf_sha512(
        input_key_material: &[u8],
        salt: &[u8],
        info: &[u8],
        length: usize,
    ) -> Result<Vec<u8>> {
        if length > 255 * 64 {
            return Err(std::io::Error::other("HKDF output too long"));
        }
        let mut output = vec![0; length];
        hkdf::Hkdf::<Sha512>::new(Some(salt), input_key_material)
            .expand(info, &mut output)
            .map_err(|_| std::io::Error::other("HKDF output too long"))?;
        Ok(output)
    }
    pub fn sha512(parts: &[&[u8]]) -> [u8; 64] {
        let mut h = Sha512::new();
        for part in parts {
            h.update(part);
        }
        h.finalize().into()
    }
    pub fn chacha_seal(
        key: &[u8; 32],
        nonce: &[u8; 12],
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>> {
        ChaCha20Poly1305::new(key.into())
            .encrypt(
                nonce.into(),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| std::io::Error::other("ChaCha20-Poly1305 seal failed"))
    }
    pub fn chacha_open(
        key: &[u8; 32],
        nonce: &[u8; 12],
        ciphertext_and_tag: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>> {
        ChaCha20Poly1305::new(key.into())
            .decrypt(
                nonce.into(),
                Payload {
                    msg: ciphertext_and_tag,
                    aad,
                },
            )
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "mac check in ChaCha20Poly1305 failed",
                )
            })
    }
    pub fn nonce64(counter: u64) -> [u8; 12] {
        let mut nonce = [0; 12];
        nonce[4..].copy_from_slice(&counter.to_le_bytes());
        nonce
    }
    pub fn nonce_label(label: &str) -> [u8; 12] {
        let mut nonce = [0; 12];
        for (a, b) in nonce[4..].iter_mut().zip(label.chars()) {
            *a = if b.is_ascii() { b as u8 } else { b'?' };
        }
        nonce
    }
}
pub const MAC_BITS: u64 = 128;
pub const NONCE_SIZE: u64 = 12;
pub const LABEL_SIZE: u64 = 8;
pub const MAX_RETAINED_BUFFER: u64 = 1048576;
