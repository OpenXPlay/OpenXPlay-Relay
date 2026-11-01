// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use mdns_sd::{DaemonEvent, ServiceDaemon, ServiceInfo};
use std::{
    collections::{BTreeMap, HashMap},
    io,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex, Weak},
};
pub const HOST: &str = "openxplay-admin.local.";
pub const CAR_HOST: &str = "openxplay-car.local.";
pub const LEGACY_HOST: &str = "openxplay-linux.local.";
pub const SERVICE_TYPE: &str = "_openxplay._tcp.local.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub interface: String,
    pub addresses: Vec<Ipv4Addr>,
    pub car: bool,
}
/// Address records and transport sockets share the same interface boundary.
pub fn assignments(
    addresses: &[(String, Ipv4Addr, u8)],
    iphone: &str,
    car_up: bool,
) -> Vec<Assignment> {
    let mut grouped: BTreeMap<String, Vec<Ipv4Addr>> = BTreeMap::new();
    for (interface, address, _) in addresses {
        if interface.starts_with("oxpusb")
            || address.is_loopback()
            || address.is_unspecified()
            || address.is_multicast()
        {
            continue;
        }
        if interface == iphone && !car_up {
            continue;
        }
        grouped.entry(interface.clone()).or_default().push(*address);
    }
    grouped
        .into_iter()
        .map(|(interface, mut addresses)| {
            addresses.sort();
            addresses.dedup();
            Assignment {
                car: interface == iphone,
                interface,
                addresses,
            }
        })
        .collect()
}
pub fn service_info(
    addresses: &[Ipv4Addr],
    admin_port: u16,
    receiver_port: u16,
) -> io::Result<ServiceInfo> {
    service_info_for(
        HOST,
        crate::branding::DEVICE_NAME,
        addresses,
        admin_port,
        receiver_port,
    )
}
fn service_info_for(
    host: &str,
    instance: &str,
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
    ServiceInfo::new(SERVICE_TYPE, instance, host, &ips[..], admin_port, txt)
        .map(ServiceInfo::enable_addr_interface_only)
        .map_err(io::Error::other)
}
/// CarPlay and admin discovery borrow the same responder, even across controller restarts.
#[derive(Default)]
pub struct Responders(Mutex<BTreeMap<String, Weak<Responder>>>);
pub struct Responder {
    pub daemon: ServiceDaemon,
    monitor: mdns_sd::Receiver<DaemonEvent>,
    names: Mutex<BTreeMap<String, String>>,
}
impl Responders {
    pub fn acquire(&self, interface: &str, addresses: &[Ipv4Addr]) -> io::Result<Arc<Responder>> {
        let mut responders = self.0.lock().unwrap();
        if let Some(responder) = responders.get(interface).and_then(Weak::upgrade) {
            responder
                .daemon
                .disable_interface(mdns_sd::IfKind::All)
                .map_err(io::Error::other)?;
            for address in addresses {
                responder
                    .daemon
                    .enable_interface(mdns_sd::IfKind::Addr((*address).into()))
                    .map_err(io::Error::other)?;
            }
            return Ok(responder);
        }
        let daemon = ServiceDaemon::new().map_err(io::Error::other)?;
        daemon
            .disable_interface(mdns_sd::IfKind::All)
            .map_err(io::Error::other)?;
        for address in addresses {
            daemon
                .enable_interface(mdns_sd::IfKind::Addr((*address).into()))
                .map_err(io::Error::other)?;
        }
        let monitor = daemon.monitor().map_err(io::Error::other)?;
        let responder = Arc::new(Responder {
            daemon,
            monitor,
            names: Mutex::default(),
        });
        responders.insert(interface.into(), Arc::downgrade(&responder));
        Ok(responder)
    }
}
impl Responder {
    pub fn advertised_name(&self, host: &str) -> String {
        let mut names = self.names.lock().unwrap();
        while let Ok(event) = self.monitor.try_recv() {
            if let DaemonEvent::NameChange(change) = event {
                for name in names.values_mut() {
                    if *name == change.original {
                        *name = change.new_name.clone();
                    }
                }
                names.insert(change.original, change.new_name);
            }
        }
        names
            .entry(host.into())
            .or_insert_with(|| host.into())
            .trim_end_matches('.')
            .into()
    }
}
impl Drop for Responder {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}
struct Announcement {
    assignment: Assignment,
    responder: Arc<Responder>,
    services: Vec<(String, &'static str)>,
}
pub struct MdnsHost {
    responders: Arc<Responders>,
    announcements: Vec<Announcement>,
    admin_port: u16,
    receiver_port: u16,
}
impl MdnsHost {
    pub fn new(
        responders: Arc<Responders>,
        assignments: &[Assignment],
        admin_port: u16,
        receiver_port: u16,
    ) -> io::Result<Self> {
        let mut host = Self {
            responders,
            announcements: Vec::new(),
            admin_port,
            receiver_port,
        };
        host.update(assignments)?;
        Ok(host)
    }
    pub fn update(&mut self, assignments: &[Assignment]) -> io::Result<()> {
        let mut old = std::mem::take(&mut self.announcements);
        for assignment in assignments {
            if let Some(index) = old.iter().position(|a| a.assignment == *assignment) {
                self.announcements.push(old.remove(index));
                continue;
            }
            if let Some(index) = old
                .iter()
                .position(|a| a.assignment.interface == assignment.interface)
            {
                let previous = old.remove(index);
                for (fullname, _) in previous.services {
                    let _ = previous.responder.daemon.unregister(&fullname);
                }
            }
            let responder = self
                .responders
                .acquire(&assignment.interface, &assignment.addresses)?;
            let hosts: &[&str] = if assignment.car {
                &[CAR_HOST]
            } else {
                &[HOST, LEGACY_HOST]
            };
            let mut services = Vec::new();
            for host in hosts {
                let instance = if *host == LEGACY_HOST {
                    "OpenXPlay legacy"
                } else {
                    crate::branding::DEVICE_NAME
                };
                let advertised = format!("{}.", responder.advertised_name(host));
                let info = service_info_for(
                    &advertised,
                    instance,
                    &assignment.addresses,
                    self.admin_port,
                    self.receiver_port,
                )?;
                services.push((info.get_fullname().to_owned(), *host));
                responder.daemon.register(info).map_err(io::Error::other)?;
            }
            self.announcements.push(Announcement {
                assignment: assignment.clone(),
                responder,
                services,
            });
        }
        for announcement in old {
            for (fullname, _) in announcement.services {
                let _ = announcement.responder.daemon.unregister(&fullname);
            }
        }
        Ok(())
    }
    pub fn advertised_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for announcement in self
            .announcements
            .iter()
            .filter(|a| a.assignment.car)
            .chain(self.announcements.iter().filter(|a| !a.assignment.car))
        {
            for (_, host) in &announcement.services {
                if *host != LEGACY_HOST {
                    let name = announcement.responder.advertised_name(host);
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }
    pub fn stop(&mut self) {
        for announcement in self.announcements.drain(..) {
            for (fullname, _) in announcement.services {
                let _ = announcement.responder.daemon.unregister(&fullname);
            }
        }
    }
}
impl Drop for MdnsHost {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_follow_interfaces_and_roles_in_hotspot_and_shared_wifi() {
        let addresses = vec![
            ("wlan0".into(), "10.77.0.1".parse().unwrap(), 24),
            ("wlan1".into(), "10.79.0.1".parse().unwrap(), 24),
            ("eth0".into(), "192.168.5.201".parse().unwrap(), 24),
        ];
        for car_up in [false, true] {
            let assigned = assignments(&addresses, "wlan0", car_up);
            assert_eq!(assigned.len(), if car_up { 3 } else { 2 });
            for assignment in assigned {
                assert_eq!(assignment.car, assignment.interface == "wlan0");
                assert_eq!(assignment.addresses.len(), 1);
            }
        }
        let assigned = assignments(&addresses, "wlan1", true);
        assert!(
            assigned
                .iter()
                .find(|a| a.interface == "wlan1")
                .unwrap()
                .car
        );
        assert!(
            !assigned
                .iter()
                .find(|a| a.interface == "wlan0")
                .unwrap()
                .car
        );
    }
}
