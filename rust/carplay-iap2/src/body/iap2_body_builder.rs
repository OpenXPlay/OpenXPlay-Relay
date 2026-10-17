// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// State owned by `Iap2BodyBuilder`; construct through its factory.
pub struct Iap2BodyBuilder {
    message_id: u16,
    fields: Vec<Iap2FieldSpec>,
    entries: Vec<(Iap2Parameter, Iap2WireType)>,
}
impl Iap2BodyBuilder {
    pub fn of(endpoint: &Iap2Endpoint) -> Self {
        Self {
            message_id: endpoint.id,
            fields: endpoint.fields.clone(),
            entries: Vec::new(),
        }
    }

    pub fn opaque(message_id: u16) -> Self {
        Self {
            message_id,
            fields: Vec::new(),
            entries: Vec::new(),
        }
    }

    pub fn u8(&mut self, id: u16, value: u8) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::u8(value)?,
            },
            Iap2WireType::U8,
        ));
        Ok(self)
    }

    pub fn i8(&mut self, id: u16, value: i8) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::i8(value)?,
            },
            Iap2WireType::I8,
        ));
        Ok(self)
    }

    pub fn bool(&mut self, id: u16, value: bool) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::bool(value)?,
            },
            Iap2WireType::U8,
        ));
        Ok(self)
    }

    pub fn u16(&mut self, id: u16, value: u16) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::u16(value)?,
            },
            Iap2WireType::U16,
        ));
        Ok(self)
    }

    pub fn i16(&mut self, id: u16, value: i16) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::i16(value)?,
            },
            Iap2WireType::I16,
        ));
        Ok(self)
    }

    pub fn u32(&mut self, id: u16, value: u32) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::u32(value)?,
            },
            Iap2WireType::U32,
        ));
        Ok(self)
    }

    pub fn i32(&mut self, id: u16, value: i32) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::i32(value)?,
            },
            Iap2WireType::I32,
        ));
        Ok(self)
    }

    pub fn u64(&mut self, id: u16, value: u64) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::u64(value)?,
            },
            Iap2WireType::U64,
        ));
        Ok(self)
    }

    pub fn bytes(&mut self, id: u16, value: &[u8]) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: value.to_vec(),
            },
            Iap2WireType::Bytes,
        ));
        Ok(self)
    }

    pub fn string(&mut self, id: u16, value: &str) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::string(value)?,
            },
            Iap2WireType::String,
        ));
        Ok(self)
    }

    pub fn strings(&mut self, id: u16, value: &[String]) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::strings(value)?,
            },
            Iap2WireType::StringList,
        ));
        Ok(self)
    }

    pub fn u16_list(&mut self, id: u16, value: &[u16]) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Iap2WireCodec::u16_list(value)?,
            },
            Iap2WireType::U16List,
        ));
        Ok(self)
    }

    pub fn raw(&mut self, id: u16, value: &[u8]) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: value.to_vec(),
            },
            Iap2WireType::Opaque,
        ));
        Ok(self)
    }

    pub fn void(&mut self, id: u16) -> Result<&mut Self> {
        self.entries.push((
            Iap2Parameter {
                id,
                payload: Vec::new(),
            },
            Iap2WireType::Void,
        ));
        Ok(self)
    }

    pub fn group(&mut self, id: u16, mut body: Iap2BodyBuilder) -> Result<&mut Self> {
        if let Some(spec) = self.fields.iter().find(|s| s.id == id) {
            body.fields = spec.children.clone();
        }
        self.entries.push((
            Iap2Parameter {
                id,
                payload: body.encode_validated()?,
            },
            Iap2WireType::Group,
        ));
        Ok(self)
    }

    pub fn build(&self) -> Result<Iap2Frame> {
        Ok(Iap2Frame {
            message_id: self.message_id,
            payload: self.encode_validated()?,
        })
    }
}

impl Iap2BodyBuilder {
    pub fn optional_u8(&mut self, id: u16, value: Option<u8>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.u8(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_i8(&mut self, id: u16, value: Option<i8>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.i8(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_u16(&mut self, id: u16, value: Option<u16>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.u16(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_i16(&mut self, id: u16, value: Option<i16>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.i16(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_u32(&mut self, id: u16, value: Option<u32>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.u32(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_i32(&mut self, id: u16, value: Option<i32>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.i32(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_u64(&mut self, id: u16, value: Option<u64>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.u64(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_bytes(&mut self, id: u16, value: Option<&[u8]>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.bytes(id, value)?;
        }
        Ok(self)
    }

    pub fn optional_string(&mut self, id: u16, value: Option<&str>) -> Result<&mut Self> {
        if let Some(value) = value {
            self.string(id, value)?;
        }
        Ok(self)
    }
}

impl Iap2BodyBuilder {
    fn encode_validated(&self) -> Result<Vec<u8>> {
        if !self.fields.is_empty() {
            for (p, ty) in &self.entries {
                let spec = self
                    .fields
                    .iter()
                    .find(|s| s.id == p.id)
                    .ok_or_else(|| invalid(format!("Unknown body parameter 0x{:04x}", p.id)))?;
                if spec.wire_type != Iap2WireType::Opaque && spec.wire_type != *ty {
                    return Err(invalid(format!(
                        "Body parameter 0x{:04x} expected {:?}, got {:?}",
                        p.id, spec.wire_type, ty
                    )));
                }
                if !spec.repeatable && self.entries.iter().filter(|(v, _)| v.id == p.id).count() > 1
                {
                    return Err(invalid(format!(
                        "Body parameter 0x{:04x} is not repeatable",
                        p.id
                    )));
                }
            }
            for spec in &self.fields {
                if spec.required && !self.entries.iter().any(|(p, _)| p.id == spec.id) {
                    return Err(invalid(format!(
                        "Missing required body parameter 0x{:04x} ({})",
                        spec.id, spec.name
                    )));
                }
            }
        }
        Iap2ParameterList {
            parameters: self.entries.iter().map(|(p, _)| p.clone()).collect(),
        }
        .encode()
    }
}
