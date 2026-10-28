// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Incremental USBMUX framing with narrowly validated four-byte trailer recovery.
use crate::{invalid, Result};
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbMuxFrame {
    pub protocol: u32,
    pub length: usize,
    pub word8: u32,
    pub sequence: u16,
    pub payload: Vec<u8>,
}
pub struct UsbMuxFrameBuffer {
    bytes: Vec<u8>,
    optional: Option<UsbMuxFrame>,
    reports: usize,
    last_read: usize,
    diagnostic: Arc<dyn Fn(String) + Send + Sync>,
}
impl Default for UsbMuxFrameBuffer {
    fn default() -> Self {
        Self::new(Arc::new(|_| {}))
    }
}
impl UsbMuxFrameBuffer {
    pub fn new(diagnostic: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        Self {
            bytes: vec![],
            optional: None,
            reports: 0,
            last_read: 0,
            diagnostic,
        }
    }
    pub fn buffered_bytes(&self) -> usize {
        self.bytes.len()
    }
    pub fn append(&mut self, transfer: &[u8]) {
        self.last_read = transfer.len();
        self.bytes.extend_from_slice(transfer);
    }
    pub fn take_frame(&mut self) -> Result<Option<UsbMuxFrame>> {
        if self.bytes.len() < 16 {
            return Ok(None);
        }
        let mut length = be32(&self.bytes, 4) as usize;
        if !(16..=65536).contains(&length) {
            let b = &self.bytes;
            let previous = self.optional.as_ref();
            if previous.is_some() && be32(b, 0) != 6 && b.len() < 20 {
                return Ok(None);
            }
            let protocol = be32(b, 4);
            let following = be32(b, 8) as usize;
            let candidate =
                previous.is_some() && be32(b, 0) != 6 && b.len() >= 16 && be32(b, 12) == 0xfaceface;
            let valid = if candidate && protocol == 6 && (36..=65536).contains(&following) {
                if b.len() < 40 {
                    return Ok(None);
                }
                let h = ((b[32] >> 4) as usize) * 4;
                (20..=following - 16).contains(&h) && be16(b, 20) != 0 && be16(b, 22) != 0
            } else if candidate && protocol == 1 && (18..=1040).contains(&following) {
                if b.len() < 4 + following {
                    return Ok(None);
                }
                diagnostic_payload(&b[20..4 + following])
            } else {
                previous.is_some()
                    && be32(b, 0) != 6
                    && protocol == 0
                    && following == 20
                    && be32(b, 12) == 2
            };
            if valid {
                let previous = self.optional.take().unwrap();
                self.bytes.drain(..4);
                if self.reports < 4 {
                    self.report(format!("USBMUX optional reply padding skipped bytes=4 previousProtocol={} previousLength={} nextProtocol={protocol} nextLength={following} lastUsbReadBytes={} bufferedBytes={}",previous.protocol,previous.length,self.last_read,self.bytes.len()));
                }
                self.reports += 1;
                length = be32(&self.bytes, 4) as usize;
            } else {
                self.report(format!("USBMUX framing rejected declaredLength={length} bufferedBytes={} lastUsbReadBytes={} optionalReplyPadding={}",self.bytes.len(),self.last_read,self.optional.is_some()));
                return Err(invalid(format!("Invalid USBMUX frame length {length}")));
            }
        }
        if self.bytes.len() < length {
            return Ok(None);
        }
        let frame = UsbMuxFrame {
            protocol: be32(&self.bytes, 0),
            length,
            word8: be32(&self.bytes, 8),
            sequence: be16(&self.bytes, 12),
            payload: self.bytes[16..length].to_vec(),
        };
        self.bytes.drain(..length);
        let optional = if frame.protocol == 0 && frame.length == 20 && frame.word8 == 2 {
            true
        } else if frame.protocol == 1 && frame.word8 == 0xfaceface {
            diagnostic_payload(&frame.payload)
        } else if frame.protocol == 6 && frame.word8 == 0xfaceface && frame.payload.len() >= 20 {
            let h = (frame.payload[12] >> 4) as usize * 4;
            (20..=frame.payload.len()).contains(&h)
                && be16(&frame.payload, 0) != 0
                && be16(&frame.payload, 2) != 0
        } else {
            false
        };
        self.optional = optional.then(|| frame.clone());
        Ok(Some(frame))
    }
    fn report(&self, line: String) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.diagnostic)(line)));
    }
}
fn diagnostic_payload(b: &[u8]) -> bool {
    (2..=1024).contains(&b.len()) && b[0] == 4 && b[1..].iter().all(|c| (0x20..=0x7e).contains(c))
}
pub(crate) fn be16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}
pub(crate) fn be32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(b[o..o + 4].try_into().unwrap())
}
