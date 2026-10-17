// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
use num_bigint::{BigInt, Sign};
pub struct BplistCodec;
pub const APPLE_EPOCH_SECONDS: f64 = 978307200.0;
fn bad(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}
fn width(n: u64) -> usize {
    if n > 0xffffffff {
        8
    } else if n > 0xffff {
        4
    } else if n > 0xff {
        2
    } else {
        1
    }
}
fn be(n: u64, size: usize) -> Vec<u8> {
    n.to_be_bytes()[8 - size..].to_vec()
}
fn integer(n: u64) -> Vec<u8> {
    let size = width(n);
    let mut out = vec![0x10 | size.trailing_zeros() as u8];
    out.extend(be(n, size));
    out
}
fn marker(kind: u8, count: usize) -> Vec<u8> {
    if count < 15 {
        vec![(kind << 4) | count as u8]
    } else {
        let mut out = vec![(kind << 4) | 15];
        out.extend(integer(count as u64));
        out
    }
}
impl BplistCodec {
    pub fn encode(root: &BplistValue) -> Result<Vec<u8>> {
        fn add(
            value: &BplistValue,
            nodes: &mut Vec<(Vec<u8>, Vec<usize>)>,
            depth: usize,
        ) -> Result<usize> {
            if depth > 128 || nodes.len() >= 1_000_000 {
                return Err(bad("bplist: object limit"));
            }
            let index = nodes.len();
            nodes.push((Vec::new(), Vec::new()));
            let (mut body, mut refs) = (Vec::new(), Vec::new());
            match value {
                BplistValue::Null => body.push(0),
                BplistValue::Boolean(v) => body.push(if *v { 9 } else { 8 }),
                BplistValue::Integer(v) => {
                    if v.sign() == Sign::Minus {
                        return Err(bad("bplist: negative integers are not supported"));
                    }
                    let digits = v.to_u64_digits().1;
                    if digits.len() > 1 {
                        return Err(bad("bplist: integer exceeds 64 bits"));
                    }
                    body = integer(digits.first().copied().unwrap_or(0));
                }
                BplistValue::Real(v) => {
                    body.push(0x23);
                    body.extend(v.to_be_bytes());
                }
                BplistValue::DateMillis(v) => {
                    body.push(0x33);
                    body.extend((*v as f64 / 1000.0 - APPLE_EPOCH_SECONDS).to_be_bytes());
                }
                BplistValue::Data(v) => {
                    body = marker(4, v.len());
                    body.extend(v);
                }
                BplistValue::String(v) => {
                    if v.is_ascii() {
                        body = marker(5, v.len());
                        body.extend(v.as_bytes());
                    } else {
                        let utf: Vec<u16> = v.encode_utf16().collect();
                        body = marker(6, utf.len());
                        for c in utf {
                            body.extend(c.to_be_bytes());
                        }
                    }
                }
                BplistValue::Array(v) => {
                    body = marker(10, v.len());
                    for item in v {
                        refs.push(add(item, nodes, depth + 1)?);
                    }
                }
                BplistValue::Dictionary(v) => {
                    body = marker(13, v.len());
                    for (key, _) in v {
                        refs.push(add(&BplistValue::String(key.clone()), nodes, depth + 1)?);
                    }
                    for (_, value) in v {
                        refs.push(add(value, nodes, depth + 1)?);
                    }
                }
            }
            nodes[index] = (body, refs);
            Ok(index)
        }
        let mut nodes = Vec::new();
        let top = add(root, &mut nodes, 0)?;
        let ref_size = width(nodes.len() as u64);
        let mut out = b"bplist00".to_vec();
        let mut offsets = Vec::new();
        for (body, refs) in &nodes {
            offsets.push(out.len());
            out.extend(body);
            for r in refs {
                out.extend(be(*r as u64, ref_size));
            }
        }
        let table = out.len();
        let offset_size = width(table as u64);
        for offset in offsets {
            out.extend(be(offset as u64, offset_size));
        }
        out.extend([0; 6]);
        out.extend([offset_size as u8, ref_size as u8]);
        out.extend((nodes.len() as u64).to_be_bytes());
        out.extend((top as u64).to_be_bytes());
        out.extend((table as u64).to_be_bytes());
        Ok(out)
    }
    pub fn decode(bytes: &[u8]) -> Result<BplistValue> {
        if bytes.len() < 40 {
            return Err(bad("bplist: too short"));
        }
        if &bytes[..8] != b"bplist00" {
            return Err(bad("bplist: bad magic"));
        }
        fn number(bytes: &[u8], pos: usize, size: usize) -> Result<u64> {
            if size == 0 || size > 8 {
                return Err(bad("bplist: invalid integer size"));
            }
            let end = pos.checked_add(size).ok_or_else(|| bad("bplist: bounds"))?;
            let src = bytes
                .get(pos..end)
                .ok_or_else(|| bad("bplist: truncated object"))?;
            Ok(src.iter().fold(0, |n, b| (n << 8) | *b as u64))
        }
        let trailer = bytes.len() - 32;
        let os = bytes[trailer + 6] as usize;
        let rs = bytes[trailer + 7] as usize;
        if !(1..=8).contains(&os) || !(1..=8).contains(&rs) {
            return Err(bad("bplist: invalid table width"));
        }
        let count = number(bytes, trailer + 8, 8)?;
        let top = number(bytes, trailer + 16, 8)?;
        let table = number(bytes, trailer + 24, 8)?;
        if count == 0
            || count > 1_000_000
            || top >= count
            || table < 8
            || table > trailer as u64
            || count
                .checked_mul(os as u64)
                .and_then(|v| table.checked_add(v))
                .is_none_or(|v| v > trailer as u64)
        {
            return Err(bad("bplist: invalid offset table"));
        }
        let mut offsets = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let offset = number(bytes, table as usize + i * os, os)?;
            if offset < 8 || offset >= table {
                return Err(bad("bplist: invalid object offset"));
            }
            offsets.push(offset as usize);
        }
        struct Reader<'a> {
            bytes: &'a [u8],
            offsets: Vec<usize>,
            rs: usize,
            budget: usize,
        }
        impl Reader<'_> {
            fn read(&mut self, index: usize, depth: usize) -> Result<BplistValue> {
                if depth > 128 || self.budget == 0 {
                    return Err(bad("bplist: object limit"));
                }
                self.budget -= 1;
                let mut pos = *self
                    .offsets
                    .get(index)
                    .ok_or_else(|| bad("bplist: bad reference"))?;
                let mark = self.bytes[pos];
                pos += 1;
                let kind = mark >> 4;
                let nibble = mark & 15;
                let mut count = nibble as usize;
                if matches!(kind, 4 | 5 | 6 | 10 | 12 | 13) && nibble == 15 {
                    let m = *self
                        .bytes
                        .get(pos)
                        .ok_or_else(|| bad("bplist: truncated count"))?;
                    pos += 1;
                    if m >> 4 != 1 || m & 15 > 3 {
                        return Err(bad("bplist: invalid count"));
                    }
                    let size = 1usize << (m & 15);
                    count = usize::try_from(number(self.bytes, pos, size)?)
                        .map_err(|_| bad("bplist: count overflow"))?;
                    pos += size;
                }
                let size = match kind {
                    1 | 2 => 1usize
                        .checked_shl(nibble as u32)
                        .ok_or_else(|| bad("bplist: size overflow"))?,
                    3 => 8,
                    4 | 5 => count,
                    6 => count
                        .checked_mul(2)
                        .ok_or_else(|| bad("bplist: count overflow"))?,
                    8 => nibble as usize + 1,
                    10 | 12 => count
                        .checked_mul(self.rs)
                        .ok_or_else(|| bad("bplist: count overflow"))?,
                    13 => count
                        .checked_mul(self.rs * 2)
                        .ok_or_else(|| bad("bplist: count overflow"))?,
                    _ => 0,
                };
                let end = pos.checked_add(size).ok_or_else(|| bad("bplist: bounds"))?;
                let data = self
                    .bytes
                    .get(pos..end)
                    .ok_or_else(|| bad("bplist: truncated object"))?;
                Ok(match kind {
                    0 => match nibble {
                        0 => BplistValue::Null,
                        8 => BplistValue::Boolean(false),
                        9 => BplistValue::Boolean(true),
                        _ => return Err(bad(format!("bplist: unsupported primitive 0x0{nibble}"))),
                    },
                    1 | 8 => BplistValue::Integer(BigInt::from_bytes_be(Sign::Plus, data)),
                    2 => BplistValue::Real(match size {
                        4 => f32::from_be_bytes(data.try_into().unwrap()) as f64,
                        8 => f64::from_be_bytes(data.try_into().unwrap()),
                        _ => return Err(bad(format!("bplist: unsupported real size {size}"))),
                    }),
                    3 => BplistValue::DateMillis(
                        ((f64::from_be_bytes(data.try_into().unwrap()) + APPLE_EPOCH_SECONDS)
                            * 1000.0
                            + 0.5)
                            .floor() as i64,
                    ),
                    4 => BplistValue::Data(data.to_vec()),
                    5 => BplistValue::String(
                        data.iter()
                            .map(|b| if b.is_ascii() { *b as char } else { '\u{fffd}' })
                            .collect(),
                    ),
                    6 => BplistValue::String(String::from_utf16_lossy(
                        &data
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|&p| u16::from_be_bytes(p))
                            .collect::<Vec<_>>(),
                    )),
                    10 | 12 => {
                        let mut values = Vec::new();
                        for i in 0..count {
                            let r = number(self.bytes, pos + i * self.rs, self.rs)?;
                            values.push(self.read(
                                usize::try_from(r).map_err(|_| bad("bplist: bad reference"))?,
                                depth + 1,
                            )?);
                        }
                        BplistValue::Array(values)
                    }
                    13 => {
                        let mut values: Vec<(String, BplistValue)> = Vec::new();
                        for i in 0..count {
                            let key = number(self.bytes, pos + i * self.rs, self.rs)?;
                            let val = number(self.bytes, pos + (count + i) * self.rs, self.rs)?;
                            let key = match self.read(key as usize, depth + 1)? {
                                BplistValue::String(k) => k,
                                v => format!("{v:?}"),
                            };
                            let val = self.read(val as usize, depth + 1)?;
                            if let Some((_, old)) = values.iter_mut().find(|(k, _)| k == &key) {
                                *old = val;
                            } else {
                                values.push((key, val));
                            }
                        }
                        BplistValue::Dictionary(values)
                    }
                    _ => return Err(bad(format!("bplist: unsupported object type 0x{kind:x}"))),
                })
            }
        }
        Reader {
            bytes: &bytes[..table as usize],
            offsets,
            rs,
            budget: 1_000_000,
        }
        .read(top as usize, 0)
    }
}
/// Lossless binary-plist value model. Dictionary order is wire-significant.
/// `plist::Value` alone cannot represent null, so the codec owns this boundary value.
#[derive(Clone, Debug, PartialEq)]
pub enum BplistValue {
    /// Binary plist primitive 0x00.
    Null,
    /// Binary plist boolean.
    Boolean(bool),
    /// Signed or unsigned source integer without narrowing through f64.
    Integer(num_bigint::BigInt),
    /// IEEE-754 real.
    Real(f64),
    /// UTC milliseconds since the Unix epoch.
    DateMillis(i64),
    /// Opaque binary data.
    Data(Vec<u8>),
    /// Unicode string.
    String(String),
    /// Ordered sequence, including null values.
    Array(Vec<BplistValue>),
    /// Insertion-ordered string keys and values.
    Dictionary(Vec<(String, BplistValue)>),
}

impl BplistValue {
    pub fn get(&self, key: &str) -> Option<&Self> {
        if let Self::Dictionary(v) = self {
            v.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else {
            None
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Integer(v) if v.sign() != Sign::Minus => {
                let v = v.to_u64_digits().1;
                if v.len() <= 1 {
                    Some(v.first().copied().unwrap_or(0))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(v) => Some(*v),
            Self::Integer(v) => v.to_string().parse().ok(),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_array(&self) -> Option<&[Self]> {
        if let Self::Array(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_data(&self) -> Option<&[u8]> {
        if let Self::Data(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn dict(entries: impl IntoIterator<Item = (impl Into<String>, Self)>) -> Self {
        Self::Dictionary(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}
impl From<u64> for BplistValue {
    fn from(v: u64) -> Self {
        Self::Integer(v.into())
    }
}
impl From<u32> for BplistValue {
    fn from(v: u32) -> Self {
        Self::Integer(v.into())
    }
}
impl From<bool> for BplistValue {
    fn from(v: bool) -> Self {
        Self::Boolean(v)
    }
}
impl From<&str> for BplistValue {
    fn from(v: &str) -> Self {
        Self::String(v.into())
    }
}
impl From<String> for BplistValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}
