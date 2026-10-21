// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! Trusted HTTPS with a free domain name: the relay publishes its head-unit-facing private IPv4
//! under a dynamic DNS name, obtains a Let's Encrypt certificate for that name over ACME DNS-01
//! and serves it. Off by default; without `public-name.json` nothing changes.
pub mod acme;
pub mod cert;
pub mod config;
pub mod dns;
pub mod http;
pub mod jws;
pub mod local_dns;
pub mod providers;
pub mod schedule;
pub mod service;
pub mod store;
pub mod time;

pub use service::{Host, Network, PublicNameService, PublicNameStatus, Served};

#[cfg(test)]
pub(crate) mod testing;
