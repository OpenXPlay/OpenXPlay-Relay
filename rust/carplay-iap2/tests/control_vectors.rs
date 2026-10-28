// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
mod common;
use carplay_iap2::transport::{
    iap2_identification_client::identification_rejection,
    iap2_wireless_control_client::{car_play_availability_diagnostic, WirelessControlDeadline},
};
use carplay_iap2::*;
use common::*;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[test]
fn rejection_retains_protocol_fixture_unsupported_message_ids() {
    let r = identification_rejection(&Iap2Frame {
        message_id: 0x1d03,
        payload: unhex("0004001e00060006415a00040014"),
    })
    .unwrap();
    assert_eq!(r.parameter_ids, [6, 20, 30]);
    assert_eq!(
        r.unsupported_messages_sent.as_ref().unwrap().message_ids,
        [0x415a]
    );
    assert!(r.unsupported_messages_received.is_none());
    assert!(r.diagnostic().contains("before MFi authentication"));
    assert!(r.diagnostic().contains("sent=[0x415a]"));
}
#[test]
fn rejection_ids_are_unsigned_big_endian_and_separate() {
    let r = identification_rejection(&frame(
        0x1d03,
        &[(6, &[0x50, 1]), (7, &[0xaa, 0, 255, 255])],
    ))
    .unwrap();
    assert_eq!(r.unsupported_messages_sent.unwrap().message_ids, [0x5001]);
    assert_eq!(
        r.unsupported_messages_received.unwrap().message_ids,
        [0xaa00, 0xffff]
    );
}
#[test]
fn empty_and_odd_payloads_do_not_invent_ids() {
    let r =
        identification_rejection(&frame(0x1d03, &[(6, &[]), (7, &[0x41, 0x5a, 0x50])])).unwrap();
    let sent = r.unsupported_messages_sent.unwrap();
    let recv = r.unsupported_messages_received.unwrap();
    assert!(sent.message_ids.is_empty());
    assert_eq!(sent.empty_payload_count, 1);
    assert_eq!(sent.malformed_payload_count, 0);
    assert_eq!(recv.empty_payload_count, 0);
    assert_eq!(recv.malformed_payload_count, 1);
}
#[test]
fn duplicate_parameters_and_ids_are_visible_in_wire_order() {
    let r = identification_rejection(&frame(
        0x1d03,
        &[
            (6, &[0x50, 0, 0x50, 0]),
            (6, &[]),
            (6, &[0x52, 0]),
            (7, &[0]),
            (7, &[0x4e, 0x0d]),
        ],
    ))
    .unwrap();
    let sent = r.unsupported_messages_sent.unwrap();
    assert_eq!(sent.message_ids, [0x5000, 0x5000, 0x5200]);
    assert_eq!(sent.payload_count, 3);
    assert!(sent
        .diagnostic()
        .contains("duplicateParameters=true duplicateRetainedIds=true"));
}
#[test]
fn rejected_message_diagnostics_are_bounded_per_direction() {
    let values = (0u16..40).flat_map(u16::to_be_bytes).collect::<Vec<_>>();
    let r = identification_rejection(&frame(
        0x1d03,
        &[(6, &values[..40]), (6, &values[40..]), (7, &values)],
    ))
    .unwrap();
    for list in [r.unsupported_messages_sent, r.unsupported_messages_received] {
        let l = list.unwrap();
        assert_eq!(l.message_ids, (0u16..32).collect::<Vec<_>>());
        assert_eq!(l.omitted_message_count, 8);
        assert!(!l.diagnostic().contains("0x0020"));
    }
}
#[test]
fn rejection_does_not_export_other_payloads() {
    let r = identification_rejection(&frame(0x1d03, &[(1, b"private phone name")])).unwrap();
    assert!(!r.diagnostic().contains("private phone name"));
}
#[test]
fn only_message_list_rejections_can_be_corrected() {
    let older = frame(0x1d03, &[(6, &[0x43, 1]), (7, &[0x43, 0])]);
    assert_eq!(
        identification_rejection(&older)
            .unwrap()
            .omittable_messages(),
        Some(vec![0x4300, 0x4301])
    );
    for f in [
        frame(0x1d03, &[(16, &[])]),
        frame(0x1d03, &[(6, &[0x43, 1]), (16, &[])]),
        frame(0x1d03, &[(6, &[0xaa, 1])]),
        frame(0x1d03, &[(7, &[0x43, 0, 0x50])]),
        frame(0x1d03, &[(6, &[])]),
    ] {
        assert!(identification_rejection(&f)
            .unwrap()
            .omittable_messages()
            .is_none());
    }
}
#[test]
fn omitted_messages_leave_both_lists_in_every_transport() {
    for wireless in [false, true] {
        let mut c = config();
        if !wireless {
            c.wireless = None;
        }
        let b = Iap2BodyReader::of(
            &Iap2IdentificationClient::identification_information(&c, &[0x4300, 0x4301]).unwrap(),
        )
        .unwrap();
        assert!(!b.u16_list(6).unwrap().contains(&0x4301));
        assert!(!b.u16_list(7).unwrap().contains(&0x4300));
        assert!(b.u16_list(6).unwrap().contains(&0xaa03));
        assert!(b.u16_list(7).unwrap().contains(&0xaa05));
    }
}
fn identify_script(replies: &[Iap2Frame]) -> (Result<()>, Vec<Iap2Frame>) {
    let p = Phone::script(replies);
    let s = p.session();
    let r =
        Iap2IdentificationClient::new(s.clone()).identify(&config(), Duration::from_millis(500));
    std::thread::sleep(Duration::from_millis(120));
    let frames = p.sent_frames();
    s.close().unwrap();
    (r, frames)
}
#[test]
fn older_ios_identified_on_second_attempt() {
    let (r, s) = identify_script(&[
        frame(0x1d00, &[]),
        frame(0x1d03, &[(6, &[0x43, 1]), (7, &[0x43, 0])]),
        frame(0x1d02, &[]),
    ]);
    r.unwrap();
    assert_eq!(s.len(), 2);
    assert!(Iap2BodyReader::of(&s[0])
        .unwrap()
        .u16_list(6)
        .unwrap()
        .contains(&0x4301));
    assert!(!Iap2BodyReader::of(&s[1])
        .unwrap()
        .u16_list(6)
        .unwrap()
        .contains(&0x4301));
}
#[test]
fn second_rejection_fails() {
    let rejected = frame(0x1d03, &[(6, &[0x43, 1])]);
    let (r, s) = identify_script(&[frame(0x1d00, &[]), rejected.clone(), rejected]);
    assert!(r.is_err());
    assert_eq!(s.len(), 2);
}
#[test]
fn uncorrectable_rejection_fails_without_resending() {
    let (r, s) = identify_script(&[frame(0x1d00, &[]), frame(0x1d03, &[(16, &[])])]);
    assert!(r.is_err());
    assert_eq!(s.len(), 1);
}
#[test]
fn restarted_identification_keeps_correction() {
    let (r, s) = identify_script(&[
        frame(0x1d00, &[]),
        frame(0x1d03, &[(6, &[0x43, 1])]),
        frame(0x1d00, &[]),
        frame(0x1d02, &[]),
    ]);
    r.unwrap();
    assert_eq!(s.len(), 3);
    assert!(!Iap2BodyReader::of(&s[2])
        .unwrap()
        .u16_list(6)
        .unwrap()
        .contains(&0x4301));
}
#[test]
fn wireless_identity_advertises_components_without_usb() {
    let b = Iap2BodyReader::of(
        &Iap2IdentificationClient::identification_information(&config(), &[]).unwrap(),
    )
    .unwrap();
    assert!(!b.has(16));
    assert_eq!(b.u8(8).unwrap(), 0);
    assert_eq!(b.group(17).unwrap().string(1).unwrap(), "blue");
    assert_eq!(
        b.group(17).unwrap().bytes(3).unwrap(),
        [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]
    );
    assert_eq!(b.group(24).unwrap().string(1).unwrap(), "LIVI");
    assert!(b.u16_list(6).unwrap().contains(&0x5703));
    assert!(!b.u16_list(6).unwrap().contains(&0xae03));
}
#[test]
fn wireless_roles_omit_excluded_vehicle_location_and_route() {
    for role in [
        Iap2WirelessLinkRole::BluetoothBootstrap,
        Iap2WirelessLinkRole::RuntimeTunnel,
    ] {
        let c = config();
        let c = c.for_wireless_link(role, c.wireless.clone().unwrap());
        let b = Iap2BodyReader::of(
            &Iap2IdentificationClient::identification_information(&c, &[]).unwrap(),
        )
        .unwrap();
        for id in [20, 21, 22, 30] {
            assert!(!b.has(id));
        }
        for id in [0xa101, 0xfffb, 0x5200] {
            assert!(!b.u16_list(6).unwrap().contains(&id));
        }
    }
}
#[test]
fn availability_diagnostic_does_not_export_identifiers() {
    let inner = frame(0, &[(0, &[1]), (1, b"Jane's iPhone\0")]).payload;
    let f = frame(0x4300, &[(1, &inner)]);
    assert_eq!(
        car_play_availability_diagnostic(&f),
        "iap2 availability wired=unknown wireless=true themeAssets=unknown"
    );
    assert!(car_play_availability_diagnostic(&Iap2Frame {
        message_id: 0x4300,
        payload: vec![0]
    })
    .contains("decode=failed"));
}
#[test]
fn wireless_deadline_extends_only_while_live() {
    let now = Instant::now();
    let mut d = WirelessControlDeadline::new(Some(Duration::from_secs(1)), now);
    assert_eq!(d.remaining(now, false), Duration::from_secs(1));
    assert_eq!(
        d.remaining(now + Duration::from_secs(1), true),
        Duration::from_secs(300)
    );
    assert_eq!(
        d.remaining(now + Duration::from_secs(2), false),
        Duration::ZERO
    );
}
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
    calls: Mutex<Vec<String>>,
    stages: Mutex<Vec<Iap2WirelessControlStage>>,
    wifi_failure: bool,
    start_failure: bool,
}
impl WirelessControlEvents for Events {
    fn on_stage_changed(&self, stage: Iap2WirelessControlStage) {
        self.stages.lock().unwrap().push(stage);
    }
    fn before_wifi_configuration(&self) -> Result<()> {
        self.calls.lock().unwrap().push("wifi-before".into());
        if self.wifi_failure {
            return Err(std::io::Error::other(
                "network changed before Wi-Fi details",
            ));
        }
        Ok(())
    }
    fn before_start_session(&self) -> Result<()> {
        self.calls.lock().unwrap().push("before".into());
        if self.start_failure {
            return Err(std::io::Error::other("network changed before StartSession"));
        }
        Ok(())
    }
    fn on_start_session_sent(&self, e: Iap2StartSessionSent) {
        assert!(e.sent_at_nanos > 0);
        self.calls.lock().unwrap().push("sent".into());
    }
    fn on_ready(&self) {
        self.calls.lock().unwrap().push("ready".into());
    }
    fn on_progress(&self, m: &str) {
        self.calls.lock().unwrap().push(m.into());
    }
    fn keep_alive(&self) -> bool {
        false
    }
    fn on_wifi_configuration_sent(&self) {
        self.calls.lock().unwrap().push("wifi-sent".into());
    }
}
/// The accessory's network as the bootstrap sees it while its access point starts.
#[derive(Default)]
struct Network {
    ready: Mutex<bool>,
    failed: Mutex<Option<String>>,
}
impl WirelessEndpointSource for Network {
    fn endpoint(&self) -> Result<Option<Iap2WirelessCarPlayEndpoint>> {
        if let Some(e) = self.failed.lock().unwrap().clone() {
            return Err(std::io::Error::other(e));
        }
        Ok(self.ready.lock().unwrap().then(endpoint))
    }
}
fn authenticated_then(requests: &[Iap2Frame]) -> Arc<Phone> {
    let mut script = vec![
        frame(0x1d00, &[]),
        frame(0x1d02, &[]),
        frame(0xaa00, &[]),
        frame(0xaa02, &[(0, &[7; 32])]),
        frame(0xaa05, &[]),
    ];
    script.extend_from_slice(requests);
    Phone::script(&script)
}
fn wait_until(mut test: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !test() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn ids(p: &Phone) -> Vec<u16> {
    p.sent_frames().iter().map(|f| f.message_id).collect()
}
#[test]
fn answers_with_the_network_wait_for_it_after_authentication() {
    // The iPhone asks for the Wi-Fi configuration right after authentication and then reports
    // CarPlay availability, while the access point is still starting.
    let p = authenticated_then(&[
        frame(0x5702, &[]),
        Iap2Frame {
            message_id: 0x4300,
            payload: vec![0],
        },
        frame(0x4e0d, &[(0, &[1])]),
    ]);
    let session = p.session();
    let network = Arc::new(Network::default());
    let events = Arc::new(Events::default());
    let run = {
        let (session, network, events) = (session.clone(), network.clone(), events.clone());
        std::thread::spawn(move || {
            Iap2WirelessControlClient::new(session, Arc::new(Auth)).run_deferred(
                &config(),
                network.as_ref(),
                Some(Duration::from_millis(600)),
                events.as_ref(),
            )
        })
    };
    // Identification, authentication and the subscriptions go out without the network, and
    // later requests are still read; the answers that carry the network wait for it.
    wait_until(|| {
        let calls = events.calls.lock().unwrap();
        calls.iter().any(|c| c == "ready")
            && calls
                .iter()
                .any(|c| c == "iap2 rx=0x4e0d wireless-carplay-update available=true")
    });
    std::thread::sleep(Duration::from_millis(100));
    {
        let calls = events.calls.lock().unwrap();
        assert!(calls
            .iter()
            .any(|c| c == "iap2 answer waits for the accessory network"));
        assert!(!calls
            .iter()
            .any(|c| c.starts_with("iap2 tx=") || c == "wifi-sent" || c == "before"));
    }
    // The access point is up: the answers go out in request order.
    *network.ready.lock().unwrap() = true;
    let result = run.join().unwrap().unwrap();
    assert_eq!(result.wifi_configurations_sent, 1);
    assert_eq!(result.car_play_start_sessions_sent, 1);
    assert_eq!(result.stage, Iap2WirelessControlStage::CarplayStartSent);
    assert_eq!(
        ids(&p),
        [0x1d01, 0xaa01, 0xaa03, 0x5000, 0xae00, 0x4157, 0x4154, 0x5703, 0x4301],
        "Wi-Fi configuration before StartSession"
    );
    let calls = events.calls.lock().unwrap();
    assert_eq!(calls.iter().filter(|s| *s == "wifi-before").count(), 1);
    for (index, call) in calls.iter().enumerate() {
        if call == "wifi-before" {
            assert!(calls[index + 1].starts_with("iap2 tx=0x5703"));
            assert_eq!(calls[index + 2], "wifi-sent");
        }
    }
    assert_eq!(
        *events.stages.lock().unwrap(),
        [
            Iap2WirelessControlStage::Identified,
            Iap2WirelessControlStage::Authenticated,
            Iap2WirelessControlStage::Subscribed,
            Iap2WirelessControlStage::WifiConfigSent,
            Iap2WirelessControlStage::CarplayStartSent,
        ]
    );
    let position = |name: &str| calls.iter().position(|c| c.starts_with(name)).unwrap();
    assert!(position("iap2 accessory network ready afterMs=") < position("wifi-sent"));
    assert!(position("wifi-sent") < position("before"));
    assert_eq!(calls.iter().filter(|c| *c == "wifi-sent").count(), 1);
    drop(calls);
    session.close().unwrap();
}
#[test]
fn network_validation_failure_prevents_wifi_and_start_session_writes() {
    for request in [0x5702, 0x4e0e, 0x4300] {
        let phone = authenticated_then(&[frame(request, &[])]);
        let session = phone.session();
        let events = Events {
            wifi_failure: request != 0x4300,
            start_failure: request == 0x4300,
            ..Default::default()
        };
        let error = Iap2WirelessControlClient::new(session.clone(), Arc::new(Auth))
            .run(
                &config(),
                &endpoint(),
                Some(Duration::from_millis(500)),
                &events,
            )
            .unwrap_err();
        assert!(error.to_string().starts_with("network changed before"));
        let sent = ids(&phone);
        assert!(!sent.contains(&0x5703));
        assert!(!sent.contains(&0x4301));
        let calls = events.calls.lock().unwrap();
        assert!(!calls.iter().any(|s| s == "wifi-sent" || s == "sent"));
        session.close().unwrap();
    }
}
#[test]
fn the_wait_for_the_network_is_bounded() {
    let p = authenticated_then(&[frame(0x5702, &[])]);
    let session = p.session();
    let started = Instant::now();
    let error = Iap2WirelessControlClient::new(session.clone(), Arc::new(Auth))
        .with_endpoint_wait(Duration::from_millis(100))
        .run_deferred(
            &config(),
            &Network::default(),
            Some(Duration::from_secs(5)),
            &Events::default(),
        )
        .unwrap_err();
    assert!(error.to_string().starts_with("Accessory network not ready"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(!ids(&p).contains(&0x5703));
    session.close().unwrap();
}
#[test]
fn a_failed_network_ends_the_bring_up() {
    let p = authenticated_then(&[frame(0x5702, &[]), frame(0x5702, &[])]);
    let session = p.session();
    let network = Network::default();
    *network.failed.lock().unwrap() = Some("hotspot failed".into());
    let error = Iap2WirelessControlClient::new(session.clone(), Arc::new(Auth))
        .run_deferred(
            &config(),
            &network,
            Some(Duration::from_secs(5)),
            &Events::default(),
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "hotspot failed");
    assert!(!ids(&p).contains(&0x5703));
    session.close().unwrap();
}
#[test]
fn wireless_flow_identifies_authenticates_then_bounds_wifi_responses() {
    let mut script = vec![
        frame(0x1d00, &[]),
        frame(0x1d02, &[]),
        frame(0xaa00, &[]),
        frame(0xaa02, &[(0, &[7; 32])]),
        frame(0xaa05, &[]),
    ];
    for _ in 0..7 {
        script.push(frame(0x5702, &[]));
    }
    script.extend([
        Iap2Frame {
            message_id: 0x4300,
            payload: vec![0],
        },
        frame(0x4e0d, &[(0, &[1])]),
    ]);
    for _ in 0..4 {
        script.push(frame(0x4e0e, &[]));
    }
    let p = Phone::script(&script);
    let session = p.session();
    let events = Events::default();
    let result = Iap2WirelessControlClient::new(session.clone(), Arc::new(Auth))
        .run(
            &config(),
            &endpoint(),
            Some(Duration::from_millis(500)),
            &events,
        )
        .unwrap();
    assert_eq!(result.wifi_configurations_sent, 7);
    assert_eq!(result.post_transport_wifi_configurations_sent, 2);
    assert_eq!(result.car_play_start_sessions_sent, 1);
    assert!(result.transport_notification_seen && result.wireless_car_play_available_seen);
    assert_eq!(
        result.stage,
        Iap2WirelessControlStage::PostTransportWifiConfigSent
    );
    let sent = p.sent_frames();
    assert_eq!(sent[0].message_id, 0x1d01);
    assert_eq!(sent[1].message_id, 0xaa01);
    assert_eq!(sent[2].message_id, 0xaa03);
    let subscription_bytes: Vec<_> = sent[3..7]
        .iter()
        .map(|f| hex(&f.encoded_frame().unwrap()))
        .collect();
    assert_eq!(subscription_bytes, [
        "4040002e5000001800000004000100040004000400060004000c0004001a00100001000400000004000100040007",
        "40400012ae00000400040004000500040006",
        "404000124157000400000004000400040005",
        "4040001e415400040000000400010004000200040003000400040004000b",
    ]);
    assert_eq!(sent.iter().filter(|f| f.message_id == 0x5703).count(), 7);
    let calls = events.calls.lock().unwrap();
    assert_eq!(calls.iter().filter(|s| *s == "wifi-before").count(), 7);
    assert_eq!(
        *events.stages.lock().unwrap(),
        [
            Iap2WirelessControlStage::Identified,
            Iap2WirelessControlStage::Authenticated,
            Iap2WirelessControlStage::Subscribed,
            Iap2WirelessControlStage::WifiConfigSent,
            Iap2WirelessControlStage::CarplayStartSent,
            Iap2WirelessControlStage::TransportNotified,
            Iap2WirelessControlStage::PostTransportWifiConfigSent,
        ]
    );
    let before = calls.iter().position(|s| s == "before").unwrap();
    assert_eq!(calls[before + 1], "sent");
    drop(calls);
    session.close().unwrap();
}
#[test]
fn authentication_rejects_unexpected_or_failed_messages() {
    for message in [0xaa04, 0x5001] {
        let p = Phone::script(&[frame(message, &[])]);
        let s = p.session();
        s.await_ready(Duration::from_secs(1)).unwrap();
        assert!(Iap2MfiAuthenticationClient::new(Arc::new(Auth))
            .run(&s, Duration::from_millis(300))
            .is_err());
        s.close().unwrap();
    }
}
#[test]
fn closed_channels_return_terminal_errors() {
    let p = Phone::script(&[]);
    let s = p.session();
    s.close().unwrap();
    assert!(s.recv(Duration::ZERO).is_err());
    assert!(s.await_ready(Duration::ZERO).is_err());
}

#[test]
fn unlimited_wireless_tunnel_uses_bounded_polls() {
    let now = Instant::now();
    let mut d = WirelessControlDeadline::new(None, now);
    assert_eq!(d.remaining(now, false), Duration::from_secs(300));
    assert_eq!(
        d.remaining(now + Duration::from_secs(86400), false),
        Duration::from_secs(300)
    );
}
#[test]
fn wireless_deadline_checks_liveness_only_after_expiry() {
    let now = Instant::now();
    let mut d = WirelessControlDeadline::new(Some(Duration::from_secs(1)), now);
    assert_eq!(
        d.remaining_with(now, || panic!(
            "must not inspect live session during bring-up"
        )),
        Duration::from_secs(1)
    );
    assert_eq!(
        d.remaining_with(now + Duration::from_secs(1), || false),
        Duration::ZERO
    );
}
#[test]
fn authenticated_wireless_control_survives_a_full_day_while_live() {
    let now = Instant::now();
    let mut d = WirelessControlDeadline::new(Some(Duration::from_secs(300)), now);
    assert_eq!(
        d.remaining(now + Duration::from_secs(300), true),
        Duration::from_secs(300)
    );
    assert_eq!(
        d.remaining(now + Duration::from_secs(86400), true),
        Duration::from_secs(300)
    );
    assert_eq!(
        d.remaining(now + Duration::from_secs(86401), false),
        Duration::ZERO
    );
}

#[test]
fn start_session_event_requires_successful_send_and_follows_write() {
    use carplay_iap2::transport::iap2_wireless_control_client::send_start_session;
    let calls = std::cell::RefCell::new(Vec::new());
    send_start_session(
        &endpoint(),
        |frame| {
            assert_eq!(frame.message_id, 0x4301);
            calls.borrow_mut().push("write");
            Ok(())
        },
        |event| {
            assert!(event.sent_at_nanos > 0);
            calls.borrow_mut().push("event");
        },
    )
    .unwrap();
    assert_eq!(*calls.borrow(), vec!["write", "event"]);
    assert!(send_start_session(
        &endpoint(),
        |_| Err(std::io::Error::other("write failed")),
        |_| panic!("failed write must not emit event")
    )
    .is_err());
}

#[test]
fn default_transport_identification_golden() {
    let expected = [
        "404001181d01000900004c49564900000900014c49564900000900024c49564900000c00033031323334353600000a0004312e302e300000080005312e300000200006aa01aa035000500252005203ae00ae024157415941544156ae034301001e0007aa00aa02aa04aa05ea00ea01500152015202ae0141584155430000050008020006000900140028000a0005000001001a0001636f6d2e7368696c6170692e7863657274706c61790000050002000007000c656e000007000d656e00002c001000060000000000150001555342486f73745472616e73706f72740000040002000500030300040004002e001e00060000002a00120001526f75746547756964616e636500000600020040000600040040000600060008",
        "4040010f1d01000900004c49564900000900014c49564900000900024c49564900000c00033031323334353600000a0004312e302e300000080005312e3000001c0006aa01aa0350005002ae00ae0241574159415441564301570300200007aa00aa02aa04aa05ea00ea015001ae014158415543004e0d4e0e570200050008000006000900140028000a0005000001001a0001636f6d2e7368696c6170692e7863657274706c61790000050002000007000c656e000007000d656e00002e001100060000000000090001626c75650000040002000a0003aabbccddeeff00090004626c7565000004000500250018000600000001000900014c49564900000400020006000300010004000400040005",
    ];
    for (wireless, expected) in [false, true].into_iter().zip(expected) {
        let mut c = config();
        if !wireless {
            c.wireless = None;
        }
        assert!(!c.route_guidance);
        assert_eq!(
            hex(
                &Iap2IdentificationClient::identification_information(&c, &[])
                    .unwrap()
                    .encoded_frame()
                    .unwrap()
            ),
            expected
        );
    }
}
#[test]
fn route_opt_in_matches_diplay_identification_and_subscription() {
    for wireless in [false, true] {
        let mut c = config();
        c.route_guidance = true;
        if !wireless {
            c.wireless = None;
        }
        let b = Iap2BodyReader::of(
            &Iap2IdentificationClient::identification_information(&c, &[]).unwrap(),
        )
        .unwrap();
        let g = b.group(30).unwrap();
        assert_eq!(g.u16(0).unwrap(), 42);
        assert_eq!(g.string(1).unwrap(), "RouteGuidance");
        for (id, value) in [(2, 64), (4, 64), (6, 8)] {
            assert_eq!(g.u16(id).unwrap(), value);
        }
        assert_eq!(&b.u16_list(6).unwrap()[4..6], &[0x5200, 0x5203]);
        assert_eq!(&b.u16_list(7).unwrap()[7..9], &[0x5201, 0x5202]);
        let omitted = Iap2BodyReader::of(
            &Iap2IdentificationClient::identification_information(&c, &[0x5200, 0x5201]).unwrap(),
        )
        .unwrap();
        assert!(!omitted.u16_list(6).unwrap().contains(&0x5200));
        assert!(!omitted.u16_list(7).unwrap().contains(&0x5201));
    }
    let original = Iap2ControlMessages::subscriptions().unwrap();
    assert_eq!(
        Iap2ControlMessages::subscriptions_with_route_guidance(false).unwrap(),
        original
    );
    let mut enabled = Iap2ControlMessages::subscriptions_with_route_guidance(true).unwrap();
    assert_eq!(
        hex(&enabled.remove(1).encoded_frame().unwrap()),
        "40400014520000060000002a0004000100040002"
    );
    assert_eq!(enabled, original);
}

#[test]
fn route_subscription_follows_successful_authentication_only() {
    for success in [false, true] {
        let p = Phone::script(&[
            frame(0x1d00, &[]),
            frame(0x1d02, &[]),
            frame(0xaa00, &[]),
            frame(0xaa02, &[(0, &[7; 32])]),
            frame(if success { 0xaa05 } else { 0xaa04 }, &[]),
        ]);
        let session = p.session();
        let mut c = config();
        c.route_guidance = true;
        let result = Iap2WirelessControlClient::new(session.clone(), Arc::new(Auth)).run(
            &c,
            &endpoint(),
            Some(Duration::from_millis(500)),
            &Events::default(),
        );
        assert_eq!(result.is_ok(), success);
        let sent = p.sent_frames();
        assert_eq!(
            sent.iter().filter(|f| f.message_id == 0x5200).count(),
            usize::from(success)
        );
        if success {
            assert_eq!(
                sent.iter().map(|f| f.message_id).collect::<Vec<_>>(),
                [0x1d01, 0xaa01, 0xaa03, 0x5000, 0x5200, 0xae00, 0x4157, 0x4154]
            );
        }
        session.close().unwrap();
    }
}
