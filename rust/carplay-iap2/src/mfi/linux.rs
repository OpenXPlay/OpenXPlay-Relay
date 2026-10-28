// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-27
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

//! Linux I2C and usbfs backends. Handles own their descriptors and never change device permissions.
use super::{ch341::Usb, coprocessor::I2c};
use crate::{invalid, Result};
use std::{
    fs::{File, OpenOptions},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

fn ioctl<T>(file: &File, request: libc::c_ulong, data: &mut T) -> Result<i32> {
    loop {
        // SAFETY: callers pair each request with its Linux UAPI layout and live backing buffers.
        let result = unsafe { libc::ioctl(file.as_raw_fd(), request as _, data as *mut T) };
        if result >= 0 {
            return Ok(result);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
// Typed ioctl seam: fake drivers exercise the UAPI requests without device nodes.
trait Driver: Send + Sync {
    fn functions(&self) -> Result<libc::c_ulong> {
        Err(std::io::Error::other("Not an I2C adapter"))
    }
    fn transfer(&self, _: &mut [Message]) -> Result<i32> {
        Err(std::io::Error::other("Not an I2C adapter"))
    }
    fn interface(&self, operation: u32, value: u32) -> Result<()>;
    fn bulk(&self, endpoint: u8, bytes: &mut [u8], timeout: Duration) -> Result<usize>;
}
impl Driver for File {
    fn functions(&self) -> Result<libc::c_ulong> {
        let mut value = 0;
        ioctl(self, 0x0705, &mut value)?;
        Ok(value)
    }
    fn transfer(&self, messages: &mut [Message]) -> Result<i32> {
        let mut request = Transfer {
            messages: messages.as_mut_ptr(),
            count: messages.len() as u32,
        };
        ioctl(self, 0x0707, &mut request)
    }
    fn interface(&self, operation: u32, mut value: u32) -> Result<()> {
        ioctl(self, ioc(2, operation, 4), &mut value).map(|_| ())
    }
    fn bulk(&self, endpoint: u8, bytes: &mut [u8], timeout: Duration) -> Result<usize> {
        let mut request = Bulk {
            endpoint: endpoint.into(),
            length: bytes.len() as u32,
            timeout: timeout.as_millis().clamp(1, u32::MAX as u128) as u32,
            data: bytes.as_mut_ptr().cast(),
        };
        ioctl(self, ioc(3, 2, std::mem::size_of::<Bulk>()), &mut request).map(|n| n as usize)
    }
}
fn open(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC)
        .open(path)
}
pub fn valid_i2c_path(path: &str) -> bool {
    path.strip_prefix("/dev/i2c-")
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
pub struct NativeI2c {
    driver: Mutex<Box<dyn Driver>>,
}
impl NativeI2c {
    /// I2C_FUNCS must advertise raw I2C, since SMBus-only adapters cannot carry this protocol.
    pub fn open(path: &str) -> Result<Self> {
        if !valid_i2c_path(path) {
            return Err(invalid("I2C device path must be /dev/i2c-N"));
        }
        Self::from_driver(Box::new(open(Path::new(path))?))
    }
    fn from_driver(driver: Box<dyn Driver>) -> Result<Self> {
        let functions = driver.functions()?;
        if functions & 1 == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Adapter does not support raw I2C",
            ));
        }
        Ok(Self {
            driver: Mutex::new(driver),
        })
    }
}
#[repr(C)]
struct Message {
    address: u16,
    flags: u16,
    length: u16,
    buffer: *mut u8,
}
#[repr(C)]
struct Transfer {
    messages: *mut Message,
    count: u32,
}
impl I2c for NativeI2c {
    fn transaction(&self, address: u8, write: &[u8], read: usize) -> Result<Vec<u8>> {
        if address > 127 || write.len() > 65535 || read > 65535 || write.is_empty() && read == 0 {
            return Err(invalid("Invalid I2C transaction"));
        }
        let mut bytes = vec![0; read];
        let mut messages = Vec::with_capacity(2);
        if !write.is_empty() {
            messages.push(Message {
                address: address.into(),
                flags: 0,
                length: write.len() as u16,
                buffer: write.as_ptr() as *mut u8,
            });
        }
        if read > 0 {
            messages.push(Message {
                address: address.into(),
                flags: 1,
                length: read as u16,
                buffer: bytes.as_mut_ptr(),
            });
        }
        let result = self.driver.lock().unwrap().transfer(&mut messages)?;
        if result != messages.len() as i32 {
            return Err(std::io::Error::other("Incomplete I2C transfer"));
        }
        Ok(bytes)
    }
}
/// USB identity and endpoint information only; never collect serial numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbDevice {
    pub path: PathBuf,
    pub interface: u32,
    pub input: u8,
    pub output: u8,
    pub packet: usize,
}
fn number(path: &Path, name: &str, radix: u32) -> Option<u32> {
    u32::from_str_radix(std::fs::read_to_string(path.join(name)).ok()?.trim(), radix).ok()
}
/// Match only deployment-approved USB identities and an interface containing both bulk endpoints.
pub fn discover(sysfs: &Path, devfs: &Path, allowed: &[(u16, u16)]) -> Result<Vec<UsbDevice>> {
    let mut devices = Vec::new();
    for entry in std::fs::read_dir(sysfs)? {
        let path = entry?.path();
        let Some(vendor) = number(&path, "idVendor", 16) else {
            continue;
        };
        let Some(product) = number(&path, "idProduct", 16) else {
            continue;
        };
        if !allowed.contains(&(vendor as u16, product as u16)) {
            continue;
        }
        let Some(bus) = number(&path, "busnum", 10) else {
            continue;
        };
        let Some(device) = number(&path, "devnum", 10) else {
            continue;
        };
        // usbfs exposes the device/configuration descriptors through an ordinary read.
        let node = devfs.join(format!("{bus:03}/{device:03}"));
        let descriptors = std::fs::read(&node)?;
        if let Some((interface, input, output, packet)) = endpoints(&descriptors) {
            devices.push(UsbDevice {
                path: node,
                interface,
                input,
                output,
                packet,
            });
        }
    }
    devices.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(devices)
}
fn endpoints(bytes: &[u8]) -> Option<(u32, u8, u8, usize)> {
    let (mut interface, mut input, mut output, mut packet) = (None, None, None, 32);
    let mut offset = 0;
    while offset + 2 <= bytes.len() {
        let length = bytes[offset] as usize;
        if length < 2 || offset + length > bytes.len() {
            return None;
        }
        let d = &bytes[offset..offset + length];
        match (d[1], length) {
            (4, 9..) => {
                if let (Some(i), Some(r), Some(w)) = (interface, input, output) {
                    return Some((i, r, w, packet));
                }
                interface = (d[3] == 0).then_some(d[2] as u32);
                input = None;
                output = None;
            }
            (5, 7..) if interface.is_some() && d[3] & 3 == 2 => {
                if d[2] & 128 != 0 && input.is_none() {
                    input = Some(d[2]);
                    packet = (u16::from_le_bytes([d[4], d[5]]) as usize).clamp(1, 32);
                }
                if d[2] & 128 == 0 && output.is_none() {
                    output = Some(d[2]);
                }
            }
            _ => (),
        }
        offset += length;
    }
    Some((interface?, input?, output?, packet))
}
#[repr(C)]
struct Bulk {
    endpoint: u32,
    length: u32,
    timeout: u32,
    data: *mut libc::c_void,
}
// Linux generic ioctl encoding (aarch64 and x86 Linux); size follows the target pointer width.
const fn ioc(direction: u32, number: u32, size: usize) -> libc::c_ulong {
    ((direction << 30) | ((size as u32) << 16) | ((b'U' as u32) << 8) | number) as libc::c_ulong
}
pub struct Usbfs {
    driver: Box<dyn Driver>,
    device: UsbDevice,
}
impl Usbfs {
    pub fn probe(device: &UsbDevice) -> Result<()> {
        open(&device.path).map(|_| ())
    }
    pub fn open(device: UsbDevice) -> Result<Self> {
        let file = open(&device.path)?;
        Self::from_driver(device, Box::new(file))
    }
    fn from_driver(device: UsbDevice, driver: Box<dyn Driver>) -> Result<Self> {
        // Refuse a busy interface instead of detaching an unrelated kernel driver.
        driver.interface(15, device.interface)?;
        Ok(Self { driver, device })
    }
    fn bulk(&self, endpoint: u8, bytes: &mut [u8], timeout: Duration) -> Result<usize> {
        let result = self.driver.bulk(endpoint, bytes, timeout);
        if result.is_err() {
            let _ = self.driver.interface(21, endpoint.into());
        }
        result
    }
}
impl Drop for Usbfs {
    fn drop(&mut self) {
        let _ = self.driver.interface(16, self.device.interface);
    }
}
impl Usb for Usbfs {
    fn write(&mut self, bytes: &[u8], timeout: Duration) -> Result<()> {
        let mut bytes = bytes.to_vec();
        if self.bulk(self.device.output, &mut bytes, timeout)? != bytes.len() {
            return Err(std::io::Error::other("Incomplete CH341 USB write"));
        }
        Ok(())
    }
    fn read(
        &mut self,
        maximum: usize,
        first: Duration,
        quiet: Duration,
        allow_empty: bool,
    ) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(maximum);
        while result.len() < maximum {
            let mut packet = vec![0; (maximum - result.len()).min(self.device.packet)];
            match self.bulk(
                self.device.input,
                &mut packet,
                if result.is_empty() { first } else { quiet },
            ) {
                Ok(n) if n > 0 && n <= packet.len() => result.extend(&packet[..n]),
                Ok(_) if !result.is_empty() || allow_empty => break,
                Err(e)
                    if (e.raw_os_error() == Some(libc::ETIMEDOUT))
                        && (!result.is_empty() || allow_empty) =>
                {
                    break
                }
                Err(e) => return Err(e),
                _ => return Err(std::io::Error::other("CH341 USB read returned no data")),
            }
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Arc};
    #[derive(Clone)]
    struct Fake {
        functions: libc::c_ulong,
        state: Arc<Mutex<VecDeque<Step>>>,
    }
    enum Step {
        I2c(u8, Vec<u8>, Vec<u8>, i32),
        Interface(u32, u32),
        Bulk(u8, usize, u128, Result<Vec<u8>>),
    }
    impl Driver for Fake {
        fn functions(&self) -> Result<libc::c_ulong> {
            Ok(self.functions)
        }
        fn transfer(&self, messages: &mut [Message]) -> Result<i32> {
            let Some(Step::I2c(address, write, read, result)) =
                self.state.lock().unwrap().pop_front()
            else {
                panic!("Unexpected I2C transfer")
            };
            for message in messages {
                assert_eq!(message.address, address as u16);
                // SAFETY: the production adapter keeps each request's backing buffers alive.
                if message.flags == 0 {
                    assert_eq!(
                        unsafe {
                            std::slice::from_raw_parts(message.buffer, message.length as usize)
                        },
                        write
                    );
                } else {
                    let bytes = unsafe {
                        std::slice::from_raw_parts_mut(message.buffer, message.length as usize)
                    };
                    assert_eq!(bytes.len(), read.len());
                    bytes.copy_from_slice(&read);
                }
            }
            Ok(result)
        }
        fn interface(&self, operation: u32, value: u32) -> Result<()> {
            let Some(Step::Interface(expected, number)) = self.state.lock().unwrap().pop_front()
            else {
                panic!("Unexpected USB interface operation")
            };
            assert_eq!((operation, value), (expected, number));
            Ok(())
        }
        fn bulk(&self, endpoint: u8, bytes: &mut [u8], timeout: Duration) -> Result<usize> {
            let Some(Step::Bulk(expected, len, ms, result)) =
                self.state.lock().unwrap().pop_front()
            else {
                panic!("Unexpected USB bulk operation")
            };
            assert_eq!(
                (endpoint, bytes.len(), timeout.as_millis()),
                (expected, len, ms)
            );
            let response = result?;
            bytes[..response.len()].copy_from_slice(&response);
            Ok(response.len())
        }
    }
    fn fake(steps: Vec<Step>) -> Fake {
        Fake {
            functions: 1,
            state: Arc::new(Mutex::new(steps.into())),
        }
    }
    fn device() -> UsbDevice {
        UsbDevice {
            path: PathBuf::new(),
            interface: 2,
            input: 0x82,
            output: 2,
            packet: 32,
        }
    }
    #[test]
    fn discovery_matches_only_configured_identity_and_bulk_interface() {
        let root = std::env::temp_dir().join(format!(
            "mfi-usbfs-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sys = root.join("sys");
        let dev = root.join("dev");
        let node = sys.join("1-2");
        std::fs::create_dir_all(&node).unwrap();
        std::fs::create_dir_all(dev.join("001")).unwrap();
        for (key, value) in [
            ("idVendor", "1a86"),
            ("idProduct", "5512"),
            ("busnum", "1"),
            ("devnum", "2"),
        ] {
            std::fs::write(node.join(key), value).unwrap();
        }
        let descriptor = [
            9, 4, 0, 0, 2, 255, 0, 0, 0, 7, 5, 0x82, 2, 32, 0, 0, 7, 5, 2, 2, 32, 0, 0,
        ];
        std::fs::write(dev.join("001/002"), descriptor).unwrap();
        assert!(discover(&sys, &dev, &[(0x1a86, 0x5523)])
            .unwrap()
            .is_empty());
        let found = discover(&sys, &dev, &[(0x1a86, 0x5512)]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].interface, found[0].input, found[0].output),
            (0, 0x82, 2)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_probe_and_ioctl_messages_preserve_separate_stop_transactions() {
        let driver = fake(vec![
            Step::I2c(0x11, vec![2], vec![], 1),
            Step::I2c(0x11, vec![], vec![3], 1),
            Step::I2c(0x11, vec![5], vec![], 0),
        ]);
        let native = NativeI2c::from_driver(Box::new(driver.clone())).unwrap();
        assert!(native.transaction(0x11, &[2], 0).unwrap().is_empty());
        assert_eq!(native.transaction(0x11, &[], 1).unwrap(), [3]);
        assert!(native.transaction(0x11, &[5], 0).is_err());
        assert!(native.transaction(128, &[2], 0).is_err());
        assert!(native.transaction(0x11, &[], 65536).is_err());
        assert!(driver.state.lock().unwrap().is_empty());
        let smbus = Fake {
            functions: 0,
            ..fake(vec![])
        };
        assert!(
            matches!(NativeI2c::from_driver(Box::new(smbus)),Err(e) if e.kind()==std::io::ErrorKind::Unsupported)
        );
    }
    #[test]
    fn usbfs_packets_quiet_timeout_halt_recovery_and_release() {
        let driver = fake(vec![
            Step::Interface(15, 2),
            Step::Bulk(0x82, 32, 1000, Ok(vec![7; 32])),
            Step::Bulk(
                0x82,
                1,
                25,
                Err(std::io::Error::from_raw_os_error(libc::ETIMEDOUT)),
            ),
            Step::Interface(21, 0x82),
            Step::Interface(16, 2),
        ]);
        let mut usb = Usbfs::from_driver(device(), Box::new(driver.clone())).unwrap();
        assert_eq!(
            usb.read(33, Duration::from_secs(1), Duration::from_millis(25), false)
                .unwrap(),
            vec![7; 32]
        );
        drop(usb);
        assert!(driver.state.lock().unwrap().is_empty());
    }
    #[test]
    fn usbfs_fatal_disconnect_is_not_accepted_as_a_short_answer() {
        let driver = fake(vec![
            Step::Interface(15, 2),
            Step::Bulk(
                0x82,
                2,
                1000,
                Err(std::io::Error::from_raw_os_error(libc::ENODEV)),
            ),
            Step::Interface(21, 0x82),
            Step::Interface(16, 2),
        ]);
        let mut usb = Usbfs::from_driver(device(), Box::new(driver.clone())).unwrap();
        assert!(usb
            .read(2, Duration::from_secs(1), Duration::from_millis(25), true)
            .is_err());
        drop(usb);
        assert!(driver.state.lock().unwrap().is_empty());
    }

    #[test]
    fn usb_descriptors_require_bulk_pair_on_one_interface() {
        let bytes = [
            9, 4, 2, 0, 2, 255, 0, 0, 0, 7, 5, 0x82, 2, 32, 0, 0, 7, 5, 2, 2, 32, 0, 0,
        ];
        assert_eq!(endpoints(&bytes), Some((2, 0x82, 2, 32)));
        assert_eq!(endpoints(&bytes[..18]), None);
        assert_eq!(endpoints(&[0, 4]), None);
        assert_eq!(
            ioc(3, 2, std::mem::size_of::<Bulk>()) as u32,
            if cfg!(target_pointer_width = "64") {
                0xc0185502
            } else {
                0xc0105502
            }
        );
    }
}
