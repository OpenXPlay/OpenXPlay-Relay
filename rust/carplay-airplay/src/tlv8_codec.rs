// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct Tlv8Item {
    pub kind: u8,
    pub value: Vec<u8>,
}
pub struct Tlv8Codec;
impl Tlv8Codec {
    pub fn encode(items: &[Tlv8Item]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut previous = None;
        for item in items {
            if previous == Some(item.kind) {
                out.extend_from_slice(&[255, 0]);
            }
            if item.value.is_empty() {
                out.extend_from_slice(&[item.kind, 0]);
            } else {
                for chunk in item.value.chunks(255) {
                    out.extend_from_slice(&[item.kind, chunk.len() as u8]);
                    out.extend_from_slice(chunk);
                }
            }
            previous = Some(item.kind);
        }
        Ok(out)
    }
    pub fn decode(buffer: &[u8]) -> Result<BTreeMap<u8, Vec<u8>>> {
        let mut out: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
        let (mut pos, mut last, mut last_len) = (0, None, 0);
        while pos + 2 <= buffer.len() {
            let kind = buffer[pos];
            let len = buffer[pos + 1] as usize;
            pos += 2;
            if pos + len > buffer.len() {
                break;
            }
            if last == Some(kind) && last_len == 255 {
                out.entry(kind)
                    .or_default()
                    .extend_from_slice(&buffer[pos..pos + len]);
            } else {
                out.insert(kind, buffer[pos..pos + len].to_vec());
            }
            pos += len;
            last = Some(kind);
            last_len = len;
        }
        Ok(out)
    }
}
pub const SEPARATOR_TYPE: u64 = 255;
pub const MAX_FRAGMENT: u64 = 255;
