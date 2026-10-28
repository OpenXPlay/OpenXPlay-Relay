// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! ZIP selection is by filename only; entry contents are never searched for identities.
use super::{Error, Secret, MAX_ARCHIVE, MAX_FILE};
use miniz_oxide::{
    inflate::stream::{inflate, InflateState},
    DataFormat, MZFlush, MZStatus,
};
use zeroize::Zeroizing;
const BAD: &str = "identity_error_zip";
pub fn is_zip(bytes: &[u8]) -> bool {
    [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"]
        .iter()
        .any(|s| bytes.starts_with(*s))
}
fn u16_at(b: &[u8], n: usize) -> Result<usize, Error> {
    Ok(u16::from_le_bytes(b.get(n..n + 2).ok_or(BAD)?.try_into().map_err(|_| BAD)?) as usize)
}
fn u32_at(b: &[u8], n: usize) -> Result<usize, Error> {
    Ok(u32::from_le_bytes(b.get(n..n + 4).ok_or(BAD)?.try_into().map_err(|_| BAD)?) as usize)
}
fn crc_table() -> [u32; 256] {
    std::array::from_fn(|n| {
        let mut c = n as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(c & 1));
        }
        c
    })
}
struct Inflater(Box<InflateState>);
impl Drop for Inflater {
    fn drop(&mut self) {
        self.0.reset(DataFormat::Raw);
        std::hint::black_box(&mut self.0);
    }
}
fn entry(
    data: &[u8],
    method: usize,
    expected: usize,
    crc: usize,
    keep: bool,
    total: &mut usize,
) -> Result<Secret, Error> {
    let mut out = Zeroizing::new(Vec::new());
    let mut checksum = !0u32;
    let table = crc_table();
    let mut size = 0;
    let mut consume = |bytes: &[u8]| -> Result<(), Error> {
        size += bytes.len();
        *total += bytes.len();
        if *total > MAX_ARCHIVE {
            return Err("identity_error_limit");
        }
        if keep && size > MAX_FILE {
            return Err("identity_error_size");
        }
        for byte in bytes {
            checksum = (checksum >> 8) ^ table[((checksum ^ *byte as u32) & 255) as usize];
        }
        if keep {
            out.extend_from_slice(bytes);
        }
        Ok(())
    };
    match method {
        0 => consume(data)?,
        8 => {
            let mut state = Inflater(InflateState::new_boxed(DataFormat::Raw));
            let mut remaining = data;
            let mut buffer = Zeroizing::new([0u8; 4096]);
            loop {
                let result = inflate(&mut state.0, remaining, &mut buffer[..], MZFlush::None);
                consume(&buffer[..result.bytes_written])?;
                remaining = &remaining[result.bytes_consumed..];
                match result.status {
                    Ok(MZStatus::StreamEnd) if remaining.is_empty() => break,
                    Ok(MZStatus::Ok) if result.bytes_consumed + result.bytes_written > 0 => {}
                    _ => return Err(BAD),
                }
            }
        }
        _ => return Err(BAD),
    }
    if size != expected || !checksum != crc as u32 {
        return Err(BAD);
    }
    Ok(out)
}
pub fn read(bytes: &[u8]) -> Result<Vec<Secret>, Error> {
    if bytes.len() > MAX_ARCHIVE {
        return Err("identity_error_limit");
    }
    let end = (bytes.len().saturating_sub(65557)..bytes.len().saturating_sub(21))
        .rev()
        .find(|i| bytes.get(*i..*i + 4) == Some(b"PK\x05\x06"))
        .ok_or(BAD)?;
    if end + 22 + u16_at(bytes, end + 20)? != bytes.len()
        || u16_at(bytes, end + 4)? != 0
        || u16_at(bytes, end + 6)? != 0
    {
        return Err(BAD);
    }
    let count = u16_at(bytes, end + 10)?;
    if count > 20_000 {
        return Err("identity_error_limit");
    }
    if count != u16_at(bytes, end + 8)? {
        return Err(BAD);
    }
    let mut at = u32_at(bytes, end + 16)?;
    if at.checked_add(u32_at(bytes, end + 12)?) != Some(end) {
        return Err(BAD);
    }
    let mut total = 0;
    let mut exact: [Vec<Secret>; 2] = std::array::from_fn(|_| Vec::new());
    let mut fallback: [Vec<Secret>; 2] = std::array::from_fn(|_| Vec::new());
    for _ in 0..count {
        if bytes.get(at..at + 4) != Some(b"PK\x01\x02") {
            return Err(BAD);
        }
        let flags = u16_at(bytes, at + 8)?;
        if flags & (1 | 64) != 0 {
            return Err("identity_error_encrypted");
        }
        let method = u16_at(bytes, at + 10)?;
        let crc = u32_at(bytes, at + 16)?;
        let compressed = u32_at(bytes, at + 20)?;
        let expanded = u32_at(bytes, at + 24)?;
        if expanded > MAX_ARCHIVE.saturating_sub(total) {
            return Err("identity_error_limit");
        }
        let name_len = u16_at(bytes, at + 28)?;
        let name_bytes = bytes.get(at + 46..at + 46 + name_len).ok_or(BAD)?;
        let name = String::from_utf8_lossy(name_bytes)
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = if name.ends_with(".pk8") {
            Some(0)
        } else if name.ends_with(".p7b") {
            Some(1)
        } else {
            None
        };
        if kind.is_some() && expanded > MAX_FILE {
            return Err("identity_error_size");
        }
        let local = u32_at(bytes, at + 42)?;
        if bytes.get(local..local + 4) != Some(b"PK\x03\x04")
            || u16_at(bytes, local + 6)? != flags
            || u16_at(bytes, local + 8)? != method
        {
            return Err(BAD);
        }
        let local_name_len = u16_at(bytes, local + 26)?;
        if bytes.get(local + 30..local + 30 + local_name_len) != Some(name_bytes) {
            return Err(BAD);
        }
        let start = local + 30 + local_name_len + u16_at(bytes, local + 28)?;
        let finish = start
            .checked_add(compressed)
            .filter(|v| *v <= u32_at(bytes, end + 16).unwrap_or(0))
            .ok_or(BAD)?;
        let data = entry(
            bytes.get(start..finish).ok_or(BAD)?,
            method,
            expanded,
            crc,
            kind.is_some(),
            &mut total,
        )?;
        if let Some(kind) = kind {
            let candidates = if name == "identity.pk8" || name == "certificate.p7b" {
                &mut exact[kind]
            } else {
                &mut fallback[kind]
            };
            if candidates.len() < 2 {
                candidates.push(data);
            }
        }
        at = at
            .checked_add(46 + name_len + u16_at(bytes, at + 30)? + u16_at(bytes, at + 32)?)
            .ok_or(BAD)?;
    }
    if at != end {
        return Err(BAD);
    }
    let mut selected = Vec::new();
    for kind in 0..2 {
        let candidates = if exact[kind].is_empty() {
            &mut fallback[kind]
        } else {
            &mut exact[kind]
        };
        if candidates.len() > 1 {
            return Err(if kind == 0 {
                "identity_error_duplicate_key"
            } else {
                "identity_error_duplicate_certificate"
            });
        }
        selected.append(candidates);
    }
    if selected.is_empty() {
        return Err("identity_error_missing");
    }
    Ok(selected)
}
