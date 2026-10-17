// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
use std::net::Ipv4Addr;
pub struct HotspotAddressPolicy;
impl HotspotAddressPolicy {
    pub fn select_subnet(occupied: &[String]) -> Result<Ipv4Addr> {
        let nets: Vec<_> = occupied
            .iter()
            .filter_map(|s| {
                let (a, p) = s.split_once('/')?;
                Some((
                    u32::from(a.parse::<Ipv4Addr>().ok()?),
                    p.parse::<u32>().ok()?,
                ))
            })
            .filter(|(_, p)| *p <= 32)
            .collect();
        for n in (0..16)
            .map(|i| u32::from(Ipv4Addr::new(10, 77, i, 0)))
            .chain((200..216).map(|i| u32::from(Ipv4Addr::new(172, 31, i, 0))))
        {
            if nets.iter().all(|(a, p)| {
                let bits = (*p).min(24);
                let mask = if bits == 0 {
                    0
                } else {
                    u32::MAX << (32 - bits)
                };
                a & mask != n & mask
            }) {
                return Ok(Ipv4Addr::from(n | 1));
            }
        }
        Err(std::io::Error::other("No free IPv4 subnet for the hotspot"))
    }
}
pub const FAMILY_IPV4: &str = "IPv4";
pub const FAMILY_IPV6: &str = "IPv6";
