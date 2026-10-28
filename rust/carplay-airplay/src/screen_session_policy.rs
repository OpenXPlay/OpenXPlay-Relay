// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-27
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

/// Captured by one engine; host preference changes do not alter a running connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenSessionPolicy {
    pub isolate_sessions: bool,
    pub reset_on_reconnect: bool,
    pub serialize_delivery: bool,
}

impl Default for ScreenSessionPolicy {
    fn default() -> Self {
        Self {
            isolate_sessions: true,
            reset_on_reconnect: true,
            serialize_delivery: true,
        }
    }
}

/// Optional host policy for screen lifecycle experiments.
pub trait ScreenSessionPolicyProvider {
    fn screen_session_policy(&self) -> ScreenSessionPolicy;
}
