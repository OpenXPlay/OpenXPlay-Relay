// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// `LinkState` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    Idle,
    Detecting,
    Negotiating,
    Normal,
    Dead,
}
/// `Iap2LinkConfig` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2LinkConfig {
    pub max_outgoing: u8,

    pub max_outgoing_delta: u8,

    pub acknowledgement_timeout_millis: u16,

    pub control_session_version: u8,

    pub zero_acknowledgements: bool,

    pub maximum_queued_packets: usize,
    /// Maximum out-of-order frames (default 64).
    pub maximum_out_of_order_packets: usize,
    /// Maximum output bytes awaiting transport (default 1048576).
    pub maximum_pending_output_bytes: usize,
    /// Maximum buffered events (default 256).
    pub maximum_pending_events: usize,
}
/// `LinkEvent` states; preserve wire values in its codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkEvent {
    Control(Vec<u8>),
    Session(u8, Vec<u8>),
    Writable(bool),
    Dead(Option<String>),
}
/// `SessionDescriptor` protocol data.
#[derive(Clone, Debug)]
pub struct SessionDescriptor {
    pub id: u8,

    pub kind: u8,

    pub version: u8,
}
/// `SynchronizationPayload` protocol data.
#[derive(Clone, Debug)]
pub struct SynchronizationPayload {
    pub max_outgoing: u8,

    pub max_length: u16,

    pub retransmission_timeout_millis: u16,

    pub acknowledgement_timeout_millis: u16,

    pub max_retransmissions: u8,

    pub max_acknowledgements: u8,

    pub sessions: Vec<SessionDescriptor>,
}

