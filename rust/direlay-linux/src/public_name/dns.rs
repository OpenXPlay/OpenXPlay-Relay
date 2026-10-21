// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! A minimal DNS client: TXT queries over UDP straight to a provider's name servers, to see the
//! ACME challenge record before Let's Encrypt looks for it.
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::{Duration, Instant},
};

const TXT: u16 = 16;
const CLASS_IN: u16 = 1;

/// A query for `name` (no trailing dot); recursion is not requested from authoritative servers.
pub fn encode_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut message = Vec::with_capacity(18 + name.len());
    message.extend_from_slice(&id.to_be_bytes());
    message.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.').filter(|l| !l.is_empty()) {
        message.push(label.len().min(63) as u8);
        message.extend_from_slice(&label.as_bytes()[..label.len().min(63)]);
    }
    message.push(0);
    message.extend_from_slice(&qtype.to_be_bytes());
    message.extend_from_slice(&CLASS_IN.to_be_bytes());
    message
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TxtAnswer {
    /// Each TXT record's strings joined, in answer order.
    pub values: Vec<String>,
    /// The server set TC, or the message ended early: more records may exist.
    pub truncated: bool,
    pub rcode: u8,
}

/// Reads a possibly compressed name at `offset`; returns it and the offset after it.
fn read_name(message: &[u8], mut offset: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    for _ in 0..128 {
        let len = *message.get(offset)? as usize;
        if len == 0 {
            return Some((labels.join("."), end.unwrap_or(offset + 1)));
        }
        if len & 0xc0 == 0xc0 {
            let pointer = ((len & 0x3f) << 8) | *message.get(offset + 1)? as usize;
            end.get_or_insert(offset + 2);
            offset = pointer;
            continue;
        }
        let label = message.get(offset + 1..offset + 1 + len)?;
        labels.push(String::from_utf8_lossy(label).to_ascii_lowercase());
        offset += 1 + len;
    }
    None
}

/// Parses the response to query `id` for TXT records of `name`. Records the message cuts off
/// mark the answer truncated instead of failing it.
pub fn parse_txt_answer(id: u16, name: &str, message: &[u8]) -> Option<TxtAnswer> {
    if message.len() < 12 || u16::from_be_bytes([message[0], message[1]]) != id {
        return None;
    }
    let flags = u16::from_be_bytes([message[2], message[3]]);
    if flags & 0x8000 == 0 {
        return None;
    }
    let count = |at: usize| u16::from_be_bytes([message[at], message[at + 1]]) as usize;
    let mut answer = TxtAnswer {
        truncated: flags & 0x0200 != 0,
        rcode: (flags & 0x000f) as u8,
        ..Default::default()
    };
    let mut offset = 12;
    for _ in 0..count(4) {
        let Some((_, next)) = read_name(message, offset) else {
            answer.truncated = true;
            return Some(answer);
        };
        offset = next + 4;
    }
    let wanted = name.trim_end_matches('.').to_ascii_lowercase();
    for _ in 0..count(6) {
        let Some((owner, next)) = read_name(message, offset) else {
            answer.truncated = true;
            break;
        };
        let Some(header) = message.get(next..next + 10) else {
            answer.truncated = true;
            break;
        };
        let rtype = u16::from_be_bytes([header[0], header[1]]);
        let class = u16::from_be_bytes([header[2], header[3]]);
        let length = u16::from_be_bytes([header[8], header[9]]) as usize;
        let Some(data) = message.get(next + 10..next + 10 + length) else {
            answer.truncated = true;
            break;
        };
        offset = next + 10 + length;
        if rtype != TXT || class != CLASS_IN || owner != wanted {
            continue;
        }
        let mut value = Vec::new();
        let mut at = 0;
        while at < data.len() {
            let len = data[at] as usize;
            let Some(part) = data.get(at + 1..at + 1 + len) else {
                answer.truncated = true;
                break;
            };
            value.extend_from_slice(part);
            at += 1 + len;
        }
        answer
            .values
            .push(String::from_utf8_lossy(&value).into_owned());
    }
    Some(answer)
}

