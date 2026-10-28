// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-28

//! Kernel Ethernet/IPv6 for the userspace USB link. No USB protocol lives here.
use crate::platform::CommandRunner;
use std::{
    io,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const TAP_NAME: &str = "oxpusb0";
pub const MTU: usize = 1500;
pub const MAX_FRAME: usize = MTU + 14;
const IO_TIMEOUT: Duration = Duration::from_millis(100);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

/// One Ethernet frame per operation. Implementations must honor timeouts and close must
/// promptly wake concurrent operations. A receive timeout returns None, never a partial frame.
pub trait EthernetPort: Send + Sync {
    fn send_frame(&self, frame: &[u8], timeout: Duration) -> io::Result<()>;
    fn receive_frame(&self, timeout: Duration) -> io::Result<Option<Vec<u8>>>;
    fn close(&self) -> io::Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TapSetting {
    Mac([u8; 6]),
    Mtu(u32),
    Ipv6(u32),
    Up,
}
trait TapIo: EthernetPort {
    fn configure(&self, setting: TapSetting) -> io::Result<()>;
    fn index(&self) -> io::Result<u32>;
}
trait TapDriver {
    fn open(&self, name: &str) -> io::Result<Arc<dyn TapIo>>;
    fn write_sysctl(&self, path: &Path, value: &str) -> io::Result<()>;
}

pub struct TapDevice {
    io: Arc<dyn TapIo>,
    index: u32,
}
impl TapDevice {
    pub fn create(mac: [u8; 6], runner: &dyn CommandRunner) -> io::Result<Self> {
        Self::create_with_proc_root(mac, Path::new("/proc/sys"), runner)
    }
    pub fn create_with_proc_root(
        mac: [u8; 6],
        proc_root: &Path,
        runner: &dyn CommandRunner,
    ) -> io::Result<Self> {
        Self::create_with(&SystemDriver, mac, proc_root, runner)
    }
    fn create_with(
        driver: &dyn TapDriver,
        mac: [u8; 6],
        proc_root: &Path,
        runner: &dyn CommandRunner,
    ) -> io::Result<Self> {
        let io = driver.open(TAP_NAME)?;
        let setup = (|| {
            unmanage(runner)?;
            io.configure(TapSetting::Mac(mac))?;
            io.configure(TapSetting::Mtu(MTU as u32))?;
            for (key, value) in [
                ("addr_gen_mode", "1"),
                ("accept_dad", "0"),
                ("accept_ra", "0"),
                ("disable_ipv6", "0"),
            ] {
                driver.write_sysctl(
                    &proc_root.join("net/ipv6/conf").join(TAP_NAME).join(key),
                    value,
                )?;
            }
            let index = io.index()?;
            io.configure(TapSetting::Ipv6(index))?;
            io.configure(TapSetting::Up)?;
            Ok(index)
        })();
        match setup {
            Ok(index) => Ok(Self { io, index }),
            Err(error) => {
                let _ = io.close();
                Err(error)
            }
        }
    }
    pub fn name(&self) -> &str {
        TAP_NAME
    }
    pub fn index(&self) -> u32 {
        self.index
    }
    pub fn close(&self) -> io::Result<()> {
        self.io.close()
    }
}
impl EthernetPort for TapDevice {
    fn send_frame(&self, frame: &[u8], timeout: Duration) -> io::Result<()> {
        self.io.send_frame(frame, timeout)
    }
    fn receive_frame(&self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        self.io.receive_frame(timeout)
    }
    fn close(&self) -> io::Result<()> {
        self.close()
    }
}
impl Drop for TapDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
fn unmanage(runner: &dyn CommandRunner) -> io::Result<()> {
    let run = |args: &[&str]| {
        runner.run(
            &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            Duration::from_secs(3),
        )
    };
    let state = match run(&[
        "nmcli",
        "-g",
        "GENERAL.NM-MANAGED",
        "device",
        "show",
        TAP_NAME,
    ]) {
        Ok(state) => state,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if state.exit_code == 0 && state.output.trim() == "yes" {
        run(&["nmcli", "device", "set", TAP_NAME, "managed", "no"])?
            .checked("Unmanage wired TAP")?;
    }
    Ok(())
}

/// Two direct loops, with at most one frame in flight per direction and no frame queue.
/// The failure callback runs once on a worker; it should notify the controller and return.
pub struct FramePump {
    stopped: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    tap: Arc<dyn EthernetPort>,
    port: Arc<dyn EthernetPort>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}
impl FramePump {
    pub fn start(
        tap: Arc<TapDevice>,
        port: Arc<dyn EthernetPort>,
        failure: Arc<dyn Fn(io::Error) + Send + Sync>,
    ) -> io::Result<Self> {
        Self::start_ports(tap, port, failure)
    }
    fn start_ports(
        tap: Arc<dyn EthernetPort>,
        port: Arc<dyn EthernetPort>,
        failure: Arc<dyn Fn(io::Error) + Send + Sync>,
    ) -> io::Result<Self> {
        let pump = Self {
            stopped: Arc::new(AtomicBool::new(false)),
            dropped: Arc::new(AtomicU64::new(0)),
            tap: tap.clone(),
            port: port.clone(),
            workers: Mutex::default(),
        };
        for (name, source, target) in [
            ("usb-to-tap", port.clone(), tap.clone()),
            ("tap-to-usb", tap, port),
        ] {
            let stopped = pump.stopped.clone();
            let dropped = pump.dropped.clone();
            let failure = failure.clone();
            let worker = crate::platform::spawn_worker(
                name,
                Box::new(move || {
                    let result = (|| -> io::Result<()> {
                        while !stopped.load(Ordering::Acquire) {
                            if let Some(frame) = source.receive_frame(IO_TIMEOUT)? {
                                if frame.len() > MAX_FRAME {
                                    dropped.fetch_add(1, Ordering::Relaxed);
                                } else if !stopped.load(Ordering::Acquire) {
                                    target.send_frame(&frame, IO_TIMEOUT)?;
                                }
                            }
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        if !stopped.swap(true, Ordering::AcqRel) {
                            let _ = source.close();
                            let _ = target.close();
                            failure(error);
                        }
                    }
                }),
            );
            match worker {
                Ok(worker) => pump.workers.lock().unwrap().push(worker),
                Err(error) => {
                    let _ = pump.close();
                    return Err(error);
                }
            }
        }
        Ok(pump)
    }
    pub fn dropped_oversize(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn close(&self) -> io::Result<()> {
        self.close_with_timeout(CLOSE_TIMEOUT)
    }
    fn close_with_timeout(&self, timeout: Duration) -> io::Result<()> {
        self.stopped.store(true, Ordering::Release);
        let result = self.tap.close().and(self.port.close());
        let deadline = Instant::now() + timeout;
        let mut workers = self.workers.lock().unwrap();
        while workers.iter().any(|w| !w.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let mut pending = Vec::new();
        for worker in workers.drain(..) {
            if worker.is_finished() {
                let _ = worker.join();
            } else {
                pending.push(worker);
            }
        }
        *workers = pending;
        if workers.is_empty() {
            result
        } else {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Wired frame workers did not stop",
            ))
        }
    }
}
impl Drop for FramePump {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

struct SystemDriver;
impl TapDriver for SystemDriver {
    fn open(&self, name: &str) -> io::Result<Arc<dyn TapIo>> {
        #[cfg(target_os = "linux")]
        {
            Ok(Arc::new(linux::SystemTap::open(name)?))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = name;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "TAP requires Linux",
            ))
        }
    }
    fn write_sysctl(&self, path: &Path, value: &str) -> io::Result<()> {
        std::fs::write(path, value)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::fs::OpenOptionsExt,
        },
    };
    struct Handles {
        tap: File,
        wake: OwnedFd,
    }
    pub(super) struct SystemTap {
        name: String,
        handles: Mutex<Option<Arc<Handles>>>,
        closed: AtomicBool,
    }
    fn checked(n: libc::c_int) -> io::Result<()> {
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    fn request(name: &str) -> libc::ifreq {
        let mut req: libc::ifreq = unsafe { std::mem::zeroed() };
        for (dest, byte) in req.ifr_name.iter_mut().zip(name.bytes()) {
            *dest = byte as _;
        }
        req
    }
    fn socket(family: i32) -> io::Result<OwnedFd> {
        let fd = unsafe { libc::socket(family, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }
    fn attach(name: &str) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open("/dev/net/tun")?;
        let mut req = request(name);
        req.ifr_ifru.ifru_flags = (libc::IFF_TAP | libc::IFF_NO_PI) as _;
        checked(unsafe { libc::ioctl(file.as_raw_fd(), libc::TUNSETIFF as _, &mut req) })?;
        Ok(file)
    }
    impl SystemTap {
        pub(super) fn open(name: &str) -> io::Result<Self> {
            let c_name = std::ffi::CString::new(name).unwrap();
            if unsafe { libc::if_nametoindex(c_name.as_ptr()) } != 0 {
                // A nonpersistent TAP dies with its owner. Clear persistence on a stale TAP,
                // then close the final handle so the old interface is removed before recreation.
                let stale = attach(name)?;
                checked(unsafe { libc::ioctl(stale.as_raw_fd(), libc::TUNSETPERSIST as _, 0) })?;
                drop(stale);
            }
            let tap = attach(name)?;
            let wake = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
            if wake < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                name: name.into(),
                handles: Mutex::new(Some(Arc::new(Handles {
                    tap,
                    wake: unsafe { OwnedFd::from_raw_fd(wake) },
                }))),
                closed: AtomicBool::new(false),
            })
        }
        fn handles(&self) -> io::Result<Arc<Handles>> {
            self.handles
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "TAP closed"))
        }
        fn ready(&self, h: &Handles, events: i16, timeout: Duration) -> io::Result<bool> {
            let deadline = Instant::now() + timeout;
            loop {
                if self.closed.load(Ordering::Acquire) {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "TAP closed"));
                }
                let mut fds = [
                    libc::pollfd {
                        fd: h.tap.as_raw_fd(),
                        events,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: h.wake.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                let millis = deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(i32::MAX as u128) as i32;
                let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, millis) };
                if n < 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(e);
                }
                if fds[1].revents != 0 {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "TAP closed"));
                }
                if fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    return Err(io::Error::other("TAP unavailable"));
                }
                return Ok(n > 0 && fds[0].revents & events != 0);
            }
        }
    }
    impl TapIo for SystemTap {
        fn index(&self) -> io::Result<u32> {
            let _h = self.handles()?;
            let mut req = request(&self.name);
            let fd = socket(libc::AF_INET)?;
            checked(unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCGIFINDEX as _, &mut req) })?;
            Ok(unsafe { req.ifr_ifru.ifru_ifindex as u32 })
        }
        fn configure(&self, setting: TapSetting) -> io::Result<()> {
            let _h = self.handles()?;
            let mut req = request(&self.name);
            let fd = socket(if matches!(setting, TapSetting::Ipv6(_)) {
                libc::AF_INET6
            } else {
                libc::AF_INET
            })?;
            match setting {
                TapSetting::Mac(mac) => {
                    let mut addr: libc::sockaddr = unsafe { std::mem::zeroed() };
                    addr.sa_family = libc::ARPHRD_ETHER;
                    for (dest, byte) in addr.sa_data.iter_mut().zip(mac) {
                        *dest = byte as _;
                    }
                    req.ifr_ifru.ifru_hwaddr = addr;
                    checked(unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCSIFHWADDR as _, &req) })
                }
                TapSetting::Mtu(mtu) => {
                    req.ifr_ifru.ifru_mtu = mtu as _;
                    checked(unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCSIFMTU as _, &req) })
                }
                TapSetting::Ipv6(index) => {
                    #[repr(C)]
                    struct In6Ifreq {
                        addr: libc::in6_addr,
                        prefix: u32,
                        index: i32,
                    }
                    let addr = In6Ifreq {
                        addr: libc::in6_addr {
                            s6_addr: "fe80::2".parse::<std::net::Ipv6Addr>().unwrap().octets(),
                        },
                        prefix: 64,
                        index: index as i32,
                    };
                    checked(unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCSIFADDR as _, &addr) })
                }
                TapSetting::Up => {
                    checked(unsafe {
                        libc::ioctl(fd.as_raw_fd(), libc::SIOCGIFFLAGS as _, &mut req)
                    })?;
                    req.ifr_ifru.ifru_flags =
                        (unsafe { req.ifr_ifru.ifru_flags } as i32 | libc::IFF_UP) as _;
                    checked(unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCSIFFLAGS as _, &req) })
                }
            }
        }
    }
    impl EthernetPort for SystemTap {
        fn send_frame(&self, frame: &[u8], timeout: Duration) -> io::Result<()> {
            let h = self.handles()?;
            if !self.ready(&h, libc::POLLOUT, timeout)? {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "TAP write timed out",
                ));
            }
            let n = unsafe { libc::write(h.tap.as_raw_fd(), frame.as_ptr().cast(), frame.len()) };
            if n < 0 {
                Err(io::Error::last_os_error())
            } else if n as usize != frame.len() {
                Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Partial TAP frame",
                ))
            } else {
                Ok(())
            }
        }
        fn receive_frame(&self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
            let h = self.handles()?;
            if !self.ready(&h, libc::POLLIN, timeout)? {
                return Ok(None);
            }
            // One extra byte detects oversize frames even if the driver truncates them.
            let mut frame = vec![0; MAX_FRAME + 1];
            let n =
                unsafe { libc::read(h.tap.as_raw_fd(), frame.as_mut_ptr().cast(), frame.len()) };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::WouldBlock {
                    return Ok(None);
                }
                return Err(e);
            }
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "TAP ended"));
            }
            frame.truncate(n as usize);
            Ok(Some(frame))
        }
        fn close(&self) -> io::Result<()> {
            self.closed.store(true, Ordering::Release);
            if let Some(h) = self.handles.lock().unwrap().take() {
                let wake: u64 = 1;
                checked(
                    unsafe { libc::write(h.wake.as_raw_fd(), (&wake as *const u64).cast(), 8) }
                        as i32,
                )?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::CommandResult;
    use std::{collections::VecDeque, sync::Condvar};
    #[derive(Default)]
    struct PortState {
        frames: VecDeque<io::Result<Vec<u8>>>,
        sent: Vec<Vec<u8>>,
        closed: bool,
        send_error: bool,
    }
    #[derive(Default)]
    struct Port {
        state: Mutex<PortState>,
        changed: Condvar,
        trace: Arc<Mutex<Vec<String>>>,
    }
    impl Port {
        fn push(&self, frame: io::Result<Vec<u8>>) {
            self.state.lock().unwrap().frames.push_back(frame);
            self.changed.notify_all();
        }
    }
    impl EthernetPort for Port {
        fn send_frame(&self, frame: &[u8], timeout: Duration) -> io::Result<()> {
            assert_eq!(timeout, IO_TIMEOUT);
            let mut state = self.state.lock().unwrap();
            if state.closed || state.send_error {
                return Err(io::Error::other("send failed"));
            }
            state.sent.push(frame.to_vec());
            Ok(())
        }
        fn receive_frame(&self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
            let state = self.state.lock().unwrap();
            let (mut state, _) = self
                .changed
                .wait_timeout_while(state, timeout, |s| !s.closed && s.frames.is_empty())
                .unwrap();
            if state.closed {
                return Err(io::Error::other("closed"));
            }
            state.frames.pop_front().transpose()
        }
        fn close(&self) -> io::Result<()> {
            self.state.lock().unwrap().closed = true;
            self.trace.lock().unwrap().push("close".into());
            self.changed.notify_all();
            Ok(())
        }
    }
    impl TapIo for Port {
        fn configure(&self, setting: TapSetting) -> io::Result<()> {
            self.trace.lock().unwrap().push(format!("{setting:?}"));
            Ok(())
        }
        fn index(&self) -> io::Result<u32> {
            Ok(42)
        }
    }
    struct Driver {
        port: Arc<Port>,
        fail_key: Option<&'static str>,
    }
    impl TapDriver for Driver {
        fn open(&self, name: &str) -> io::Result<Arc<dyn TapIo>> {
            self.port.trace.lock().unwrap().push(format!("open {name}"));
            Ok(self.port.clone())
        }
        fn write_sysctl(&self, path: &Path, value: &str) -> io::Result<()> {
            self.port
                .trace
                .lock()
                .unwrap()
                .push(format!("{}={value}", path.display()));
            if self.fail_key.is_some_and(|key| path.ends_with(key)) {
                Err(io::Error::other("sysctl failed"))
            } else {
                Ok(())
            }
        }
    }
    struct Runner {
        trace: Arc<Mutex<Vec<String>>>,
        managed: Option<bool>,
    }
    impl CommandRunner for Runner {
        fn run(&self, args: &[String], _: Duration) -> io::Result<CommandResult> {
            self.trace.lock().unwrap().push(args.join(" "));
            match self.managed {
                None => Err(io::Error::new(io::ErrorKind::NotFound, "nmcli")),
                Some(managed) => Ok(CommandResult {
                    exit_code: 0,
                    output: if managed { "yes" } else { "no" }.into(),
                }),
            }
        }
    }
    fn wait_for(check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !check() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn setup_orders_network_manager_sysctls_address_and_up() {
        let port = Arc::new(Port::default());
        let runner = Runner {
            trace: port.trace.clone(),
            managed: Some(true),
        };
        let tap = TapDevice::create_with(
            &Driver {
                port: port.clone(),
                fail_key: None,
            },
            [2, 1, 2, 3, 4, 5],
            Path::new("/fake/proc"),
            &runner,
        )
        .unwrap();
        assert_eq!(tap.name(), "oxpusb0");
        assert_eq!(tap.index(), 42);
        assert_eq!(
            *port.trace.lock().unwrap(),
            [
                "open oxpusb0",
                "nmcli -g GENERAL.NM-MANAGED device show oxpusb0",
                "nmcli device set oxpusb0 managed no",
                "Mac([2, 1, 2, 3, 4, 5])",
                "Mtu(1500)",
                "/fake/proc/net/ipv6/conf/oxpusb0/addr_gen_mode=1",
                "/fake/proc/net/ipv6/conf/oxpusb0/accept_dad=0",
                "/fake/proc/net/ipv6/conf/oxpusb0/accept_ra=0",
                "/fake/proc/net/ipv6/conf/oxpusb0/disable_ipv6=0",
                "Ipv6(42)",
                "Up",
            ]
        );
        tap.close().unwrap();
        assert!(port.state.lock().unwrap().closed);
    }
    #[test]
    fn setup_skips_absent_or_unmanaged_nmcli_and_cleans_up_on_failure() {
        for managed in [None, Some(false)] {
            let port = Arc::new(Port::default());
            let runner = Runner {
                trace: port.trace.clone(),
                managed,
            };
            assert!(TapDevice::create_with(
                &Driver {
                    port: port.clone(),
                    fail_key: Some("accept_ra")
                },
                [0; 6],
                Path::new("/proc/sys"),
                &runner
            )
            .is_err());
            let trace = port.trace.lock().unwrap();
            assert!(!trace.iter().any(|line| line.contains("managed no")
                || line.starts_with("Ipv6")
                || line == "Up"));
            assert_eq!(trace.last().unwrap(), "close");
        }
    }
    #[test]
    fn pumps_both_directions_and_drops_oversize_without_forwarding() {
        let tap = Arc::new(Port::default());
        let port = Arc::new(Port::default());
        let pump =
            FramePump::start_ports(tap.clone(), port.clone(), Arc::new(|e| panic!("{e}"))).unwrap();
        tap.push(Ok(vec![1; MAX_FRAME]));
        tap.push(Ok(vec![2; MAX_FRAME + 1]));
        port.push(Ok(vec![3; 14]));
        port.push(Ok(vec![4; MAX_FRAME + 1]));
        wait_for(|| {
            pump.dropped_oversize() == 2
                && tap.state.lock().unwrap().sent.len() == 1
                && port.state.lock().unwrap().sent.len() == 1
        });
        assert_eq!(port.state.lock().unwrap().sent[0], vec![1; MAX_FRAME]);
        assert_eq!(tap.state.lock().unwrap().sent[0], vec![3; 14]);
        pump.close().unwrap();
        pump.close().unwrap();
        assert!(pump.workers.lock().unwrap().is_empty());
    }
    #[test]
    fn first_fatal_error_is_reported_once_and_wakes_other_worker() {
        for (from_tap, send_error) in [(false, false), (true, false), (false, true), (true, true)] {
            let tap = Arc::new(Port::default());
            let port = Arc::new(Port::default());
            let (source, target) = if from_tap {
                (&tap, &port)
            } else {
                (&port, &tap)
            };
            target.state.lock().unwrap().send_error = send_error;
            source.push(if send_error {
                Ok(vec![0; 14])
            } else {
                Err(io::Error::other("receive failed"))
            });
            let (tx, rx) = std::sync::mpsc::channel();
            let pump = FramePump::start_ports(
                tap.clone(),
                port.clone(),
                Arc::new(move |e| {
                    tx.send(e.to_string()).unwrap();
                }),
            )
            .unwrap();
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                if send_error {
                    "send failed"
                } else {
                    "receive failed"
                }
            );
            pump.close().unwrap();
            assert!(rx.try_recv().is_err());
            assert!(tap.state.lock().unwrap().closed && port.state.lock().unwrap().closed);
        }
    }
    #[test]
    fn close_wait_is_bounded_even_if_a_port_does_not_wake_its_receiver() {
        struct Stalled {
            entered: Mutex<Option<std::sync::mpsc::Sender<()>>>,
            release: (Mutex<bool>, Condvar),
        }
        impl EthernetPort for Stalled {
            fn send_frame(&self, _: &[u8], _: Duration) -> io::Result<()> {
                Ok(())
            }
            fn receive_frame(&self, _: Duration) -> io::Result<Option<Vec<u8>>> {
                if let Some(tx) = self.entered.lock().unwrap().take() {
                    tx.send(()).unwrap();
                }
                let guard = self.release.0.lock().unwrap();
                drop(
                    self.release
                        .1
                        .wait_while(guard, |released| !*released)
                        .unwrap(),
                );
                Ok(None)
            }
            fn close(&self) -> io::Result<()> {
                Ok(())
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let port = Arc::new(Stalled {
            entered: Mutex::new(Some(tx)),
            release: (Mutex::new(false), Condvar::new()),
        });
        let pump = FramePump::start_ports(
            Arc::new(Port::default()),
            port.clone(),
            Arc::new(|e| panic!("{e}")),
        )
        .unwrap();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let start = Instant::now();
        assert_eq!(
            pump.close_with_timeout(Duration::from_millis(20))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_millis(500));
        *port.release.0.lock().unwrap() = true;
        port.release.1.notify_all();
        pump.close().unwrap();
        assert!(pump.workers.lock().unwrap().is_empty());
    }
    #[test]
    fn idle_close_wakes_workers_without_reporting_failure() {
        let (tx, rx) = std::sync::mpsc::channel();
        let pump = FramePump::start_ports(
            Arc::new(Port::default()),
            Arc::new(Port::default()),
            Arc::new(move |_| {
                tx.send(()).unwrap();
            }),
        )
        .unwrap();
        let start = Instant::now();
        pump.close().unwrap();
        assert!(start.elapsed() < CLOSE_TIMEOUT);
        assert!(rx.try_recv().is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires root, /dev/net/tun and an unused oxpusb0 on Linux"]
    fn real_tap_create_close() {
        let tap =
            TapDevice::create([2, 0, 0, 0, 0, 2], &crate::platform::ProcessCommandRunner).unwrap();
        assert!(tap.index() > 0);
        tap.close().unwrap();
        assert!(!Path::new("/sys/class/net/oxpusb0").exists());
    }
}
