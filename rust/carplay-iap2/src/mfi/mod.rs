// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
/// iap2 mfi authentication client protocol module.
pub mod iap2_mfi_authentication_client;
/// local mfi authentication client protocol module.
pub mod local_mfi_authentication_client;
/// mfi authentication client protocol module.
pub mod mfi_authentication_client;
pub use mfi_authentication_client::{BaaCertificatePair, MfiAuthenticator, MfiCertificateType};

pub mod ch341;
pub mod coprocessor;
#[cfg(any(target_os = "linux", test))]
pub mod linux;
pub mod remote;
