// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// State owned by `Iap2WireCodec`; construct through its factory.
pub struct Iap2WireCodec {
    _private: (),
}
impl Iap2WireCodec {
    pub fn u8(value: u8) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    pub fn i8(value: i8) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    pub fn bool(value: bool) -> Result<Vec<u8>> {
        Self::u8(u8::from(value))
    }

    pub fn u16(value: u16) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    pub fn i16(value: i16) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    pub fn u32(value: u32) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    pub fn i32(value: i32) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    pub fn u64(value: u64) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    pub fn string(value: &str) -> Result<Vec<u8>> {
        if value.contains('\0') {
            return Err(invalid("NUL-terminated strings must not contain U+0000"));
        }
        Ok([value.as_bytes(), &[0]].concat())
    }

    pub fn strings(value: &[String]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for v in value {
            out.extend(Self::string(v)?);
        }
        Ok(out)
    }

    pub fn u16_list(value: &[u16]) -> Result<Vec<u8>> {
        Ok(value.iter().flat_map(|v| v.to_be_bytes()).collect())
    }

    pub fn read_u8(bytes: &[u8]) -> Result<u8> {
        Ok(u8::from_be_bytes(
            bytes
                .get(..1)
                .ok_or_else(|| invalid("Truncated CSM scalar"))?
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_i8(bytes: &[u8]) -> Result<i8> {
        Ok(i8::from_be_bytes(
            bytes
                .get(..1)
                .ok_or_else(|| invalid("Truncated CSM scalar"))?
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_bool(bytes: &[u8]) -> Result<bool> {
        Ok(Self::read_u8(bytes)? != 0)
    }

    pub fn read_u16(bytes: &[u8]) -> Result<u16> {
        Ok(u16::from_be_bytes(
            bytes
                .get(..2)
                .ok_or_else(|| invalid("Truncated CSM scalar"))?
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_i16(bytes: &[u8]) -> Result<i16> {
        Ok(i16::from_be_bytes(
            bytes
                .get(..2)
                .ok_or_else(|| invalid("Truncated CSM scalar"))?
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_u32(bytes: &[u8]) -> Result<u32> {
        Ok(u32::from_be_bytes(
            bytes
                .get(..4)
                .ok_or_else(|| invalid("Truncated CSM scalar"))?
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_i32(bytes: &[u8]) -> Result<i32> {
        Ok(i32::from_be_bytes(
            bytes
                .get(..4)
                .ok_or_else(|| invalid("Truncated CSM scalar"))?
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_u64(bytes: &[u8]) -> Result<u64> {
        Ok(u64::from_be_bytes(
            bytes
                .get(..8)
                .ok_or_else(|| invalid("Truncated CSM scalar"))?
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_string(bytes: &[u8]) -> Result<String> {
        if bytes.last() != Some(&0) {
            return Err(invalid("CSM string is not NUL terminated"));
        }
        Ok(String::from_utf8_lossy(&bytes[..bytes.len() - 1]).into_owned())
    }

    pub fn read_strings(bytes: &[u8]) -> Result<Vec<String>> {
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        if bytes.last() != Some(&0) {
            return Err(invalid("CSM string[] is not NUL terminated"));
        }
        Ok(bytes[..bytes.len() - 1]
            .split(|b| *b == 0)
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .collect())
    }

    pub fn read_u16_list(bytes: &[u8]) -> Result<Vec<u16>> {
        if !bytes.len().is_multiple_of(2) {
            return Err(invalid(format!(
                "CSM u16[] payload has odd length {}",
                bytes.len()
            )));
        }
        Ok(bytes
            .chunks_exact(2)
            .map(|v| u16::from_be_bytes([v[0], v[1]]))
            .collect())
    }

    pub fn void() -> Vec<u8> {
        Vec::new()
    }

    pub fn bytes(value: &[u8]) -> Vec<u8> {
        value.to_vec()
    }
}
