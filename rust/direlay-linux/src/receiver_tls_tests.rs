// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use super::*;
use std::{
    fs::File,
    os::fd::{FromRawFd, OwnedFd},
    sync::{atomic::AtomicUsize, mpsc},
};

pub(crate) struct Request {
    pub value: Value,
    pub events: EventSink,
}
impl Request {
    pub fn applied(&self) {
        (self.events)(
            json!({"type":"web-tls","id":self.value["id"],"applied":true,
            "trustedName":self.value["trusted"]["name"],"algorithm":"SHA-256",
            "fingerprint":"self-signed", "trustedFingerprint":"trusted"}),
        );
    }
    pub fn rejected(&self) {
        (self.events)(json!({"type":"web-tls","id":self.value["id"],"error":"tls_update_failed"}));
    }
}
pub(crate) struct FakeChildFactory {
    requests: mpsc::Sender<Request>,
    pub launches: Mutex<Vec<Vec<OsString>>>,
    pub stops: Arc<AtomicUsize>,
    exited: Arc<AtomicBool>,
}
impl FakeChildFactory {
    pub fn new() -> (Arc<Self>, mpsc::Receiver<Request>) {
        let (tx, rx) = mpsc::channel();
        (
            Arc::new(Self {
                requests: tx,
                launches: Mutex::default(),
                stops: Arc::default(),
                exited: Arc::default(),
            }),
            rx,
        )
    }
    pub fn wait_started(&self) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.launches.lock().unwrap().is_empty() {
            assert!(Instant::now() < deadline, "fake child not started");
            thread::sleep(Duration::from_millis(2));
        }
    }
}
struct FakeChild {
    pipe: Option<ControlPipe>,
    reader: Option<JoinHandle<()>>,
    stops: Arc<AtomicUsize>,
    exited: Arc<AtomicBool>,
}
impl RelayChild for FakeChild {
    fn control(&self) -> Option<ControlPipe> {
        self.pipe.clone()
    }
    fn exited(&mut self) -> io::Result<bool> {
        Ok(self.exited.swap(false, Ordering::AcqRel))
    }
    fn stop(&mut self) -> io::Result<()> {
        self.pipe.take();
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
}
impl ChildFactory for FakeChildFactory {
    fn spawn(
        &self,
        _: &Path,
        args: &[OsString],
        events: EventSink,
    ) -> io::Result<Box<dyn RelayChild>> {
        assert_eq!(
            args.iter().filter(|arg| *arg == "--control-stdin").count(),
            1
        );
        let mut fds = [-1; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let input = unsafe { File::from_raw_fd(fds[0]) };
        let output = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        let pipe = ControlPipe::new(ChildStdin::from(output))?;
        let tx = self.requests.clone();
        let reader = thread::spawn(move || {
            for line in BufReader::new(input).lines() {
                let value = serde_json::from_str(&line.unwrap()).unwrap();
                if tx
                    .send(Request {
                        value,
                        events: events.clone(),
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.launches.lock().unwrap().push(args.to_vec());
        Ok(Box::new(FakeChild {
            pipe: Some(pipe),
            reader: Some(reader),
            stops: self.stops.clone(),
            exited: self.exited.clone(),
        }))
    }
}
fn supervisor(factory: Arc<FakeChildFactory>) -> RelaySupervisor {
    let settings = SupervisorSettings {
        binary: "/fake/oxp-relay".into(),
        socket: "/tmp/nonexistent-web-tls-test.sock".into(),
        web_port: 8443,
        web_root: "/web".into(),
        plugins: None,
        state_dir: "/state".into(),
        addresses: vec!["10.77.0.1".parse().unwrap()],
        pairing: false,
        carlink: None,
        pem_tls: None,
        default_tls: None,
    };
    let mut supervisor = RelaySupervisor::new(settings, factory.clone(), Arc::new(|_| {}));
    supervisor.start().unwrap();
    factory.wait_started();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !supervisor.running() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    supervisor
}
#[test]
fn matching_success_updates_the_complete_set_and_automatic_retry_arguments() {
    let (factory, rx) = FakeChildFactory::new();
    let mut supervisor = supervisor(factory.clone());
    let sans: Vec<_> = (1..=16).map(|n| Ipv4Addr::new(192, 168, 1, n)).collect();
    thread::scope(|scope| {
        let update =
            scope.spawn(|| supervisor.update_web_tls(None, sans.clone(), Duration::from_secs(2)));
        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(request.value["sans"].as_array().unwrap().len(), 17);
        assert_eq!(request.value["sans"][0], "openxplay-linux.local");
        (request.events)(
            json!({"type":"web-tls","id":"unrelated","applied":true,"trustedName":null}),
        );
        assert_eq!(
            supervisor.web_tls.settings().sans,
            ["10.77.0.1".parse::<Ipv4Addr>().unwrap()]
        );
        request.applied();
        update.join().unwrap().unwrap();
    });
    assert_eq!(supervisor.web_tls.settings().sans, sans);
    factory.exited.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(3);
    while factory.launches.lock().unwrap().len() < 2 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let launches = factory.launches.lock().unwrap();
    assert!(launches[1]
        .windows(2)
        .any(|a| a[0] == "--tls-san" && a[1] == "192.168.1.16"));
    drop(launches);
    supervisor.stop();
}
#[test]
fn rejection_timeout_wrong_name_and_late_reply_preserve_the_served_set() {
    let (factory, rx) = FakeChildFactory::new();
    let mut supervisor = supervisor(factory);
    let before = supervisor.web_tls.settings().sans;
    let mut late = None;
    for kind in ["error", "timeout", "wrong-name", "success"] {
        thread::scope(|scope| {
            let update = scope.spawn(|| {
                supervisor.update_web_tls(
                    None,
                    vec![Ipv4Addr::new(3, 3, 3, 1)],
                    Duration::from_millis(80),
                )
            });
            let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            if let Some(old) = late.take() {
                Request::applied(&old);
            }
            match kind {
                "error" => request.rejected(),
                "wrong-name" => (request.events)(
                    json!({"type":"web-tls","id":request.value["id"],"applied":true,"trustedName":"unexpected.example"}),
                ),
                "success" => request.applied(),
                _ => {}
            }
            let result = update.join().unwrap();
            if kind == "success" {
                result.unwrap();
            } else {
                assert!(result.is_err());
            }
            if kind == "timeout" {
                late = Some(request);
            }
        });
        if kind != "success" {
            assert_eq!(supervisor.web_tls.settings().sans, before);
        }
    }
    supervisor.stop();
}

#[test]
fn replacing_the_child_wakes_the_waiter_and_rejects_old_generation_replies() {
    let (factory, rx) = FakeChildFactory::new();
    let mut supervisor = supervisor(factory.clone());
    let old = thread::scope(|scope| {
        let update = scope.spawn(|| {
            supervisor.update_web_tls(
                None,
                vec![Ipv4Addr::new(3, 3, 3, 1)],
                Duration::from_secs(2),
            )
        });
        let old = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        factory.exited.store(true, Ordering::Release);
        assert_eq!(
            update.join().unwrap().unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        old
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while factory.launches.lock().unwrap().len() < 2 || !supervisor.running() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    thread::scope(|scope| {
        let update = scope.spawn(|| {
            supervisor.update_web_tls(
                None,
                vec![Ipv4Addr::new(3, 3, 3, 2)],
                Duration::from_secs(2),
            )
        });
        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // Even a matching ID from the retired reader cannot acknowledge the new child.
        (old.events)(
            json!({"type":"web-tls","id":request.value["id"],"applied":true,"trustedName":null}),
        );
        assert_eq!(
            supervisor.web_tls.settings().sans,
            ["10.77.0.1".parse::<Ipv4Addr>().unwrap()]
        );
        request.applied();
        update.join().unwrap().unwrap();
    });
    supervisor.stop();
}

#[test]
fn default_certificate_requires_matching_fingerprint_and_keeps_trusted_sni_separate() {
    let (factory, rx) = FakeChildFactory::new();
    let mut supervisor = supervisor(factory.clone());
    let base = crate::identity::tls::DefaultTls {
        cert: "/private/default.pem".into(),
        key: "/private/default.key".into(),
        fingerprint: "AA:BB".into(),
    };
    for accepted in [false, true] {
        thread::scope(|scope| {
            let update = scope.spawn(|| {
                supervisor.update_web_tls_with_default(
                    None,
                    vec![],
                    Some(base.clone()),
                    Duration::from_secs(2),
                )
            });
            let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(request.value["default"]["cert"], "/private/default.pem");
            assert_eq!(request.value["trusted"], Value::Null);
            (request.events)(
                json!({"type":"web-tls","id":request.value["id"],"applied":true,"trustedName":null,"defaultFingerprint":if accepted {"AA:BB"} else {"wrong"}}),
            );
            assert_eq!(update.join().unwrap().is_ok(), accepted);
        });
        assert_eq!(
            supervisor.web_tls.settings().default_tls.is_some(),
            accepted
        );
    }
    thread::scope(|scope| {
        let update = scope.spawn(|| {
            supervisor.update_web_tls_with_default(None, vec![], None, Duration::from_secs(2))
        });
        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(request.value["default"], Value::Null);
        (request.events)(
            json!({"type":"web-tls","id":request.value["id"],"applied":true,"trustedName":null,"defaultFingerprint":null}),
        );
        update.join().unwrap().unwrap();
    });
    assert!(supervisor.web_tls.settings().default_tls.is_none());
    supervisor.stop();
}
