// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-28

use crate::{Result, WirelessHotspotInfo};

/// The admin lifecycle, independent of the iPhone transport.
pub trait CarPlayController: Send + Sync {
    fn run(&self) -> Result<()>;
    fn close(&self) -> Result<()>;
    fn stage(&self) -> String;
    fn failed(&self) -> bool;
    fn armed(&self) -> bool;
    fn set_start_when_ready(&self, automatic: bool);
    fn readiness_restored(&self);
    fn running(&self) -> bool;
    fn hotspot_info(&self) -> Option<WirelessHotspotInfo>;
    fn transport(&self) -> &'static str;
}

macro_rules! controller {
    ($ty:ty, $transport:literal) => {
        impl CarPlayController for $ty {
            fn run(&self) -> Result<()> {
                self.run()
            }
            fn close(&self) -> Result<()> {
                self.close()
            }
            fn stage(&self) -> String {
                self.stage()
            }
            fn failed(&self) -> bool {
                self.failed()
            }
            fn armed(&self) -> bool {
                self.armed()
            }
            fn set_start_when_ready(&self, automatic: bool) {
                self.set_start_when_ready(automatic)
            }
            fn readiness_restored(&self) {
                self.readiness_restored()
            }
            fn running(&self) -> bool {
                self.running()
            }
            fn hotspot_info(&self) -> Option<WirelessHotspotInfo> {
                self.hotspot_info()
            }
            fn transport(&self) -> &'static str {
                $transport
            }
        }
    };
}
controller!(crate::LinuxCarPlayController, "wireless");
controller!(crate::linux_wired_controller::LinuxWiredController, "usb");
