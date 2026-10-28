// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-27
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

//! CH341 stream framing and transport. USB transfers are supplied by the host backend.
use super::coprocessor::I2c;
use crate::{invalid, Result};
use std::{sync::Mutex, time::Duration};
pub trait Usb: Send {
    fn write(&mut self, bytes: &[u8], timeout: Duration) -> Result<()>;
    fn read(
        &mut self,
        maximum: usize,
        first: Duration,
        quiet: Duration,
        allow_empty: bool,
    ) -> Result<Vec<u8>>;
}
pub struct Ch341 {
    usb: Mutex<Box<dyn Usb>>,
}
impl Ch341 {
    pub fn new(usb: Box<dyn Usb>) -> Self {
        Self {
            usb: Mutex::new(usb),
        }
    }
}
fn validate(address: u8, write: &[u8], read: usize) -> Result<()> {
    if address > 127 || write.len() > 65535 || read > 65535 || write.is_empty() && read == 0 {
        return Err(invalid("Invalid I2C transaction"));
    }
    Ok(())
}
pub fn encode(address: u8, write: &[u8], read: usize) -> Result<Vec<u8>> {
    validate(address, write, read)?;
    let mut legacy = vec![0xaa];
    if !write.is_empty() {
        legacy.push(0x74);
        for b in std::iter::once(address << 1).chain(write.iter().copied()) {
            legacy.extend([0x80, b]);
        }
    }
    if read > 0 {
        legacy.extend([0x74, 0x80, (address << 1) | 1]);
        if read > 1 && read <= 32 {
            legacy.push(0xc0 + read as u8 - 1);
        }
        legacy.push(0xc0);
    }
    legacy.extend([0x75, 0]);
    if read <= 32 && write.len() < 63 && legacy.len() <= 32 {
        return Ok(legacy);
    }
    let mut stream = Segments {
        result: vec![],
        segment: vec![0xaa],
    };
    if !write.is_empty() {
        stream.start();
        for b in std::iter::once(address << 1).chain(write.iter().copied()) {
            stream.command(&[0x80, b]);
        }
    }
    if read > 0 {
        stream.start();
        stream.command(&[0x81, (address << 1) | 1]);
        let mut remaining = read;
        while remaining > 32 {
            stream.command(&[0xe0]);
            stream.next();
            remaining -= 32;
        }
        if remaining > 1 {
            stream.command(&[0xc0 + remaining as u8 - 1]);
        }
        stream.command(&[0xc0]);
    }
    if stream.segment.len() + 2 > 32 {
        stream.next();
    }
    stream.segment.extend([0x75, 0]);
    stream.result.extend(stream.segment);
    Ok(stream.result)
}
struct Segments {
    result: Vec<u8>,
    segment: Vec<u8>,
}
impl Segments {
    fn next(&mut self) {
        self.segment.resize(32, 0);
        self.result.append(&mut self.segment);
        self.segment.push(0xaa);
    }
    fn start(&mut self) {
        if self.segment.len() + 4 > 32 {
            self.next();
        }
        self.segment.push(0x74);
    }
    fn command(&mut self, bytes: &[u8]) {
        if self.segment.len() + bytes.len() + 1 > 32 {
            self.next();
        }
        self.segment.extend(bytes);
    }
}
fn lengths(segment: &[u8]) -> Result<(usize, usize)> {
    let (mut i, mut status, mut data) = (1, 0, 0);
    while i < segment.len() {
        let command = segment[i];
        i += 1;
        let count = ((command & 63) as usize).max(1);
        match command {
            0 => break,
            0x74 | 0x75 | 0x60..=0x63 => (),
            0x80..=0xbf => {
                if command == 0x80 {
                    status += count;
                }
                i += count;
            }
            0xc0..=0xff => data += count,
            _ => return Err(invalid("Unexpected CH341 stream command")),
        }
        if i > segment.len() {
            return Err(invalid("Truncated CH341 stream command"));
        }
    }
    Ok((status, data))
}
impl I2c for Ch341 {
    fn transaction(&self, address: u8, write: &[u8], read: usize) -> Result<Vec<u8>> {
        let stream = encode(address, write, read)?;
        let timeout = Duration::from_millis(
            (((write.len() + read + 8) as u64 * 9 * 1000).div_ceil(100_000)) + 1000,
        );
        let quiet = Duration::from_millis(25);
        let mut usb = self.usb.lock().unwrap();
        usb.write(&[0xaa, 0x61, 0], Duration::from_secs(1))?;
        if write.is_empty() && read > 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut bytes = Vec::with_capacity(read);
        for segment in stream.chunks(32) {
            usb.write(segment, timeout)?;
            let (statuses, data) = lengths(segment)?;
            if statuses + data == 0 {
                continue;
            }
            let response = usb.read(
                statuses + data,
                if data > 0 {
                    timeout
                } else {
                    quiet.min(timeout)
                },
                quiet,
                data == 0,
            )?;
            if response.len() < data || response.len() > statuses + data {
                return Err(std::io::Error::other(
                    "CH341 stream packet returned an incomplete response",
                ));
            }
            let start = response.len() - data;
            if response[..start].iter().any(|b| b & 128 != 0) {
                return Err(std::io::Error::other("CH341 reported an I2C NACK"));
            }
            bytes.extend(&response[start..]);
        }
        if bytes.len() != read {
            return Err(invalid("Unexpected CH341 transaction response length"));
        }
        Ok(bytes)
    }
}
