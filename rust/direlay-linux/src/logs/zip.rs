// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Small, bounded ZIP32 archives: UTF-8 names and raw DEFLATE, with no data descriptors.
use std::io;

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
fn word(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_le_bytes());
}
fn dword(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
fn dos_time(seconds: u64) -> (u16, u16) {
    let seconds = seconds.min(4_354_819_198).try_into().unwrap_or(0);
    let mut date = std::mem::MaybeUninit::<libc::tm>::uninit();
    if unsafe { libc::gmtime_r(&seconds, date.as_mut_ptr()) }.is_null() {
        return (0, 33);
    }
    let date = unsafe { date.assume_init() };
    if date.tm_year < 80 {
        return (0, 33);
    }
    (
        ((date.tm_hour as u16) << 11) | ((date.tm_min as u16) << 5) | (date.tm_sec as u16 / 2),
        (((date.tm_year - 80) as u16) << 9)
            | (((date.tm_mon + 1) as u16) << 5)
            | date.tm_mday as u16,
    )
}
pub fn archive(entries: &[(&str, &[u8])], seconds: u64) -> io::Result<Vec<u8>> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "ZIP32 limit or invalid name");
    let (time, date) = dos_time(seconds);
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, body) in entries {
        if name.is_empty() || name.contains(['/', '\\', '\0']) {
            return Err(invalid());
        }
        let name_len = u16::try_from(name.len()).map_err(|_| invalid())?;
        let size = u32::try_from(body.len()).map_err(|_| invalid())?;
        let offset = u32::try_from(out.len()).map_err(|_| invalid())?;
        let compressed = miniz_oxide::deflate::compress_to_vec(body, 6);
        let packed = u32::try_from(compressed.len()).map_err(|_| invalid())?;
        let crc = crc32(body);
        dword(&mut out, 0x0403_4b50);
        for value in [20, 0x800, 8, time, date] {
            word(&mut out, value);
        }
        for value in [crc, packed, size] {
            dword(&mut out, value);
        }
        word(&mut out, name_len);
        word(&mut out, 0);
        out.extend(name.as_bytes());
        out.extend(compressed);
        dword(&mut central, 0x0201_4b50);
        for value in [20, 20, 0x800, 8, time, date] {
            word(&mut central, value);
        }
        for value in [crc, packed, size] {
            dword(&mut central, value);
        }
        for value in [name_len, 0, 0, 0, 0] {
            word(&mut central, value);
        }
        dword(&mut central, 0);
        dword(&mut central, offset);
        central.extend(name.as_bytes());
    }
    let start = u32::try_from(out.len()).map_err(|_| invalid())?;
    let length = u32::try_from(central.len()).map_err(|_| invalid())?;
    let count = u16::try_from(entries.len()).map_err(|_| invalid())?;
    out.extend(central);
    dword(&mut out, 0x0605_4b50);
    for value in [0, 0, count, count] {
        word(&mut out, value);
    }
    dword(&mut out, length);
    dword(&mut out, start);
    word(&mut out, 0);
    Ok(out)
}

#[cfg(test)]
pub(super) fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    fn u16at(b: &[u8], at: usize) -> usize {
        u16::from_le_bytes(b[at..at + 2].try_into().unwrap()) as usize
    }
    fn u32at(b: &[u8], at: usize) -> usize {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
    }
    let end = bytes.len() - 22;
    assert_eq!(u32at(bytes, end), 0x0605_4b50);
    let mut central = u32at(bytes, end + 16);
    assert_eq!(central + u32at(bytes, end + 12), end);
    let mut result = Vec::new();
    for _ in 0..u16at(bytes, end + 10) {
        assert_eq!(u32at(bytes, central), 0x0201_4b50);
        let local = u32at(bytes, central + 42);
        assert_eq!(u32at(bytes, local), 0x0403_4b50);
        assert_eq!(u16at(bytes, local + 6), 0x800);
        assert_eq!(u16at(bytes, local + 8), 8);
        assert_eq!(
            &bytes[local + 6..local + 26],
            &bytes[central + 8..central + 28]
        );
        let name_len = u16at(bytes, local + 26);
        let name = &bytes[local + 30..local + 30 + name_len];
        assert_eq!(name, &bytes[central + 46..central + 46 + name_len]);
        let start = local + 30 + name_len;
        let raw = miniz_oxide::inflate::decompress_to_vec(
            &bytes[start..start + u32at(bytes, local + 18)],
        )
        .unwrap();
        assert_eq!(raw.len(), u32at(bytes, local + 22));
        assert_eq!(crc32(&raw), u32at(bytes, local + 14) as u32);
        result.push((String::from_utf8(name.to_vec()).unwrap(), raw));
        central += 46 + name_len;
    }
    assert_eq!(central, end);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_unzips_empty_unicode_and_compressed_entries_with_crc_and_clock() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        let entries = [
            ("relay.log", b"hello\nhello\n".as_slice()),
            ("\u{4fe1}\u{606f}.txt", b"".as_slice()),
        ];
        let zip = archive(&entries, 1_792_713_600).unwrap();
        assert_eq!(
            unzip(&zip),
            entries
                .iter()
                .map(|(n, b)| (n.to_string(), b.to_vec()))
                .collect::<Vec<_>>()
        );
        assert_eq!(dos_time(0), (0, 33));
        assert_eq!(dos_time(1_792_713_600), (0, (46 << 9) | (10 << 5) | 23));
        assert!(archive(&[("../bad", b"")], 0).is_err());
    }
}
