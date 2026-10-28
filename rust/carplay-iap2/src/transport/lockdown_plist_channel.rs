// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Bounded, length-prefixed XML dictionaries over a Lockdown byte stream.
use crate::{invalid, DuplexStream, Receive, Result};
use plist::{
    stream::{Event, XmlReader},
    Dictionary, Value,
};
use std::{
    io::{self, Cursor},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
pub type LockdownPlistValue = Value;
pub type LockdownDictionary = Dictionary;
pub struct LockdownPlistChannel {
    stream: Arc<dyn DuplexStream>,
    maximum: usize,
    io: Mutex<()>,
    closed: AtomicBool,
    detached: AtomicBool,
}
impl LockdownPlistChannel {
    pub fn new(stream: Arc<dyn DuplexStream>) -> Self {
        Self {
            stream,
            maximum: 1024 * 1024,
            io: Mutex::new(()),
            closed: AtomicBool::new(false),
            detached: AtomicBool::new(false),
        }
    }
    pub fn with_maximum(stream: Arc<dyn DuplexStream>, maximum: usize) -> Result<Self> {
        if !(1..=4 * 1024 * 1024).contains(&maximum) {
            return Err(invalid("maximumMessageBytes must be between 1 and 4194304"));
        }
        let mut result = Self::new(stream);
        result.maximum = maximum;
        Ok(result)
    }
    pub fn send(&self, message: &Dictionary) -> Result<()> {
        let _guard = self.io.lock().unwrap();
        self.check()?;
        self.send_locked(message)
    }
    pub fn receive(&self, timeout: Duration) -> Result<Dictionary> {
        let _guard = self.io.lock().unwrap();
        self.check()?;
        self.receive_locked(timeout)
    }
    pub fn request(&self, message: &Dictionary, timeout: Duration) -> Result<Dictionary> {
        let _guard = self.io.lock().unwrap();
        self.check()?;
        validate_timeout(timeout)?;
        self.send_locked(message)?;
        self.receive_locked(timeout)
    }
    /// Transfers stream ownership exactly once, after the current request finishes.
    pub fn detach(&self) -> Result<Arc<dyn DuplexStream>> {
        let _guard = self.io.lock().unwrap();
        self.check()?;
        self.detached.store(true, Ordering::Release);
        self.closed.store(true, Ordering::Release);
        Ok(self.stream.clone())
    }
    pub fn close(&self) -> Result<()> {
        if !self.closed.swap(true, Ordering::AcqRel) && !self.detached.load(Ordering::Acquire) {
            self.stream.close()?;
        }
        Ok(())
    }
    fn check(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Lockdown plist channel is closed",
            ))
        } else {
            Ok(())
        }
    }
    fn send_locked(&self, message: &Dictionary) -> Result<()> {
        let xml = encode(message)?;
        if xml.is_empty() || xml.len() > self.maximum {
            return Err(invalid(format!(
                "Lockdown plist message length {} is outside 1..{}",
                xml.len(),
                self.maximum
            )));
        }
        let mut frame = (xml.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&xml);
        self.stream.send(&frame)
    }
    fn receive_locked(&self, timeout: Duration) -> Result<Dictionary> {
        validate_timeout(timeout)?;
        let deadline = Instant::now() + timeout;
        let result = (|| {
            let header = self.read_fully(4, deadline)?;
            let length = u32::from_be_bytes(header.try_into().unwrap()) as usize;
            if length == 0 || length > self.maximum {
                return Err(invalid(format!(
                    "Invalid Lockdown plist message length {length} (maximum {})",
                    self.maximum
                )));
            }
            parse(&self.read_fully(length, deadline)?)
        })();
        if result.is_err() {
            let _ = self.close();
        }
        result
    }
    fn read_fully(&self, length: usize, deadline: Instant) -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(length);
        while bytes.len() < length {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(timeout_error());
            }
            match self.stream.recv(length - bytes.len(), remaining)? {
                Receive::Data(b) if !b.is_empty() && b.len() <= length - bytes.len() => {
                    bytes.extend_from_slice(&b)
                }
                Receive::Timeout => return Err(timeout_error()),
                _ => {
                    return Err(invalid(
                        "USBMUX TCP stream ended inside a Lockdown plist frame",
                    ))
                }
            }
        }
        Ok(bytes)
    }
}
impl Drop for LockdownPlistChannel {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
fn timeout_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "Timed out while reading Lockdown plist frame",
    )
}
fn validate_timeout(t: Duration) -> Result<()> {
    if t.is_zero() || t > Duration::from_secs(300) {
        Err(invalid("timeoutMillis must be between 1 and 300000"))
    } else {
        Ok(())
    }
}
pub fn encode(dictionary: &Dictionary) -> Result<Vec<u8>> {
    validate(&Value::Dictionary(dictionary.clone()), 0)?;
    let mut bytes = Vec::new();
    Value::Dictionary(dictionary.clone())
        .to_writer_xml(&mut bytes)
        .map_err(|_| invalid("Invalid Lockdown plist value"))?;
    Ok(bytes)
}
pub fn parse(bytes: &[u8]) -> Result<Dictionary> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("Lockdown plist exceeds the defensive limit"));
    }
    let mut events = XmlReader::new(Cursor::new(bytes));
    let first = events
        .next()
        .transpose()
        .map_err(|_| invalid("Invalid Lockdown plist XML"))?
        .ok_or_else(|| invalid("Lockdown plist must contain exactly one root value"))?;
    let value = parse_value(first, &mut events, 0)?;
    if events.next().is_some() {
        return Err(invalid(
            "Lockdown plist must contain exactly one root value",
        ));
    }
    match value {
        Value::Dictionary(d) => Ok(d),
        _ => Err(invalid("Lockdown plist root must be a dictionary")),
    }
}
fn parse_value(
    first: Event<'static>,
    events: &mut impl Iterator<Item = std::result::Result<Event<'static>, plist::Error>>,
    depth: usize,
) -> Result<Value> {
    if depth > 64 {
        return Err(invalid("Lockdown plist nesting exceeds 64"));
    }
    Ok(match first {
        Event::StartDictionary(_) => {
            let mut d = Dictionary::new();
            loop {
                let next = events
                    .next()
                    .transpose()
                    .map_err(|_| invalid("Invalid Lockdown plist XML"))?
                    .ok_or_else(|| invalid("Invalid Lockdown plist dictionary"))?;
                if matches!(next, Event::EndCollection) {
                    break;
                }
                let Event::String(key) = next else {
                    return Err(invalid("Invalid Lockdown plist dictionary"));
                };
                if d.len() >= 1024 {
                    return Err(invalid("Invalid Lockdown plist dictionary"));
                }
                if d.contains_key(key.as_ref()) {
                    return Err(invalid(format!("Duplicate Lockdown plist key '{key}'")));
                }
                let value = events
                    .next()
                    .transpose()
                    .map_err(|_| invalid("Invalid Lockdown plist XML"))?
                    .ok_or_else(|| invalid("Lockdown plist dictionary value is missing"))?;
                d.insert(key.into_owned(), parse_value(value, events, depth + 1)?);
            }
            Value::Dictionary(d)
        }
        Event::String(s) => Value::String(s.into_owned()),
        Event::Data(b) => Value::Data(b.into_owned()),
        Event::Boolean(b) => Value::Boolean(b),
        Event::Integer(i) if i.as_signed().is_some() => Value::Integer(i),
        _ => return Err(invalid("Unsupported Lockdown plist value")),
    })
}
fn validate(v: &Value, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(invalid("Lockdown plist nesting exceeds 64"));
    }
    match v {
        Value::Dictionary(d) => {
            if d.len() > 1024 {
                return Err(invalid("Lockdown plist dictionary exceeds 1024 entries"));
            }
            for v in d.values() {
                validate(v, depth + 1)?;
            }
        }
        Value::String(_) | Value::Data(_) | Value::Boolean(_) => (),
        Value::Integer(i) if i.as_signed().is_some() => (),
        _ => return Err(invalid("Unsupported Lockdown plist value")),
    }
    Ok(())
}
pub(crate) fn dictionary(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Dictionary {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}
