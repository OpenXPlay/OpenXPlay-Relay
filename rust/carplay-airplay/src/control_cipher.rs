// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::{AirPlayCrypto, Result};
#[derive(Clone, Debug)]
pub struct Decrypted {
    pub data: Vec<u8>,
    pub rest: Vec<u8>,
}
pub struct ControlCipher {
    read_key: [u8; 32],
    write_key: [u8; 32],
    read_counter: u64,
    write_counter: u64,
}
impl ControlCipher {
    pub fn new(read_key: [u8; 32], write_key: [u8; 32]) -> Self {
        Self {
            read_key,
            write_key,
            read_counter: 0,
            write_counter: 0,
        }
    }
    pub fn decrypt(&mut self, buffer: &[u8]) -> Result<Decrypted> {
        let mut data = Vec::new();
        let mut offset = 0;
        while buffer.len() - offset >= 2 {
            let len = u16::from_le_bytes([buffer[offset], buffer[offset + 1]]) as usize;
            let end = offset + 2 + len + 16;
            if end > buffer.len() {
                break;
            }
            data.extend(AirPlayCrypto::chacha_open(
                &self.read_key,
                &AirPlayCrypto::nonce64(self.read_counter),
                &buffer[offset + 2..end],
                &buffer[offset..offset + 2],
            )?);
            self.read_counter = self.read_counter.wrapping_add(1);
            offset = end;
        }
        Ok(Decrypted {
            data,
            rest: buffer[offset..].to_vec(),
        })
    }
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut offset = 0;
        loop {
            let end = (offset + 0x4000).min(plaintext.len());
            let chunk = &plaintext[offset..end];
            let head = (chunk.len() as u16).to_le_bytes();
            let sealed = AirPlayCrypto::chacha_seal(
                &self.write_key,
                &AirPlayCrypto::nonce64(self.write_counter),
                chunk,
                &head,
            )?;
            out.extend(head);
            out.extend(sealed);
            self.write_counter = self.write_counter.wrapping_add(1);
            offset = end;
            if offset == plaintext.len() {
                break;
            }
        }
        Ok(out)
    }
}
pub const HEADER_SIZE: u64 = 2;
pub const TAG_SIZE: u64 = 16;
pub const MAX_PAYLOAD: u64 = 0x4000;
