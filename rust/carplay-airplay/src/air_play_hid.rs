// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
/// `AirPlayContact` protocol data.
#[derive(Clone, Debug)]
pub struct AirPlayContact {
    pub id: u8,

    pub x: f64,

    pub y: f64,

    pub down: bool,
}
/// `AirPlayKnobState` protocol data.
#[derive(Clone, Debug, Default)]
pub struct AirPlayKnobState {
    pub select: bool,

    pub home: bool,

    pub back: bool,

    pub x: i8,

    pub y: i8,

    pub wheel: i8,
}
pub struct AirPlayHid;
fn device(uid: u64, name: &str, descriptor: Vec<u8>, uuid: &str) -> BplistValue {
    BplistValue::dict([
        ("hidProductID", 1u64.into()),
        ("hidVendorID", 2u64.into()),
        ("hidCountryCode", 0u64.into()),
        ("uuid", format!("{uid:x}").into()),
        ("name", name.into()),
        ("displayUUID", uuid.into()),
        ("hidDescriptor", BplistValue::Data(descriptor)),
    ])
}
impl AirPlayHid {
    pub fn touch_hid_device(x_max: u32, y_max: u32, display_uuid: &str) -> BplistValue {
        let mut d = vec![0x05, 0x0d, 0x09, 0x04, 0xa1, 0x01];
        for _ in 0..2 {
            d.extend([
                0x05,
                0x0d,
                0x09,
                0x22,
                0xa1,
                0x02,
                0x09,
                0x38,
                0x75,
                0x08,
                0x95,
                0x01,
                0x81,
                0x02,
                0x15,
                0,
                0x25,
                1,
                0x09,
                0x33,
                0x75,
                1,
                0x95,
                1,
                0x81,
                2,
                0x95,
                7,
                0x81,
                3,
                0x05,
                1,
                0x26,
                x_max as u8,
                (x_max >> 8) as u8,
                0x09,
                0x30,
                0x75,
                0x10,
                0x95,
                1,
                0x81,
                2,
                0x26,
                y_max as u8,
                (y_max >> 8) as u8,
                0x09,
                0x31,
                0x81,
                2,
                0xc0,
            ]);
        }
        d.push(0xc0);
        device(TOUCH_HID_UID, "xcertplay Touchscreen", d, display_uuid)
    }
    pub fn knob_hid_device(display_uuid: &str) -> BplistValue {
        device(
            KNOB_HID_UID,
            "xcertplay Knob",
            KNOB_DESCRIPTOR.to_vec(),
            display_uuid,
        )
    }
    pub fn media_hid_device(display_uuid: &str) -> BplistValue {
        device(
            MEDIA_HID_UID,
            "xcertplay Media",
            MEDIA_DESCRIPTOR.to_vec(),
            display_uuid,
        )
    }
    pub fn telephony_hid_device(display_uuid: &str) -> BplistValue {
        device(
            TELEPHONY_HID_UID,
            "xcertplay Telephony",
            TELEPHONY_DESCRIPTOR.to_vec(),
            display_uuid,
        )
    }
    pub fn touch_report(contacts: &[AirPlayContact]) -> Result<Vec<u8>> {
        let mut out = vec![0; 12];
        for slot in 0..2 {
            let offset = slot * 6;
            out[offset] = slot as u8;
            if let Some(c) = contacts.get(slot) {
                out[offset + 1] = u8::from(c.down);
                let x = (c.x.max(0.0) + 0.5).floor() as i64 as u16;
                let y = (c.y.max(0.0) + 0.5).floor() as i64 as u16;
                out[offset + 2..offset + 4].copy_from_slice(&x.to_le_bytes());
                out[offset + 4..offset + 6].copy_from_slice(&y.to_le_bytes());
            }
        }
        Ok(out)
    }
    pub fn knob_report(state: &AirPlayKnobState) -> Vec<u8> {
        vec![
            u8::from(state.select) | (u8::from(state.home) << 1) | (u8::from(state.back) << 2),
            state.x.max(-127) as u8,
            state.y.max(-127) as u8,
            state.wheel.max(-127) as u8,
        ]
    }
    pub fn media_report(index: u8) -> Vec<u8> {
        vec![index]
    }
    pub fn telephony_report(index: u8) -> Vec<u8> {
        vec![index]
    }
}
/// Protocol message, field, or framing constant.
pub const TOUCH_HID_UID: u64 = 0x2a2a2a2a;
/// Protocol message, field, or framing constant.
pub const KNOB_HID_UID: u64 = 0x2a2a2a2b;
/// Protocol message, field, or framing constant.
pub const MEDIA_HID_UID: u64 = 0x2a2a2a2c;
/// Protocol message, field, or framing constant.
pub const TELEPHONY_HID_UID: u64 = 0x2a2a2a2d;
/// Protocol message, field, or framing constant.
pub const TOUCH_CONTACTS: u64 = 2;
/// Protocol message, field, or framing constant.
pub const BYTES_PER_FINGER: u64 = 6;
const KNOB_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, 0x09, 0x08, 0xa1, 0x01, 0x05, 0x09, 0x09, 0x01, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01,
    0x95, 0x01, 0x81, 0x02, 0x05, 0x0c, 0x0a, 0x23, 0x02, 0x0a, 0x24, 0x02, 0x95, 0x02, 0x81, 0x02,
    0x95, 0x05, 0x81, 0x01, 0x05, 0x01, 0x09, 0x01, 0xa1, 0x00, 0x09, 0x30, 0x09, 0x31, 0x15, 0x81,
    0x25, 0x7f, 0x75, 0x08, 0x95, 0x02, 0x81, 0x02, 0xc0, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7f, 0x75,
    0x08, 0x95, 0x01, 0x81, 0x06, 0xc0,
];
const MEDIA_DESCRIPTOR: &[u8] = &[
    0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0x15, 0x00, 0x25, 0x06, 0x05, 0x0c, 0x0a, 0x00, 0x00, 0x0a,
    0xb0, 0x00, 0x0a, 0xb1, 0x00, 0x0a, 0xcd, 0x00, 0x0a, 0xb5, 0x00, 0x0a, 0xb6, 0x00, 0x0a, 0x9e,
    0x02, 0x75, 0x08, 0x95, 0x01, 0x81, 0x00, 0xc0,
];
const TELEPHONY_DESCRIPTOR: &[u8] = &[
    0x05, 0x0b, 0x09, 0x07, 0xa1, 0x01, 0x15, 0x00, 0x25, 0x11, 0x05, 0x0b, 0x09, 0x00, 0x09, 0x20,
    0x09, 0x21, 0x09, 0x26, 0x09, 0x2f, 0x09, 0xb0, 0x09, 0xb1, 0x09, 0xb2, 0x09, 0xb3, 0x09, 0xb4,
    0x09, 0xb5, 0x09, 0xb6, 0x09, 0xb7, 0x09, 0xb8, 0x09, 0xb9, 0x09, 0xba, 0x09, 0xbb, 0x05, 0x07,
    0x09, 0x2a, 0x75, 0x08, 0x95, 0x01, 0x81, 0x00, 0xc0,
];
