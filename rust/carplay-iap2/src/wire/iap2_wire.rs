// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// `Iap2Parameter` protocol data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Iap2Parameter {
    pub id: u16,

    pub payload: Vec<u8>,
}
/// `Iap2ParameterList` protocol data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Iap2ParameterList {
    pub parameters: Vec<Iap2Parameter>,
}
impl Iap2ParameterList {
    pub fn parse(encoded: &[u8]) -> Result<Self> {
        let mut parameters = Vec::new();
        let mut offset = 0;
        while offset < encoded.len() {
            if encoded.len() - offset < 4 {
                return Err(invalid("Truncated CSM parameter header"));
            }
            let length = u16::from_be_bytes([encoded[offset], encoded[offset + 1]]) as usize;
            if length < 4 || length > encoded.len() - offset {
                return Err(invalid(format!("Invalid CSM parameter length {length}")));
            }
            parameters.push(Iap2Parameter {
                id: u16::from_be_bytes([encoded[offset + 2], encoded[offset + 3]]),
                payload: encoded[offset + 4..offset + length].to_vec(),
            });
            offset += length;
        }
        Ok(Self { parameters })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for p in &self.parameters {
            let next = Iap2CsmFramer::encode_param(p.id, &p.payload)?;
            if next.len() > 65529 - out.len() {
                return Err(invalid(
                    "Encoded CSM parameters exceed the 65529-byte message body limit",
                ));
            }
            out.extend(next);
        }
        Ok(out)
    }

    pub fn first(&self, id: u16) -> Option<&Iap2Parameter> {
        self.parameters.iter().find(|p| p.id == id)
    }

    pub fn all(&self, id: u16) -> Vec<&Iap2Parameter> {
        self.parameters.iter().filter(|p| p.id == id).collect()
    }

    pub fn has(&self, id: u16) -> bool {
        self.first(id).is_some()
    }
}
/// `Iap2Frame` protocol data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Iap2Frame {
    pub message_id: u16,

    pub payload: Vec<u8>,
}
impl Iap2Frame {
    pub fn body(&self) -> Result<Iap2ParameterList> {
        Iap2ParameterList::parse(&self.payload)
    }

    pub fn encoded_frame(&self) -> Result<Vec<u8>> {
        Iap2CsmFramer::encode_frame(self.message_id, &self.payload)
    }
}

/// Protocol message, field, or framing constant.
pub const HEADER_BYTES: u64 = 4;
