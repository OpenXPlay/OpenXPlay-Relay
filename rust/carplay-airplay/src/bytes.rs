// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
pub fn concat_bytes(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}
pub fn ascii_bytes(value: &str) -> Result<Vec<u8>> {
    Ok(value
        .chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect())
}
pub fn to_hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
