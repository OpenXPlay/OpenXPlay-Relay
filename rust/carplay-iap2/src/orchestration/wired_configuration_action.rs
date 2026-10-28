// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Decide whether descriptors can be reused or a mode transition is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WiredConfigurationAction {
    Use,
    UseWithoutAppleEthernet,
    RequestTransition,
    Reject,
}
impl WiredConfigurationAction {
    pub fn diagnostic(self) -> &'static str {
        match self {
            Self::Use => "reuse-descriptors",
            Self::UseWithoutAppleEthernet => "reuse-without-apple-ethernet",
            Self::RequestTransition => "request-transition",
            Self::Reject => "reject-missing-configuration",
        }
    }
    pub fn choose(
        found: bool,
        car_play_mode: bool,
        tried: u32,
        maximum: u32,
        session_started: bool,
    ) -> Self {
        if found && car_play_mode && session_started && tried == 0 {
            Self::RequestTransition
        } else if found && car_play_mode {
            Self::Use
        } else if found && tried > 0 {
            Self::UseWithoutAppleEthernet
        } else if tried < maximum {
            Self::RequestTransition
        } else {
            Self::Reject
        }
    }
    pub fn fallback_after_rejection(
        found: bool,
        car_play_mode: bool,
        session_started: bool,
    ) -> bool {
        found && (!car_play_mode || session_started)
    }
}
