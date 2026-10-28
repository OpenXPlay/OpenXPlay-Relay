// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Locate the CDC-NCM control/data alternate settings without claiming them.
use super::iphone_car_play_configuration::{UsbConfiguration, UsbEndpoint, UsbInterface};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NcmFunction {
    pub control: UsbInterface,
    pub data: UsbInterface,
    pub status_in: Option<UsbEndpoint>,
    pub bulk_in: UsbEndpoint,
    pub bulk_out: UsbEndpoint,
}
pub struct NcmFunctionDiscovery;
impl NcmFunctionDiscovery {
    pub fn find(c: &UsbConfiguration) -> Option<NcmFunction> {
        let control = c
            .interfaces
            .iter()
            .find(|i| i.class == 2 && i.subclass == 0x0d)?;
        let data = c
            .interfaces
            .iter()
            .filter(|i| i.class == 0x0a && Self::bulk_endpoints(i).is_some())
            .min_by_key(|i| u8::from(i.alternate_setting != 1))?;
        let (bulk_in, bulk_out) = Self::bulk_endpoints(data)?;
        let status: Vec<_> = control
            .endpoints
            .iter()
            .filter(|e| e.is_in() && e.transfer_type() == 3)
            .collect();
        Some(NcmFunction {
            control: control.clone(),
            data: data.clone(),
            status_in: if status.len() == 1 {
                Some(status[0].clone())
            } else {
                None
            },
            bulk_in,
            bulk_out,
        })
    }
    fn bulk_endpoints(i: &UsbInterface) -> Option<(UsbEndpoint, UsbEndpoint)> {
        let input: Vec<_> = i
            .endpoints
            .iter()
            .filter(|e| e.is_in() && e.transfer_type() == 2)
            .collect();
        let output: Vec<_> = i
            .endpoints
            .iter()
            .filter(|e| !e.is_in() && e.transfer_type() == 2)
            .collect();
        (input.len() == 1 && output.len() == 1).then(|| (input[0].clone(), output[0].clone()))
    }
}
