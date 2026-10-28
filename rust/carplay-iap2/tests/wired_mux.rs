// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

mod wired_common;
use carplay_iap2::{
    transport::{
        iap2_usb_mux_host::*,
        lockdown_car_kit_client::LockdownCarKitClient,
        lockdown_pair_record::LockdownPairRecord,
        lockdown_pairing_client::{LockdownPairingClient, LockdownPairingError},
        lockdown_plist_channel::{self as pl},
        lockdown_tls_engine_factory::TlsConnector,
    },
    DuplexStream, Receive,
};
use plist::{Dictionary, Value};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
use wired_common::*;
struct Peer {
    stream: Arc<Memory>,
}
#[derive(Clone, Copy)]
struct Conn {
    source: u16,
    destination: u16,
}
fn be16(b: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([b[i], b[i + 1]])
}
fn be32(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes(b[i..i + 4].try_into().unwrap())
}
impl Peer {
    fn begin(stream: Arc<Memory>) -> Self {
        let p = Self { stream };
        let version = read_exact(p.stream.as_ref(), 20);
        assert_eq!(&version[4..12], &[0, 0, 0, 20, 0, 0, 0, 2]);
        p.stream
            .send(&hex("000000000000001400000002000000004928730000000000"))
            .unwrap();
        let setup = p.frame();
        assert_eq!(setup.len(), 17);
        assert_eq!(be32(&setup, 0), 2);
        assert_eq!(setup[16], 7);
        p
    }
    fn frame(&self) -> Vec<u8> {
        let mut b = read_exact(self.stream.as_ref(), 16);
        let n = be32(&b, 4) as usize;
        assert!((16..=65536).contains(&n));
        b.extend(read_exact(self.stream.as_ref(), n - 16));
        b
    }
    fn accept(&self, destination: u16) -> Conn {
        loop {
            let f = self.frame();
            assert_eq!(be32(&f, 0), 6);
            let b = &f[16..];
            if b[13] == 2 {
                assert_eq!(be16(b, 2), destination);
                assert_eq!(be32(b, 4), 0);
                let c = Conn {
                    source: destination,
                    destination: be16(b, 0),
                };
                self.send(c, 0x12, 100, &[]);
                let ack = self.frame();
                assert_eq!(ack[29], 0x10);
                assert_eq!(be32(&ack, 20), 1);
                assert_eq!(be32(&ack, 24), 101);
                return c;
            }
        }
    }
    fn send(&self, c: Conn, flags: u8, seq: u32, data: &[u8]) {
        let mut f = vec![0; 36];
        f[..4].copy_from_slice(&6u32.to_be_bytes());
        f[4..8].copy_from_slice(&((36 + data.len()) as u32).to_be_bytes());
        f[8..12].copy_from_slice(&0xfacefaceu32.to_be_bytes());
        f[16..18].copy_from_slice(&c.source.to_be_bytes());
        f[18..20].copy_from_slice(&c.destination.to_be_bytes());
        f[20..24].copy_from_slice(&seq.to_be_bytes());
        f[24..28].copy_from_slice(&1u32.to_be_bytes());
        f[28] = 0x50;
        f[29] = flags;
        f[30..32].copy_from_slice(&512u16.to_be_bytes());
        f.extend_from_slice(data);
        self.stream.send(&f).unwrap();
    }
    fn data(&self, c: Conn) -> Vec<u8> {
        loop {
            let f = self.frame();
            assert_eq!(be16(&f, 16), c.destination);
            let h = (f[28] >> 4) as usize * 4;
            if f.len() > 16 + h {
                return f[16 + h..].to_vec();
            }
            assert_eq!(f[29] & 4, 0);
        }
    }
    fn plist(&self, c: Conn) -> Dictionary {
        let mut b = self.data(c);
        while b.len() < 4 {
            b.extend(self.data(c));
        }
        let n = be32(&b, 0) as usize;
        while b.len() < n + 4 {
            b.extend(self.data(c));
        }
        assert_eq!(b.len(), n + 4);
        pl::parse(&b[4..]).unwrap()
    }
    fn reply(&self, c: Conn, d: Dictionary) {
        let xml = pl::encode(&d).unwrap();
        let mut b = (xml.len() as u32).to_be_bytes().to_vec();
        b.extend(xml);
        self.send(c, 0x10, 101, &b);
    }
}
fn dict(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Dictionary {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}
fn host_pair() -> (Arc<Iap2UsbMuxHost>, std::thread::JoinHandle<Peer>) {
    let (a, b) = memory_pair();
    let peer = std::thread::spawn(move || Peer::begin(b));
    let host = Iap2UsbMuxHost::open(a, Duration::from_millis(20), Arc::new(|_| {})).unwrap();
    (host, peer)
}
#[test]
fn captured_handshake_writes_20_then_17_and_reader_retains_trailers() {
    let (a, b) = memory_pair();
    let log = Arc::new((Mutex::new(vec![]), Condvar::new()));
    let capture = log.clone();
    b.send(&hex("000000000000001400000002000000004928730000000000"))
        .unwrap();
    let syn = hex("0000000600000024faceface00000001f27e000100000000000000015f1202007f000001");
    b.send(&[syn.clone(), hex("6d436f6e"), syn].concat())
        .unwrap();
    let host = Iap2UsbMuxHost::open(
        a.clone(),
        Duration::from_millis(20),
        Arc::new(move |s| {
            capture.0.lock().unwrap().push(s);
            capture.1.notify_all();
        }),
    )
    .unwrap();
    let reports = log.0.lock().unwrap();
    let reports = log
        .1
        .wait_timeout_while(reports, Duration::from_secs(2), |r| r.len() < 2)
        .unwrap()
        .0;
    assert_eq!(reports.len(), 2);
    assert!(reports[0].contains("previousProtocol=0 previousLength=20"));
    assert!(reports[1].contains("previousProtocol=6 previousLength=36"));
    assert_eq!(
        a.writes
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [20, 17]
    );
    host.close().unwrap();
}
#[test]
fn mux_tcp_connect_send_split_receive_fin_and_rst() {
    let (host, peer) = host_pair();
    let peer = peer.join().unwrap();
    let script = std::thread::spawn(move || {
        let c = peer.accept(62078);
        let first = peer.data(c);
        let second = peer.data(c);
        assert_eq!(first.len(), 16384);
        assert_eq!(second.len(), 3616);
        assert!(first.iter().chain(second.iter()).all(|b| *b == 7));
        peer.send(c, 0x11, 101, b"response");
        let reset = peer.accept(1234);
        peer.send(reset, 4, 101, &[]);
    });
    let stream = host.connect(62078, Duration::from_secs(2)).unwrap();
    stream.send(&[7; 20000]).unwrap();
    assert_eq!(
        stream.recv(3, Duration::from_secs(2)).unwrap(),
        Receive::Data(b"res".to_vec())
    );
    assert_eq!(
        stream.recv(100, Duration::from_secs(2)).unwrap(),
        Receive::Data(b"ponse".to_vec())
    );
    assert_eq!(
        stream.recv(100, Duration::from_secs(2)).unwrap(),
        Receive::Eof
    );
    stream.close().unwrap();
    let reset = host.connect(1234, Duration::from_secs(2));
    if let Ok(stream) = reset {
        assert_eq!(
            stream.recv(100, Duration::from_secs(2)).unwrap_err().kind(),
            std::io::ErrorKind::ConnectionReset
        );
    } else {
        assert_eq!(
            reset.err().unwrap().kind(),
            std::io::ErrorKind::ConnectionReset
        );
    }
    script.join().unwrap();
    host.close().unwrap();
}
#[test]
fn mux_connect_timeout_resets_only_that_connection() {
    let (host, peer) = host_pair();
    let peer = peer.join().unwrap();
    let script = std::thread::spawn(move || {
        let syn = peer.frame();
        assert_eq!(syn[29], 2);
        let reset = peer.frame();
        assert_eq!(reset[29], 0x14);
        let c = peer.accept(1234);
        assert_eq!(peer.data(c), b"ready");
        peer.send(c, 0x11, 101, b"still usable");
    });
    assert_eq!(
        host.connect(62078, Duration::from_millis(10))
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    let stream = host.connect(1234, Duration::from_secs(1)).unwrap();
    stream.send(b"ready").unwrap();
    assert_eq!(read_exact(stream.as_ref(), 12), b"still usable");
    script.join().unwrap();
    host.close().unwrap();
}
#[test]
fn pairing_reconnects_pending_dialog_and_reuses_generated_record() {
    use rsa::{pkcs1::EncodeRsaPublicKey, pkcs8::LineEnding, RsaPrivateKey};
    let key = RsaPrivateKey::new(&mut rand_core::OsRng, 2048).unwrap();
    let public = key.to_public_key().to_pkcs1_pem(LineEnding::LF).unwrap();
    let (host, peer) = host_pair();
    let peer = peer.join().unwrap();
    let script = std::thread::spawn(move || {
        let first = peer.accept(62078);
        let set = peer.plist(first);
        assert_eq!(
            set.get("Key").and_then(Value::as_string),
            Some("UntrustedHostBUID")
        );
        peer.reply(first, Dictionary::new());
        for (k, v) in [
            ("DevicePublicKey", Value::Data(public.into_bytes())),
            ("WiFiAddress", Value::String("02:00:00:00:00:02".into())),
        ] {
            let request = peer.plist(first);
            assert_eq!(request.get("Key").and_then(Value::as_string), Some(k));
            peer.reply(first, dict([("Value", v)]));
        }
        let pair = peer.plist(first);
        assert_eq!(pair.get("Request").and_then(Value::as_string), Some("Pair"));
        assert_eq!(
            pair.get("ProtocolVersion").and_then(Value::as_string),
            Some("2")
        );
        peer.reply(
            first,
            dict([(
                "Error",
                Value::String("PairingDialogResponsePending".into()),
            )]),
        );
        let second = peer.accept(62078);
        let set = peer.plist(second);
        assert_eq!(
            set.get("Request").and_then(Value::as_string),
            Some("SetValue")
        );
        peer.reply(second, Dictionary::new());
        let repeated = peer.plist(second);
        assert_eq!(repeated, pair);
        peer.reply(second, dict([("EscrowBag", Value::Data(vec![1, 2, 3]))]));
    });
    let record = LockdownPairingClient::new(host.clone())
        .pair(
            "OpenXPlay",
            "host",
            "buid",
            Duration::from_secs(300),
            &|| false,
        )
        .unwrap();
    assert_eq!(record.host_id, "host");
    assert_eq!(record.system_buid, "buid");
    assert_eq!(record.escrow_bag, Some(vec![1, 2, 3]));
    script.join().unwrap();
    assert!(matches!(
        LockdownPairingClient::new(host.clone()).pair(
            "label",
            "host",
            "buid",
            Duration::from_secs(1),
            &|| true
        ),
        Err(LockdownPairingError::Cancelled)
    ));
    host.close().unwrap();
}
type TlsIdentity = (Vec<Vec<u8>>, Vec<u8>);
#[derive(Default)]
struct FakeTls {
    identities: Mutex<Vec<TlsIdentity>>,
}
impl TlsConnector for FakeTls {
    fn connect(
        &self,
        stream: Arc<dyn DuplexStream>,
        chain: Vec<rustls_pki_types::CertificateDer<'static>>,
        key: rustls_pki_types::PrivateKeyDer<'static>,
    ) -> carplay_iap2::Result<Arc<dyn DuplexStream>> {
        self.identities.lock().unwrap().push((
            chain.iter().map(|c| c.as_ref().to_vec()).collect(),
            key.secret_der().to_vec(),
        ));
        Ok(stream)
    }
}
fn record() -> LockdownPairRecord {
    LockdownPairRecord {
        host_id: "host".into(),
        system_buid: "buid".into(),
        wifi_mac_address: "02:00:00:00:00:02".into(),
        device_public_key_pem: vec![],
        device_certificate_pem: vec![],
        host_private_key_pem: vec![],
        host_certificate_pem: vec![],
        root_private_key_pem: b"-----BEGIN PRIVATE KEY-----\nAQI=\n-----END PRIVATE KEY-----"
            .to_vec(),
        root_certificate_pem: b"-----BEGIN CERTIFICATE-----\nAwQ=\n-----END CERTIFICATE-----"
            .to_vec(),
        escrow_bag: None,
    }
}
#[test]
fn carkit_start_session_service_tls_chunks_and_teardown_order() {
    let (host, peer) = host_pair();
    let peer = peer.join().unwrap();
    let script = std::thread::spawn(move || {
        let lockdown = peer.accept(62078);
        let request = peer.plist(lockdown);
        assert_eq!(
            request.get("Request").and_then(Value::as_string),
            Some("StartSession")
        );
        assert_eq!(
            request.get("HostID").and_then(Value::as_string),
            Some("host")
        );
        assert_eq!(
            request.get("SystemBUID").and_then(Value::as_string),
            Some("buid")
        );
        peer.reply(lockdown, dict([("EnableSessionSSL", Value::Boolean(true))]));
        let start = peer.plist(lockdown);
        assert_eq!(
            start.get("Service").and_then(Value::as_string),
            Some("com.apple.carkit.service")
        );
        peer.reply(
            lockdown,
            dict([
                ("Port", Value::Integer(1234.into())),
                ("EnableServiceSSL", Value::Boolean(true)),
            ]),
        );
        let service = peer.accept(1234);
        for n in [256, 256, 88] {
            assert_eq!(peer.data(service), vec![7; n]);
        }
        peer.send(service, 0x10, 101, b"service");
        let mut closed = vec![];
        while closed.len() < 2 {
            let f = peer.frame();
            if f[29] & 1 != 0 {
                closed.push(be16(&f, 18));
            }
        }
        assert_eq!(closed, [1234, 62078]);
    });
    let tls = Arc::new(FakeTls::default());
    let stream = LockdownCarKitClient::new(host.clone(), tls.clone())
        .open(&record(), "OpenXPlay")
        .unwrap();
    stream.send(&[7; 600]).unwrap();
    assert_eq!(read_exact(stream.as_ref(), 7), b"service");
    stream.close().unwrap();
    script.join().unwrap();
    let identities = tls.identities.lock().unwrap();
    assert_eq!(identities.len(), 2);
    assert!(identities
        .iter()
        .all(|(chain, key)| chain == &[vec![3, 4]] && key == &[1, 2]));
    drop(identities);
    host.close().unwrap();
}
#[test]
fn carkit_record_rejection_is_typed_and_service_failure_closes_lockdown() {
    for code in ["InvalidPairRecord", "InvalidHostID"] {
        let (host, peer) = host_pair();
        let peer = peer.join().unwrap();
        let script = std::thread::spawn(move || {
            let c = peer.accept(62078);
            peer.plist(c);
            peer.reply(c, dict([("Error", Value::String(code.into()))]));
            loop {
                let f = peer.frame();
                if f[29] & 1 != 0 {
                    break;
                }
            }
        });
        let e = LockdownCarKitClient::new(host.clone(), Arc::new(FakeTls::default()))
            .open(&record(), "label")
            .err()
            .unwrap();
        assert!(e.invalidates_record());
        assert_eq!(e.to_string(), code);
        script.join().unwrap();
        host.close().unwrap();
    }
    let (host, peer) = host_pair();
    let peer = peer.join().unwrap();
    let script = std::thread::spawn(move || {
        let c = peer.accept(62078);
        peer.plist(c);
        peer.reply(c, dict([("EnableSessionSSL", Value::Boolean(true))]));
        peer.plist(c);
        peer.reply(c, dict([("Port", Value::Integer(0.into()))]));
        loop {
            let f = peer.frame();
            if f[29] & 1 != 0 {
                break;
            }
        }
    });
    assert!(
        LockdownCarKitClient::new(host.clone(), Arc::new(FakeTls::default()))
            .open(&record(), "label")
            .is_err()
    );
    script.join().unwrap();
    host.close().unwrap();
}
