// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use crate::*;
use carplay_airplay::*;
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
/// Upper bound for withdrawing the announcements when the receiver closes.
const GOODBYE_BUDGET: Duration = Duration::from_secs(1);
/// mdns-sd repeats each goodbye 120 ms after the first; let it go out before shutdown.
const GOODBYE_REPEAT: Duration = Duration::from_millis(150);
/// The daemon operations `close` needs, so the goodbye order is testable without sockets.
trait Announcer {
    /// Sends the goodbye for one service and waits at most `timeout` for the daemon to confirm.
    fn unregister(&self, fullname: &str, timeout: Duration) -> bool;
    fn shutdown(&self);
}
impl Announcer for mdns_sd::ServiceDaemon {
    fn unregister(&self, fullname: &str, timeout: Duration) -> bool {
        mdns_sd::ServiceDaemon::unregister(self, fullname).is_ok_and(|status| {
            matches!(
                status.recv_timeout(timeout),
                Ok(mdns_sd::UnregisterStatus::OK)
            )
        })
    }
    fn shutdown(&self) {
        let _ = mdns_sd::ServiceDaemon::shutdown(self);
    }
}
/// Withdraws every service before shutdown, so the iPhone drops the old TXT record (and its
/// HEVC feature bit) at once instead of keeping it until the next announcement.
fn withdraw(announcer: &dyn Announcer, services: &[String], budget: Duration, repeat: Duration) {
    let deadline = Instant::now() + budget;
    let mut sent = false;
    for name in services {
        sent |= announcer.unregister(name, deadline.saturating_duration_since(Instant::now()));
    }
    if sent {
        std::thread::sleep(repeat.min(deadline.saturating_duration_since(Instant::now())));
    }
    announcer.shutdown();
}
pub struct LinuxBonjour {
    address: Ipv4Addr,
    config: AirPlayConfig,
    identity: Arc<AirPlayIdentity>,
    events: Arc<JsonEvents>,
    /// The daemon and the full names of the services it announces.
    dns: Mutex<Option<(mdns_sd::ServiceDaemon, Vec<String>)>>,
    closed: Arc<AtomicBool>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl LinuxBonjour {
    pub fn new(
        address: Ipv4Addr,
        config: AirPlayConfig,
        identity: Arc<AirPlayIdentity>,
        events: Arc<JsonEvents>,
    ) -> Self {
        Self {
            address,
            config,
            identity,
            events,
            dns: Mutex::new(None),
            closed: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        }
    }
    pub fn start(&self) -> Result<()> {
        let dns = mdns_sd::ServiceDaemon::new().map_err(std::io::Error::other)?;
        dns.disable_interface(mdns_sd::IfKind::All)
            .map_err(std::io::Error::other)?;
        dns.enable_interface(mdns_sd::IfKind::Addr(self.address.into()))
            .map_err(std::io::Error::other)?;
        let records = CarPlayBonjourProtocol::air_play_txt_records(&self.config, &self.identity);
        let host = format!("carplay-{}.local.", self.config.device_id.replace(':', ""));
        let service = mdns_sd::ServiceInfo::new(
            "_airplay._tcp.local.",
            &self.config.device_name,
            &host,
            std::net::IpAddr::V4(self.address),
            self.config.port,
            records,
        )
        .map_err(std::io::Error::other)?;
        let services = vec![service.get_fullname().to_string()];
        dns.register(service).map_err(std::io::Error::other)?;
        let rx = dns
            .browse("_carplay-ctrl._tcp.local.")
            .map_err(std::io::Error::other)?;
        *self.dns.lock().unwrap() = Some((dns, services));
        let address = self.address;
        let (closed, events, config) = (
            self.closed.clone(),
            self.events.clone(),
            self.config.clone(),
        );
        *self.worker.lock().unwrap() = Some(crate::platform::spawn_worker(
            "bonjour-probe",
            Box::new(move || {
                let mut seen = HashSet::new();
                while !closed.load(Ordering::Acquire) {
                    match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(mdns_sd::ServiceEvent::ServiceRemoved(_, name)) => {
                            seen.remove(&name);
                        }
                        Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                            if !seen.insert(info.get_fullname().to_string()) {
                                continue;
                            }
                            let Some(ip) =
                                info.get_addresses().iter().find(|a| a.is_ipv4()).copied()
                            else {
                                continue;
                            };
                            let port = info.get_port();
                            if port == 0 {
                                continue;
                            }
                            events.emit("bonjour-resolved",&serde_json::json!({"service":info.get_fullname(),"host":ip.to_string(),"port":port}));
                            for attempt in 1..=7 {
                                if closed.load(Ordering::Acquire) {
                                    break;
                                }
                                let result = (|| -> Result<String> {
                                    let mut socket =
                                        connect_probe_socket(address, SocketAddr::new(ip, port))?;
                                    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
                                    socket.set_write_timeout(Some(Duration::from_secs(3)))?;
                                    let request = CarPlayBonjourProtocol::connect_probe_request(
                                        &ip.to_string(),
                                        port,
                                        &config.source_version,
                                        &config.device_id,
                                    )?;
                                    socket.write_all(request.as_bytes())?;
                                    let mut status = Vec::new();
                                    while status.len() < 4096 {
                                        let mut b = [0];
                                        if socket.read(&mut b)? == 0 {
                                            break;
                                        }
                                        if b[0] == b'\n' {
                                            break;
                                        }
                                        status.push(b[0]);
                                    }
                                    if status.is_empty() {
                                        return Err(std::io::Error::other(
                                            "CarPlay probe returned no status",
                                        ));
                                    }
                                    Ok(String::from_utf8_lossy(&status).trim().to_string())
                                })();
                                match result{Ok(status)=>{events.emit("bonjour-probed",&serde_json::json!({"attempt":attempt,"status":status}));break},Err(e)=>events.emit("bonjour-probe-failed",&serde_json::json!({"attempt":attempt,"message":e.to_string()}))}
                                let start = Instant::now();
                                while attempt < 7
                                    && start.elapsed() < Duration::from_millis(1500)
                                    && !closed.load(Ordering::Acquire)
                                {
                                    std::thread::sleep(Duration::from_millis(50));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }),
        )?);
        Ok(())
    }
    pub fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        let dns = self.dns.lock().unwrap().take();
        if let Some((dns, services)) = dns {
            withdraw(&dns, &services, GOODBYE_BUDGET, GOODBYE_REPEAT);
        }
        if let Some(w) = self.worker.lock().unwrap().take() {
            let _ = w.join();
        }
        Ok(())
    }
}

pub fn features_txt(features: u64) -> String {
    if features >> 32 == 0 {
        format!("0x{features:x}")
    } else {
        format!("0x{:x},0x{:x}", features as u32, features >> 32)
    }
}

pub struct CarPlayBonjourProtocol;
impl CarPlayBonjourProtocol {
    pub fn features_txt(features: u64) -> String {
        features_txt(features)
    }
    pub fn air_play_txt_records(
        config: &AirPlayConfig,
        identity: &AirPlayIdentity,
    ) -> HashMap<String, String> {
        HashMap::from([
            ("deviceid".into(), config.device_id.clone()),
            (
                "features".into(),
                features_txt(AirPlayInfoPlist::features(config)),
            ),
            ("flags".into(), "0x4".into()),
            ("model".into(), config.model.clone()),
            ("srcvers".into(), config.source_version.clone()),
            ("protovers".into(), "1.1".into()),
            ("pi".into(), identity.pairing_id.clone()),
            ("pk".into(), identity.public_key_hex()),
        ])
    }
    pub fn connect_probe_request(
        host: &str,
        port: u16,
        source_version: &str,
        device_id: &str,
    ) -> Result<String> {
        let host = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host)
            .split('%')
            .next()
            .unwrap_or("");
        let id = device_id.replace(':', "");
        for (valid, message) in [
            (!host.trim().is_empty(), "host must not be blank"),
            (port != 0, "port must be in 1..65535"),
            (
                !source_version.is_empty(),
                "sourceVersion must not be empty",
            ),
            (
                !source_version.contains(['\r', '\n']),
                "sourceVersion must not contain a line break",
            ),
            (
                !host.contains(['\r', '\n']),
                "host must not contain a line break",
            ),
            (!id.is_empty(), "deviceId must contain a hexadecimal value"),
            (
                !id.contains(['\r', '\n']),
                "deviceId must not contain a line break",
            ),
        ] {
            if !valid {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    message,
                ));
            }
        }
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        Ok(format!("GET /ctrl-int/1/connect HTTP/1.1\r\nHost: {host}:{port}\r\nUser-Agent: AirPlay/{source_version}\r\nAirPlay-Receiver-Device-ID: {id}\r\nConnection: close\r\n\r\n"))
    }
}
fn connect_probe_socket(source: Ipv4Addr, target: SocketAddr) -> Result<TcpStream> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = source;
        TcpStream::connect_timeout(&target, Duration::from_secs(3))
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let SocketAddr::V4(target) = target else {
            return Err(std::io::Error::other("CarPlay probe requires IPv4"));
        };
        let raw = unsafe {
            libc::socket(
                libc::AF_INET,
                libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                0,
            )
        };
        if raw < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let addr = |ip: Ipv4Addr, port: u16| libc::sockaddr_in {
            sin_family: libc::AF_INET as _,
            sin_port: port.to_be(),
            sin_addr: libc::in_addr {
                s_addr: u32::from_ne_bytes(ip.octets()),
            },
            sin_zero: [0; 8],
        };
        let local = addr(source, 0);
        let remote = addr(*target.ip(), target.port());
        let len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
        if unsafe { libc::bind(raw, &local as *const _ as _, len) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe { libc::connect(raw, &remote as *const _ as _, len) } < 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(e);
            }
            let mut poll = libc::pollfd {
                fd: raw,
                events: libc::POLLOUT,
                revents: 0,
            };
            let n = unsafe { libc::poll(&mut poll, 1, 3000) };
            if n < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "CarPlay probe connect timed out",
                ));
            }
            let mut code = 0i32;
            let mut size = std::mem::size_of_val(&code) as libc::socklen_t;
            if unsafe {
                libc::getsockopt(
                    raw,
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    &mut code as *mut _ as _,
                    &mut size,
                )
            } < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            if code != 0 {
                return Err(std::io::Error::from_raw_os_error(code));
            }
        }
        let socket = TcpStream::from(fd);
        socket.set_nonblocking(false)?;
        let _ = socket.as_raw_fd();
        Ok(socket)
    }
}

