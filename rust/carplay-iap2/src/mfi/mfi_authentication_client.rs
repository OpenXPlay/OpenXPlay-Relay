// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// `BaaCertificatePair` protocol data.
#[derive(Clone, Debug)]
pub struct BaaCertificatePair {
    pub leaf: Vec<u8>,

    pub intermediate: Vec<u8>,
}
/// `MfiCertificateType` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MfiCertificateType {
    Mfi,
    Baa,
}
/// Platform interface for `MfiAuthenticator`. Implementations must support bounded shutdown.
pub trait MfiAuthenticator: Send + Sync {
    fn certificate_type(&self) -> MfiCertificateType {
        MfiCertificateType::Mfi
    }
    /// Errors propagate to the lifecycle owner.
    fn protocol_major(&self) -> Result<u8>;
    /// Errors propagate to the lifecycle owner.
    fn read_certificate(&self, maximum_output_length: usize) -> Result<Vec<u8>>;
    /// Errors propagate to the lifecycle owner.
    fn sign_challenge(&self, challenge: &[u8]) -> Result<Vec<u8>>;
    /// Errors propagate to the lifecycle owner.
    fn baa_certificates(&self) -> Result<BaaCertificatePair>;
}

/// Protocol message, field, or framing constant.
pub const DEFAULT_MAXIMUM_CERTIFICATE_OUTPUT_LENGTH: u64 = 65_525;
/// Protocol message, field, or framing constant.
pub const MAX_REGISTER_READ_BYTES: u64 = 65_525;
/// Protocol message, field, or framing constant.
pub const BYTE_MASK: u64 = 0xff;
/// Protocol message, field, or framing constant.
pub const PROTOCOL_MAJOR_REGISTER: u64 = 0x02;
/// Protocol message, field, or framing constant.
pub const ERROR_REGISTER: u64 = 0x05;
/// Protocol message, field, or framing constant.
pub const AUTH_CONTROL_STATUS_REGISTER: u64 = 0x10;
/// Protocol message, field, or framing constant.
pub const RESPONSE_LENGTH_REGISTER: u64 = 0x11;
/// Protocol message, field, or framing constant.
pub const RESPONSE_DATA_REGISTER: u64 = 0x12;
/// Protocol message, field, or framing constant.
pub const CHALLENGE_LENGTH_REGISTER: u64 = 0x20;
/// Protocol message, field, or framing constant.
pub const CHALLENGE_DATA_REGISTER: u64 = 0x21;
/// Protocol message, field, or framing constant.
pub const CERTIFICATE_LENGTH_REGISTER: u64 = 0x30;
/// Protocol message, field, or framing constant.
pub const CERTIFICATE_DATA_REGISTER: u64 = 0x31;
/// Protocol message, field, or framing constant.
pub const CERTIFICATE_REGISTER_WINDOW_BYTES: u64 = 128;
/// Protocol message, field, or framing constant.
pub const AUTH_START_COMMAND: u64 = 0x01;
/// Protocol message, field, or framing constant.
pub const AUTH_SUCCESS_STATUS: u64 = 0x10;
/// Protocol message, field, or framing constant.
pub const MINIMUM_CHALLENGE_BYTES: u64 = 1;
/// Protocol message, field, or framing constant.
pub const MAXIMUM_CHALLENGE_BYTES: u64 = 128;
/// Protocol message, field, or framing constant.
pub const IO_RETRY_TIMEOUT_MILLIS: u64 = 2_000;
/// Protocol message, field, or framing constant.
pub const IO_RETRY_DELAY_MICROS: u64 = 20_000;
/// Protocol message, field, or framing constant.
pub const INITIAL_AUTH_DELAY_MILLIS: u64 = 10;
/// Protocol message, field, or framing constant.
pub const AUTH_POLL_MILLIS: u64 = 10;
/// Protocol message, field, or framing constant.
pub const AUTH_TIMEOUT_MILLIS: u64 = 3_000;
/// Protocol message, field, or framing constant.
pub const MICROS_PER_MILLISECOND: u64 = 1_000;
/// Protocol message, field, or framing constant.
pub const NANOS_PER_MICROSECOND: u64 = 1_000;
/// Protocol message, field, or framing constant.
pub const NANOS_PER_MILLISECOND: u64 = 1_000_000;
