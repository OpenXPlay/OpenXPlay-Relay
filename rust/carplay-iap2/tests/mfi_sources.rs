// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-27.
// Created by riko chen on 2026-10-27

use base64ct::{Base64, Encoding};
use carplay_iap2::{
    mfi::{
        ch341::{self, Ch341, Usb},
        coprocessor::{Clock, Coprocessor, I2c},
        remote::{self, Http, RemoteAuthenticator},
    },
    MfiAuthenticator, MfiCertificateType, Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Default)]
struct FakeHttp {
    responses: Mutex<VecDeque<Result<remote::Response>>>,
    requests: Mutex<Vec<remote::Request>>,
}
impl Http for FakeHttp {
    fn send(&self, r: remote::Request) -> Result<remote::Response> {
        self.requests.lock().unwrap().push(r);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unexpected request")
    }
}
fn response(status: u16, body: Value) -> Result<remote::Response> {
    Ok(remote::Response {
        status,
        body: serde_json::to_vec(&body).unwrap(),
    })
}
fn certificate(bytes: &[u8], kind: &str) -> Value {
    json!({"protocolMajor":3,"type":kind,"certificate":Base64::encode_string(bytes),"certificateSha256":Sha256::digest(bytes).iter().map(|b|format!("{b:02x}")).collect::<String>()})
}
fn remote(
    responses: Vec<Result<remote::Response>>,
) -> (Result<RemoteAuthenticator>, Arc<FakeHttp>) {
    let http = Arc::new(FakeHttp {
        responses: Mutex::new(responses.into()),
        ..Default::default()
    });
    (
        RemoteAuthenticator::connect(
            " https://mfi.example.org/base/ ",
            "private-token".into(),
            http.clone(),
        ),
        http,
    )
}
#[test]
fn remote_resets_caches_checks_digest_and_signs_with_stable_retry_id() {
    let (client, http) = remote(vec![
        response(200, json!({})),
        response(503, json!({})),
        response(200, certificate(&[1, 2, 3], "mfi")),
        response(429, json!({})),
        response(200, json!({"signature":"BAU="})),
    ]);
    let client = client.unwrap();
    assert_eq!(client.protocol_major().unwrap(), 3);
    assert_eq!(client.read_certificate(3).unwrap(), [1, 2, 3]);
    assert_eq!(client.read_certificate(100).unwrap(), [1, 2, 3]);
    assert_eq!(client.sign_challenge(&[9]).unwrap(), [4, 5]);
    assert!(client.sign_challenge(&[]).is_err());
    assert!(client.sign_challenge(&[0; 129]).is_err());
    assert!(client.read_certificate(2).is_err());
    assert!(client.baa_certificates().is_err());
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].url, "https://mfi.example.org/base/mfi/reset");
    assert_eq!(requests[0].method, "POST");
    for request in requests.iter() {
        assert!(request
            .headers
            .contains(&("Authorization", "Bearer private-token".into())));
    }
    assert_eq!(requests[3].body, requests[4].body);
    let sign: Value = serde_json::from_slice(requests[3].body.as_ref().unwrap()).unwrap();
    assert_eq!(sign["challenge"], "CQ==");
    assert_eq!(sign["requestId"].as_str().unwrap().len(), 36);
}
#[test]
fn remote_baa_uses_the_iap2_package_and_rejects_invalid_lengths() {
    let package = [0, 0, 0, 2, 0, 0, 0, 1, 1, 2, 3];
    let (client, _) = remote(vec![
        response(200, json!({})),
        response(200, certificate(&package, "BAA")),
    ]);
    let client = client.unwrap();
    assert_eq!(client.certificate_type(), MfiCertificateType::Baa);
    let pair = client.baa_certificates().unwrap();
    assert_eq!(pair.leaf, [1, 2]);
    assert_eq!(pair.intermediate, [3]);
    assert_eq!(
        client.read_certificate(100).unwrap(),
        carplay_iap2::Iap2AuthenticationMessages::baa_certificate_package(&[1, 2], &[3])
            .unwrap()
            .payload
    );
    for package in [&package[..10], &[255; 8][..], &[0; 8][..]] {
        assert!(remote(vec![
            response(200, json!({})),
            response(200, certificate(package, "baa"))
        ])
        .0
        .is_err());
    }
}
#[test]
fn remote_rejects_digest_major_type_base64_and_oversized_http() {
    for key in ["certificateSha256", "protocolMajor", "type", "certificate"] {
        let mut value = certificate(&[1], "mfi");
        value[key] = json!("invalid");
        assert!(remote(vec![response(200, json!({})), response(200, value)])
            .0
            .is_err());
    }
    assert!(remote(vec![
        response(200, json!({})),
        Ok(remote::Response {
            status: 200,
            body: vec![0; remote::MAX_HTTP_BODY + 1]
        })
    ])
    .0
    .is_err());
}
#[test]
fn remote_reset_never_retries_and_neither_http_nor_transport_errors_expose_secrets() {
    let (client, http) = remote(vec![response(500, json!({"detail":"private-token"}))]);
    assert!(!client.err().unwrap().to_string().contains("private-token"));
    assert_eq!(http.requests.lock().unwrap().len(), 1);
    let (client, http) = remote(vec![
        response(200, json!({})),
        response(401, json!({"detail":"private-token"})),
    ]);
    assert!(!client.err().unwrap().to_string().contains("private-token"));
    assert_eq!(http.requests.lock().unwrap().len(), 2);
    let (client, http) = remote(vec![
        response(200, json!({})),
        Err(std::io::Error::other("private-token")),
        Err(std::io::Error::other("private-token")),
    ]);
    assert!(!client.err().unwrap().to_string().contains("private-token"));
    assert_eq!(http.requests.lock().unwrap().len(), 3);
    assert!(remote(vec![response(302, json!({}))]).0.is_err());
}
#[derive(Default)]
struct FakeClock(Mutex<Duration>);
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        *self.0.lock().unwrap()
    }
    fn sleep(&self, d: Duration) {
        *self.0.lock().unwrap() += d;
    }
}
struct Step(u8, Vec<u8>, usize, Result<Vec<u8>>);
struct Bus(Mutex<VecDeque<Step>>);
impl I2c for Bus {
    fn transaction(&self, address: u8, write: &[u8], read: usize) -> Result<Vec<u8>> {
        let Step(a, w, n, result) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("Unexpected transaction");
        assert_eq!((address, write, read), (a, w.as_slice(), n));
        result
    }
}
fn select(register: u8) -> Step {
    Step(0x11, vec![register], 0, Ok(vec![]))
}
fn read(value: Vec<u8>) -> Step {
    Step(0x11, vec![], value.len(), Ok(value))
}
fn bus(mut steps: Vec<Step>) -> (Coprocessor, Arc<Bus>, Arc<FakeClock>) {
    let mut all = vec![
        Step(0x10, vec![0], 0, Err(std::io::Error::other("NACK"))),
        select(0),
        read(vec![7]),
    ];
    all.append(&mut steps);
    let bus = Arc::new(Bus(Mutex::new(all.into())));
    let clock = Arc::new(FakeClock::default());
    let client =
        Coprocessor::scan_with_clock(bus.clone(), clock.clone(), Duration::from_secs(15)).unwrap();
    (client, bus, clock)
}
#[test]
fn coprocessor_probes_in_order_and_reads_incrementing_128_byte_certificate_windows() {
    let (client, bus, _) = bus(vec![
        select(2),
        read(vec![9]),
        select(0x30),
        read(vec![1, 4]),
        select(0x31),
        read(vec![1; 128]),
        select(0x32),
        read(vec![2; 128]),
        select(0x33),
        read(vec![3; 4]),
    ]);
    assert_eq!(client.protocol_major().unwrap(), 9);
    assert_eq!(
        client.read_certificate(65525).unwrap(),
        [vec![1; 128], vec![2; 128], vec![3; 4]].concat()
    );
    assert!(bus.0.lock().unwrap().is_empty());
}
#[test]
fn coprocessor_sign_sequence_retries_the_complete_select_read_pair_and_polls() {
    let (client, bus, clock) = bus(vec![
        Step(0x11, vec![0x20, 0, 2], 0, Ok(vec![])),
        Step(0x11, vec![0x21, 9, 8], 0, Ok(vec![])),
        Step(0x11, vec![0x10, 1], 0, Ok(vec![])),
        select(0x10),
        Step(0x11, vec![], 1, Err(std::io::Error::other("asleep"))),
        select(0x10),
        read(vec![0]),
        select(0x10),
        read(vec![0x10]),
        select(0x11),
        read(vec![0, 3]),
        select(0x12),
        read(vec![4, 5, 6]),
    ]);
    assert_eq!(client.sign_challenge(&[9, 8]).unwrap(), [4, 5, 6]);
    assert_eq!(clock.now(), Duration::from_millis(40));
    assert!(client.sign_challenge(&[]).is_err());
    assert!(client.sign_challenge(&[0; 129]).is_err());
    assert!(bus.0.lock().unwrap().is_empty());
}
struct Absent;
impl I2c for Absent {
    fn transaction(&self, _: u8, _: &[u8], read: usize) -> Result<Vec<u8>> {
        Ok(vec![255; read])
    }
}
#[test]
fn coprocessor_scan_rejects_floating_bus_and_obeys_probe_deadline() {
    let clock = Arc::new(FakeClock::default());
    assert!(
        Coprocessor::scan_with_clock(Arc::new(Absent), clock.clone(), Duration::from_secs(15))
            .is_err()
    );
    assert_eq!(clock.now(), Duration::from_secs(15));
}
struct NeverCompletes;
impl I2c for NeverCompletes {
    fn transaction(&self, _: u8, write: &[u8], read: usize) -> Result<Vec<u8>> {
        if write.is_empty() {
            return Ok(vec![3; read]);
        }
        Ok(vec![])
    }
}
#[test]
fn coprocessor_authentication_timeout_uses_best_effort_error_register() {
    let clock = Arc::new(FakeClock::default());
    let client = Coprocessor::scan_with_clock(
        Arc::new(NeverCompletes),
        clock.clone(),
        Duration::from_secs(15),
    )
    .unwrap();
    let error = client.sign_challenge(&[1]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("error 0x3"));
    assert_eq!(clock.now(), Duration::from_millis(3010));
}
#[test]
fn coprocessor_rejects_zero_certificate_and_short_register_response_without_retry() {
    let (client, bus, _) = bus(vec![select(0x30), read(vec![0, 0])]);
    assert!(client.read_certificate(100).is_err());
    assert!(bus.0.lock().unwrap().is_empty());
    let (client, _, clock) = self::bus(vec![select(2), Step(0x11, vec![], 1, Ok(vec![]))]);
    assert!(client.protocol_major().is_err());
    assert_eq!(clock.now(), Duration::ZERO);
}
struct FakeUsb {
    steps: VecDeque<(bool, Vec<u8>, usize)>,
    written: Arc<Mutex<Vec<Vec<u8>>>>,
}
impl Usb for FakeUsb {
    fn write(&mut self, b: &[u8], _: Duration) -> Result<()> {
        self.written.lock().unwrap().push(b.to_vec());
        Ok(())
    }
    fn read(&mut self, max: usize, _: Duration, _: Duration, empty: bool) -> Result<Vec<u8>> {
        let (e, b, m) = self.steps.pop_front().unwrap();
        assert_eq!((empty, max), (e, m));
        Ok(b)
    }
}
#[test]
fn ch341_small_combined_transaction_checks_status_and_splits_read_from_tail() {
    let written = Arc::new(Mutex::new(vec![]));
    let usb = FakeUsb {
        steps: vec![(false, vec![0, 0, 0, 3, 4], 5)].into(),
        written: written.clone(),
    };
    let bridge = Ch341::new(Box::new(usb));
    assert_eq!(bridge.transaction(0x11, &[2], 2).unwrap(), [3, 4]);
    assert_eq!(
        *written.lock().unwrap(),
        vec![
            vec![0xaa, 0x61, 0],
            vec![0xaa, 0x74, 0x80, 0x22, 0x80, 2, 0x74, 0x80, 0x23, 0xc1, 0xc0, 0x75, 0]
        ]
    );
    let usb = FakeUsb {
        steps: vec![(true, vec![0x80], 2)].into(),
        written,
    };
    assert!(Ch341::new(Box::new(usb))
        .transaction(0x11, &[2], 0)
        .is_err());
}
#[test]
fn ch341_segmented_read_suppresses_address_status_and_supports_maximum_length() {
    let written = Arc::new(Mutex::new(vec![]));
    let usb = FakeUsb {
        steps: vec![(false, vec![1; 32], 32), (false, vec![2], 1)].into(),
        written: written.clone(),
    };
    assert_eq!(
        Ch341::new(Box::new(usb))
            .transaction(0x11, &[], 33)
            .unwrap(),
        [vec![1; 32], vec![2]].concat()
    );
    let packets = written.lock().unwrap();
    assert_eq!(&packets[1][..6], &[0xaa, 0x74, 0x81, 0x23, 0xe0, 0]);
    assert_eq!(packets[2], vec![0xaa, 0xc0, 0x75, 0]);
    let maximum = ch341::encode(0x11, &[], 65535).unwrap();
    assert!(maximum.chunks(32).all(|p| p[0] == 0xaa));
    assert_eq!(maximum.iter().filter(|&&b| b == 0x75).count(), 1);
    assert_eq!(maximum.iter().filter(|&&b| b == 0xc0).count(), 1);
    assert!(ch341::encode(0x11, &[], 65536).is_err());
    assert!(ch341::encode(128, &[1], 0).is_err());
}
#[test]
fn ch341_accepts_unanswered_write_and_rejects_missing_read_data() {
    let written = Arc::new(Mutex::new(vec![]));
    let usb = FakeUsb {
        steps: vec![(true, vec![], 2)].into(),
        written: written.clone(),
    };
    assert!(Ch341::new(Box::new(usb))
        .transaction(0x11, &[2], 0)
        .unwrap()
        .is_empty());
    let usb = FakeUsb {
        steps: vec![(false, vec![0], 3)].into(),
        written,
    };
    assert!(Ch341::new(Box::new(usb)).transaction(0x11, &[], 2).is_err());
}
