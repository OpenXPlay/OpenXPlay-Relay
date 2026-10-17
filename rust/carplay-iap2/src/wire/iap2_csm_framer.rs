// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// State owned by `Iap2CsmFramer`; construct through its factory.
pub struct Iap2CsmFramer {
    receive: Vec<u8>,
}
impl Iap2CsmFramer {
    pub fn new() -> Self {
        Self {
            receive: Vec::new(),
        }
    }

    pub fn offer(&mut self, chunk: &[u8]) -> Result<Vec<Iap2Frame>> {
        let mut frames = Vec::new();
        for byte in chunk {
            self.receive.push(*byte);
            loop {
                if self.receive.len() >= 2 && self.receive[..2] != [0x40, 0x40] {
                    self.receive.remove(0);
                    continue;
                }
                if self.receive.len() < 6 {
                    break;
                }
                let length = u16::from_be_bytes([self.receive[2], self.receive[3]]) as usize;
                if length < 6 {
                    self.receive.remove(0);
                    continue;
                }
                if self.receive.len() < length {
                    break;
                }
                frames.push(Iap2Frame {
                    message_id: u16::from_be_bytes([self.receive[4], self.receive[5]]),
                    payload: self.receive[6..length].to_vec(),
                });
                self.receive.drain(..length);
            }
        }
        Ok(frames)
    }

    pub fn encode_frame(message_id: u16, payload: &[u8]) -> Result<Vec<u8>> {
        if payload.len() > 65529 {
            return Err(invalid("CSM payload exceeds 65529 bytes"));
        }
        let mut out = vec![0x40, 0x40];
        out.extend(((payload.len() + 6) as u16).to_be_bytes());
        out.extend(message_id.to_be_bytes());
        out.extend(payload);
        Ok(out)
    }

    pub fn encode_param(parameter_id: u16, payload: &[u8]) -> Result<Vec<u8>> {
        if payload.len() > 65531 {
            return Err(invalid("CSM parameter payload exceeds 65531 bytes"));
        }
        let mut out = ((payload.len() + 4) as u16).to_be_bytes().to_vec();
        out.extend(parameter_id.to_be_bytes());
        out.extend(payload);
        Ok(out)
    }

    pub fn split_for_link(frame: &Iap2Frame, link_chunk_size: usize) -> Result<Vec<Vec<u8>>> {
        if !(1..=65525).contains(&link_chunk_size) {
            return Err(invalid("iAP2 link chunk size must be in 1..65525"));
        }
        Ok(frame
            .encoded_frame()?
            .chunks(link_chunk_size)
            .map(|c| c.to_vec())
            .collect())
    }
}

/// Protocol message, field, or framing constant.
pub const START: u64 = 0x4040;
/// Protocol message, field, or framing constant.
pub const HEADER_BYTES: u64 = 6;
/// Protocol message, field, or framing constant.
pub const MAX_FRAME_BYTES: u64 = 0xffff;
/// Protocol message, field, or framing constant.
pub const MAX_PARAM_BYTES: u64 = 0xffff;
/// Protocol message, field, or framing constant.
pub const MAX_LINK_CHUNK_BYTES: u64 = 65_525;
/// Protocol message, field, or framing constant.
pub const START_HIGH: u64 = 0x40;
/// Protocol message, field, or framing constant.
pub const START_LOW: u64 = 0x40;

/// Constant expression: `HEADER_BYTES`.
pub const MIN_FRAME_BYTES: u64 = 6;

/// Constant expression: `MAX_FRAME_BYTES - HEADER_BYTES`.
pub const MAX_BODY_BYTES: u64 = 65529;

impl Default for Iap2CsmFramer {
    fn default() -> Self {
        Self::new()
    }
}
