// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
pub struct AirPlayPortSelector;
impl AirPlayPortSelector {
    pub fn select(preferred: u16, bind: &dyn Fn(u16) -> Result<u16>) -> Result<u16> {
        for p in std::iter::once(preferred)
            .chain((7001..=7010).filter(|p| *p != preferred))
            .chain(std::iter::once(0))
        {
            match bind(p) {
                Ok(port) => return Ok(port),
                Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {}
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "No common AirPlay port available for the selected interface addresses",
        ))
    }
}
