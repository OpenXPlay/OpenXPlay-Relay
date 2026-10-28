// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Ethernet frame transport over CDC-NCM with bounded queues and damaged-block recovery.
use super::{
    iphone_usb_host::{UsbError, UsbHostDevice, UsbResult},
    ncm_function_discovery::NcmFunction,
    ntb16_codec::{u16le, u32le, Ntb16Codec, NTH16_SIG},
};
use crate::{invalid, Receive, Result};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
type Diagnostic = Arc<dyn Fn(String) + Send + Sync>;
/// Incremental NTB stream decoder, also useful for deterministic completion replay tests.
pub struct NcmFrameBuffer {
    bytes: Vec<u8>,
    frames: VecDeque<Vec<u8>>,
    queued: usize,
    pad: bool,
    packet: usize,
    resync_times: VecDeque<Instant>,
    resync_count: usize,
    resync_reason: Option<String>,
    skipped: usize,
    failure: Option<String>,
    diagnostic: Diagnostic,
}
impl NcmFrameBuffer {
    pub fn new(packet_size: u16, diagnostic: Diagnostic) -> Self {
        Self {
            bytes: vec![],
            frames: VecDeque::new(),
            queued: 0,
            pad: false,
            packet: usize::from(packet_size.max(1)),
            resync_times: VecDeque::new(),
            resync_count: 0,
            resync_reason: None,
            skipped: 0,
            failure: None,
            diagnostic,
        }
    }
    pub fn append(&mut self, bytes: &[u8]) -> Result<()> {
        self.check()?;
        if self.bytes.len() + bytes.len() > 131072 {
            return self.fail("NCM read buffer exceeded its bounds".into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub fn take_frame(&mut self) -> Result<Option<Vec<u8>>> {
        self.check()?;
        if self.frames.is_empty() {
            self.drain()?;
        }
        let frame = self.frames.pop_front();
        if let Some(f) = &frame {
            self.queued -= f.len();
        }
        Ok(frame)
    }
    fn check(&self) -> Result<()> {
        if let Some(e) = &self.failure {
            Err(invalid(e.clone()))
        } else {
            Ok(())
        }
    }
    fn fail<T>(&mut self, message: String) -> Result<T> {
        self.failure = Some(message.clone());
        Err(invalid(message))
    }
    fn drain(&mut self) -> Result<()> {
        loop {
            if self.pad && !self.bytes.is_empty() {
                self.pad = false;
                match self.bytes[0] {
                    0 => {
                        self.bytes.drain(..1);
                    }
                    0x4e => (),
                    _ => {
                        if !self.resync("Invalid NTB16 short-packet pad".into())? {
                            return Ok(());
                        }
                    }
                }
            }
            if self.bytes.len() < 12 {
                return Ok(());
            }
            let length = u16le(&self.bytes, 8);
            if u32le(&self.bytes, 0) != NTH16_SIG {
                if !self.resync("NCM read buffer does not begin with an NTB16 header".into())? {
                    return Ok(());
                }
                continue;
            }
            if length < 28 {
                if !self.resync(format!("Invalid NTB16 block length {length}"))? {
                    return Ok(());
                }
                continue;
            }
            if self.bytes.len() < length {
                return Ok(());
            }
            for frame in Ntb16Codec::parse(&self.bytes[..length]) {
                if self.frames.len() >= 256 || self.queued + frame.len() > 1 << 20 {
                    return self.fail("NCM frame queue exceeded its bounds".into());
                }
                self.queued += frame.len();
                self.frames.push_back(frame);
            }
            self.bytes.drain(..length);
            self.pad = length.is_multiple_of(self.packet);
            if let Some(reason) = self.resync_reason.take() {
                if self.resync_count <= 5 || self.resync_count.is_multiple_of(50) {
                    report(
                        &self.diagnostic,
                        format!(
                            "NCM resynchronized after {reason}; skipped={} resyncs={}",
                            self.skipped, self.resync_count
                        ),
                    );
                }
            }
        }
    }
    fn resync(&mut self, reason: String) -> Result<bool> {
        if self.resync_reason.is_none() {
            let now = Instant::now();
            while self
                .resync_times
                .front()
                .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(10))
            {
                self.resync_times.pop_front();
            }
            if self.resync_times.len() >= 16 {
                return self.fail(format!("{reason} (repeated)"));
            }
            self.resync_times.push_back(now);
            self.resync_count += 1;
            self.resync_reason = Some(reason);
            self.skipped = 0;
        }
        let next = (1..=self.bytes.len().saturating_sub(12)).find(|&o| {
            let b = &self.bytes;
            u32le(b, o) == NTH16_SIG
                && u16le(b, o + 4) == 12
                && u16le(b, o + 8) >= 28
                && u16le(b, o + 10) >= 12
                && u16le(b, o + 10).is_multiple_of(4)
                && u16le(b, o + 10) + 8 <= u16le(b, o + 8)
        });
        let skip = next.unwrap_or_else(|| self.bytes.len().saturating_sub(11));
        self.bytes.drain(..skip);
        self.skipped += skip;
        Ok(next.is_some())
    }
}
struct Writer {
    sequence: u16,
    accepted: bool,
    dropped: usize,
}
pub struct NcmUsbBridge {
    device: Arc<dyn UsbHostDevice>,
    function: NcmFunction,
    mac: [u8; 6],
    claimed: Vec<u8>,
    closed: AtomicBool,
    reader: Mutex<NcmFrameBuffer>,
    writer: Mutex<Writer>,
    diagnostic: Diagnostic,
    status: Mutex<Option<thread::JoinHandle<()>>>,
}
impl NcmUsbBridge {
    /// Claims NCM on the caller's shared configured handle; descriptor MAC takes priority over fallback.
    pub fn open(
        device: Arc<dyn UsbHostDevice>,
        function: NcmFunction,
        fallback_mac: [u8; 6],
        diagnostic: Diagnostic,
    ) -> UsbResult<Arc<Self>> {
        let mac = read_host_mac(device.as_ref(), &function)?.unwrap_or(fallback_mac);
        let mut claimed = Vec::new();
        let setup = (|| {
            let first = if function.control.number == function.data.number {
                function.data.number
            } else {
                function.control.number
            };
            retry_usb_setup(|| device.claim_interface(first), thread::sleep)?;
            claimed.push(first);
            if first != function.data.number {
                retry_usb_setup(
                    || device.claim_interface(function.data.number),
                    thread::sleep,
                )?;
                claimed.push(function.data.number);
            }
            retry_usb_setup(
                || {
                    device.set_alternate_setting(
                        function.data.number,
                        function.data.alternate_setting,
                    )
                },
                thread::sleep,
            )?;
            Ok(())
        })();
        if let Err(e) = setup {
            for i in claimed.iter().rev() {
                let _ = device.release_interface(*i);
            }
            return Err(e);
        }
        let packet = function.bulk_in.max_packet_size.max(1);
        let result = Arc::new(Self {
            device,
            function,
            mac,
            claimed,
            closed: AtomicBool::new(false),
            reader: Mutex::new(NcmFrameBuffer::new(packet, diagnostic.clone())),
            writer: Mutex::new(Writer {
                sequence: 0,
                accepted: false,
                dropped: 0,
            }),
            diagnostic,
            status: Mutex::new(None),
        });
        if let Some(endpoint) = result.function.status_in.clone() {
            let weak = Arc::downgrade(&result);
            let worker = thread::Builder::new()
                .name("ncm-status-in".into())
                .spawn(move || {
                    while let Some(bridge) = weak.upgrade() {
                        if bridge.closed.load(Ordering::Acquire) {
                            break;
                        }
                        if let Err(e) = bridge.device.interrupt_in(
                            endpoint.address,
                            usize::from(endpoint.max_packet_size.max(64)),
                            Duration::from_millis(20),
                        ) {
                            if e.is_device_gone() {
                                break;
                            }
                        }
                        drop(bridge);
                        thread::sleep(Duration::from_millis(500));
                    }
                })?;
            *result.status.lock().unwrap() = Some(worker);
        }
        Ok(result)
    }
    pub fn host_mac(&self) -> [u8; 6] {
        self.mac
    }
    /// A NAK timeout drops this Ethernet frame while retaining the usable bridge for retry.
    pub fn send(&self, frame: &[u8], timeout: Duration) -> UsbResult<()> {
        self.check()?;
        if timeout.is_zero() {
            return Err(UsbError::Protocol("timeoutMillis must be positive".into()));
        }
        let mut w = self.writer.lock().unwrap();
        let block = Ntb16Codec::build(
            frame,
            w.sequence,
            self.function.bulk_out.max_packet_size.max(1),
        )?;
        w.sequence = w.sequence.wrapping_add(1);
        let effective = if w.accepted {
            timeout
        } else {
            timeout.min(Duration::from_millis(20))
        };
        let n = match self
            .device
            .bulk_out(self.function.bulk_out.address, &block, effective)
        {
            Err(UsbError::Timeout) => 0,
            r => r?,
        };
        if n == 0 {
            if !w.accepted {
                w.dropped += 1;
            }
            return Ok(());
        }
        let message = if !w.accepted {
            w.accepted = true;
            Some(format!(
                "NCM bulk-out ready after {} dropped frames",
                w.dropped
            ))
        } else {
            None
        };
        if n != block.len() {
            return Err(UsbError::Protocol(format!(
                "NCM write transferred {n} of {} bytes",
                block.len()
            )));
        }
        drop(w);
        if let Some(message) = message {
            report(&self.diagnostic, message);
        }
        Ok(())
    }
    pub fn recv(&self, timeout: Duration) -> UsbResult<Option<Vec<u8>>> {
        self.check()?;
        let deadline = Instant::now() + timeout;
        let mut r = self.reader.lock().unwrap();
        loop {
            self.check()?;
            if let Some(f) = r.take_frame()? {
                return Ok(Some(f));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            match self
                .device
                .bulk_in(self.function.bulk_in.address, 32768, remaining)?
            {
                Receive::Data(b) => r.append(&b)?,
                Receive::Timeout => return Ok(None),
                Receive::Eof => return Err(UsbError::Closed),
            }
        }
    }
    pub fn close(&self) -> UsbResult<()> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        if let Some(t) = self.status.lock().unwrap().take() {
            if t.thread().id() != thread::current().id() {
                let _ = t.join();
            }
        }
        let mut failure = None;
        for i in self.claimed.iter().rev() {
            if let Err(e) = self.device.release_interface(*i) {
                failure.get_or_insert(e);
            }
        }
        failure.map_or(Ok(()), Err)
    }
    fn check(&self) -> UsbResult<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(UsbError::Closed)
        } else {
            Ok(())
        }
    }
}
impl Drop for NcmUsbBridge {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
/// Five setup attempts, spaced 100 ms apart; unplug is immediately terminal.
pub fn retry_usb_setup(
    mut action: impl FnMut() -> UsbResult<()>,
    mut pause: impl FnMut(Duration),
) -> UsbResult<usize> {
    for attempt in 1..=5 {
        match action() {
            Ok(()) => return Ok(attempt),
            Err(e) if e.is_device_gone() || attempt == 5 => return Err(e),
            Err(_) => pause(Duration::from_millis(100)),
        }
    }
    unreachable!()
}
fn read_host_mac(device: &dyn UsbHostDevice, function: &NcmFunction) -> UsbResult<Option<[u8; 6]>> {
    let mut b = function.control.extra.as_slice();
    let mut index = None;
    while b.len() >= 2 {
        let n = b[0] as usize;
        if n < 2 || n > b.len() {
            break;
        }
        if n >= 4 && b[1] == 0x24 && b[2] == 0x0f && b[3] != 0 {
            index = Some(b[3]);
            break;
        }
        b = &b[n..];
    }
    let Some(index) = index else {
        return Ok(None);
    };
    let bytes = match device.control_in(
        0x80,
        6,
        0x300 | u16::from(index),
        0x409,
        256,
        Duration::from_secs(1),
    ) {
        Err(e) if e.is_device_gone() => return Err(e),
        Err(_) => return Ok(None),
        Ok(v) => v,
    };
    if bytes.len() < 4 || bytes[1] != 3 {
        return Ok(None);
    }
    let n = (bytes[0] as usize).min(bytes.len());
    if n < 4 {
        return Ok(None);
    }
    let text = String::from_utf16_lossy(
        &bytes[2..n]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>(),
    );
    let hex = text
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>();
    if hex.len() != 12 {
        return Ok(None);
    }
    let mut mac = [0; 6];
    for (i, v) in mac.iter_mut().enumerate() {
        *v = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
            .map_err(|_| invalid("Invalid NCM MAC descriptor"))?;
    }
    Ok(Some(mac))
}
fn report(callback: &Diagnostic, line: String) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(line)));
}