#[derive(Clone)]
struct Packet {
    sequence: u8,
    session_id: u8,
    payload: Vec<u8>,
    retransmissions: u8,
    deadline: u64,
}
#[derive(Clone)]
struct Header {
    length: usize,
    control: u8,
    sequence: u8,
    acknowledgement: u8,
    session_id: u8,
}
/// Pure link reliability state machine; time is supplied by its caller.
pub struct Iap2LinkEngine {
    config: Iap2LinkConfig,
    state: LinkState,
    peer: SynchronizationPayload,
    peer_received: bool,
    sent: u8,
    sent_acked: Option<u8>,
    received: u8,
    received_acked: Option<u8>,
    accumulated: usize,
    writable: bool,
    unacked: Vec<Packet>,
    queued: std::collections::VecDeque<Packet>,
    out_of_order: Vec<Packet>,
    events: std::collections::VecDeque<LinkEvent>,
    output: Vec<u8>,
    receive: Vec<u8>,
    awaiting: Option<Header>,
    marker_deadline: Option<u64>,
    sync_deadline: Option<u64>,
    send_ack_deadline: Option<u64>,
    receive_ack_deadline: Option<u64>,
}
pub const IAP2_MARKER: [u8; 6] = [0xff, 0x55, 2, 0, 0xee, 0x10];
impl Default for Iap2LinkConfig {
    fn default() -> Self {
        Self {
            max_outgoing: 30,
            max_outgoing_delta: 0,
            acknowledgement_timeout_millis: 500,
            control_session_version: 1,
            zero_acknowledgements: false,
            maximum_queued_packets: 64,
            maximum_out_of_order_packets: 64,
            maximum_pending_output_bytes: 1048576,
            maximum_pending_events: 256,
        }
    }
}
impl SynchronizationPayload {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![1, self.max_outgoing];
        b.extend(self.max_length.to_be_bytes());
        b.extend(self.retransmission_timeout_millis.to_be_bytes());
        b.extend(self.acknowledgement_timeout_millis.to_be_bytes());
        b.extend([self.max_retransmissions, self.max_acknowledgements]);
        for s in &self.sessions {
            b.extend([s.id, s.kind, s.version]);
        }
        b
    }
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < 10 || b[0] != 1 {
            return None;
        }
        Some(Self {
            max_outgoing: b[1],
            max_length: u16::from_be_bytes([b[2], b[3]]),
            retransmission_timeout_millis: u16::from_be_bytes([b[4], b[5]]),
            acknowledgement_timeout_millis: u16::from_be_bytes([b[6], b[7]]),
            max_retransmissions: b[8],
            max_acknowledgements: b[9],
            sessions: b[10..]
                .chunks_exact(3)
                .map(|s| SessionDescriptor {
                    id: s[0],
                    kind: s[1],
                    version: s[2],
                })
                .collect(),
        })
    }
}
fn distance(seq: u8, previous: Option<u8>) -> usize {
    previous.map(|p| seq.wrapping_sub(p) as usize).unwrap_or(0)
}
fn checksum(b: &[u8]) -> u8 {
    0u8.wrapping_sub(b.iter().fold(0u8, |s, b| s.wrapping_add(*b)))
}
impl Iap2LinkEngine {
    pub fn new(config: Iap2LinkConfig) -> Self {
        assert!((1..=256).contains(&config.maximum_queued_packets));
        assert!((1..=256).contains(&config.maximum_out_of_order_packets));
        assert!(config.maximum_pending_output_bytes > 0 && config.maximum_pending_events > 0);
        let peer = Self::local_sync(&config);
        Self {
            config,
            state: LinkState::Idle,
            peer,
            peer_received: false,
            sent: 99,
            sent_acked: None,
            received: 0,
            received_acked: None,
            accumulated: 0,
            writable: false,
            unacked: Vec::new(),
            queued: Default::default(),
            out_of_order: Vec::new(),
            events: Default::default(),
            output: Vec::new(),
            receive: Vec::new(),
            awaiting: None,
            marker_deadline: None,
            sync_deadline: None,
            send_ack_deadline: None,
            receive_ack_deadline: None,
        }
    }
    fn local_sync(c: &Iap2LinkConfig) -> SynchronizationPayload {
        SynchronizationPayload {
            max_outgoing: c.max_outgoing,
            max_length: 65535,
            retransmission_timeout_millis: if c.zero_acknowledgements { 0 } else { 4000 },
            acknowledgement_timeout_millis: if c.zero_acknowledgements {
                0
            } else {
                c.acknowledgement_timeout_millis
            },
            max_retransmissions: if c.zero_acknowledgements { 0 } else { 4 },
            max_acknowledgements: if c.zero_acknowledgements { 0 } else { 3 },
            sessions: vec![
                SessionDescriptor {
                    id: 10,
                    kind: 0,
                    version: c.control_session_version,
                },
                SessionDescriptor {
                    id: 11,
                    kind: 2,
                    version: 1,
                },
                SessionDescriptor {
                    id: 12,
                    kind: 1,
                    version: 2,
                },
            ],
        }
    }
    pub fn state(&self) -> LinkState {
        self.state
    }
    pub fn writable(&self) -> bool {
        self.writable
    }
    pub fn peer_synchronization(&self) -> &SynchronizationPayload {
        &self.peer
    }
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
    pub fn poll_event(&mut self) -> Option<LinkEvent> {
        self.events.pop_front()
    }
    pub fn next_deadline_millis(&self) -> Option<u64> {
        [
            self.marker_deadline,
            self.sync_deadline,
            self.send_ack_deadline,
            self.receive_ack_deadline,
        ]
        .into_iter()
        .flatten()
        .min()
    }
    pub fn start(&mut self, wired_initiator: bool, now_millis: u64) -> Result<()> {
        if self.state != LinkState::Idle {
            return Ok(());
        }
        self.state = LinkState::Detecting;
        self.append(&IAP2_MARKER);
        if self.state != LinkState::Dead {
            self.marker_deadline = Some(now_millis + 1000);
            if wired_initiator {
                self.negotiate(now_millis);
            }
        }
        Ok(())
    }
    fn negotiate(&mut self, now: u64) {
        self.state = LinkState::Negotiating;
        self.marker_deadline = None;
        self.synchronize();
        if self.state != LinkState::Dead {
            self.sync_deadline = Some(now + 500);
        }
    }
    fn synchronize(&mut self) {
        self.write_packet(
            Some(&Self::local_sync(&self.config).encode()),
            self.sent,
            0x80,
            0,
        );
    }
    pub fn feed(&mut self, bytes: &[u8], now_millis: u64) -> Result<()> {
        for b in bytes {
            if self.state == LinkState::Dead {
                break;
            }
            if self.receive.len() == 65535 {
                self.die(Some("iAP2 inbound frame exceeds 65535 bytes".into()));
                break;
            }
            self.receive.push(*b);
            while self.parse(now_millis) && self.state != LinkState::Dead {}
        }
        Ok(())
    }
    fn parse(&mut self, now: u64) -> bool {
        if self.state == LinkState::Dead {
            return false;
        }
        if self.state == LinkState::Detecting {
            if self.receive.len() < 6 {
                return false;
            }
            if self.receive[..6] != IAP2_MARKER {
                self.die(Some("iAP2 marker was not received".into()));
                return false;
            }
            self.receive.drain(..6);
            self.negotiate(now);
            return true;
        }
        if let Some(h) = self.awaiting.clone() {
            let n = h.length - 9;
            if self.receive.len() < n {
                return false;
            }
            let mut b: Vec<u8> = self.receive.drain(..n).collect();
            self.awaiting = None;
            if checksum(&b) == 0 {
                b.pop();
                self.process(h, Some(b), now);
            }
            return true;
        }
        let mut discarded = false;
        while self.receive.len() >= 2 && self.receive[..2] != [0xff, 0x5a] {
            self.receive.remove(0);
            discarded = true;
        }
        if self.receive.len() < 9 {
            return discarded;
        }
        let b: Vec<u8> = self.receive.drain(..9).collect();
        if checksum(&b) != 0 {
            return true;
        }
        let h = Header {
            length: u16::from_be_bytes([b[2], b[3]]) as usize,
            control: b[4],
            sequence: b[5],
            acknowledgement: b[6],
            session_id: b[7],
        };
        if h.length < 9 {
            return true;
        }
        if h.length == 9 {
            self.process(h, None, now);
        } else {
            self.awaiting = Some(h);
        }
        true
    }
    fn process(&mut self, h: Header, payload: Option<Vec<u8>>, now: u64) {
        if h.control & 0x10 != 0 {
            self.die(Some("peer sent iAP2 reset".into()));
            return;
        }
        if h.control & 0x80 != 0 {
            if let Some(sync) = payload.as_deref().and_then(SynchronizationPayload::decode) {
                if self.state == LinkState::Negotiating {
                    self.peer = sync;
                    self.peer_received = true;
                    self.received = h.sequence;
                    self.received_acked = Some(h.sequence);
                    self.ack();
                }
            }
        }
        if h.control & 0x40 != 0 {
            self.accumulated += 1;
            self.acknowledge(h.acknowledgement, now);
        }
        if h.control & 0x20 != 0 {
            if let Some(missing) = &payload {
                self.extended_ack(missing);
            }
        }
        if h.control & !0x40 == 0 {
            if let Some(payload) = payload {
                self.data(
                    Packet {
                        sequence: h.sequence,
                        session_id: h.session_id,
                        payload,
                        retransmissions: 0,
                        deadline: 0,
                    },
                    now,
                );
            }
        }
        if self.peer.max_acknowledgements > 0
            && self.accumulated >= self.peer.max_acknowledgements as usize
        {
            self.accumulated = 0;
            self.received_acked = Some(self.received);
            self.ack();
        }
    }
    fn acceptable(&self, b: &[u8]) -> bool {
        self.peer.max_length > 10 && b.len() <= self.peer.max_length as usize - 10
    }
    fn acknowledge(&mut self, ack: u8, now: u64) {
        if self.state == LinkState::Negotiating {
            if !self.peer_received
                || self.peer.max_length <= 10
                || self.queued.iter().any(|p| !self.acceptable(&p.payload))
            {
                self.die(Some(
                    "peer iAP2 maxLength cannot carry pending control packets".into(),
                ));
                return;
            }
            self.state = LinkState::Normal;
            self.sync_deadline = None;
            self.set_writable(true);
        }
        self.sent_acked = Some(ack);
        self.receive_ack_deadline = None;
        while let Some(p) = self.unacked.first() {
            let d = distance(p.sequence, self.sent_acked);
            if d > 0 && d <= self.peer.max_acknowledgements as usize + 10 {
                self.receive_ack_deadline = Some(p.deadline);
                break;
            }
            self.unacked.remove(0);
        }
        while distance(self.sent, self.sent_acked) < self.peer.max_outgoing as usize {
            let Some(p) = self.queued.pop_front() else {
                break;
            };
            self.send_packet(p, now);
            self.set_writable(true);
        }
    }
    fn extended_ack(&mut self, missing: &[u8]) {
        if self.state != LinkState::Normal {
            return;
        }
        for i in 0..self.unacked.len() {
            if !missing.contains(&self.unacked[i].sequence) {
                continue;
            }
            self.unacked[i].retransmissions = self.unacked[i].retransmissions.wrapping_add(1);
            let p = self.unacked[i].clone();
            if p.retransmissions == self.peer.max_retransmissions {
                self.die(Some(format!(
                    "iAP2 packet {} was not acknowledged",
                    p.sequence
                )));
                return;
            }
            self.write_packet(Some(&p.payload), p.sequence, 0x40, p.session_id);
            self.send_ack_deadline = None;
            self.receive_ack_deadline = Some(p.deadline);
        }
    }
    fn data(&mut self, p: Packet, now: u64) {
        let d = distance(p.sequence, Some(self.received));
        if d == 0 || d > self.peer.max_outgoing as usize + 10 {
            self.ack();
            return;
        }
        if self.out_of_order.iter().any(|v| v.sequence == p.sequence) {
            return;
        }
        if self.out_of_order.len() >= self.config.maximum_out_of_order_packets {
            self.die(Some("iAP2 out-of-order packet limit exceeded".into()));
            return;
        }
        let sequence = p.sequence;
        self.out_of_order.push(p);
        if d > 1 {
            if d >= self.peer.max_outgoing as usize {
                let mut missing = Vec::new();
                let mut seq = self.received;
                while distance(sequence, Some(seq)) > 1 {
                    seq = seq.wrapping_add(1);
                    missing.push(seq);
                }
                self.send_ack_deadline = None;
                self.write_packet(Some(&missing), self.sent, 0x20, 0);
            }
            return;
        }
        let received = self.received;
        self.out_of_order
            .sort_by_key(|p| distance(p.sequence, Some(received)));
        while self
            .out_of_order
            .first()
            .is_some_and(|p| distance(p.sequence, Some(self.received)) == 1)
        {
            let p = self.out_of_order.remove(0);
            self.received = p.sequence;
            self.event(if p.session_id == 10 {
                LinkEvent::Control(p.payload)
            } else {
                LinkEvent::Session(p.session_id, p.payload)
            });
        }
        if self.peer.max_acknowledgements == 0 {
            return;
        }
        let window = (self.peer.max_outgoing as isize - self.config.max_outgoing_delta as isize)
            .max(1) as usize;
        if distance(self.received, self.received_acked) >= window {
            self.send_ack_deadline = None;
            self.received_acked = Some(self.received);
            self.ack();
        } else {
            self.send_ack_deadline = Some(now + self.peer.acknowledgement_timeout_millis as u64);
        }
    }
    pub fn send_control(&mut self, bytes: &[u8], now_millis: u64) -> Result<()> {
        self.send_session(10, bytes, now_millis)
    }
    pub fn send_session(&mut self, session_id: u8, bytes: &[u8], now_millis: u64) -> Result<()> {
        if bytes.len() > 65525 {
            return Err(invalid("iAP2 session payload exceeds 65525 bytes"));
        }
        if (self.peer_received || self.state == LinkState::Normal) && !self.acceptable(bytes) {
            return Err(invalid(format!(
                "iAP2 session payload exceeds peer maxLength {}",
                self.peer.max_length
            )));
        }
        self.send_packet(
            Packet {
                sequence: 0,
                session_id,
                payload: bytes.to_vec(),
                retransmissions: 0,
                deadline: 0,
            },
            now_millis,
        );
        Ok(())
    }
    fn send_packet(&mut self, mut p: Packet, now: u64) {
        if self.state != LinkState::Normal
            || distance(self.sent, self.sent_acked) > self.peer.max_outgoing as usize
        {
            if self.queued.len() >= self.config.maximum_queued_packets {
                self.die(Some("iAP2 outbound queue limit exceeded".into()));
            } else {
                self.queued.push_back(p);
                self.set_writable(false);
            }
            return;
        }
        self.sent = self.sent.wrapping_add(1);
        p.sequence = self.sent;
        p.retransmissions = 0;
        p.deadline = now + self.peer.retransmission_timeout_millis as u64;
        self.send_ack_deadline = None;
        self.write_packet(Some(&p.payload), p.sequence, 0x40, p.session_id);
        self.received_acked = Some(self.received);
        if self.peer.max_retransmissions > 0 {
            self.receive_ack_deadline = Some(p.deadline);
            self.unacked.push(p);
        } else {
            self.sent_acked = Some(self.sent);
        }
    }
    pub fn advance_time(&mut self, now_millis: u64) -> Result<()> {
        while self.state != LinkState::Dead
            && self.next_deadline_millis().is_some_and(|d| d <= now_millis)
        {
            if self.marker_deadline.is_some_and(|d| d <= now_millis) {
                self.marker_deadline = None;
                if self.state == LinkState::Detecting {
                    self.append(&IAP2_MARKER);
                    if self.state != LinkState::Dead {
                        self.marker_deadline = Some(now_millis + 1000);
                    }
                }
            } else if self.sync_deadline.is_some_and(|d| d <= now_millis) {
                self.sync_deadline = None;
                if self.state == LinkState::Negotiating {
                    self.synchronize();
                    if self.state != LinkState::Dead {
                        self.sync_deadline = Some(now_millis + 500);
                    }
                }
            } else if self.send_ack_deadline.is_some_and(|d| d <= now_millis) {
                self.send_ack_deadline = None;
                if self.state == LinkState::Normal {
                    self.received_acked = Some(self.received);
                    self.ack();
                }
            } else {
                self.receive_ack_deadline = None;
                self.retransmit(now_millis);
            }
        }
        Ok(())
    }
    fn retransmit(&mut self, now: u64) {
        if self.state != LinkState::Normal {
            return;
        }
        let Some((i, _)) = self
            .unacked
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.deadline)
        else {
            return;
        };
        let second = self
            .unacked
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, p)| p.deadline)
            .min();
        self.unacked[i].deadline = now + self.peer.retransmission_timeout_millis as u64;
        self.unacked[i].retransmissions = self.unacked[i].retransmissions.wrapping_add(1);
        let p = self.unacked[i].clone();
        if p.retransmissions == self.peer.max_retransmissions {
            self.die(Some(format!(
                "iAP2 packet {} was not acknowledged",
                p.sequence
            )));
            return;
        }
        self.write_packet(Some(&p.payload), p.sequence, 0x40, p.session_id);
        self.receive_ack_deadline = Some(second.unwrap_or(p.deadline));
    }
    fn ack(&mut self) {
        self.write_packet(None, self.sent, 0x40, 0);
    }
    fn write_packet(&mut self, payload: Option<&[u8]>, sequence: u8, control: u8, session: u8) {
        if self.state == LinkState::Dead {
            return;
        }
        self.accumulated = 0;
        let length = payload.map_or(9, |p| p.len() + 10);
        let mut b = vec![
            0xff,
            0x5a,
            (length >> 8) as u8,
            length as u8,
            control,
            sequence,
            self.received,
            session,
        ];
        b.push(checksum(&b));
        if let Some(p) = payload {
            b.extend(p);
            b.push(checksum(p));
        }
        self.append(&b);
    }
    fn append(&mut self, b: &[u8]) {
        if self.state == LinkState::Dead {
            return;
        }
        if b.len()
            > self
                .config
                .maximum_pending_output_bytes
                .saturating_sub(self.output.len())
        {
            self.die(Some("iAP2 pending output limit exceeded".into()));
        } else {
            self.output.extend(b);
        }
    }
    fn event(&mut self, e: LinkEvent) {
        if self.state == LinkState::Dead {
            return;
        }
        if self.events.len() >= self.config.maximum_pending_events {
            self.die(Some("iAP2 pending event limit exceeded".into()));
        } else {
            self.events.push_back(e);
        }
    }
    fn set_writable(&mut self, v: bool) {
        if self.writable != v {
            self.writable = v;
            self.event(LinkEvent::Writable(v));
        }
    }
    fn die(&mut self, reason: Option<String>) {
        if self.state == LinkState::Dead {
            return;
        }
        self.state = LinkState::Dead;
        self.writable = false;
        self.marker_deadline = None;
        self.sync_deadline = None;
        self.send_ack_deadline = None;
        self.receive_ack_deadline = None;
        self.output.clear();
        self.events.clear();
        self.events.push_back(LinkEvent::Dead(reason));
    }
    pub fn feed_eof(&mut self) {
        self.die(None);
    }
}
/// Protocol message, field, or framing constant.
pub const CONTROL_SESSION_ID: u64 = 10;
/// Protocol message, field, or framing constant.
pub const EA_SESSION_ID: u64 = 11;
/// Protocol message, field, or framing constant.
pub const FILE_TRANSFER_SESSION_ID: u64 = 12;
/// Protocol message, field, or framing constant.
pub const HEADER_BYTES: u64 = 9;
/// Protocol message, field, or framing constant.
pub const CHECKSUM_BYTES: u64 = 1;
/// Protocol message, field, or framing constant.
pub const MAX_WIRE_FRAME_BYTES: u64 = 0xffff;
/// Protocol message, field, or framing constant.
pub const SESSION_DESCRIPTOR_BYTES: u64 = 3;
/// Protocol message, field, or framing constant.
pub const SYNCHRONIZATION_FIXED_BYTES: u64 = 10;
/// Protocol message, field, or framing constant.
pub const SYNCHRONIZATION_VERSION: u64 = 1;
/// Protocol message, field, or framing constant.
pub const LINK_START_LOW: u64 = 0x5a;
/// Protocol message, field, or framing constant.
pub const CONTROL_SYNCHRONIZE: u64 = 0x80;
/// Protocol message, field, or framing constant.
pub const CONTROL_ACKNOWLEDGEMENT: u64 = 0x40;
/// Protocol message, field, or framing constant.
pub const CONTROL_EXTENDED_ACKNOWLEDGEMENT: u64 = 0x20;
/// Protocol message, field, or framing constant.
pub const CONTROL_RESET: u64 = 0x10;
/// Protocol message, field, or framing constant.
pub const INITIAL_SEQUENCE: u64 = 99;
/// Protocol message, field, or framing constant.
pub const MARKER_RESEND_MILLIS: u64 = 1_000;
/// Protocol message, field, or framing constant.
pub const SYNCHRONIZATION_RESEND_MILLIS: u64 = 500;
/// Protocol message, field, or framing constant.
pub const DEFAULT_RETRANSMISSION_TIMEOUT_MILLIS: u64 = 4_000;
/// Protocol message, field, or framing constant.
pub const DEFAULT_MAX_RETRANSMISSIONS: u64 = 4;
/// Protocol message, field, or framing constant.
pub const DEFAULT_MAX_ACKNOWLEDGEMENTS: u64 = 3;
/// Protocol message, field, or framing constant.
pub const ACK_SLACK: u64 = 10;

/// Constant expression: `MAX_WIRE_FRAME_BYTES - HEADER_BYTES - CHECKSUM_BYTES`.
pub const MAX_PAYLOAD_BYTES: u64 = 65525;

/// Constant expression: `0xff`.
pub const LINK_START_HIGH: u64 = 255;
