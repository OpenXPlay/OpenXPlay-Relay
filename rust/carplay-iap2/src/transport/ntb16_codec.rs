// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! One Ethernet datagram per NTB16; parsing also accepts bounded NDP chains.
use crate::{invalid, Result};
pub const NTH16_SIG: u32 = 0x484d434e;
pub const NDP16_SIG: u32 = 0x304d434e;
pub struct Ntb16Codec;
impl Ntb16Codec {
    /// `packet_size` is the OUT endpoint's wMaxPacketSize, including at full speed.
    pub fn build(frame: &[u8], sequence: u16, packet_size: u16) -> Result<Vec<u8>> {
        if frame.is_empty() {
            return Err(invalid("frame must not be empty"));
        }
        if frame.len() > 65507 {
            return Err(invalid("frame exceeds the NTB16 length field"));
        }
        if packet_size == 0 {
            return Err(invalid("USB packet size must be positive"));
        }
        let length = 28 + frame.len();
        let mut b = vec![0; length];
        b[0..4].copy_from_slice(&NTH16_SIG.to_le_bytes());
        b[12..16].copy_from_slice(&NDP16_SIG.to_le_bytes());
        for (offset, value) in [
            (4, 12),
            (6, sequence),
            (8, length as u16),
            (10, 12),
            (16, 16),
            (20, 28),
            (22, frame.len() as u16),
        ] {
            b[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        b[28..].copy_from_slice(frame);
        if length.is_multiple_of(packet_size as usize) {
            b.push(0);
        }
        Ok(b)
    }
    pub fn parse(block: &[u8]) -> Vec<Vec<u8>> {
        if block.len() < 12 || u32le(block, 0) != NTH16_SIG {
            return vec![];
        }
        let mut frames = Vec::new();
        let mut ndp = u16le(block, 10);
        for _ in 0..(block.len() / 4).max(1) {
            if ndp == 0
                || ndp + 12 > block.len()
                || u32le(block, ndp) & 0xffffff != NDP16_SIG & 0xffffff
            {
                break;
            }
            let end = (ndp + u16le(block, ndp + 4)).min(block.len());
            let next = u16le(block, ndp + 6);
            let mut entry = ndp + 8;
            while entry + 4 <= end {
                let start = u16le(block, entry);
                let length = u16le(block, entry + 2);
                if start == 0 || length == 0 {
                    break;
                }
                if start + length <= block.len() {
                    frames.push(block[start..start + length].to_vec());
                }
                entry += 4;
            }
            ndp = next;
        }
        frames
    }
    pub fn parse_range(block: &[u8], offset: usize, length: usize) -> Result<Vec<Vec<u8>>> {
        let end = offset
            .checked_add(length)
            .filter(|e| *e <= block.len())
            .ok_or_else(|| invalid("NTB16 range is outside the source buffer"))?;
        Ok(Self::parse(&block[offset..end]))
    }
}
pub(crate) fn u16le(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes([b[o], b[o + 1]]) as usize
}
pub(crate) fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
