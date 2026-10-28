// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
#![allow(dead_code)]
use carplay_iap2::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
pub fn endpoint() -> Iap2WirelessCarPlayEndpoint {
    Iap2WirelessCarPlayEndpoint {
        ssid: "LIVI".into(),
        passphrase: "secret123".into(),
        channel: 36,
        security: Iap2WirelessSecurity::Wpa3Transition,
        ip_addresses: vec!["192.168.2.1".into()],
        air_play_port: 49152,
        device_identifier: "dev-1".into(),
        public_key: "aabbcc".into(),
        source_version: "1.0".into(),
        access_point_bssid: None,
    }
}
pub fn config() -> Iap2IdentificationConfig {
    Iap2IdentificationConfig {
        car_play_usb_interface_number: 3,
        route_guidance: false,
        name: "LIVI".into(),
        model_identifier: "LIVI".into(),
        manufacturer: "LIVI".into(),
        serial_number: "0123456".into(),
        firmware_version: "1.0.0".into(),
        hardware_version: "1.0".into(),
        language: "en".into(),
        external_accessory_protocol: "com.shilapi.xcertplay".into(),
        wireless: Some(Iap2WirelessIdentification {
            bluetooth_mac: "AA:BB:CC:DD:EE:FF".into(),
            ssid: "LIVI".into(),
        }),
    }
}
pub fn frame(id: u16, parameters: &[(u16, &[u8])]) -> Iap2Frame {
    Iap2Frame {
        message_id: id,
        payload: Iap2ParameterList {
            parameters: parameters
                .iter()
                .map(|(id, b)| Iap2Parameter {
                    id: *id,
                    payload: b.to_vec(),
                })
                .collect(),
        }
        .encode()
        .unwrap(),
    }
}
pub fn checksum(bytes: &[u8]) -> u8 {
    0u8.wrapping_sub(bytes.iter().fold(0u8, |s, b| s.wrapping_add(*b)))
}
pub fn packet(control: u8, sequence: u8, ack: u8, session: u8, payload: Option<&[u8]>) -> Vec<u8> {
    let n = payload.map_or(9, |b| b.len() + 10);
    let mut b = vec![
        255,
        90,
        (n >> 8) as u8,
        n as u8,
        control,
        sequence,
        ack,
        session,
    ];
    b.push(checksum(&b));
    if let Some(p) = payload {
        b.extend(p);
        b.push(checksum(p));
    }
    b
}
pub fn sync() -> SynchronizationPayload {
    SynchronizationPayload {
        max_outgoing: 8,
        max_length: 4096,
        retransmission_timeout_millis: 2000,
        acknowledgement_timeout_millis: 500,
        max_retransmissions: 4,
        max_acknowledgements: 3,
        sessions: vec![
            SessionDescriptor {
                id: 10,
                kind: 0,
                version: 2,
            },
            SessionDescriptor {
                id: 12,
                kind: 1,
                version: 2,
            },
        ],
    }
}
pub fn ready_engine() -> Iap2LinkEngine {
    let mut e = Iap2LinkEngine::new(Default::default());
    e.start(true, 0).unwrap();
    e.take_output();
    e.feed(&packet(0xc0, 1, 99, 0, Some(&sync().encode())), 1)
        .unwrap();
    assert!(e.writable());
    while e.poll_event().is_some() {}
    e.take_output();
    e
}
#[derive(Default)]
struct State {
    input: VecDeque<Vec<u8>>,
    output: Vec<u8>,
    closed: bool,
}
#[derive(Default)]
pub struct Phone {
    state: Mutex<State>,
    changed: Condvar,
}
impl Phone {
    pub fn script(frames: &[Iap2Frame]) -> Arc<Self> {
        let p = Arc::new(Self::default());
        let mut s = p.state.lock().unwrap();
        s.input
            .push_back(packet(0xc0, 1, 99, 0, Some(&sync().encode())));
        for (i, f) in frames.iter().enumerate() {
            s.input.push_back(packet(
                0x40,
                (i + 2) as u8,
                99,
                10,
                Some(&f.encoded_frame().unwrap()),
            ));
        }
        drop(s);
        p
    }
    pub fn session(self: &Arc<Self>) -> Arc<Iap2Session> {
        Iap2Session::open_tunnel(self.clone(), Arc::new(|_| {})).unwrap()
    }
    pub fn sent_frames(&self) -> Vec<Iap2Frame> {
        let s = self.state.lock().unwrap();
        let mut offset = 0;
        let mut framer = Iap2CsmFramer::new();
        let mut frames = Vec::new();
        while offset + 9 <= s.output.len() {
            if s.output[offset..].starts_with(&[255, 85]) {
                offset += 6;
                continue;
            }
            let n = u16::from_be_bytes([s.output[offset + 2], s.output[offset + 3]]) as usize;
            if n < 9 || offset + n > s.output.len() {
                break;
            }
            if s.output[offset + 7] == 10 && n > 9 {
                frames.extend(framer.offer(&s.output[offset + 9..offset + n - 1]).unwrap());
            }
            offset += n;
        }
        frames
    }
}
impl DuplexStream for Phone {
    fn send(&self, b: &[u8]) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        s.output.extend(b);
        let mut offset = 0;
        while offset + 9 <= b.len() {
            if b[offset..].starts_with(&[255, 85]) {
                offset += 6;
                continue;
            }
            let n = u16::from_be_bytes([b[offset + 2], b[offset + 3]]) as usize;
            if n < 9 || offset + n > b.len() {
                break;
            }
            if b[offset + 7] != 0 {
                s.input.push_back(packet(0x40, 1, b[offset + 5], 0, None));
            }
            offset += n;
        }
        self.changed.notify_all();
        Ok(())
    }
    fn recv(&self, _max: usize, timeout: Duration) -> Result<Receive> {
        let s = self.state.lock().unwrap();
        let mut s = self
            .changed
            .wait_timeout_while(s, timeout, |s| s.input.is_empty() && !s.closed)
            .unwrap()
            .0;
        if s.closed {
            Ok(Receive::Eof)
        } else {
            Ok(s.input
                .pop_front()
                .map(Receive::Data)
                .unwrap_or(Receive::Timeout))
        }
    }
    fn close(&self) -> Result<()> {
        self.state.lock().unwrap().closed = true;
        self.changed.notify_all();
        Ok(())
    }
}
