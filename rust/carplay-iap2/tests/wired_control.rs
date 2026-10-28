// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

mod common;
use carplay_iap2::{transport::iap2_wired_control_client::*, *};
use common::*;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct Auth;
impl MfiAuthenticator for Auth {
    fn protocol_major(&self) -> Result<u8> {
        Ok(3)
    }
    fn read_certificate(&self, _: usize) -> Result<Vec<u8>> {
        Ok(vec![1, 2])
    }
    fn sign_challenge(&self, c: &[u8]) -> Result<Vec<u8>> {
        assert_eq!(c, [7; 32]);
        Ok(vec![8; 64])
    }
    fn baa_certificates(&self) -> Result<BaaCertificatePair> {
        unreachable!()
    }
}
#[derive(Default)]
struct Events {
    stages: Mutex<Vec<Iap2WiredControlStage>>,
    authenticated: Mutex<usize>,
    start: Mutex<Vec<Iap2StartSessionSent>>,
    incoming: Mutex<Vec<u16>>,
}
impl WiredControlEvents for Events {
    fn on_stage_changed(&self, s: Iap2WiredControlStage) {
        self.stages.lock().unwrap().push(s);
    }
    fn on_authenticated(&self) {
        *self.authenticated.lock().unwrap() += 1;
    }
    fn on_start_session_sent(&self, e: Iap2StartSessionSent) {
        self.start.lock().unwrap().push(e);
    }
    fn on_incoming(&self, f: &Iap2Frame) {
        self.incoming.lock().unwrap().push(f.message_id);
    }
}
fn wired_endpoint() -> Iap2WiredCarPlayEndpoint {
    Iap2WiredCarPlayEndpoint::new(7000, [2, 0, 0, 0, 0, 2], "abcd".into(), "1.0".into()).unwrap()
}
#[test]
fn wired_start_session_and_configured_current_match_vectors() {
    let frame = Iap2WiredControlClient::car_play_start_session(&wired_endpoint()).unwrap();
    let b = Iap2BodyReader::of(&frame).unwrap();
    assert_eq!(frame.message_id, 0x4301);
    assert!(!b.has(1));
    let addresses = b.group(0).unwrap();
    assert_eq!(addresses.list().len(), 1);
    assert_eq!(addresses.bytes(0).unwrap(), b"fe80::2\0");
    assert_eq!(b.u32(2).unwrap(), 7000);
    assert_eq!(b.string(3).unwrap(), "02:00:00:00:00:02");
    assert_eq!(b.string(4).unwrap(), "abcd");
    assert_eq!(b.string(5).unwrap(), "1.0");
    for current in [2400, 1500, 500] {
        let frame = Iap2WiredControlClient::power_source_update(current).unwrap();
        let b = Iap2BodyReader::of(&frame).unwrap();
        assert_eq!(frame.message_id, 0xae03);
        assert_eq!(b.bytes(0).unwrap(), current.to_be_bytes());
        assert_eq!(b.u8(1).unwrap(), 1);
    }
    let strings = Iap2WiredControlClient::subscriptions()
        .unwrap()
        .iter()
        .map(|f| hex(&f.encoded_frame().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(strings,["4040002e5000001800000004000100040004000400060004000c0004001a00100001000400000004000100040007","40400014520000060000002a0004000100040002","40400012ae00000400040004000500040006","404000124157000400000004000400040005","4040001e415400040000000400010004000200040003000400040004000b"]);
}
#[test]
fn wired_identification_compares_every_parameter_and_usb_interface_is_configurable() {
    let mut c = config();
    c.wireless = None;
    let frame = Iap2IdentificationClient::identification_information(&c, &[]).unwrap();
    let b = Iap2BodyReader::of(&frame).unwrap();
    assert_eq!(
        b.list().iter().map(|p| p.id).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 16, 30]
    );
    for (id, s) in [
        (0, "LIVI"),
        (1, "LIVI"),
        (2, "LIVI"),
        (3, "0123456"),
        (4, "1.0.0"),
        (5, "1.0"),
        (12, "en"),
    ] {
        assert_eq!(b.string(id).unwrap(), s);
    }
    assert_eq!(b.strings(13).unwrap(), ["en"]);
    assert_eq!(
        b.u16_list(6).unwrap(),
        [
            0xaa01, 0xaa03, 0x5000, 0x5002, 0x5200, 0x5203, 0xae00, 0xae02, 0x4157, 0x4159, 0x4154,
            0x4156, 0xae03, 0x4301
        ]
    );
    assert_eq!(
        b.u16_list(7).unwrap(),
        [
            0xaa00, 0xaa02, 0xaa04, 0xaa05, 0xea00, 0xea01, 0x5001, 0x5201, 0x5202, 0xae01, 0x4158,
            0x4155, 0x4300
        ]
    );
    assert_eq!(b.u8(8).unwrap(), 2);
    assert_eq!(b.u16(9).unwrap(), 20);
    let ea = b.group(10).unwrap();
    assert_eq!(ea.u8(0).unwrap(), 1);
    assert_eq!(ea.string(1).unwrap(), "com.shilapi.xcertplay");
    assert_eq!(ea.u8(2).unwrap(), 0);
    let usb = b.group(16).unwrap();
    assert_eq!(
        usb.list().iter().map(|p| p.id).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    assert_eq!(usb.u16(0).unwrap(), 0);
    assert_eq!(usb.string(1).unwrap(), "USBHostTransport");
    assert!(usb.bytes(2).unwrap().is_empty());
    assert_eq!(usb.u8(3).unwrap(), 3);
    assert!(usb.bytes(4).unwrap().is_empty());
    let route = b.group(30).unwrap();
    assert_eq!(route.u16(0).unwrap(), 42);
    assert_eq!(route.string(1).unwrap(), "RouteGuidance");
    assert_eq!(
        (
            route.u16(2).unwrap(),
            route.u16(4).unwrap(),
            route.u16(6).unwrap()
        ),
        (64, 64, 8)
    );
    c.car_play_usb_interface_number = 7;
    assert_eq!(
        Iap2BodyReader::of(&Iap2IdentificationClient::identification_information(&c, &[]).unwrap())
            .unwrap()
            .group(16)
            .unwrap()
            .u8(3)
            .unwrap(),
        7
    );
}
#[test]
fn wired_identify_authenticate_power_subscribe_answer_every_availability() {
    let p = Phone::script(&[
        frame(0x1d00, &[]),
        frame(0x1d02, &[]),
        frame(0xaa00, &[]),
        frame(0xaa02, &[(0, &[7; 32])]),
        frame(0xaa05, &[]),
        frame(0x4300, &[]),
        frame(0x4300, &[]),
        frame(0x5001, &[]),
    ]);
    let trace = Arc::new(Mutex::new(vec![]));
    let t = trace.clone();
    let session =
        Iap2Session::open_wired(p.clone(), Arc::new(move |s| t.lock().unwrap().push(s))).unwrap();
    let events = Events::default();
    let mut c = config();
    c.wireless = None;
    let result = Iap2WiredControlClient::new(session.clone(), Arc::new(Auth))
        .run(
            &c,
            &wired_endpoint(),
            500,
            Some(Duration::from_millis(150)),
            &events,
        )
        .unwrap();
    assert_eq!(result.terminal, Iap2WiredControlTerminal::TimedOut);
    assert_eq!(result.car_play_start_sessions_sent, 2);
    assert_eq!(result.forwarded_frames, 1);
    assert_eq!(*events.authenticated.lock().unwrap(), 1);
    assert_eq!(events.start.lock().unwrap().len(), 1);
    assert!(events.start.lock().unwrap()[0].sent_at_nanos > 0);
    assert_eq!(*events.incoming.lock().unwrap(), [0x5001]);
    assert_eq!(
        *events.stages.lock().unwrap(),
        [
            Iap2WiredControlStage::Identified,
            Iap2WiredControlStage::Authenticated,
            Iap2WiredControlStage::Subscribed,
            Iap2WiredControlStage::CarPlayStartSent
        ]
    );
    assert_eq!(
        p.sent_frames()
            .iter()
            .map(|f| f.message_id)
            .collect::<Vec<_>>(),
        [0x1d01, 0xaa01, 0xaa03, 0xae03, 0x5000, 0x5200, 0xae00, 0x4157, 0x4154, 0x4301, 0x4301]
    );
    assert!(trace
        .lock()
        .unwrap()
        .iter()
        .any(|s| s.starts_with("IAP2 TX [wired] 0x4301")));
    session.close().unwrap();
}
#[test]
fn failed_authentication_has_no_power_subscriptions_or_callbacks() {
    let p = Phone::script(&[frame(0x1d00, &[]), frame(0x1d02, &[]), frame(0xaa04, &[])]);
    let session = Iap2Session::open_wired(p.clone(), Arc::new(|_| {})).unwrap();
    let events = Events::default();
    let mut c = config();
    c.wireless = None;
    assert!(Iap2WiredControlClient::new(session.clone(), Arc::new(Auth))
        .run(
            &c,
            &wired_endpoint(),
            500,
            Some(Duration::from_millis(150)),
            &events
        )
        .is_err());
    assert!(p.sent_frames().iter().all(|f| f.message_id == 0x1d01));
    assert!(events.start.lock().unwrap().is_empty());
    assert_eq!(*events.authenticated.lock().unwrap(), 0);
    session.close().unwrap();
}
#[test]
fn wired_endpoint_rejects_invalid_literals_and_formats_wired_frame() {
    let mut e = wired_endpoint();
    for address in ["host.invalid", "192.168.1.1", "fe80::2%3", ""] {
        e.ipv6_addresses = vec![address.into()];
        assert!(e.validate().is_err());
    }
    let f = Iap2CarPlayMessages::start_wired_session(&wired_endpoint()).unwrap();
    let formatted = Iap2FrameFormatter::format_with_context(Iap2TraceDirection::Tx, "wired", &f);
    assert!(formatted.starts_with("IAP2 TX [wired] 0x4301 CarPlayStartSession"));
    assert!(formatted.contains("wiredIP: \"fe80::2\""));
    assert!(formatted.contains("airPlayPort: u32=7000"));
    assert_eq!(
        Iap2WiredControlClient::car_play_availability_summary(&[]),
        "iap2 4300 wiredAvailable=null usbTransport=none"
    );
}
#[test]
fn wired_control_deadline_full_day_handshake_and_finite_vectors() {
    let start = Instant::now();
    let mut d = Iap2ControlDeadline::new(Duration::MAX, start);
    assert_eq!(d.remaining_millis(start + Duration::from_millis(59999)), 1);
    assert_eq!(d.remaining_millis(start + Duration::from_secs(60)), 0);
    d.authenticated();
    for minutes in [6, 90, 25 * 60] {
        assert_eq!(
            d.remaining_millis(start + Duration::from_secs(minutes * 60)),
            30000
        );
    }
    let mut finite = Iap2ControlDeadline::new(Duration::from_millis(100), start);
    finite.authenticated();
    assert_eq!(
        finite.remaining_millis(start + Duration::from_millis(100)),
        0
    );
}
