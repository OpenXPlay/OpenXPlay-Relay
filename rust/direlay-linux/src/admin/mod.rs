// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Linux admin service contract tests.
pub mod auth;
pub mod server;
pub mod types;
pub use auth::{AdminAuth, AuthStore, FileAuthStore};
pub use server::{AdminServer, AdminServerConfig, Request, Response};
pub use types::*;
