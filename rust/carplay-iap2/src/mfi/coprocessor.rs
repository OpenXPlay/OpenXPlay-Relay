// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-27.
// Created by riko chen on 2026-10-27

//! Serialized coprocessor register protocol shared by native I2C and USB bridges.
use super::{BaaCertificatePair, MfiAuthenticator};
use crate::{invalid, Result};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub trait I2c: Send + Sync {
    fn transaction(&self, address: u8, write: &[u8], read: usize) -> Result<Vec<u8>>;
}
/// A clock seam keeps retries and authentication deadlines deterministic in transport tests.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}
struct SystemClock(Instant);
impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
static COPROCESSOR_LOCK: Mutex<()> = Mutex::new(());
pub struct Coprocessor {
    transport: Arc<dyn I2c>,
    address: u8,
    clock: Arc<dyn Clock>,
}
impl Coprocessor {
    pub fn scan(transport: Arc<dyn I2c>) -> Result<Self> {
        Self::scan_with_clock(
            transport,
            Arc::new(SystemClock(Instant::now())),
            Duration::from_secs(15),
        )
    }
    pub fn scan_with_clock(
        transport: Arc<dyn I2c>,
        clock: Arc<dyn Clock>,
        timeout: Duration,
    ) -> Result<Self> {
        let _guard = COPROCESSOR_LOCK.lock().unwrap();
        let deadline = clock.now() + timeout;
        loop {
            for address in [0x10, 0x11] {
                if transport
                    .transaction(address, &[0], 0)
                    .is_ok_and(|v| v.is_empty())
                    && transport
                        .transaction(address, &[], 1)
                        .is_ok_and(|v| v.len() == 1 && v[0] != 0 && v[0] != 255)
                {
                    return Ok(Self {
                        transport,
                        address,
                        clock,
                    });
                }
            }
            if clock.now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No MFi authentication coprocessor responded to the device probe",
                ));
            }
            clock.sleep(Duration::from_millis(20));
        }
    }
    fn retry<T>(&self, mut action: impl FnMut() -> Result<T>) -> Result<T> {
        let deadline = self.clock.now() + Duration::from_secs(2);
        loop {
            match action() {
                Ok(v) => return Ok(v),
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Err(e),
                Err(e) if self.clock.now() >= deadline => return Err(e),
                Err(_) => self.clock.sleep(Duration::from_millis(20)),
            }
        }
    }
    fn select(&self, register: u8) -> Result<()> {
        if !self
            .transport
            .transaction(self.address, &[register], 0)?
            .is_empty()
        {
            return Err(invalid("Register select returned data"));
        }
        Ok(())
    }
    fn read(&self, register: u8, length: usize) -> Result<Vec<u8>> {
        self.retry(|| {
            self.select(register)?;
            let data = self.transport.transaction(self.address, &[], length)?;
            if data.len() != length {
                return Err(invalid("Unexpected MFi register response length"));
            }
            Ok(data)
        })
    }
    fn short(&self, register: u8) -> Result<usize> {
        let bytes = self.read(register, 2)?;
        Ok(u16::from_be_bytes(bytes.try_into().unwrap()) as usize)
    }
    fn write(&self, register: u8, data: &[u8]) -> Result<()> {
        let request = [&[register], data].concat();
        self.retry(|| {
            if !self
                .transport
                .transaction(self.address, &request, 0)?
                .is_empty()
            {
                return Err(invalid("MFi register write returned data"));
            }
            Ok(())
        })
    }
}
impl MfiAuthenticator for Coprocessor {
    fn protocol_major(&self) -> Result<u8> {
        let _guard = COPROCESSOR_LOCK.lock().unwrap();
        Ok(self.read(0x02, 1)?[0])
    }
    fn read_certificate(&self, maximum: usize) -> Result<Vec<u8>> {
        let _guard = COPROCESSOR_LOCK.lock().unwrap();
        if !(1..=65_525).contains(&maximum) {
            return Err(invalid("Invalid maximum certificate length"));
        }
        let length = self.short(0x30)?;
        if !(1..=maximum).contains(&length) {
            return Err(invalid("Invalid MFi certificate length"));
        }
        let mut bytes = Vec::with_capacity(length);
        for (index, offset) in (0..length).step_by(128).enumerate() {
            bytes.extend(self.read(0x31u8.wrapping_add(index as u8), (length - offset).min(128))?);
        }
        Ok(bytes)
    }
    fn sign_challenge(&self, challenge: &[u8]) -> Result<Vec<u8>> {
        let _guard = COPROCESSOR_LOCK.lock().unwrap();
        if !(1..=128).contains(&challenge.len()) {
            return Err(invalid("challenge must be 1..128 bytes"));
        }
        self.write(0x20, &(challenge.len() as u16).to_be_bytes())?;
        self.write(0x21, challenge)?;
        self.write(0x10, &[1])?;
        self.clock.sleep(Duration::from_millis(10));
        let deadline = self.clock.now() + Duration::from_secs(3);
        loop {
            match self.read(0x10, 1) {
                Ok(v) if v[0] == 0x10 => break,
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Err(e),
                _ => (),
            }
            if self.clock.now() >= deadline {
                let code = self
                    .retry(|| self.select(5))
                    .and_then(|()| self.transport.transaction(self.address, &[], 1))
                    .ok()
                    .filter(|v| v.len() == 1)
                    .map(|v| v[0]);
                let message = code.map_or_else(
                    || "MFi authentication failed (error code unavailable)".into(),
                    |code| format!("MFi authentication failed with error 0x{code:x}"),
                );
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, message));
            }
            self.clock.sleep(Duration::from_millis(10));
        }
        let length = self.short(0x11)?;
        if !(1..=65_525).contains(&length) {
            return Err(invalid("Invalid signature length"));
        }
        self.read(0x12, length)
    }
    fn baa_certificates(&self) -> Result<BaaCertificatePair> {
        Err(invalid("Authenticator does not provide BAA certificates"))
    }
}
