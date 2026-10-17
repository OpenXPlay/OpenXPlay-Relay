// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::{AirPlayCrypto, MfiAuthenticator, Result};
use carplay_iap2::mfi::MfiCertificateType;
use ctr::cipher::{KeyIvInit, StreamCipher};
use sha1::Sha1;
use sha2::{Digest, Sha256};
pub struct MfiSapAuthSetup;
impl MfiSapAuthSetup {
    pub fn handle(body: &[u8], mfi: &dyn MfiAuthenticator) -> Result<Option<Vec<u8>>> {
        Self::handle_with_certificate_type(body, mfi, mfi.certificate_type())
    }
    pub fn handle_with_certificate_type(
        body: &[u8],
        mfi: &dyn MfiAuthenticator,
        certificate_type: MfiCertificateType,
    ) -> Result<Option<Vec<u8>>> {
        if body.len() != 33 || body[0] != 1 {
            return Ok(None);
        }
        let peer: [u8; 32] = body[1..].try_into().unwrap();
        let pair = AirPlayCrypto::x25519_generate()?;
        let shared = match AirPlayCrypto::x25519_shared(&pair.private_key, &peer) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        let key = Sha1::digest([b"AES-KEY".as_slice(), &shared].concat());
        let iv = Sha1::digest([b"AES-IV".as_slice(), &shared].concat());
        let transcript = [pair.public_key, peer].concat();
        let (certificate, mut signature, intermediate) =
            if certificate_type == MfiCertificateType::Baa {
                let certificates = mfi.baa_certificates()?;
                (
                    certificates.leaf,
                    mfi.sign_challenge(&transcript)?,
                    Some(certificates.intermediate),
                )
            } else {
                let certificate = mfi.read_certificate(65525)?;
                let digest = if mfi.protocol_major()? == 2 {
                    Sha1::digest(&transcript).to_vec()
                } else {
                    Sha256::digest(&transcript).to_vec()
                };
                (certificate, mfi.sign_challenge(&digest)?, None)
            };
        let mut cipher = ctr::Ctr128BE::<aes::Aes128>::new_from_slices(&key[..16], &iv[..16])
            .map_err(|_| std::io::Error::other("SAP AES key length"))?;
        cipher.apply_keystream(&mut signature);
        let mut out = pair.public_key.to_vec();
        append_blob(&mut out, &certificate)?;
        append_blob(&mut out, &signature)?;
        if let Some(intermediate) = intermediate {
            append_blob(&mut out, &encode_baa_intermediate(&intermediate))?;
        }
        Ok(Some(out))
    }
}
fn append_blob(out: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    let len =
        u32::try_from(value.len()).map_err(|_| std::io::Error::other("SAP blob too large"))?;
    out.extend(len.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}
pub const VERSION: u64 = 1;
pub const PUBLIC_KEY_BYTES: u64 = 32;

fn encode_baa_intermediate(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0xe1, 0x44, b'b', b'a', b'I', b'C'];
    match data.len() {
        0..=0x20 => out.push(0x70 + data.len() as u8),
        0x21..=0xff => out.extend([0x91, data.len() as u8]),
        0x100..=0xffff => {
            out.push(0x92);
            out.extend((data.len() as u16).to_le_bytes());
        }
        _ => {
            out.push(0x93);
            out.extend((data.len() as u32).to_le_bytes());
        }
    }
    out.extend_from_slice(data);
    out
}
