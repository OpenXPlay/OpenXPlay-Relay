// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Descriptor-based selection of the iPhone's USBMUX and NCM configuration.
use crate::{invalid, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbEndpoint {
    pub address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}
impl UsbEndpoint {
    pub fn is_in(&self) -> bool {
        self.address & 0x80 != 0
    }
    pub fn transfer_type(&self) -> u8 {
        self.attributes & 3
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbInterface {
    pub number: u8,
    pub alternate_setting: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub endpoints: Vec<UsbEndpoint>,
    pub extra: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbConfiguration {
    pub value: u8,
    pub interfaces: Vec<UsbInterface>,
}
/// Raw USB device and configuration descriptors, including class-specific descriptors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbDescriptors {
    pub device: Vec<u8>,
    pub configurations: Vec<UsbConfiguration>,
}
impl UsbDescriptors {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut result = Self {
            device: Vec::new(),
            configurations: Vec::new(),
        };
        let mut offset = 0;
        while offset < bytes.len() {
            if bytes.len() - offset < 2 {
                return Err(invalid("Truncated USB descriptor"));
            }
            let length = bytes[offset] as usize;
            if length < 2 || length > bytes.len() - offset {
                return Err(invalid("Invalid USB descriptor length"));
            }
            let d = &bytes[offset..offset + length];
            match (d[1], length) {
                (1, 18..) => result.device = d.to_vec(),
                (2, 9..) => result.configurations.push(UsbConfiguration {
                    value: d[5],
                    interfaces: Vec::new(),
                }),
                (4, 9..) => {
                    let c = result
                        .configurations
                        .last_mut()
                        .ok_or_else(|| invalid("USB interface without configuration"))?;
                    c.interfaces.push(UsbInterface {
                        number: d[2],
                        alternate_setting: d[3],
                        class: d[5],
                        subclass: d[6],
                        protocol: d[7],
                        endpoints: Vec::new(),
                        extra: Vec::new(),
                    });
                }
                (5, 7..) => {
                    let i = result
                        .configurations
                        .last_mut()
                        .and_then(|c| c.interfaces.last_mut())
                        .ok_or_else(|| invalid("USB endpoint without interface"))?;
                    i.endpoints.push(UsbEndpoint {
                        address: d[2],
                        attributes: d[3],
                        max_packet_size: u16::from_le_bytes([d[4], d[5]]) & 0x7ff,
                        interval: d[6],
                    });
                }
                _ => {
                    if let Some(i) = result
                        .configurations
                        .last_mut()
                        .and_then(|c| c.interfaces.last_mut())
                    {
                        i.extra.extend_from_slice(d);
                    }
                }
            }
            offset += length;
        }
        Ok(result)
    }
    pub fn usb_version(&self) -> Option<String> {
        self.device
            .get(12..14)
            .map(|v| format!("{:x}.{:02x}", v[1], v[0]))
    }
}
pub struct IphoneCarPlayConfiguration;
impl IphoneCarPlayConfiguration {
    pub fn find(configurations: &[UsbConfiguration]) -> Option<&UsbConfiguration> {
        configurations
            .iter()
            .find(|c| Self::is_car_play_mode(c))
            .or_else(|| {
                configurations
                    .iter()
                    .find(|c| Self::usb_mux_interface(c).is_some() && Self::has_ncm(c))
            })
    }
    pub fn is_car_play_mode(c: &UsbConfiguration) -> bool {
        Self::usb_mux_interface(c).is_some()
            && Self::has_ncm(c)
            && c.interfaces
                .iter()
                .any(|i| (i.class, i.subclass, i.protocol) == (0xff, 0xfd, 1))
    }
    fn has_ncm(c: &UsbConfiguration) -> bool {
        c.interfaces
            .iter()
            .any(|i| (i.class, i.subclass) == (2, 0x0d))
    }
    pub fn describe(c: &UsbConfiguration) -> String {
        c.interfaces
            .iter()
            .map(|i| {
                format!(
                    "{}/{}:{:x}.{:x}.{:x}x{}",
                    i.number,
                    i.alternate_setting,
                    i.class,
                    i.subclass,
                    i.protocol,
                    i.endpoints.len()
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
    pub fn usb_mux_interface(c: &UsbConfiguration) -> Option<&UsbInterface> {
        c.interfaces
            .iter()
            .find(|i| (i.class, i.subclass, i.protocol) == (0xff, 0xfe, 2))
    }
    /// Returns OUT then IN; ambiguous fallback endpoint sets are rejected.
    pub fn usb_mux_endpoints(i: &UsbInterface) -> Option<(UsbEndpoint, UsbEndpoint)> {
        fn pick(i: &UsbInterface, input: bool, preferred: u8) -> Option<UsbEndpoint> {
            let endpoints: Vec<_> = i
                .endpoints
                .iter()
                .filter(|e| e.is_in() == input && e.transfer_type() == 2)
                .collect();
            endpoints
                .iter()
                .find(|e| e.address == preferred)
                .copied()
                .or_else(|| (endpoints.len() == 1).then(|| endpoints[0]))
                .cloned()
        }
        Some((pick(i, false, 4)?, pick(i, true, 0x85)?))
    }
}
