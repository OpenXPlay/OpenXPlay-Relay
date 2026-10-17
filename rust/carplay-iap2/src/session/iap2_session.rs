// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::{sync::Arc, time::Duration};

pub struct Iap2Session {
    channel: Arc<Iap2CsmChannel>,
    trace: Arc<dyn Fn(String) + Send + Sync>,
    context: &'static str,
}
impl Iap2Session {
    pub fn open_wireless(
        stream: Arc<dyn DuplexStream>,
        trace: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            channel: Iap2CsmChannel::open_wireless(stream)?,
            context: "wireless",
            trace,
        }))
    }
    pub fn open_tunnel(
        stream: Arc<dyn DuplexStream>,
        trace: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            channel: Iap2CsmChannel::open_tunnel(stream)?,
            context: "wireless-tunnel",
            trace,
        }))
    }
    pub fn await_ready(&self, timeout: Duration) -> Result<bool> {
        let ready = self.channel.await_ready(timeout)?;
        self.emit(format!("IAP2 READY [{}] ready={ready}", self.context));
        Ok(ready)
    }
    pub fn send(&self, frame: &Iap2Frame, timeout: Duration) -> Result<()> {
        match self.channel.send(frame, timeout) {
            Ok(()) => {
                self.emit(Iap2FrameFormatter::format_with_context(
                    Iap2TraceDirection::Tx,
                    self.context,
                    frame,
                ));
                Ok(())
            }
            Err(e) => {
                self.emit(Iap2FrameFormatter::format_failure(
                    Iap2TraceDirection::Tx,
                    &frame.encoded_frame().unwrap_or_default(),
                    &e.to_string(),
                ));
                Err(e)
            }
        }
    }
    pub fn send_raw(&self, bytes: &[u8], timeout: Duration) -> Result<()> {
        if bytes.len() < 6
            || bytes[..2] != [0x40, 0x40]
            || u16::from_be_bytes([bytes[2], bytes[3]]) as usize != bytes.len()
        {
            return Err(invalid("CSM frame length must equal its complete size"));
        }
        self.send(
            &Iap2Frame {
                message_id: u16::from_be_bytes([bytes[4], bytes[5]]),
                payload: bytes[6..].to_vec(),
            },
            timeout,
        )
    }
    pub fn recv(&self, timeout: Duration) -> Result<Option<Iap2Frame>> {
        let frame = match self.channel.recv(timeout) {
            Ok(frame) => frame,
            Err(error) => {
                self.emit(
                    Iap2FrameFormatter::format_failure(
                        Iap2TraceDirection::Rx,
                        &[],
                        &error.to_string(),
                    )
                    .replacen("[]", &format!("[{}]", self.context), 1),
                );
                return Err(error);
            }
        };
        if let Some(f) = &frame {
            self.emit(Iap2FrameFormatter::format_with_context(
                Iap2TraceDirection::Rx,
                self.context,
                f,
            ));
        }
        Ok(frame)
    }
    pub fn close(&self) -> Result<()> {
        let result = self.channel.close();
        self.emit(format!("IAP2 CLOSE [{}]", self.context));
        result
    }
    pub fn is_closed(&self) -> bool {
        self.channel.is_closed()
    }
}

/// Protocol message, field, or framing constant.
pub const DEFAULT_SEND_TIMEOUT_MILLIS: u64 = 5_000;

impl Iap2Session {
    fn emit(&self, message: String) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.trace)(message)));
    }
}
