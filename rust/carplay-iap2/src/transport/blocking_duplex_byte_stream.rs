// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use std::time::Duration;

/// `Receive` states; preserve wire values in its codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Receive {
    Data(Vec<u8>),
    Timeout,
    Eof,
}
/// Platform interface for `DuplexStream`. Implementations must support bounded shutdown.
pub trait DuplexStream: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn send(&self, data: &[u8]) -> Result<()>;
    /// Errors propagate to the lifecycle owner.
    fn recv(&self, max_bytes: usize, timeout: Duration) -> Result<Receive>;
    /// Errors propagate to the lifecycle owner.
    fn close(&self) -> Result<()>;
}

pub use DuplexStream as BlockingDuplexByteStream;