#[cfg(test)]
mod goodbye_tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<(String, Duration)>>,
        /// Models a daemon that never confirms: `unregister` blocks for its whole timeout.
        stalled: bool,
    }
    impl Announcer for Recorder {
        fn unregister(&self, fullname: &str, timeout: Duration) -> bool {
            self.calls.lock().unwrap().push((fullname.into(), timeout));
            if self.stalled {
                std::thread::sleep(timeout);
            }
            !self.stalled
        }
        fn shutdown(&self) {
            self.calls
                .lock()
                .unwrap()
                .push(("shutdown".into(), Duration::ZERO));
        }
    }
    fn names(recorder: &Recorder) -> Vec<String> {
        let calls = recorder.calls.lock().unwrap();
        calls.iter().map(|(name, _)| name.clone()).collect()
    }
    #[test]
    fn every_service_says_goodbye_before_shutdown() {
        let recorder = Recorder::default();
        let services = [
            "a._airplay._tcp.local.".to_string(),
            "b._raop._tcp.local.".into(),
        ];
        let started = Instant::now();
        withdraw(
            &recorder,
            &services,
            Duration::from_secs(1),
            Duration::from_millis(20),
        );
        assert!(started.elapsed() >= Duration::from_millis(20));
        assert_eq!(
            names(&recorder),
            ["a._airplay._tcp.local.", "b._raop._tcp.local.", "shutdown"]
        );
        assert!(recorder.calls.lock().unwrap()[..2]
            .iter()
            .all(|(_, timeout)| *timeout <= Duration::from_secs(1)));
    }
    #[test]
    fn a_stalled_daemon_is_shut_down_within_the_budget() {
        let recorder = Recorder {
            stalled: true,
            ..Default::default()
        };
        let services = ["a".to_string(), "b".into()];
        let started = Instant::now();
        withdraw(
            &recorder,
            &services,
            Duration::from_millis(100),
            Duration::from_secs(5),
        );
        assert!(started.elapsed() < Duration::from_millis(600));
        assert_eq!(names(&recorder), ["a", "b", "shutdown"]);
        assert!(recorder.calls.lock().unwrap()[1].1 < Duration::from_millis(50));
    }
}
