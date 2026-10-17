// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// State owned by `Iap2BodyReader`; construct through its factory.
pub struct Iap2BodyReader {
    parameters: Iap2ParameterList,
}
impl Iap2BodyReader {
    pub fn of(frame: &Iap2Frame) -> Result<Self> {
        Ok(Self {
            parameters: frame.body()?,
        })
    }

    pub fn has(&self, id: u16) -> bool {
        self.parameters.has(id)
    }

    pub fn list(&self) -> &[Iap2Parameter] {
        &self.parameters.parameters
    }

    pub fn u8(&self, id: u16) -> Result<u8> {
        Iap2WireCodec::read_u8(&self.scalar(id, 1)?)
    }

    pub fn i8(&self, id: u16) -> Result<i8> {
        Iap2WireCodec::read_i8(&self.scalar(id, 1)?)
    }

    pub fn bool(&self, id: u16) -> Result<bool> {
        Iap2WireCodec::read_bool(&self.scalar(id, 1)?)
    }

    pub fn u16(&self, id: u16) -> Result<u16> {
        Iap2WireCodec::read_u16(&self.scalar(id, 2)?)
    }

    pub fn i16(&self, id: u16) -> Result<i16> {
        Iap2WireCodec::read_i16(&self.scalar(id, 2)?)
    }

    pub fn u32(&self, id: u16) -> Result<u32> {
        Iap2WireCodec::read_u32(&self.scalar(id, 4)?)
    }

    pub fn i32(&self, id: u16) -> Result<i32> {
        Iap2WireCodec::read_i32(&self.scalar(id, 4)?)
    }

    pub fn u64(&self, id: u16) -> Result<u64> {
        Iap2WireCodec::read_u64(&self.scalar(id, 8)?)
    }

    pub fn bytes(&self, id: u16) -> Result<Vec<u8>> {
        self.raw(id)
    }

    pub fn string(&self, id: u16) -> Result<String> {
        Iap2WireCodec::read_string(&self.raw(id)?)
            .map_err(|_| invalid(format!("Malformed parameter 0x{id:04x}")))
    }

    pub fn strings(&self, id: u16) -> Result<Vec<String>> {
        Iap2WireCodec::read_strings(&self.raw(id)?)
            .map_err(|_| invalid(format!("Malformed parameter 0x{id:04x}")))
    }

    pub fn u16_list(&self, id: u16) -> Result<Vec<u16>> {
        Iap2WireCodec::read_u16_list(&self.raw(id)?)
            .map_err(|_| invalid(format!("Malformed parameter 0x{id:04x}")))
    }

    pub fn group(&self, id: u16) -> Result<Self> {
        Ok(Self {
            parameters: Iap2ParameterList::parse(&self.raw(id)?)?,
        })
    }

    pub fn optional_u8(&self, id: u16) -> Result<Option<u8>> {
        if self.has(id) {
            self.u8(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_u16(&self, id: u16) -> Result<Option<u16>> {
        if self.has(id) {
            self.u16(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_u32(&self, id: u16) -> Result<Option<u32>> {
        if self.has(id) {
            self.u32(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_u64(&self, id: u16) -> Result<Option<u64>> {
        if self.has(id) {
            self.u64(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_string(&self, id: u16) -> Result<Option<String>> {
        if self.has(id) {
            self.string(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_bytes(&self, id: u16) -> Result<Option<Vec<u8>>> {
        if self.has(id) {
            self.bytes(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_group(&self, id: u16) -> Result<Option<Self>> {
        if self.has(id) {
            self.group(id).map(Some)
        } else {
            Ok(None)
        }
    }
}

impl Iap2BodyReader {
    pub fn optional_i8(&self, id: u16) -> Result<Option<i8>> {
        if self.has(id) {
            self.i8(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_bool(&self, id: u16) -> Result<Option<bool>> {
        if self.has(id) {
            self.bool(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_i16(&self, id: u16) -> Result<Option<i16>> {
        if self.has(id) {
            self.i16(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_i32(&self, id: u16) -> Result<Option<i32>> {
        if self.has(id) {
            self.i32(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_strings(&self, id: u16) -> Result<Option<Vec<String>>> {
        if self.has(id) {
            self.strings(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn optional_u16_list(&self, id: u16) -> Result<Option<Vec<u16>>> {
        if self.has(id) {
            self.u16_list(id).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list().is_empty()
    }

    pub fn first(&self, id: u16) -> Option<Iap2Parameter> {
        self.parameters.first(id).cloned()
    }

    pub fn all(&self, id: u16) -> Vec<Iap2Parameter> {
        self.parameters.all(id).into_iter().cloned().collect()
    }

    pub fn raw(&self, id: u16) -> Result<Vec<u8>> {
        Ok(self
            .first(id)
            .ok_or_else(|| invalid(format!("Missing required parameter 0x{id:04x}")))?
            .payload
            .clone())
    }
}

impl Iap2BodyReader {
    fn scalar(&self, id: u16, n: usize) -> Result<Vec<u8>> {
        let bytes = self.raw(id)?;
        if bytes.len() != n {
            return Err(invalid(format!(
                "Parameter 0x{id:04x} expected {n} bytes, got {}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }
}
