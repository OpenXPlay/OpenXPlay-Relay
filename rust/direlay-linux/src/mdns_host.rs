// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Admin service discovery; distinct from linux_bonjour.rs CarPlay discovery.
use mdns_sd::{DaemonEvent, ServiceDaemon, ServiceInfo};
use std::{
    collections::HashMap,
    io,
    net::{IpAddr, Ipv4Addr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
pub const HOST: &str = "openxplay-linux.local.";
pub const SERVICE_TYPE: &str = "_openxplay._tcp.local.";
pub fn service_info(
    addresses: &[Ipv4Addr],
    admin_port: u16,
    receiver_port: u16,
) -> io::Result<ServiceInfo> {
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|a| a.is_unspecified() || a.is_multicast())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "mDNS requires host IPv4 addresses",
        ));
    }
    let txt = HashMap::from([
        ("platform".to_owned(), "linux".to_owned()),
        ("receiver".to_owned(), receiver_port.to_string()),
        ("version".to_owned(), env!("CARGO_PKG_VERSION").to_owned()),
    ]);
    let ips: Vec<IpAddr> = addresses.iter().copied().map(IpAddr::V4).collect();
    ServiceInfo::new(
        SERVICE_TYPE,
        crate::branding::DEVICE_NAME,
        HOST,
        &ips[..],
        admin_port,
        txt,
    )
    .map(ServiceInfo::enable_addr_interface_only)
    .map_err(io::Error::other)
}
pub struct MdnsHost {
    daemon: ServiceDaemon,
    name: Arc<Mutex<String>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    fullname: String,
    admin_port: u16,
    receiver_port: u16,
}
impl MdnsHost {
    /// Caller supplies only LAN/setup addresses, excluding the iPhone-facing hotspot.
    pub fn new(addresses: &[Ipv4Addr], admin_port: u16, receiver_port: u16) -> io::Result<Self> {
        let info = service_info(addresses, admin_port, receiver_port)?;
        let fullname = info.get_fullname().to_owned();
        let daemon = ServiceDaemon::new().map_err(io::Error::other)?;
        // Restrict transport interfaces as well as record addresses.
        daemon
            .disable_interface(mdns_sd::IfKind::All)
            .map_err(io::Error::other)?;
        for ip in addresses {
            daemon
                .enable_interface(mdns_sd::IfKind::Addr(IpAddr::V4(*ip)))
                .map_err(io::Error::other)?;
        }
        let monitor = daemon.monitor().map_err(io::Error::other)?;
        daemon.register(info).map_err(io::Error::other)?;
        let name = Arc::new(Mutex::new(HOST.trim_end_matches('.').into()));
        let stopped = Arc::new(AtomicBool::new(false));
        let (n, s) = (name.clone(), stopped.clone());
        let worker = thread::Builder::new()
            .name("admin-mdns".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    if let Ok(DaemonEvent::NameChange(change)) =
                        monitor.recv_timeout(Duration::from_millis(200))
                    {
                        if change.original == HOST {
                            *n.lock().unwrap() = change.new_name.trim_end_matches('.').into();
                        }
                    }
                }
            })?;
        Ok(Self {
            daemon,
            name,
            stopped,
            worker: Some(worker),
            fullname,
            admin_port,
            receiver_port,
        })
    }
    pub fn advertised_name(&self) -> String {
        self.name.lock().unwrap().clone()
    }
    pub fn update_addresses(&self, addresses: &[Ipv4Addr]) -> io::Result<()> {
        let info = service_info(addresses, self.admin_port, self.receiver_port)?;
        self.daemon
            .disable_interface(mdns_sd::IfKind::All)
            .map_err(io::Error::other)?;
        for ip in addresses {
            self.daemon
                .enable_interface(mdns_sd::IfKind::Addr(IpAddr::V4(*ip)))
                .map_err(io::Error::other)?;
        }
        self.daemon.register(info).map_err(io::Error::other)
    }
    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for MdnsHost {
    fn drop(&mut self) {
        self.stop();
    }
}
