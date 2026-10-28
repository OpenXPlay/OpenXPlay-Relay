// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Portable USB host contracts and the iPhone mode/USBMUX pipe operations.
use super::iphone_car_play_configuration::{
    IphoneCarPlayConfiguration, UsbConfiguration, UsbDescriptors, UsbEndpoint,
};
use super::usb_configuration_selection::{UsbConfigurationOutcome, UsbConfigurationSelection};
use crate::{DuplexStream, Receive, Result};
use std::{
    fmt, io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

/// Identity of one Apple enumeration; a new device number represents a new session node.
#[derive(Clone, Debug, PartialEq)]
pub struct UsbDeviceInfo {
    pub bus_number: u16,
    pub device_number: u16,
    pub sysfs_path: PathBuf,
    pub speed_mbps: f64,
    pub product: Option<String>,
    pub serial: Option<String>,
}
/// USB failures keep unplug and another userspace owner's busy claim distinguishable.
#[derive(Debug)]
pub enum UsbError {
    DeviceGone,
    InterfaceBusy(u8),
    Timeout,
    Closed,
    Protocol(String),
    Io(io::Error),
}
impl UsbError {
    pub fn is_device_gone(&self) -> bool {
        matches!(self, Self::DeviceGone)
    }
}
impl fmt::Display for UsbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceGone => f.write_str("iPhone unplugged"),
            Self::InterfaceBusy(i) => write!(
                f,
                "another program (usbmuxd?) holds the iPhone (interface {i})"
            ),
            Self::Timeout => f.write_str("USB transfer timed out"),
            Self::Closed => f.write_str("USB device is closed"),
            Self::Protocol(s) => f.write_str(s),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for UsbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(e) = self {
            Some(e)
        } else {
            None
        }
    }
}
impl From<io::Error> for UsbError {
    fn from(e: io::Error) -> Self {
        match e.raw_os_error() {
            Some(19 | 108) => Self::DeviceGone,
            _ if e.kind() == io::ErrorKind::TimedOut => Self::Timeout,
            _ => Self::Io(e),
        }
    }
}
impl From<UsbError> for io::Error {
    fn from(e: UsbError) -> Self {
        let kind = match e {
            UsbError::Timeout => io::ErrorKind::TimedOut,
            UsbError::DeviceGone | UsbError::Closed => io::ErrorKind::NotConnected,
            UsbError::InterfaceBusy(_) => io::ErrorKind::ResourceBusy,
            UsbError::Protocol(_) => io::ErrorKind::InvalidData,
            UsbError::Io(ref e) => e.kind(),
        };
        Self::new(kind, e)
    }
}
pub type UsbResult<T> = std::result::Result<T, UsbError>;
/// Enumerates Apple devices and opens one enumeration. No permission prompts or mode changes.
pub trait UsbHostBus: Send + Sync {
    fn enumerate(&self) -> UsbResult<Vec<UsbDeviceInfo>>;
    fn open(&self, device: &UsbDeviceInfo) -> UsbResult<Arc<dyn UsbHostDevice>>;
}
/// Thread-safe shared handle for USBMUX and NCM. IN timeouts retain both queued requests and bytes.
/// `close` cancels/reaps pending IN requests; completed bytes remain readable before EOF.
/// Releasing an interface must not reattach its kernel driver during the session.
pub trait UsbHostDevice: Send + Sync {
    /// Raw device and all configuration descriptors, including class-specific bytes.
    fn raw_descriptors(&self) -> UsbResult<Vec<u8>>;
    /// Standard GET_CONFIGURATION; None only for a readable short response.
    fn active_configuration(&self) -> UsbResult<Option<u8>>;
    /// Raw SET_CONFIGURATION. Use UsbConfigurationSelection for detach-and-retry.
    fn set_configuration(&self, value: u8) -> UsbResult<()>;
    /// Detach a bound kernel driver without reattaching it or retaining a claim.
    fn detach_kernel_driver(&self, interface: u8) -> UsbResult<()>;
    /// Claim an interface, detaching its kernel driver; EBUSY becomes InterfaceBusy.
    fn claim_interface(&self, interface: u8) -> UsbResult<()>;
    /// Release a userspace claim without requesting kernel-driver reattachment.
    fn release_interface(&self, interface: u8) -> UsbResult<()>;
    /// Select an alternate setting on an already claimed interface.
    fn set_alternate_setting(&self, interface: u8, alternate: u8) -> UsbResult<()>;
    /// Bounded control IN, returning exactly the bytes transferred, including short replies.
    fn control_in(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
        timeout: Duration,
    ) -> UsbResult<Vec<u8>>;
    /// Bounded control OUT, returning the actual transferred length.
    fn control_out(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
        timeout: Duration,
    ) -> UsbResult<usize>;
    /// Bounded bulk OUT; a short write remains visible to the protocol owner.
    fn bulk_out(&self, endpoint: u8, data: &[u8], timeout: Duration) -> UsbResult<usize>;
    /// Bulk IN. Timeout keeps the pending request and every byte already received.
    fn bulk_in(&self, endpoint: u8, max_bytes: usize, timeout: Duration) -> UsbResult<Receive>;
    /// Interrupt IN with the same lossless timeout contract as bulk IN.
    fn interrupt_in(&self, endpoint: u8, max_bytes: usize, timeout: Duration)
        -> UsbResult<Receive>;
    /// Clear an endpoint stall when the protocol owner elects to retry.
    fn clear_halt(&self, endpoint: u8) -> UsbResult<()>;
    /// Enumeration link speed in megabits per second; zero means unknown.
    fn link_speed_mbps(&self) -> f64;
    /// Cancel and reap IN requests and close the handle. Buffered completions precede EOF.
    fn close(&self) -> UsbResult<()>;
}
#[derive(Debug)]
pub enum CarPlayModeOutcome {
    ReenumerationRequested,
    Rejected {
        transferred: isize,
        diagnostic: String,
    },
}
pub struct IphoneUsbHost;
impl IphoneUsbHost {
    pub fn discover(device: &dyn UsbHostDevice) -> UsbResult<UsbDescriptors> {
        Ok(UsbDescriptors::parse(&device.raw_descriptors()?)?)
    }
    /// Does not close the shared handle; the lifecycle owner closes it before awaiting re-enumeration.
    pub fn request_car_play_mode(
        device: &dyn UsbHostDevice,
        descriptors: &UsbDescriptors,
    ) -> UsbResult<CarPlayModeOutcome> {
        let transferred = match device.control_in(0xc0, 0x52, 0, 4, 1, Duration::from_secs(1)) {
            Ok(v) => v.len() as isize,
            Err(e) if e.is_device_gone() => return Err(e),
            Err(_) => -1,
        };
        if transferred == 1 {
            return Ok(CarPlayModeOutcome::ReenumerationRequested);
        }
        let (read, mode) = match device.control_in(0xc0, 0x45, 0, 0, 4, Duration::from_secs(1)) {
            Ok(v) => (v.len() as isize, v),
            Err(e) if e.is_device_gone() => return Err(e),
            Err(_) => (-1, vec![]),
        };
        let configs = descriptors
            .configurations
            .iter()
            .map(|c| format!("{}[{}]", c.value, IphoneCarPlayConfiguration::describe(c)))
            .collect::<Vec<_>>();
        Ok(CarPlayModeOutcome::Rejected {
            transferred,
            diagnostic: Self::usb_mode_diagnostic(
                read,
                &mode,
                descriptors.usb_version().as_deref(),
                &configs,
            ),
        })
    }
    pub fn usb_mode_diagnostic(
        read: isize,
        mode: &[u8],
        version: Option<&str>,
        configurations: &[String],
    ) -> String {
        let n = (read.max(0) as usize).min(mode.len());
        let bytes = mode[..n]
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(":");
        let current = if read == 4 {
            bytes
        } else if (1..4).contains(&read) {
            format!("{bytes} (short:{read})")
        } else {
            format!("unreadable({read})")
        };
        format!("CarPlay USB mode request rejected; current mode={current} usbVersion={} configurations=[{}]",version.unwrap_or("unknown"),configurations.join(", "))
    }
    pub fn select_configuration(
        device: &dyn UsbHostDevice,
        descriptors: &UsbDescriptors,
        target: &UsbConfiguration,
    ) -> UsbResult<Option<UsbConfigurationOutcome>> {
        let ids = descriptors
            .configurations
            .iter()
            .flat_map(|c| c.interfaces.iter().map(|i| i.number))
            .collect::<Vec<_>>();
        let result = UsbConfigurationSelection::select(device, target.value, &ids)?;
        if result.as_ref().is_some_and(|o| o.wrong_configuration()) {
            return Err(UsbError::Protocol(result.unwrap().diagnostic()));
        }
        // Keep Apple Ethernet claimed so ipheth cannot bind while userspace drives NCM.
        let mut claimed = std::collections::BTreeSet::new();
        for i in &target.interfaces {
            if (i.class, i.subclass, i.protocol) == (0xff, 0xfd, 1) && claimed.insert(i.number) {
                device.claim_interface(i.number)?;
            }
        }
        Ok(result)
    }
}
/// A claimed USBMUX endpoint pair; closing it releases only that interface, not the shared device.
pub struct Iap2UsbSession {
    device: Arc<dyn UsbHostDevice>,
    interface: u8,
    output: UsbEndpoint,
    input: UsbEndpoint,
    closed: AtomicBool,
    read: Mutex<()>,
    write: Mutex<()>,
}
impl Iap2UsbSession {
    pub fn open(
        device: Arc<dyn UsbHostDevice>,
        configuration: &UsbConfiguration,
    ) -> UsbResult<Arc<Self>> {
        let interface =
            IphoneCarPlayConfiguration::usb_mux_interface(configuration).ok_or_else(|| {
                UsbError::Protocol("CarPlay configuration exposes no USBMUX interface".into())
            })?;
        let (output, input) =
            IphoneCarPlayConfiguration::usb_mux_endpoints(interface).ok_or_else(|| {
                UsbError::Protocol("USBMUX interface exposes no bulk endpoint pair".into())
            })?;
        device.claim_interface(interface.number)?;
        Ok(Arc::new(Self {
            device,
            interface: interface.number,
            output,
            input,
            closed: AtomicBool::new(false),
            read: Mutex::new(()),
            write: Mutex::new(()),
        }))
    }
    pub fn write(&self, data: &[u8], timeout: Duration) -> UsbResult<()> {
        let _lock = self.write.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Err(UsbError::Closed);
        }
        if data.is_empty() {
            return Ok(());
        }
        let n = self.device.bulk_out(self.output.address, data, timeout)?;
        if n != data.len() {
            return Err(UsbError::Protocol(format!(
                "USBMUX write transferred {n} of {} bytes",
                data.len()
            )));
        }
        Ok(())
    }
}
impl DuplexStream for Iap2UsbSession {
    fn send(&self, data: &[u8]) -> Result<()> {
        Ok(self.write(data, Duration::from_secs(2))?)
    }
    fn recv(&self, max_bytes: usize, timeout: Duration) -> Result<Receive> {
        let _lock = self.read.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Ok(Receive::Eof);
        }
        Ok(self
            .device
            .bulk_in(self.input.address, max_bytes, timeout)?)
    }
    fn close(&self) -> Result<()> {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.device.release_interface(self.interface)?;
        }
        Ok(())
    }
}

impl Drop for Iap2UsbSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
