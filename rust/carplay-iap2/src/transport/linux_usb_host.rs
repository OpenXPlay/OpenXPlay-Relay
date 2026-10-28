// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Linux usbfs host. Async IN requests survive timeouts and share one completion reaper.
use super::iphone_usb_host::{UsbDeviceInfo, UsbError, UsbHostBus, UsbHostDevice, UsbResult};
use crate::{invalid, Receive, Result};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct LinuxUsbHostBus {
    sysfs: PathBuf,
    devfs: PathBuf,
}
impl Default for LinuxUsbHostBus {
    fn default() -> Self {
        Self::new()
    }
}
impl LinuxUsbHostBus {
    pub fn new() -> Self {
        Self::with_paths("/sys/bus/usb/devices", "/dev/bus/usb")
    }
    /// Alternate roots permit descriptor/enumeration tests without actual USB devices.
    pub fn with_paths(sysfs: impl Into<PathBuf>, devfs: impl Into<PathBuf>) -> Self {
        Self {
            sysfs: sysfs.into(),
            devfs: devfs.into(),
        }
    }
}
impl UsbHostBus for LinuxUsbHostBus {
    fn enumerate(&self) -> UsbResult<Vec<UsbDeviceInfo>> {
        let mut devices = Vec::new();
        for entry in std::fs::read_dir(&self.sysfs)? {
            let path = entry?.path();
            if number(&path, "idVendor", 16) != Some(0x05ac) {
                continue;
            }
            let (Some(bus), Some(device)) =
                (number(&path, "busnum", 10), number(&path, "devnum", 10))
            else {
                continue;
            };
            devices.push(UsbDeviceInfo {
                bus_number: bus as u16,
                device_number: device as u16,
                speed_mbps: text(&path, "speed")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.),
                product: text(&path, "product"),
                serial: text(&path, "serial"),
                sysfs_path: path,
            });
        }
        devices.sort_by_key(|d| (d.bus_number, d.device_number));
        Ok(devices)
    }
    fn open(&self, device: &UsbDeviceInfo) -> UsbResult<Arc<dyn UsbHostDevice>> {
        let path = self.devfs.join(format!(
            "{:03}/{:03}",
            device.bus_number, device.device_number
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(path)?;
        let mut descriptors = Vec::new();
        file.read_to_end(&mut descriptors)?;
        Ok(Arc::new(LinuxUsbHostDevice::from_driver(
            Box::new(NativeDriver {
                file: Mutex::new(Some(file)),
            }),
            descriptors,
            device.speed_mbps,
        )))
    }
}
fn text(path: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(path.join(name))
        .ok()
        .map(|v| v.trim().to_owned())
}
fn number(path: &Path, name: &str, radix: u32) -> Option<u32> {
    u32::from_str_radix(&text(path, name)?, radix).ok()
}
const fn ioc(direction: u32, number: u32, size: usize) -> libc::c_ulong {
    ((direction << 30) | ((size as u32) << 16) | ((b'U' as u32) << 8) | number) as libc::c_ulong
}
#[repr(C)]
struct Control {
    request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    timeout: u32,
    data: *mut libc::c_void,
}
#[repr(C)]
struct Bulk {
    endpoint: u32,
    length: u32,
    timeout: u32,
    data: *mut libc::c_void,
}
#[repr(C)]
struct SetInterface {
    interface: u32,
    alternate: u32,
}
#[repr(C)]
struct DisconnectClaim {
    interface: u32,
    flags: u32,
    driver: [u8; 256],
}
#[repr(C)]
struct InterfaceIoctl {
    interface: i32,
    code: i32,
    data: *mut libc::c_void,
}
#[repr(C)]
struct Urb {
    kind: u8,
    endpoint: u8,
    status: i32,
    flags: u32,
    buffer: *mut libc::c_void,
    buffer_length: i32,
    actual_length: i32,
    start_frame: i32,
    packets: i32,
    error_count: i32,
    signr: u32,
    usercontext: *mut libc::c_void,
}
// SAFETY: a pending URB and its heap buffer are accessed only under the device state lock;
// the kernel writes them until reap or descriptor close, before their allocations are freed.
unsafe impl Send for Urb {}
fn ioctl<T>(file: &File, request: libc::c_ulong, value: &mut T) -> Result<i32> {
    loop {
        // SAFETY: all callers use the matching UAPI layout and live backing allocations.
        let n = unsafe { libc::ioctl(file.as_raw_fd(), request as _, value as *mut T) };
        if n >= 0 {
            return Ok(n);
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}
fn millis(d: Duration) -> u32 {
    d.as_millis().clamp(1, u32::MAX as u128) as u32
}
trait Driver: Send + Sync {
    fn control(&self, r: &mut Control) -> Result<usize>;
    fn interface(&self, request: u32, value: u32) -> Result<()>;
    fn set_interface(&self, interface: u8, alternate: u8) -> Result<()>;
    fn disconnect_claim(&self, interface: u8) -> Result<()>;
    fn disconnect(&self, interface: u8) -> Result<()>;
    fn bulk_out(&self, endpoint: u8, data: &[u8], timeout: Duration) -> Result<usize>;
    fn submit(&self, urb: &mut Urb) -> Result<()>;
    fn reap(&self) -> Result<Option<usize>>;
    fn discard(&self, token: usize) -> Result<()>;
    fn poll(&self, timeout: Duration) -> Result<()>;
    fn close(&self);
}
struct NativeDriver {
    file: Mutex<Option<File>>,
}
impl NativeDriver {
    fn with<T>(&self, f: impl FnOnce(&File) -> Result<T>) -> Result<T> {
        let guard = self.file.lock().unwrap();
        f(guard
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "USB device is closed"))?)
    }
}
impl Driver for NativeDriver {
    fn control(&self, r: &mut Control) -> Result<usize> {
        self.with(|f| ioctl(f, ioc(3, 0, std::mem::size_of::<Control>()), r).map(|n| n as usize))
    }
    fn interface(&self, request: u32, mut value: u32) -> Result<()> {
        self.with(|f| ioctl(f, ioc(2, request, 4), &mut value).map(|_| ()))
    }
    fn set_interface(&self, interface: u8, alternate: u8) -> Result<()> {
        self.with(|f| {
            ioctl(
                f,
                ioc(2, 4, 8),
                &mut SetInterface {
                    interface: interface.into(),
                    alternate: alternate.into(),
                },
            )
            .map(|_| ())
        })
    }
    fn disconnect_claim(&self, interface: u8) -> Result<()> {
        self.with(|f| {
            ioctl(
                f,
                ioc(2, 27, 264),
                &mut DisconnectClaim {
                    interface: interface.into(),
                    flags: 0,
                    driver: [0; 256],
                },
            )
            .map(|_| ())
        })
    }
    fn disconnect(&self, interface: u8) -> Result<()> {
        self.with(|f| {
            ioctl(
                f,
                ioc(3, 18, std::mem::size_of::<InterfaceIoctl>()),
                &mut InterfaceIoctl {
                    interface: interface.into(),
                    code: ioc(0, 22, 0) as i32,
                    data: std::ptr::null_mut(),
                },
            )
            .map(|_| ())
        })
    }
    fn bulk_out(&self, endpoint: u8, data: &[u8], timeout: Duration) -> Result<usize> {
        self.with(|f| {
            ioctl(
                f,
                ioc(3, 2, std::mem::size_of::<Bulk>()),
                &mut Bulk {
                    endpoint: endpoint.into(),
                    length: data.len() as u32,
                    timeout: millis(timeout),
                    data: data.as_ptr() as *mut _,
                },
            )
            .map(|n| n as usize)
        })
    }
    fn submit(&self, urb: &mut Urb) -> Result<()> {
        self.with(|f| ioctl(f, ioc(2, 10, std::mem::size_of::<Urb>()), urb).map(|_| ()))
    }
    fn reap(&self) -> Result<Option<usize>> {
        self.with(|f| {
            let mut ptr: *mut Urb = std::ptr::null_mut();
            match ioctl(f, ioc(1, 13, std::mem::size_of::<*mut Urb>()), &mut ptr) {
                Ok(_) => Ok(Some(ptr as usize)),
                Err(e) if e.raw_os_error() == Some(libc::EAGAIN) => Ok(None),
                Err(e) => Err(e),
            }
        })
    }
    fn discard(&self, token: usize) -> Result<()> {
        self.with(|f| {
            // SAFETY: DISCARDURB takes the submitted URB address itself, not a pointer to that address.
            let n = unsafe { libc::ioctl(f.as_raw_fd(), ioc(0, 11, 0) as _, token as *mut Urb) };
            if n < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    }
    fn poll(&self, timeout: Duration) -> Result<()> {
        self.with(|f| {
            let mut fd = libc::pollfd {
                fd: f.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            // SAFETY: fd is a live one-element pollfd array for the duration of the call.
            let n = unsafe { libc::poll(&mut fd, 1, millis(timeout).min(i32::MAX as u32) as i32) };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(e);
                }
            }
            if fd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Err(io::Error::from_raw_os_error(libc::ENODEV));
            }
            Ok(())
        })
    }
    fn close(&self) {
        self.file.lock().unwrap().take();
    }
}
struct Pending {
    urb: Box<Urb>,
    buffer: Vec<u8>,
    queued: bool,
    ready: VecDeque<Vec<u8>>,
    failure: Option<i32>,
}
struct InState {
    closed: bool,
    endpoints: BTreeMap<u8, Pending>,
}
pub struct LinuxUsbHostDevice {
    driver: Box<dyn Driver>,
    descriptors: Vec<u8>,
    speed: f64,
    state: Mutex<InState>,
}
impl LinuxUsbHostDevice {
    fn from_driver(driver: Box<dyn Driver>, descriptors: Vec<u8>, speed: f64) -> Self {
        Self {
            driver,
            descriptors,
            speed,
            state: Mutex::new(InState {
                closed: false,
                endpoints: BTreeMap::new(),
            }),
        }
    }
    fn check(&self) -> UsbResult<()> {
        if self.state.lock().unwrap().closed {
            Err(UsbError::Closed)
        } else {
            Ok(())
        }
    }
    fn reap(&self, s: &mut InState) -> UsbResult<()> {
        while let Some(token) = self.driver.reap()? {
            let p = s
                .endpoints
                .values_mut()
                .find(|p| std::ptr::from_ref(p.urb.as_ref()) as usize == token)
                .ok_or_else(|| UsbError::Protocol("usbfs reaped an unknown URB".into()))?;
            p.queued = false;
            let n = (p.urb.actual_length.max(0) as usize).min(p.buffer.len());
            if n > 0 {
                p.ready.push_back(p.buffer[..n].to_vec());
            }
            if p.urb.status < 0 && ![libc::ENOENT, libc::ECONNRESET].contains(&-p.urb.status) {
                p.failure = Some(-p.urb.status);
            }
        }
        Ok(())
    }
    fn input(
        &self,
        endpoint: u8,
        max_bytes: usize,
        timeout: Duration,
        kind: u8,
    ) -> UsbResult<Receive> {
        if endpoint & 0x80 == 0 || max_bytes == 0 {
            return Err(UsbError::Protocol(
                "Invalid USB IN endpoint or buffer size".into(),
            ));
        }
        let deadline = Instant::now() + timeout;
        loop {
            {
                let mut s = self.state.lock().unwrap();
                let reaped = if !s.closed { self.reap(&mut s) } else { Ok(()) };
                let closed = s.closed;
                if let Some(p) = s.endpoints.get_mut(&endpoint) {
                    if let Some(mut b) = p.ready.pop_front() {
                        if b.len() > max_bytes {
                            p.ready.push_front(b.split_off(max_bytes));
                        }
                        return Ok(Receive::Data(b));
                    }
                    if let Some(e) = p.failure {
                        return Err(io::Error::from_raw_os_error(e).into());
                    }
                }
                reaped?;
                if closed {
                    return Ok(Receive::Eof);
                }
                let p = s.endpoints.entry(endpoint).or_insert_with(|| {
                    let mut buffer = vec![0; max_bytes.min(65536)];
                    let urb = Box::new(Urb {
                        kind,
                        endpoint,
                        status: 0,
                        flags: 0,
                        buffer: buffer.as_mut_ptr().cast(),
                        buffer_length: buffer.len() as i32,
                        actual_length: 0,
                        start_frame: 0,
                        packets: 0,
                        error_count: 0,
                        signr: 0,
                        usercontext: std::ptr::null_mut(),
                    });
                    Pending {
                        urb,
                        buffer,
                        queued: false,
                        ready: VecDeque::new(),
                        failure: None,
                    }
                });
                if p.urb.kind != kind {
                    return Err(UsbError::Protocol("USB IN endpoint type changed".into()));
                }
                if !p.queued {
                    p.urb.status = 0;
                    p.urb.actual_length = 0;
                    self.driver.submit(&mut p.urb)?;
                    p.queued = true;
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(Receive::Timeout);
            }
            // Short polls let concurrent endpoint readers and close acquire the driver lock.
            self.driver.poll(remaining.min(Duration::from_millis(20)))?;
        }
    }
}
impl UsbHostDevice for LinuxUsbHostDevice {
    fn raw_descriptors(&self) -> UsbResult<Vec<u8>> {
        Ok(self.descriptors.clone())
    }
    fn active_configuration(&self) -> UsbResult<Option<u8>> {
        let v = self.control_in(0x80, 8, 0, 0, 1, Duration::from_secs(1))?;
        Ok((v.len() == 1).then(|| v[0]))
    }
    fn set_configuration(&self, value: u8) -> UsbResult<()> {
        self.check()?;
        Ok(self.driver.interface(5, value.into())?)
    }
    fn detach_kernel_driver(&self, interface: u8) -> UsbResult<()> {
        self.check()?;
        match self.driver.disconnect(interface) {
            Ok(()) => Ok(()),
            Err(e) if matches!(e.raw_os_error(), Some(libc::ENODATA | libc::EINVAL)) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
    fn claim_interface(&self, interface: u8) -> UsbResult<()> {
        self.check()?;
        let result = match self.driver.disconnect_claim(interface) {
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(libc::ENOTTY | libc::EINVAL | libc::ENOSYS)
                ) =>
            {
                self.detach_kernel_driver(interface)?;
                self.driver.interface(15, interface.into())
            }
            result => result,
        };
        result.map_err(|e| {
            if e.raw_os_error() == Some(libc::EBUSY) {
                UsbError::InterfaceBusy(interface)
            } else {
                e.into()
            }
        })
    }
    fn release_interface(&self, interface: u8) -> UsbResult<()> {
        self.check()?;
        Ok(self.driver.interface(16, interface.into())?)
    }
    fn set_alternate_setting(&self, interface: u8, alternate: u8) -> UsbResult<()> {
        self.check()?;
        Ok(self.driver.set_interface(interface, alternate)?)
    }
    fn control_in(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
        timeout: Duration,
    ) -> UsbResult<Vec<u8>> {
        self.check()?;
        let mut bytes = vec![0; length as usize];
        let mut r = Control {
            request_type,
            request,
            value,
            index,
            length,
            timeout: millis(timeout),
            data: bytes.as_mut_ptr().cast(),
        };
        let n = self.driver.control(&mut r)?;
        bytes.truncate(n);
        Ok(bytes)
    }
    fn control_out(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> UsbResult<usize> {
        self.check()?;
        let length =
            u16::try_from(data.len()).map_err(|_| invalid("USB control payload exceeds u16"))?;
        Ok(self.driver.control(&mut Control {
            request_type,
            request,
            value,
            index,
            length,
            timeout: millis(timeout),
            data: data.as_ptr() as *mut _,
        })?)
    }
    fn bulk_out(&self, endpoint: u8, data: &[u8], timeout: Duration) -> UsbResult<usize> {
        self.check()?;
        if endpoint & 0x80 != 0 || data.len() > i32::MAX as usize {
            return Err(UsbError::Protocol("Invalid USB OUT transfer".into()));
        }
        Ok(self.driver.bulk_out(endpoint, data, timeout)?)
    }
    fn bulk_in(&self, endpoint: u8, max_bytes: usize, timeout: Duration) -> UsbResult<Receive> {
        self.input(endpoint, max_bytes, timeout, 3)
    }
    fn interrupt_in(
        &self,
        endpoint: u8,
        max_bytes: usize,
        timeout: Duration,
    ) -> UsbResult<Receive> {
        self.input(endpoint, max_bytes, timeout, 1)
    }
    fn clear_halt(&self, endpoint: u8) -> UsbResult<()> {
        self.check()?;
        Ok(self.driver.interface(21, endpoint.into())?)
    }
    fn link_speed_mbps(&self) -> f64 {
        self.speed
    }
    fn close(&self) -> UsbResult<()> {
        let mut s = self.state.lock().unwrap();
        if s.closed {
            return Ok(());
        }
        s.closed = true;
        for p in s.endpoints.values() {
            if p.queued {
                let _ = self
                    .driver
                    .discard(std::ptr::from_ref(p.urb.as_ref()) as usize);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut failure = None;
        while s.endpoints.values().any(|p| p.queued) {
            if let Err(error) = self.reap(&mut s) {
                failure = Some(error);
                break;
            }
            if s.endpoints.values().any(|p| p.queued) {
                if Instant::now() >= deadline {
                    failure = Some(UsbError::Io(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Timed out draining cancelled USB IN request",
                    )));
                    break;
                }
                let _ = self.driver.poll(Duration::from_millis(10));
            }
        }
        // Closing the fd is authoritative: the kernel can no longer write any pending buffers.
        self.driver.close();
        for p in s.endpoints.values_mut() {
            p.queued = false;
        }
        failure.map_or(Ok(()), Err)
    }
}
impl Drop for LinuxUsbHostDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct FakeState {
        submitted: Vec<usize>,
        completions: VecDeque<(usize, Vec<u8>, i32)>,
        discard_bytes: Vec<u8>,
        discard_fails: bool,
        events: Vec<String>,
        claim_error: Option<i32>,
        disconnect_claim_error: Option<i32>,
        reap_error: Option<i32>,
    }
    struct Fake(Arc<Mutex<FakeState>>);
    impl Driver for Fake {
        fn control(&self, _: &mut Control) -> Result<usize> {
            Ok(0)
        }
        fn interface(&self, r: u32, v: u32) -> Result<()> {
            let mut s = self.0.lock().unwrap();
            s.events.push(format!("{r}:{v}"));
            if r == 15 {
                if let Some(e) = s.claim_error {
                    return Err(io::Error::from_raw_os_error(e));
                }
            }
            Ok(())
        }
        fn set_interface(&self, _: u8, _: u8) -> Result<()> {
            Ok(())
        }
        fn disconnect_claim(&self, i: u8) -> Result<()> {
            let mut s = self.0.lock().unwrap();
            s.events.push(format!("disconnect-claim:{i}"));
            s.disconnect_claim_error
                .map_or(Ok(()), |e| Err(io::Error::from_raw_os_error(e)))
        }
        fn disconnect(&self, i: u8) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .events
                .push(format!("disconnect:{i}"));
            Ok(())
        }
        fn bulk_out(&self, _: u8, b: &[u8], _: Duration) -> Result<usize> {
            Ok(b.len())
        }
        fn submit(&self, u: &mut Urb) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .submitted
                .push(std::ptr::from_mut(u) as usize);
            Ok(())
        }
        fn reap(&self) -> Result<Option<usize>> {
            let mut s = self.0.lock().unwrap();
            if let Some((token, bytes, status)) = s.completions.pop_front() {
                // SAFETY: the device retains this submitted URB and buffer until reap completes.
                let u = unsafe { &mut *(token as *mut Urb) };
                assert!(bytes.len() <= u.buffer_length as usize);
                // SAFETY: the fake exclusively writes the still-pending kernel buffer, within bounds.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        u.buffer.cast::<u8>(),
                        bytes.len(),
                    );
                }
                u.actual_length = bytes.len() as i32;
                u.status = status;
                return Ok(Some(token));
            }
            s.reap_error
                .map_or(Ok(None), |e| Err(io::Error::from_raw_os_error(e)))
        }
        fn discard(&self, t: usize) -> Result<()> {
            let mut s = self.0.lock().unwrap();
            s.events.push("discard".into());
            let bytes = s.discard_bytes.clone();
            s.completions.push_back((t, bytes, -libc::ENOENT));
            if s.discard_fails {
                Err(io::Error::from_raw_os_error(libc::EINVAL))
            } else {
                Ok(())
            }
        }
        fn poll(&self, _: Duration) -> Result<()> {
            Ok(())
        }
        fn close(&self) {
            self.0.lock().unwrap().events.push("close".into());
        }
    }
    fn device() -> (LinuxUsbHostDevice, Arc<Mutex<FakeState>>) {
        let s = Arc::new(Mutex::new(FakeState::default()));
        (
            LinuxUsbHostDevice::from_driver(Box::new(Fake(s.clone())), vec![], 480.),
            s,
        )
    }
    #[test]
    fn timeout_keeps_one_urb_and_next_reap_keeps_every_byte() {
        let (d, s) = device();
        assert_eq!(
            d.bulk_in(0x85, 20, Duration::ZERO).unwrap(),
            Receive::Timeout
        );
        assert_eq!(
            d.bulk_in(0x85, 20, Duration::ZERO).unwrap(),
            Receive::Timeout
        );
        let token = {
            let s = s.lock().unwrap();
            assert_eq!(s.submitted.len(), 1);
            s.submitted[0]
        };
        s.lock()
            .unwrap()
            .completions
            .push_back((token, vec![1, 2, 3, 4], 0));
        assert_eq!(
            d.bulk_in(0x85, 2, Duration::ZERO).unwrap(),
            Receive::Data(vec![1, 2])
        );
        assert_eq!(
            d.bulk_in(0x85, 2, Duration::ZERO).unwrap(),
            Receive::Data(vec![3, 4])
        );
        assert_eq!(s.lock().unwrap().submitted.len(), 1);
        assert_eq!(
            d.bulk_in(0x85, 20, Duration::ZERO).unwrap(),
            Receive::Timeout
        );
        assert_eq!(s.lock().unwrap().submitted.len(), 2);
    }
    #[test]
    fn close_discards_and_reaps_partial_or_racing_completion_bytes() {
        for cancelled in [true, false] {
            let (d, s) = device();
            d.bulk_in(0x85, 20, Duration::ZERO).unwrap();
            {
                let mut s = s.lock().unwrap();
                s.discard_bytes = vec![0x40, 0x40, 0, 6];
                s.discard_fails = !cancelled;
            }
            d.close().unwrap();
            assert_eq!(
                d.bulk_in(0x85, 20, Duration::ZERO).unwrap(),
                Receive::Data(vec![0x40, 0x40, 0, 6])
            );
            assert_eq!(d.bulk_in(0x85, 20, Duration::ZERO).unwrap(), Receive::Eof);
            let s = s.lock().unwrap();
            assert_eq!(s.events, ["discard", "close"]);
        }
    }
    #[test]
    fn partial_completion_precedes_its_device_gone_error() {
        let (d, s) = device();
        d.bulk_in(0x85, 20, Duration::ZERO).unwrap();
        let token = s.lock().unwrap().submitted[0];
        s.lock()
            .unwrap()
            .completions
            .push_back((token, vec![1, 2, 3], -libc::ENODEV));
        assert_eq!(
            d.bulk_in(0x85, 20, Duration::ZERO).unwrap(),
            Receive::Data(vec![1, 2, 3])
        );
        assert!(matches!(
            d.bulk_in(0x85, 20, Duration::ZERO),
            Err(UsbError::DeviceGone)
        ));
        d.close().unwrap();
    }
    #[test]
    fn empty_cancel_is_eof_after_an_ordinary_timeout() {
        let (d, _) = device();
        assert_eq!(
            d.bulk_in(0x85, 20, Duration::ZERO).unwrap(),
            Receive::Timeout
        );
        d.close().unwrap();
        assert_eq!(d.bulk_in(0x85, 20, Duration::ZERO).unwrap(), Receive::Eof);
    }
    #[test]
    fn reaper_dispatches_bulk_and_interrupt_completions_without_losing_either() {
        let (d, s) = device();
        d.bulk_in(0x85, 20, Duration::ZERO).unwrap();
        d.interrupt_in(0x83, 64, Duration::ZERO).unwrap();
        let tokens = s.lock().unwrap().submitted.clone();
        s.lock()
            .unwrap()
            .completions
            .extend([(tokens[1], vec![9], 0), (tokens[0], vec![1, 2], 0)]);
        assert_eq!(
            d.bulk_in(0x85, 20, Duration::ZERO).unwrap(),
            Receive::Data(vec![1, 2])
        );
        assert_eq!(
            d.interrupt_in(0x83, 64, Duration::ZERO).unwrap(),
            Receive::Data(vec![9])
        );
    }
    #[test]
    fn unplug_and_busy_claim_are_typed() {
        let (d, s) = device();
        s.lock().unwrap().reap_error = Some(libc::ENODEV);
        assert!(matches!(
            d.bulk_in(0x85, 20, Duration::ZERO),
            Err(UsbError::DeviceGone)
        ));
        s.lock().unwrap().reap_error = None;
        s.lock().unwrap().disconnect_claim_error = Some(libc::EBUSY);
        let e = d.claim_interface(1).unwrap_err();
        assert!(matches!(e, UsbError::InterfaceBusy(1)));
        assert_eq!(
            e.to_string(),
            "another program (usbmuxd?) holds the iPhone (interface 1)"
        );
        assert!(matches!(
            UsbError::from(io::Error::from_raw_os_error(108)),
            UsbError::DeviceGone
        ));
    }
    #[test]
    fn disconnect_claim_falls_back_only_when_unsupported() {
        for error in [libc::ENOTTY, libc::EINVAL, libc::ENOSYS] {
            let (d, s) = device();
            s.lock().unwrap().disconnect_claim_error = Some(error);
            d.claim_interface(3).unwrap();
            assert_eq!(
                s.lock().unwrap().events,
                ["disconnect-claim:3", "disconnect:3", "15:3"]
            );
        }
        let (d, s) = device();
        s.lock().unwrap().disconnect_claim_error = Some(libc::ENODEV);
        assert!(matches!(d.claim_interface(3), Err(UsbError::DeviceGone)));
        assert_eq!(s.lock().unwrap().events, ["disconnect-claim:3"]);
    }
    #[test]
    fn uapi_layouts_follow_target_pointer_width() {
        let pointer = std::mem::size_of::<usize>();
        assert_eq!(
            std::mem::size_of::<Control>(),
            if pointer == 8 { 24 } else { 16 }
        );
        assert_eq!(
            std::mem::size_of::<Bulk>(),
            if pointer == 8 { 24 } else { 16 }
        );
        assert_eq!(
            std::mem::size_of::<Urb>(),
            if pointer == 8 { 56 } else { 44 }
        );
        assert_eq!(
            std::mem::size_of::<InterfaceIoctl>(),
            if pointer == 8 { 16 } else { 12 }
        );
        assert_eq!(std::mem::size_of::<DisconnectClaim>(), 264);
        assert_eq!(std::mem::size_of::<SetInterface>(), 8);
    }
    #[test]
    fn sysfs_enumeration_filters_apple_and_keeps_node_identity() {
        let root = std::env::temp_dir().join(format!("oxp-wired-usb-{}", std::process::id()));
        let path = root.join("1-2");
        std::fs::create_dir_all(&path).unwrap();
        for (k, v) in [
            ("idVendor", "05ac"),
            ("busnum", "1"),
            ("devnum", "7"),
            ("speed", "480"),
            ("product", "iPhone"),
            ("serial", "test-serial"),
        ] {
            std::fs::write(path.join(k), v).unwrap();
        }
        let other = root.join("1-3");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("idVendor"), "1234").unwrap();
        let bus = LinuxUsbHostBus::with_paths(&root, root.join("dev"));
        let devices = bus.enumerate().unwrap();
        assert_eq!(devices.len(), 1);
        let d = &devices[0];
        assert_eq!((d.bus_number, d.device_number, d.speed_mbps), (1, 7, 480.));
        assert_eq!(d.product.as_deref(), Some("iPhone"));
        assert_eq!(d.serial.as_deref(), Some("test-serial"));
        assert_eq!(d.sysfs_path, path);
        std::fs::remove_dir_all(root).unwrap();
    }
}
