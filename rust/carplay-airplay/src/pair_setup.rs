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
pub struct PairSetup {
    identity: Arc<AirPlayIdentity>,
    pairings: Arc<dyn PairingStore>,
    srp: Option<Srp6aSession>,
    session_key: Option<Vec<u8>>,
    pub complete: bool,
}
impl PairSetup {
    pub fn new(identity: Arc<AirPlayIdentity>, pairings: Arc<dyn PairingStore>) -> Self {
        Self {
            identity,
            pairings,
            srp: None,
            session_key: None,
            complete: false,
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
                let srp = Srp6a::start(SETUP_USERNAME, SETUP_CODE)?;
                let out =
                    response(&[item(6, [2]), item(3, srp.public_key()), item(2, srp.salt())])?;
                self.srp = Some(srp);
                Ok(out)
            }
            3 => {
                let Some(srp) = &self.srp else {
                    return error(4);
                };
                let (Ok(a), Ok(proof)) = (required(tlv, 3), required(tlv, 4)) else {
                    return error(4);
                };
                let result = srp.verify(a, proof)?;
                if !result.ok {
                    return error(4);
                }
                self.session_key = result.session_key;
                response(&[item(6, [4]), item(4, result.server_proof.unwrap())])
            }
            5 => {
                let Some(key) = &self.session_key else {
                    return error(6);
                };
                let Ok(encrypted) = required(tlv, 5) else {
                    return error(6);
                };
                let encryption_key = derive(key, ENCRYPT_SALT, ENCRYPT_INFO)?;
                let sub = Tlv8Codec::decode(&AirPlayCrypto::chacha_open(
                    &encryption_key,
                    &AirPlayCrypto::nonce_label(MSG05),
                    encrypted,
                    &[],
                )?)?;
                let (Ok(id), Ok(ltpk), Ok(sig)) =
                    (required(&sub, 1), required(&sub, 3), required(&sub, 10))
                else {
                    return error(6);
                };
                let ltpk = key32(ltpk)?;
                let sign_key = derive(key, CONTROLLER_SIGN_SALT, CONTROLLER_SIGN_INFO)?;
                if !AirPlayCrypto::ed25519_verify(&ltpk, &[&sign_key[..], id, &ltpk].concat(), sig)
                {
                    return error(6);
                }
                self.pairings.save(&String::from_utf8_lossy(id), &ltpk)?;
                let accessory_id = self.identity.pairing_id.as_bytes();
                let sign_key = derive(key, ACCESSORY_SIGN_SALT, ACCESSORY_SIGN_INFO)?;
                let sig = AirPlayCrypto::ed25519_sign(
                    &self.identity.private_key,
                    &[&sign_key[..], accessory_id, &self.identity.public_key].concat(),
                )?;
                let sub = response(&[
                    item(1, accessory_id),
                    item(3, self.identity.public_key),
                    item(10, sig),
                ])?;
                let sealed = AirPlayCrypto::chacha_seal(
                    &encryption_key,
                    &AirPlayCrypto::nonce_label(MSG06),
                    &sub,
                    &[],
                )?;
                self.complete = true;
                response(&[item(6, [6]), item(5, sealed)])
            }
            _ => error(state),
        }
    }
}
/// Protocol message, field, or framing constant.
pub const TYPE_METHOD: u64 = 0x00;
/// Protocol message, field, or framing constant.
pub const TYPE_IDENTIFIER: u64 = 0x01;
/// Protocol message, field, or framing constant.
pub const TYPE_SALT: u64 = 0x02;
/// Protocol message, field, or framing constant.
pub const TYPE_PUBLIC_KEY: u64 = 0x03;
/// Protocol message, field, or framing constant.
pub const TYPE_PROOF: u64 = 0x04;
/// Protocol message, field, or framing constant.
pub const TYPE_ENCRYPTED_DATA: u64 = 0x05;
/// Protocol message, field, or framing constant.
pub const TYPE_STATE: u64 = 0x06;
/// Protocol message, field, or framing constant.
pub const TYPE_ERROR: u64 = 0x07;
/// Protocol message, field, or framing constant.
pub const TYPE_SIGNATURE: u64 = 0x0a;
/// Protocol message, field, or framing constant.
pub const SETUP_USERNAME: &str = "Pair-Setup";
/// Protocol message, field, or framing constant.
pub const SETUP_CODE: &str = "3939";
/// Protocol message, field, or framing constant.
pub const ERROR_AUTHENTICATION: u64 = 2;
/// Protocol message, field, or framing constant.
pub const ENCRYPT_SALT: &str = "Pair-Setup-Encrypt-Salt";
/// Protocol message, field, or framing constant.
pub const ENCRYPT_INFO: &str = "Pair-Setup-Encrypt-Info";
/// Protocol message, field, or framing constant.
pub const CONTROLLER_SIGN_SALT: &str = "Pair-Setup-Controller-Sign-Salt";
/// Protocol message, field, or framing constant.
pub const CONTROLLER_SIGN_INFO: &str = "Pair-Setup-Controller-Sign-Info";
/// Protocol message, field, or framing constant.
pub const ACCESSORY_SIGN_SALT: &str = "Pair-Setup-Accessory-Sign-Salt";
/// Protocol message, field, or framing constant.
pub const ACCESSORY_SIGN_INFO: &str = "Pair-Setup-Accessory-Sign-Info";
/// Protocol message, field, or framing constant.
pub const MSG05: &str = "PS-Msg05";
/// Protocol message, field, or framing constant.
pub const MSG06: &str = "PS-Msg06";
