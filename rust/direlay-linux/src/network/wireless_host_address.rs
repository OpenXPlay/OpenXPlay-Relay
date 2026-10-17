// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

pub struct WirelessHostAddress;
impl WirelessHostAddress {
    pub fn select(addresses: &[std::net::Ipv4Addr]) -> Option<std::net::Ipv4Addr> {
        addresses.iter().copied().find(|a| {
            !a.is_loopback() && !a.is_unspecified() && !a.is_link_local() && !a.is_multicast()
        })
    }
}
