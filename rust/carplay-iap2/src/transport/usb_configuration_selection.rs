// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Retry a configuration switch after detaching drivers that prevent it.
use super::iphone_usb_host::{UsbError, UsbHostDevice, UsbResult};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbConfigurationOutcome {
    pub target: u8,
    pub selected: bool,
    pub before: Option<u8>,
    pub after: Option<u8>,
    pub detached: Vec<u8>,
}
impl UsbConfigurationOutcome {
    pub fn wrong_configuration(&self) -> bool {
        !self.selected && self.after.is_some_and(|v| v != self.target)
    }
    pub fn diagnostic(&self) -> String {
        let value = |v: Option<u8>| v.map_or_else(|| "unknown".into(), |v| v.to_string());
        let detached = if self.detached.is_empty() {
            "none".into()
        } else {
            self.detached
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "USB configuration select target={} ok={} before={} detached={} after={}",
            self.target,
            self.selected,
            value(self.before),
            detached,
            value(self.after)
        )
    }
}
pub struct UsbConfigurationSelection;
impl UsbConfigurationSelection {
    /// Linux can detach a driver without claiming/reopening the shared USB handle.
    pub fn select(
        device: &dyn UsbHostDevice,
        target: u8,
        interfaces: &[u8],
    ) -> UsbResult<Option<UsbConfigurationOutcome>> {
        Self::select_with(
            target,
            interfaces,
            || device.set_configuration(target),
            || device.active_configuration(),
            |i| device.detach_kernel_driver(i),
            || Ok(()),
        )
    }
    /// Testable switch ordering. `drop_claims` is needed by platforms whose detach also claims.
    pub fn select_with(
        target: u8,
        interfaces: &[u8],
        mut set: impl FnMut() -> UsbResult<()>,
        mut active: impl FnMut() -> UsbResult<Option<u8>>,
        mut detach: impl FnMut(u8) -> UsbResult<()>,
        mut drop_claims: impl FnMut() -> UsbResult<()>,
    ) -> UsbResult<Option<UsbConfigurationOutcome>> {
        match set() {
            Ok(()) => return Ok(None),
            Err(e) if e.is_device_gone() => return Err(e),
            Err(_) => (),
        }
        let before = match active() {
            Err(e) if e.is_device_gone() => return Err(e),
            v => v.unwrap_or(None),
        };
        if before == Some(target) {
            return Ok(Some(UsbConfigurationOutcome {
                target,
                selected: false,
                before,
                after: before,
                detached: vec![],
            }));
        }
        let mut ids = interfaces.to_vec();
        ids.sort_unstable();
        ids.dedup();
        let mut detached = Vec::new();
        for id in ids {
            match detach(id) {
                Ok(()) => detached.push(id),
                Err(e) if e.is_device_gone() => return Err(e),
                Err(_) => (),
            }
        }
        if !detached.is_empty() {
            drop_claims()?;
        }
        let selected = match set() {
            Ok(()) => true,
            Err(e) if e.is_device_gone() => return Err(e),
            Err(_) => false,
        };
        let after = match active() {
            Err(e) if e.is_device_gone() => return Err(e),
            v => v.unwrap_or(None),
        };
        Ok(Some(UsbConfigurationOutcome {
            target,
            selected,
            before,
            after,
            detached,
        }))
    }
}
impl From<UsbConfigurationOutcome> for UsbError {
    fn from(value: UsbConfigurationOutcome) -> Self {
        UsbError::Protocol(value.diagnostic())
    }
}
