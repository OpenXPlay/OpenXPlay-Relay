// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::{collections::BTreeMap, sync::Arc};
fn item(kind: u8, value: impl Into<Vec<u8>>) -> Tlv8Item {
    Tlv8Item {
        kind,
        value: value.into(),
    }
}
fn response(items: &[Tlv8Item]) -> Result<Vec<u8>> {
    Tlv8Codec::encode(items)
}
fn error(state: u8) -> Result<Vec<u8>> {
    response(&[item(6, [state]), item(7, [2])])
}
fn required(tlv: &BTreeMap<u8, Vec<u8>>, kind: u8) -> Result<&[u8]> {
    tlv.get(&kind)
        .map(Vec::as_slice)
        .ok_or_else(|| std::io::Error::other("Missing pairing field"))
}
fn key32(v: &[u8]) -> Result<[u8; 32]> {
    v.try_into()
        .map_err(|_| std::io::Error::other("Invalid pairing key length"))
}
fn derive(key: &[u8], salt: &str, info: &str) -> Result<[u8; 32]> {
    key32(&AirPlayCrypto::hkdf_sha512(
        key,
        salt.as_bytes(),
        info.as_bytes(),
        32,
    )?)
}
pub struct PairVerify {
    identity: Arc<AirPlayIdentity>,
    pairings: Arc<dyn PairingStore>,
    ephemeral_public_key: Option<[u8; 32]>,
    client_ephemeral_public_key: Option<[u8; 32]>,
    shared_secret: Option<[u8; 32]>,
    encryption_key: Option<[u8; 32]>,
    keys: Option<([u8; 32], [u8; 32])>,
    controller_id: Option<String>,
}
impl PairVerify {
    pub fn new(identity: Arc<AirPlayIdentity>, pairings: Arc<dyn PairingStore>) -> Self {
        Self {
            identity,
            pairings,
            ephemeral_public_key: None,
            client_ephemeral_public_key: None,
            shared_secret: None,
            encryption_key: None,
            keys: None,
            controller_id: None,
        }
    }
    pub fn handle(&mut self, body: &[u8]) -> Result<Vec<u8>> {
        let tlv = Tlv8Codec::decode(body)?;
        let state = tlv.get(&6).and_then(|v| v.first()).copied().unwrap_or(0);
        match self.handle_state(state, &tlv) {
            Ok(v) => Ok(v),
            Err(_) => error(state),
        }
    }
    fn handle_state(&mut self, state: u8, tlv: &BTreeMap<u8, Vec<u8>>) -> Result<Vec<u8>> {
        match state {
            1 => {
                let Ok(client) = required(tlv, 3) else {
                    return error(2);
                };
                let client = key32(client)?;
                let pair = AirPlayCrypto::x25519_generate()?;
                self.ephemeral_public_key = Some(pair.public_key);
                self.client_ephemeral_public_key = Some(client);
                let shared = AirPlayCrypto::x25519_shared(&pair.private_key, &client)?;
                self.shared_secret = Some(shared);
                let encryption_key = derive(&shared, VERIFY_ENCRYPT_SALT, VERIFY_ENCRYPT_INFO)?;
                self.encryption_key = Some(encryption_key);
                let id = self.identity.pairing_id.as_bytes();
                let sig = AirPlayCrypto::ed25519_sign(
                    &self.identity.private_key,
                    &[&pair.public_key[..], id, &client].concat(),
                )?;
                let sub = response(&[item(1, id), item(10, sig)])?;
                let sealed = AirPlayCrypto::chacha_seal(
                    &encryption_key,
                    &AirPlayCrypto::nonce_label(MSG02),
                    &sub,
                    &[],
                )?;
                response(&[item(6, [2]), item(3, pair.public_key), item(5, sealed)])
            }
            3 => {
                let (Some(key), Some(shared), Some(own), Some(client), Ok(encrypted)) = (
                    self.encryption_key,
                    self.shared_secret,
                    self.ephemeral_public_key,
                    self.client_ephemeral_public_key,
                    required(tlv, 5),
                ) else {
                    return error(4);
                };
                let sub = Tlv8Codec::decode(&AirPlayCrypto::chacha_open(
                    &key,
                    &AirPlayCrypto::nonce_label(MSG03),
                    encrypted,
                    &[],
                )?)?;
                let (Ok(id), Ok(sig)) = (required(&sub, 1), required(&sub, 10)) else {
                    return error(4);
                };
                let identifier = String::from_utf8_lossy(id).into_owned();
                let Some(ltpk) = self.pairings.get(&identifier)? else {
                    return error(4);
                };
                if !AirPlayCrypto::ed25519_verify(&ltpk, &[&client[..], id, &own].concat(), sig) {
                    return error(4);
                }
                self.keys = Some((
                    derive(&shared, CONTROL_SALT, CONTROL_WRITE_KEY_INFO)?,
                    derive(&shared, CONTROL_SALT, CONTROL_READ_KEY_INFO)?,
                ));
                self.controller_id = Some(identifier);
                response(&[item(6, [4])])
            }
            _ => error(state),
        }
    }
    pub fn shared(&self) -> Option<[u8; 32]> {
        self.shared_secret
    }
    pub fn verified_controller_id(&self) -> Option<String> {
        self.controller_id.clone()
    }
    pub fn control_cipher(&self) -> Result<ControlCipher> {
        let Some((read, write)) = self.keys else {
            return Err(std::io::Error::other("Pair verify is not complete"));
        };
        Ok(ControlCipher::new(read, write))
    }
}
/// Protocol message, field, or framing constant.
pub const TYPE_IDENTIFIER: u64 = 0x01;
/// Protocol message, field, or framing constant.
pub const TYPE_PUBLIC_KEY: u64 = 0x03;
/// Protocol message, field, or framing constant.
pub const TYPE_ENCRYPTED_DATA: u64 = 0x05;
/// Protocol message, field, or framing constant.
pub const TYPE_STATE: u64 = 0x06;
/// Protocol message, field, or framing constant.
pub const TYPE_ERROR: u64 = 0x07;
/// Protocol message, field, or framing constant.
pub const TYPE_SIGNATURE: u64 = 0x0a;
/// Protocol message, field, or framing constant.
pub const ERROR_AUTHENTICATION: u64 = 2;
/// Protocol message, field, or framing constant.
pub const VERIFY_ENCRYPT_SALT: &str = "Pair-Verify-Encrypt-Salt";
/// Protocol message, field, or framing constant.
pub const VERIFY_ENCRYPT_INFO: &str = "Pair-Verify-Encrypt-Info";
/// Protocol message, field, or framing constant.
pub const CONTROL_SALT: &str = "Control-Salt";
/// Protocol message, field, or framing constant.
pub const CONTROL_READ_KEY_INFO: &str = "Control-Read-Encryption-Key";
/// Protocol message, field, or framing constant.
pub const CONTROL_WRITE_KEY_INFO: &str = "Control-Write-Encryption-Key";
/// Protocol message, field, or framing constant.
pub const MSG02: &str = "PV-Msg02";
/// Protocol message, field, or framing constant.
pub const MSG03: &str = "PV-Msg03";
