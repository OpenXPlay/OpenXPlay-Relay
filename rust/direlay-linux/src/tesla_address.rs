// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Tesla address policy and live, session-scoped kernel addresses.
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::{net::Ipv4Addr, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Off,
    VirtualName,
    PublicAddress,
    Teslamirror,
    TeslaAndroid,
}
impl Mode {
    pub fn uses_name(self) -> bool {
        matches!(self, Self::VirtualName | Self::Teslamirror)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Activation {
    Off,
    NeedsBrowser,
    NeedsRelayHotspot,
    NeedsName,
    Ready,
}
pub fn activation(mode: Mode, browser: bool, relay_hotspot: bool, name: bool) -> Activation {
    match () {
        _ if mode == Mode::Off => Activation::Off,
        _ if !browser => Activation::NeedsBrowser,
        _ if !relay_hotspot => Activation::NeedsRelayHotspot,
        _ if mode.uses_name() && !name => Activation::NeedsName,
        _ => Activation::Ready,
    }
}
/// Android's numeric dotted-quad policy, including usable link-local and shared space.
pub fn valid(address: &str) -> bool {
    let Ok(ip) = address.parse::<Ipv4Addr>() else {
        return false;
    };
    let [a, b, c, _] = ip.octets();
    !matches!(a, 0 | 10 | 127 | 224..=239)
        && !(a == 169 && b == 254 && !(1..=254).contains(&c))
        && !(a == 172 && (16..=31).contains(&b))
        && !(a == 192 && b == 168)
        && ip != Ipv4Addr::BROADCAST
}
pub fn tls_address(settings: &Settings) -> Option<Ipv4Addr> {
    (!settings.tesla_browser.uses_name())
        .then(|| settings.tesla_address().filter(|a| valid(a))?.parse().ok())
        .flatten()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub mode: Mode,
    pub address: Ipv4Addr,
}
#[derive(Clone)]
struct Input {
    browser: bool,
    hotspot: bool,
    interface: String,
}
impl Input {
    fn new(s: &Settings) -> Self {
        Self {
            browser: s.output != "carlink",
            hotspot: s.network_mode == "device-hotspot",
            interface: s.wifi_interface.clone(),
        }
    }
}
/// The only host operations used by Tesla sessions; addresses include interface and prefix.
pub(crate) trait AddressBackend: Send + Sync {
    fn addresses(&self) -> crate::Result<Vec<(String, Ipv4Addr, u8)>>;
    fn change(&self, interface: &str, address: Ipv4Addr, add: bool) -> crate::Result<()>;
}
pub(crate) struct KernelAddresses;
impl AddressBackend for KernelAddresses {
    fn addresses(&self) -> crate::Result<Vec<(String, Ipv4Addr, u8)>> {
        crate::linux_hotspot::interface_addresses()
    }
    fn change(&self, interface: &str, address: Ipv4Addr, add: bool) -> crate::Result<()> {
        #[cfg(target_os = "linux")]
        {
            netlink::change(interface, address, add)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (interface, address, add);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Tesla addresses require Linux rtnetlink",
            ))
        }
    }
}
/// Captures the running input's network/output choices; Tesla choices always follow live settings.
/// Errors stay here and never propagate into the CarPlay controller.
pub struct Session {
    backend: Arc<dyn AddressBackend>,
    pub active: Option<Address>,
    pub failure: Option<String>,
    pub since_millis: Option<u64>,
    prepared: Option<Input>,
    input: Option<Input>,
    running: bool,
    disabled: bool,
    attempted: bool,
    requested: Option<(Mode, Option<String>)>,
    owned: Option<(String, Ipv4Addr)>,
    journal_state: Option<serde_json::Value>,
    transitions: Vec<serde_json::Value>,
    startup: Option<(String, Vec<Ipv4Addr>)>,
}
impl Session {
    pub(crate) fn new(backend: Arc<dyn AddressBackend>) -> Self {
        Self {
            backend,
            active: None,
            failure: None,
            since_millis: None,
            prepared: None,
            input: None,
            running: false,
            disabled: false,
            attempted: false,
            requested: None,
            owned: None,
            journal_state: None,
            transitions: Vec::new(),
            startup: None,
        }
    }
    /// A bounded, secret-free state record; command output never enters this event.
    pub fn journal_changes(&mut self, settings: &Settings) -> Vec<serde_json::Value> {
        let mut changes = std::mem::take(&mut self.transitions);
        changes.extend(self.journal_change(settings));
        changes
    }
    fn journal_change(&mut self, settings: &Settings) -> Option<serde_json::Value> {
        let phase = if settings.tesla_browser == Mode::Off {
            "OFF"
        } else if self.failure.is_some() {
            "FAILED"
        } else if self.disabled || !self.running {
            "STOPPED"
        } else if self.active.is_some() {
            "READY"
        } else {
            "STARTING"
        };
        let state = serde_json::json!({
            "mode":settings.tesla_browser, "phase":phase, "address":settings.tesla_address(),
            "failureCode":self.failure.as_ref().map(|_| "tesla_address_failed")
        });
        if self.journal_state.as_ref() == Some(&state) {
            return None;
        }
        self.journal_state = Some(state.clone());
        Some(state)
    }
    pub fn prepare(&mut self, settings: &Settings, now: u64) {
        self.stop(now);
        self.running = false;
        self.input = None;
        self.prepared = Some(Input::new(settings));
        self.since_millis = Some(now);
    }
    pub fn activation(&self, settings: &Settings, name: bool) -> Activation {
        let live = Input::new(settings);
        let input = self
            .input
            .as_ref()
            .filter(|_| self.running)
            .unwrap_or(&live);
        activation(settings.tesla_browser, input.browser, input.hotspot, name)
    }
    /// A crashed run can leave only these configured /32s on the hotspot interface.
    pub fn cleanup_stale(&mut self, settings: &Settings) {
        self.startup = Some((
            settings.wifi_interface.clone(),
            [
                &settings.tesla_virtual_address,
                &settings.tesla_public_address,
                &settings.tesla_mirror_address,
                &settings.tesla_android_address,
            ]
            .into_iter()
            .filter(|a| valid(a))
            .filter_map(|a| a.parse().ok())
            .collect(),
        ));
        self.cleanup_startup();
    }
    fn cleanup_startup(&mut self) -> bool {
        let Some((interface, configured)) = &self.startup else {
            return true;
        };
        let result: crate::Result<()> = (|| {
            for (name, address, prefix) in self.backend.addresses()? {
                if &name == interface && prefix == 32 && configured.contains(&address) {
                    self.change(interface, address, false)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.startup = None;
                self.failure = None;
                true
            }
            Err(e) => {
                self.failure = Some(e.to_string());
                false
            }
        }
    }
    fn present(&self, interface: &str, address: Ipv4Addr) -> crate::Result<bool> {
        Ok(self
            .backend
            .addresses()?
            .iter()
            .any(|(name, a, prefix)| name == interface && *a == address && *prefix == 32))
    }
    fn change(&self, interface: &str, address: Ipv4Addr, add: bool) -> crate::Result<()> {
        match self.backend.change(interface, address, add) {
            // An add racing with a reapply still needs the presence check below.
            Err(e) if add && e.raw_os_error() == Some(libc::EEXIST) => Ok(()),
            Err(e)
                if !add
                    && matches!(
                        e.raw_os_error(),
                        Some(libc::ENODEV | libc::EADDRNOTAVAIL | libc::ENOENT)
                    ) =>
            {
                Ok(())
            }
            result => result,
        }
    }
    fn remove(&mut self) -> bool {
        self.active = None;
        if let Some((interface, address)) = &self.owned {
            if let Err(e) = self.change(interface, *address, false) {
                self.failure = Some(e.to_string());
                return false;
            }
        }
        self.owned = None;
        true
    }
    pub fn stop(&mut self, now: u64) {
        let before = (self.active.clone(), self.failure.clone());
        self.prepared = None;
        self.disabled = true;
        self.failure = None;
        self.requested = None;
        self.remove();
        if before != (self.active.clone(), self.failure.clone()) {
            self.since_millis = Some(now);
        }
    }
    pub fn tick(
        &mut self,
        running: bool,
        hotspot_ready: bool,
        settings: &Settings,
        name: bool,
        addresses: impl FnOnce() -> crate::Result<Vec<Ipv4Addr>>,
        now: u64,
    ) {
        let before = (self.active.clone(), self.failure.clone());
        let transitions = self.transitions.len();
        self.tick_inner(running, hotspot_ready, settings, name, addresses);
        if before != (self.active.clone(), self.failure.clone())
            || self.transitions.len() != transitions
        {
            self.since_millis = Some(now);
        }
    }
    fn tick_inner(
        &mut self,
        running: bool,
        hotspot_ready: bool,
        settings: &Settings,
        name: bool,
        addresses: impl FnOnce() -> crate::Result<Vec<Ipv4Addr>>,
    ) {
        if !self.cleanup_startup() {
            return;
        }
        if !running {
            if self.running || self.owned.is_some() {
                self.stop(0);
                self.input = None;
            }
            self.running = false;
            return;
        }
        if !self.running {
            self.input = Some(self.prepared.take().unwrap_or_else(|| Input::new(settings)));
            self.disabled = false;
            self.attempted = false;
            self.failure = None;
            self.running = true;
        }
        if self.disabled {
            self.remove();
            return;
        }
        let previous = self.active.as_ref().map(|a| a.address);
        let next = (
            settings.tesla_browser,
            settings.tesla_address().map(str::to_owned),
        );
        if self.requested.as_ref() != Some(&next) {
            if !self.remove() {
                return;
            }
            self.failure = None;
            self.attempted = false;
            self.requested = Some(next);
        }
        let input = self.input.as_ref().unwrap().clone();
        if activation(settings.tesla_browser, input.browser, input.hotspot, name)
            != Activation::Ready
            || !hotspot_ready
        {
            if self.owned.is_some() && self.remove() {
                self.attempted = false;
            }
            return;
        }
        if let Some(active) = &self.active {
            match self.present(&input.interface, active.address) {
                Ok(true) => return,
                Ok(false) => {
                    self.active = None;
                    self.failure = None;
                    self.attempted = false;
                    // Preserve both transitions even when the reinstall succeeds in this tick.
                    if let Some(change) = self.journal_change(settings) {
                        self.transitions.push(change);
                    }
                }
                Err(e) => {
                    self.failure = Some(e.to_string());
                    self.remove();
                    return;
                }
            }
        }
        if self.attempted {
            return;
        }
        self.attempted = true;
        let result = (|| {
            let text = settings.tesla_address().unwrap();
            let address = text.parse::<Ipv4Addr>().map_err(std::io::Error::other)?;
            if !valid(text)
                || addresses()?
                    .iter()
                    .any(|a| *a == address && Some(*a) != previous)
            {
                return Err(std::io::Error::other(format!(
                    "Invalid or already assigned Tesla address: {text}"
                )));
            }
            // Keep ownership on an ambiguous timeout, so cleanup still removes an applied address.
            self.owned = Some((input.interface.clone(), address));
            self.change(&input.interface, address, true)?;
            if !self.present(&input.interface, address)? {
                return Err(std::io::Error::other(format!(
                    "Tesla address is missing after add: {address}/32 on {}",
                    input.interface
                )));
            }
            self.failure = None;
            self.active = Some(Address {
                mode: settings.tesla_browser,
                address,
            });
            Ok(())
        })();
        if let Err(e) = result {
            self.failure = Some(e.to_string());
            self.remove();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cleanup_startup();
        self.remove();
    }
}

pub fn address_key(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::Off => None,
        Mode::VirtualName => Some("teslaVirtualAddress"),
        Mode::PublicAddress => Some("teslaPublicAddress"),
        Mode::Teslamirror => Some("teslaMirrorAddress"),
        Mode::TeslaAndroid => Some("teslaAndroidAddress"),
    }
}

// Native-endian UAPI headers, network-order IPv4 payloads, and four-byte aligned attributes.
#[cfg(any(target_os = "linux", test))]
mod netlink {
    use std::{io, net::Ipv4Addr};

    fn request(index: u32, address: Ipv4Addr, add: bool) -> [u8; 40] {
        let mut bytes = [0; 40];
        bytes[..4].copy_from_slice(&40_u32.to_ne_bytes());
        // RTM_NEWADDR / RTM_DELADDR, NLM_F_REQUEST | NLM_F_ACK, and CREATE | EXCL on add.
        bytes[4..6].copy_from_slice(&(if add { 20_u16 } else { 21_u16 }).to_ne_bytes());
        bytes[6..8].copy_from_slice(&(if add { 0x605_u16 } else { 5_u16 }).to_ne_bytes());
        bytes[8..12].copy_from_slice(&1_u32.to_ne_bytes());
        // ifaddrmsg: AF_INET, /32, no flags, RT_SCOPE_UNIVERSE, interface index.
        bytes[16] = libc::AF_INET as u8;
        bytes[17] = 32;
        bytes[20..24].copy_from_slice(&index.to_ne_bytes());
        for (offset, kind) in [(24, 2_u16), (32, 1_u16)] {
            bytes[offset..offset + 2].copy_from_slice(&8_u16.to_ne_bytes());
            // IFA_LOCAL and IFA_ADDRESS both describe the local host address.
            bytes[offset + 2..offset + 4].copy_from_slice(&kind.to_ne_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&address.octets());
        }
        bytes
    }
    fn acknowledgement(bytes: &[u8]) -> io::Result<()> {
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid Tesla rtnetlink ACK");
        if bytes.len() < 20 {
            return Err(invalid());
        }
        let length = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as usize;
        // NLMSG_ERROR contains a signed errno (zero is a successful ACK).
        if length < 20
            || length > bytes.len()
            || u16::from_ne_bytes(bytes[4..6].try_into().unwrap()) != 2
            || u32::from_ne_bytes(bytes[8..12].try_into().unwrap()) != 1
        {
            return Err(invalid());
        }
        match i32::from_ne_bytes(bytes[16..20].try_into().unwrap()) {
            0 => Ok(()),
            code if code < 0 => Err(io::Error::from_raw_os_error(
                code.checked_neg().ok_or_else(invalid)?,
            )),
            _ => Err(invalid()),
        }
    }
    #[cfg(target_os = "linux")]
    fn socket() -> io::Result<std::os::fd::OwnedFd> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let raw = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                libc::NETLINK_ROUTE,
            )
        };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // OwnedFd closes every error path as well as a completed transaction.
        let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
        let timeout = libc::timeval {
            tv_sec: 15,
            tv_usec: 0,
        };
        for option in [libc::SO_RCVTIMEO, libc::SO_SNDTIMEO] {
            if unsafe {
                libc::setsockopt(
                    fd.as_raw_fd(),
                    libc::SOL_SOCKET,
                    option,
                    (&timeout as *const libc::timeval).cast(),
                    std::mem::size_of_val(&timeout) as libc::socklen_t,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        let mut kernel: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        kernel.nl_family = libc::AF_NETLINK as libc::sa_family_t;
        // Port id zero addresses the kernel; connect also binds a private local port id.
        if unsafe {
            libc::connect(
                fd.as_raw_fd(),
                (&kernel as *const libc::sockaddr_nl).cast(),
                std::mem::size_of_val(&kernel) as libc::socklen_t,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(fd)
    }
    #[cfg(target_os = "linux")]
    pub(super) fn change(interface: &str, address: Ipv4Addr, add: bool) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        let interface = std::ffi::CString::new(interface)?;
        let index = unsafe { libc::if_nametoindex(interface.as_ptr()) };
        if index == 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = socket()?;
        let bytes = request(index, address, add);
        let sent = unsafe { libc::send(fd.as_raw_fd(), bytes.as_ptr().cast(), bytes.len(), 0) };
        if sent < 0 {
            return Err(io::Error::last_os_error());
        }
        if sent as usize != bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Incomplete Tesla rtnetlink request",
            ));
        }
        let mut reply = [0_u8; 4096];
        let mut sender: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        let mut length = std::mem::size_of_val(&sender) as libc::socklen_t;
        let received = unsafe {
            libc::recvfrom(
                fd.as_raw_fd(),
                reply.as_mut_ptr().cast(),
                reply.len(),
                libc::MSG_TRUNC,
                (&mut sender as *mut libc::sockaddr_nl).cast(),
                &mut length,
            )
        };
        if received < 0 {
            return Err(io::Error::last_os_error());
        }
        if received as usize > reply.len()
            || length as usize != std::mem::size_of_val(&sender)
            || sender.nl_family != libc::AF_NETLINK as libc::sa_family_t
            || sender.nl_pid != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid Tesla rtnetlink sender or length",
            ));
        }
        acknowledgement(&reply[..received as usize])
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn address_packets_target_only_a_global_host_on_the_selected_index() {
            for add in [true, false] {
                let bytes = request(37, "3.3.3.1".parse().unwrap(), add);
                assert_eq!(u32::from_ne_bytes(bytes[..4].try_into().unwrap()), 40);
                assert_eq!(
                    u16::from_ne_bytes(bytes[4..6].try_into().unwrap()),
                    if add { 20 } else { 21 }
                );
                assert_eq!(
                    u16::from_ne_bytes(bytes[6..8].try_into().unwrap()),
                    if add { 0x605 } else { 5 }
                );
                assert_eq!(&bytes[16..20], &[libc::AF_INET as u8, 32, 0, 0]);
                assert_eq!(u32::from_ne_bytes(bytes[20..24].try_into().unwrap()), 37);
                for (offset, kind) in [(24, 2), (32, 1)] {
                    assert_eq!(
                        u16::from_ne_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                        8
                    );
                    assert_eq!(
                        u16::from_ne_bytes(bytes[offset + 2..offset + 4].try_into().unwrap()),
                        kind
                    );
                    assert_eq!(&bytes[offset + 4..offset + 8], &[3, 3, 3, 1]);
                }
            }
        }
        #[test]
        fn acknowledgements_preserve_errno_and_reject_malformed_or_unrelated_messages() {
            let mut ack = [0; 36];
            ack[..4].copy_from_slice(&36_u32.to_ne_bytes());
            ack[4..6].copy_from_slice(&2_u16.to_ne_bytes());
            ack[8..12].copy_from_slice(&1_u32.to_ne_bytes());
            assert!(acknowledgement(&ack).is_ok());
            for code in [
                libc::EEXIST,
                libc::ENODEV,
                libc::EPERM,
                libc::EACCES,
                libc::EADDRNOTAVAIL,
            ] {
                ack[16..20].copy_from_slice(&(-code).to_ne_bytes());
                assert_eq!(
                    acknowledgement(&ack).unwrap_err().raw_os_error(),
                    Some(code)
                );
            }
            for end in 0..ack.len() {
                assert_eq!(
                    acknowledgement(&ack[..end]).unwrap_err().kind(),
                    io::ErrorKind::InvalidData
                );
            }
            ack[16..20].copy_from_slice(&i32::MIN.to_ne_bytes());
            assert!(acknowledgement(&ack).is_err());
            ack[16..20].copy_from_slice(&1_i32.to_ne_bytes());
            assert!(acknowledgement(&ack).is_err());
            ack[16..20].fill(0);
            ack[8..12].fill(0);
            assert!(acknowledgement(&ack).is_err());
            ack[8..12].copy_from_slice(&1_u32.to_ne_bytes());
            ack[4..6].fill(0);
            assert!(acknowledgement(&ack).is_err());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub(crate) struct FakeBackend {
        rows: Mutex<Vec<(String, Ipv4Addr, u8)>>,
        calls: Mutex<Vec<(String, Ipv4Addr, bool)>>,
        add_error: Mutex<Option<i32>>,
        delete_error: Mutex<Option<i32>>,
        suppress_add: Mutex<bool>,
        read_error: Mutex<bool>,
    }
    impl AddressBackend for FakeBackend {
        fn addresses(&self) -> crate::Result<Vec<(String, Ipv4Addr, u8)>> {
            if *self.read_error.lock().unwrap() {
                return Err(std::io::Error::other("address enumeration failed"));
            }
            Ok(self.rows.lock().unwrap().clone())
        }
        fn change(&self, interface: &str, address: Ipv4Addr, add: bool) -> crate::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push((interface.into(), address, add));
            if add && !*self.suppress_add.lock().unwrap() {
                self.rows
                    .lock()
                    .unwrap()
                    .push((interface.into(), address, 32));
            }
            if let Some(code) = *(if add {
                &self.add_error
            } else {
                &self.delete_error
            })
            .lock()
            .unwrap()
            {
                return Err(std::io::Error::from_raw_os_error(code));
            }
            if !add {
                self.rows.lock().unwrap().retain(|(name, a, prefix)| {
                    name != interface || *a != address || *prefix != 32
                });
            }
            Ok(())
        }
    }
    fn settings(mode: Mode) -> Settings {
        Settings {
            tesla_browser: mode,
            ..Default::default()
        }
    }
    fn tick(s: &mut Session, settings: &Settings) {
        s.tick(true, true, settings, true, || Ok(vec![]), 100);
    }
    #[test]
    fn tesla_defaults_policy_and_settings_round_trip() {
        for (mode, address) in [
            (Mode::VirtualName, "169.254.88.1"),
            (Mode::PublicAddress, "169.254.88.1"),
            (Mode::Teslamirror, "100.88.0.1"),
            (Mode::TeslaAndroid, "3.3.3.1"),
        ] {
            let s = settings(mode);
            assert_eq!(s.tesla_address(), Some(address));
            assert!(s.validate().is_ok());
            assert_eq!(
                serde_json::from_value::<Settings>(serde_json::to_value(&s).unwrap()).unwrap(),
                s
            );
            assert_eq!(
                tls_address(&s),
                (!mode.uses_name()).then(|| address.parse().unwrap())
            );
        }
        assert_eq!(Settings::default().tesla_browser, Mode::Off);
        assert_eq!(
            serde_json::from_str::<Settings>("{}")
                .unwrap()
                .tesla_address(),
            None
        );
        for ip in [
            "169.254.1.0",
            "169.254.254.255",
            "100.88.0.1",
            "3.3.3.1",
            "240.3.3.3",
            "255.255.255.254",
        ] {
            assert!(valid(ip), "{ip}");
        }
        for ip in [
            "",
            "localhost",
            "1.2.3",
            "01.2.3.4",
            "1.2.3.256",
            "1.2.3.4 ",
            "0.1.2.3",
            "10.1.2.3",
            "127.0.0.1",
            "169.254.0.1",
            "169.254.255.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "224.0.0.1",
            "239.255.1.2",
            "255.255.255.255",
        ] {
            assert!(!valid(ip), "{ip}");
        }
        let mut off = settings(Mode::Off);
        off.tesla_public_address = "parked edit".into();
        assert!(off.validate().is_ok());
        off.tesla_browser = Mode::PublicAddress;
        assert!(off.validate().is_err());
    }
    #[test]
    fn tesla_activation_priority_for_every_mode_and_output() {
        for mode in [
            Mode::Off,
            Mode::VirtualName,
            Mode::PublicAddress,
            Mode::Teslamirror,
            Mode::TeslaAndroid,
        ] {
            for browser in [false, true] {
                for hotspot in [false, true] {
                    for name in [false, true] {
                        let expected = if mode == Mode::Off {
                            Activation::Off
                        } else if !browser {
                            Activation::NeedsBrowser
                        } else if !hotspot {
                            Activation::NeedsRelayHotspot
                        } else if mode.uses_name() && !name {
                            Activation::NeedsName
                        } else {
                            Activation::Ready
                        };
                        assert_eq!(activation(mode, browser, hotspot, name), expected);
                    }
                }
            }
        }
    }
    #[test]
    fn live_switches_keep_captured_input_and_remove_only_owned_addresses() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let off = settings(Mode::Off);
        session.prepare(&off, 1);
        tick(&mut session, &off);
        let mut live = settings(Mode::PublicAddress);
        tick(&mut session, &live);
        for mode in [
            Mode::VirtualName,
            Mode::Teslamirror,
            Mode::TeslaAndroid,
            Mode::PublicAddress,
        ] {
            live.tesla_browser = mode;
            live.output = "carlink".into();
            live.network_mode = "shared-wifi".into();
            live.wifi_interface = "wlan9".into();
            let previous = session.active.as_ref().unwrap().address;
            session.tick(true, true, &live, true, || Ok(vec![previous]), 20);
            assert_eq!(session.active.as_ref().unwrap().mode, mode);
            assert!(session.failure.is_none());
            tick(&mut session, &live);
        }
        live.tesla_public_address = "169.254.88.2".into();
        tick(&mut session, &live);
        live.tesla_mirror_address = "100.88.0.2".into();
        tick(&mut session, &live);
        tick(&mut session, &off);
        assert!(session.active.is_none());
        tick(&mut session, &live);
        assert!(session.active.is_some());
        session.stop(30);
        tick(&mut session, &live);
        assert!(session.active.is_none());
        assert!(backend.rows.lock().unwrap().is_empty());
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 14);
        for pair in calls.as_chunks::<2>().0 {
            assert_eq!(pair[0].0, "wlan0");
            assert_eq!(pair[0].0, pair[1].0);
            assert_eq!(pair[0].1, pair[1].1);
            assert!(pair[0].2);
            assert!(!pair[1].2);
        }
    }
    #[test]
    fn waits_for_hotspot_and_name_then_stops_with_input() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::Teslamirror);
        session.tick(false, false, &live, true, || panic!("not starting"), 1);
        session.tick(true, false, &live, true, || panic!("no AP"), 2);
        session.tick(true, true, &live, false, || panic!("no name"), 3);
        tick(&mut session, &live);
        assert!(session.active.is_some());
        session.tick(false, false, &live, true, || panic!("stopping"), 200);
        assert!(session.active.is_none());
        assert_eq!(session.since_millis, Some(200));
        assert_eq!(backend.calls.lock().unwrap().len(), 2);
        for output in ["web", "both", "carlink"] {
            for network in ["shared-wifi", "device-hotspot"] {
                let mut value = settings(Mode::PublicAddress);
                value.output = output.into();
                value.network_mode = network.into();
                session.prepare(&value, 201);
                tick(&mut session, &value);
                assert_eq!(
                    session.active.is_some(),
                    output != "carlink" && network == "device-hotspot"
                );
            }
        }
        session.stop(300);
        assert!(backend.rows.lock().unwrap().is_empty());
    }
    #[test]
    fn ready_requires_a_present_host_address_on_the_selected_interface() {
        for row in [None, Some(("wlan9", 32)), Some(("wlan0", 24))] {
            let backend = Arc::new(FakeBackend::default());
            *backend.suppress_add.lock().unwrap() = true;
            if let Some((name, prefix)) = row {
                backend.rows.lock().unwrap().push((
                    name.into(),
                    "3.3.3.1".parse().unwrap(),
                    prefix,
                ));
            }
            let mut session = Session::new(backend.clone());
            let live = settings(Mode::TeslaAndroid);
            tick(&mut session, &live);
            assert!(session.active.is_none());
            assert!(session
                .failure
                .as_ref()
                .unwrap()
                .contains("missing after add"));
            assert_eq!(session.journal_changes(&live)[0]["phase"], "FAILED");
            tick(&mut session, &live);
            assert_eq!(
                backend.calls.lock().unwrap().len(),
                2,
                "one add and cleanup, then latched failure"
            );
        }
    }
    #[test]
    fn present_address_is_checked_each_tick_without_readding() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::TeslaAndroid);
        tick(&mut session, &live);
        assert_eq!(session.journal_changes(&live)[0]["phase"], "READY");
        session.tick(true, true, &live, true, || panic!("already active"), 200);
        assert_eq!(backend.calls.lock().unwrap().len(), 1);
        assert!(session.journal_changes(&live).is_empty());
        assert_eq!(session.since_millis, Some(100));
    }
    #[test]
    fn flushed_address_is_reinstalled_and_both_transitions_are_journaled() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::TeslaAndroid);
        tick(&mut session, &live);
        assert_eq!(session.journal_changes(&live)[0]["phase"], "READY");
        backend.rows.lock().unwrap().clear();
        session.tick(true, true, &live, true, || Ok(vec![]), 200);
        assert!(session.active.is_some());
        assert_eq!(session.since_millis, Some(200));
        assert_eq!(backend.calls.lock().unwrap().len(), 2);
        assert_eq!(backend.rows.lock().unwrap().len(), 1);
        let events = session.journal_changes(&live);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["phase"], "STARTING");
        assert_eq!(events[1]["phase"], "READY");
        assert!(session.journal_changes(&live).is_empty());
    }
    #[test]
    fn failed_reinstall_removes_ready_and_journals_failure() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::TeslaAndroid);
        tick(&mut session, &live);
        session.journal_changes(&live);
        backend.rows.lock().unwrap().clear();
        *backend.suppress_add.lock().unwrap() = true;
        tick(&mut session, &live);
        assert!(session.active.is_none());
        let events = session.journal_changes(&live);
        assert_eq!(events[0]["phase"], "STARTING");
        assert_eq!(events[1]["phase"], "FAILED");
    }
    #[test]
    fn eexist_is_accepted_only_after_verification() {
        for present in [true, false] {
            let backend = Arc::new(FakeBackend::default());
            *backend.add_error.lock().unwrap() = Some(libc::EEXIST);
            *backend.suppress_add.lock().unwrap() = !present;
            let mut session = Session::new(backend);
            tick(&mut session, &settings(Mode::TeslaAndroid));
            assert_eq!(session.active.is_some(), present);
            assert_eq!(session.failure.is_none(), present);
        }
    }
    #[test]
    fn enodev_and_permission_failures_are_isolated_and_latched_until_switch() {
        for code in [libc::ENODEV, libc::EPERM, libc::EACCES] {
            let backend = Arc::new(FakeBackend::default());
            *backend.suppress_add.lock().unwrap() = true;
            *backend.add_error.lock().unwrap() = Some(code);
            let mut session = Session::new(backend.clone());
            let live = settings(Mode::TeslaAndroid);
            tick(&mut session, &live);
            assert!(session.active.is_none());
            assert_eq!(
                session.failure.as_deref(),
                Some(std::io::Error::from_raw_os_error(code).to_string().as_str())
            );
            tick(&mut session, &live);
            assert_eq!(backend.calls.lock().unwrap().len(), 2);
            *backend.add_error.lock().unwrap() = None;
            *backend.suppress_add.lock().unwrap() = false;
            tick(&mut session, &settings(Mode::PublicAddress));
            assert!(session.active.is_some());
            assert!(session.failure.is_none());
        }
    }
    #[test]
    fn existing_address_collision_is_not_claimed_or_deleted() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::TeslaAndroid);
        session.tick(
            true,
            true,
            &live,
            true,
            || Ok(vec!["3.3.3.1".parse().unwrap()]),
            1,
        );
        assert!(session
            .failure
            .as_ref()
            .unwrap()
            .contains("already assigned"));
        tick(&mut session, &live);
        session.stop(2);
        assert!(backend.calls.lock().unwrap().is_empty());
    }
    #[test]
    fn enumeration_failure_never_leaves_ready() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::TeslaAndroid);
        tick(&mut session, &live);
        *backend.read_error.lock().unwrap() = true;
        tick(&mut session, &live);
        assert!(session.active.is_none());
        assert_eq!(
            session.failure.as_deref(),
            Some("address enumeration failed")
        );
        assert!(backend.rows.lock().unwrap().is_empty());
    }
    #[test]
    fn cleanup_accepts_an_already_absent_address_or_interface() {
        for code in [libc::EADDRNOTAVAIL, libc::ENODEV, libc::ENOENT] {
            let backend = Arc::new(FakeBackend::default());
            let mut session = Session::new(backend.clone());
            tick(&mut session, &settings(Mode::TeslaAndroid));
            backend.rows.lock().unwrap().clear();
            *backend.delete_error.lock().unwrap() = Some(code);
            session.stop(2);
            assert!(session.owned.is_none());
            assert!(session.failure.is_none());
        }
    }
    #[test]
    fn failed_cleanup_is_retained_and_retried_while_stopped() {
        let backend = Arc::new(FakeBackend::default());
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::TeslaAndroid);
        tick(&mut session, &live);
        *backend.delete_error.lock().unwrap() = Some(libc::EPERM);
        session.stop(2);
        assert!(session.owned.is_some());
        assert!(session.failure.is_some());
        *backend.delete_error.lock().unwrap() = None;
        session.tick(false, false, &live, true, || panic!("stopped"), 3);
        assert!(session.owned.is_none());
        assert!(session.failure.is_none());
        assert!(backend.rows.lock().unwrap().is_empty());
    }
    #[test]
    fn drop_removes_the_owned_host_address() {
        let backend = Arc::new(FakeBackend::default());
        {
            let mut session = Session::new(backend.clone());
            tick(&mut session, &settings(Mode::TeslaAndroid));
            assert_eq!(backend.rows.lock().unwrap().len(), 1);
        }
        assert!(backend.rows.lock().unwrap().is_empty());
        assert_eq!(backend.calls.lock().unwrap().len(), 2);
    }
    #[test]
    fn startup_removes_only_configured_host_addresses_on_the_hotspot_even_when_off() {
        let backend = Arc::new(FakeBackend::default());
        let mut live = settings(Mode::Off);
        live.tesla_virtual_address = "169.254.88.2".into();
        let row =
            |name: &str, address: &str, prefix| (name.to_owned(), address.parse().unwrap(), prefix);
        let preserved = vec![
            row("wlan0", "10.77.0.1", 24),
            row("wlan1", "3.3.3.1", 32),
            row("wlan0", "3.3.3.1", 24),
            row("wlan0", "3.3.3.2", 32),
            row("wlan0", "169.254.88.1", 16),
        ];
        let mut rows = preserved.clone();
        for address in ["169.254.88.2", "169.254.88.1", "100.88.0.1", "3.3.3.1"] {
            rows.push(row("wlan0", address, 32));
        }
        *backend.rows.lock().unwrap() = rows;
        let mut session = Session::new(backend.clone());
        session.cleanup_stale(&live);
        assert_eq!(*backend.rows.lock().unwrap(), preserved);
        assert_eq!(backend.calls.lock().unwrap().len(), 4);
        assert!(session.startup.is_none());
        assert!(session.active.is_none());
        assert_eq!(session.journal_changes(&live)[0]["phase"], "OFF");
    }
    #[test]
    fn startup_cleanup_retries_without_starting_a_session() {
        let backend = Arc::new(FakeBackend::default());
        backend
            .rows
            .lock()
            .unwrap()
            .push(("wlan0".into(), "3.3.3.1".parse().unwrap(), 32));
        *backend.delete_error.lock().unwrap() = Some(libc::EPERM);
        let mut session = Session::new(backend.clone());
        let live = settings(Mode::TeslaAndroid);
        session.cleanup_stale(&live);
        assert!(session.failure.is_some());
        assert!(session.startup.is_some());
        session.tick(false, false, &live, true, || panic!("cleanup only"), 1);
        assert!(!session.running);
        *backend.delete_error.lock().unwrap() = None;
        session.tick(false, false, &live, true, || panic!("cleanup only"), 2);
        assert!(session.failure.is_none());
        assert!(session.startup.is_none());
        assert!(backend.rows.lock().unwrap().is_empty());
        tick(&mut session, &live);
        assert!(session.active.is_some());
    }
}
