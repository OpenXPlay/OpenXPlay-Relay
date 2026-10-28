// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-23

use crate::{
    platform::{CommandRunner, ProcessCommandRunner},
    public_name::local_dns,
};
use std::{
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
};

pub type InterfaceAddresses =
    Arc<dyn Fn() -> crate::Result<Vec<(String, Ipv4Addr, u8)>> + Send + Sync>;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InventoryRefresh {
    Manual,
    OnDemand,
    Automatic,
}
#[derive(Clone)]
pub struct RuntimePaths {
    pub keyfiles: PathBuf,
    pub local_dns: local_dns::Paths,
    pub network_services: bool,
    pub inventory_refresh: InventoryRefresh,
    pub root: PathBuf,
    pub runner: Arc<dyn CommandRunner>,
    pub addresses: InterfaceAddresses,
    pub(crate) tesla_address: crate::tesla_address::Ownership,
}
impl RuntimePaths {
    pub fn new(state_dir: &Path) -> Self {
        let mut paths = Self::in_state_dir(state_dir, Arc::new(ProcessCommandRunner::new()));
        paths.root = "/".into();
        paths.addresses = Arc::new(crate::linux_hotspot::interface_addresses);
        paths.network_services = cfg!(target_os = "linux");
        paths.inventory_refresh = if cfg!(target_os = "linux") {
            InventoryRefresh::Automatic
        } else {
            InventoryRefresh::OnDemand
        };
        #[cfg(target_os = "linux")]
        {
            paths.keyfiles = "/run/NetworkManager/system-connections".into();
            paths.local_dns = local_dns::Paths {
                dir: local_dns::DIR.into(),
                hosts: local_dns::HOSTS.into(),
                reload: true,
            };
        }
        paths
    }
    pub fn in_state_dir(state_dir: &Path, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            keyfiles: state_dir.join("nm-keyfiles"),
            local_dns: local_dns::Paths::in_dir(state_dir.join("dnsmasq-shared.d")),
            network_services: true,
            inventory_refresh: InventoryRefresh::Automatic,
            root: state_dir.join("host"),
            runner,
            addresses: Arc::new(|| Ok(Vec::new())),
            tesla_address: Default::default(),
        }
    }
    /// Network topology excludes the Tesla session's auxiliary address. Collision and kernel
    /// presence checks must use `addresses` directly so ownership cannot hide a real conflict.
    pub fn network_addresses(&self) -> crate::Result<Vec<(String, Ipv4Addr, u8)>> {
        self.tesla_address.network_addresses(&self.addresses)
    }
    pub fn network_address_source(&self) -> InterfaceAddresses {
        let paths = self.clone();
        Arc::new(move || paths.network_addresses())
    }
    pub fn managed_dns(&self) -> Option<local_dns::Paths> {
        self.network_services.then(|| self.local_dns.clone())
    }
}