/// One TXT query to `server`, waiting up to `timeout` for its answer.
pub fn query_txt(server: SocketAddr, name: &str, timeout: Duration) -> std::io::Result<TxtAnswer> {
    use rand_core::RngCore;
    let id = rand_core::OsRng.next_u32() as u16;
    let local: SocketAddr = if server.is_ipv4() {
        ([0, 0, 0, 0], 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(local)?;
    socket.connect(server)?;
    socket.send(&encode_query(id, name, TXT))?;
    let deadline = Instant::now() + timeout;
    let mut buffer = [0u8; 1500];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        socket.set_read_timeout(Some(left))?;
        let n = socket.recv(&mut buffer)?;
        // Stray datagrams with another id are ignored.
        if let Some(answer) = parse_txt_answer(id, name, &buffer[..n]) {
            return Ok(answer);
        }
    }
}

/// The name servers' addresses, IPv4 first, one list per server; unresolvable servers are left out.
pub fn resolve_servers(hosts: &[&str]) -> Vec<Vec<SocketAddr>> {
    hosts
        .iter()
        .filter_map(|host| {
            let mut addresses: Vec<_> = (*host, 53).to_socket_addrs().ok()?.collect();
            addresses.sort_by_key(|a| a.is_ipv6());
            (!addresses.is_empty()).then_some(addresses)
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Propagation {
    pub query_timeout: Duration,
    pub interval: Duration,
    pub limit: Duration,
    /// Extra wait once every answering server has the record.
    pub settle: Duration,
    /// When no server has answered after this long (outgoing DNS blocked), stop asking.
    pub no_answer: Duration,
    /// The fixed wait when no server can be queried or none answers; then the CA looks itself.
    pub fallback: Duration,
}
impl Default for Propagation {
    fn default() -> Self {
        Self {
            query_timeout: Duration::from_secs(3),
            interval: Duration::from_secs(5),
            limit: Duration::from_secs(120),
            settle: Duration::from_secs(5),
            no_answer: Duration::from_secs(60),
            fallback: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WaitError {
    NotUpdated,
    Interrupted,
}

/// Waits until every name server that answers (at least one) returns `value` for `name`. When
/// none can be queried, or none answers within [`Propagation::no_answer`], it waits the fixed
/// [`Propagation::fallback`] instead and leaves the check to the CA: networks that block
/// outgoing DNS still get a certificate. `sleep` waits and may report an interruption.
pub fn wait_for_txt(
    servers: &[Vec<SocketAddr>],
    name: &str,
    value: &str,
    timing: Propagation,
    sleep: &mut dyn FnMut(Duration) -> bool,
) -> Result<(), WaitError> {
    let mut pause = |d| {
        if sleep(d) {
            Ok(())
        } else {
            Err(WaitError::Interrupted)
        }
    };
    if servers.is_empty() {
        return pause(timing.fallback);
    }
    let started = Instant::now();
    let mut ever_answered = false;
    loop {
        let mut answered = 0;
        let mut complete = true;
        for server in servers {
            for address in server {
                match query_txt(*address, name, timing.query_timeout) {
                    Ok(answer) if answer.values.iter().any(|v| v == value) => {
                        answered += 1;
                        break;
                    }
                    // A cut-off answer without the value proves nothing either way.
                    Ok(answer) if answer.truncated => {}
                    Ok(_) => {
                        answered += 1;
                        complete = false;
                        break;
                    }
                    Err(_) => {}
                }
            }
        }
        ever_answered |= answered > 0;
        if answered > 0 && complete {
            return pause(timing.settle);
        }
        let elapsed = started.elapsed();
        if !ever_answered && elapsed >= timing.no_answer {
            return pause(timing.fallback);
        }
        if elapsed + timing.interval > timing.limit {
            return if ever_answered {
                Err(WaitError::NotUpdated)
            } else {
                pause(timing.fallback)
            };
        }
        pause(timing.interval)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::testing::FakeDns;
    fn response(id: u16, flags: u16, records: &[(&str, &[&[u8]])]) -> Vec<u8> {
        let mut m = id.to_be_bytes().to_vec();
        m.extend_from_slice(&flags.to_be_bytes());
        m.extend_from_slice(&[0, 1]);
        m.extend_from_slice(&(records.len() as u16).to_be_bytes());
        m.extend_from_slice(&[0, 0, 0, 0]);
        let question = encode_query(id, "_acme-challenge.a.example", TXT);
        m.extend_from_slice(&question[12..]);
        for (owner, strings) in records {
            if *owner == "pointer" {
                m.extend_from_slice(&[0xc0, 12]);
            } else {
                m.extend_from_slice(
                    &encode_query(0, owner, 0)[12..encode_query(0, owner, 0).len() - 4],
                );
            }
            m.extend_from_slice(&TXT.to_be_bytes());
            m.extend_from_slice(&CLASS_IN.to_be_bytes());
            m.extend_from_slice(&60u32.to_be_bytes());
            let data: Vec<u8> = strings
                .iter()
                .flat_map(|s| std::iter::once(s.len() as u8).chain(s.iter().copied()))
                .collect();
            m.extend_from_slice(&(data.len() as u16).to_be_bytes());
            m.extend_from_slice(&data);
        }
        m
    }
    #[test]
    fn encodes_a_txt_query() {
        assert_eq!(
            encode_query(0x1234, "_acme-challenge.a.example", TXT),
            b"\x12\x34\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x0f_acme-challenge\x01a\x07example\x00\x00\x10\x00\x01"
        );
    }
    #[test]
    fn parses_records_strings_compression_and_truncation() {
        let name = "_acme-challenge.a.example";
        let message = response(
            7,
            0x8400,
            &[
                ("pointer", &[b"part-one-", b"part-two"]),
                ("_ACME-challenge.a.example", &[b"second"]),
                ("other.a.example", &[b"ignored"]),
            ],
        );
        let answer = parse_txt_answer(7, name, &message).unwrap();
        assert_eq!(answer.values, ["part-one-part-two", "second"]);
        assert!(!answer.truncated);
        assert_eq!(answer.rcode, 0);
        // Another id or a query instead of a response is not this answer.
        assert!(parse_txt_answer(8, name, &message).is_none());
        assert!(parse_txt_answer(7, name, &encode_query(7, name, TXT)).is_none());
        // TC set by the server.
        let answer =
            parse_txt_answer(7, name, &response(7, 0x8600, &[("pointer", &[b"x"])])).unwrap();
        assert!(answer.truncated);
        assert_eq!(answer.values, ["x"]);
        // A message cut inside the second record keeps the first.
        let cut = &message[..message.len() - 40];
        let answer = parse_txt_answer(7, name, cut).unwrap();
        assert!(answer.truncated);
        assert_eq!(answer.values, ["part-one-part-two"]);
        // NXDOMAIN without records; garbage never panics.
        assert_eq!(
            parse_txt_answer(7, name, &response(7, 0x8403, &[]))
                .unwrap()
                .rcode,
            3
        );
        for len in 0..message.len() {
            let _ = parse_txt_answer(7, name, &message[..len]);
        }
        let mut looped = response(7, 0x8400, &[("pointer", &[b"x"])]);
        let at = looped.len() - 14;
        looped[at] = 0xc0;
        looped[at + 1] = at as u8;
        assert!(parse_txt_answer(7, name, &looped).unwrap().truncated);
    }
    #[test]
    fn waits_for_every_answering_server_and_falls_back_without_any() {
        let timing = Propagation {
            query_timeout: Duration::from_millis(200),
            interval: Duration::from_millis(10),
            limit: Duration::from_millis(400),
            settle: Duration::from_millis(1),
            no_answer: Duration::from_millis(50),
            fallback: Duration::from_millis(2),
        };
        let name = "_acme-challenge.a.example";
        let fresh = FakeDns::start();
        let stale = FakeDns::start();
        fresh.set(name, Some("value"));
        stale.set(name, Some("old"));
        let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let sleeps = std::cell::RefCell::new(Vec::new());
        let mut sleep = |d: Duration| {
            sleeps.borrow_mut().push(d);
            std::thread::sleep(d);
            true
        };
        let servers = vec![vec![fresh.address()], vec![silent.local_addr().unwrap()]];
        assert_eq!(
            wait_for_txt(&servers, name, "value", timing, &mut sleep),
            Ok(())
        );
        let servers = vec![vec![fresh.address()], vec![stale.address()]];
        assert_eq!(
            wait_for_txt(&servers, name, "value", timing, &mut sleep),
            Err(WaitError::NotUpdated)
        );
        stale.set(name, Some("value"));
        assert_eq!(
            wait_for_txt(&servers, name, "value", timing, &mut sleep),
            Ok(())
        );
        sleeps.borrow_mut().clear();
        let none = vec![vec![silent.local_addr().unwrap()]];
        assert_eq!(
            wait_for_txt(&none, name, "value", timing, &mut sleep),
            Ok(())
        );
        assert_eq!(wait_for_txt(&[], name, "value", timing, &mut sleep), Ok(()));
        assert_eq!(*sleeps.borrow(), [timing.fallback, timing.fallback]);
        let mut stop = |_| false;
        assert_eq!(
            wait_for_txt(&[vec![stale.address()]], name, "missing", timing, &mut stop),
            Err(WaitError::Interrupted)
        );
        assert!(fresh.queries() >= 3);
    }
}
