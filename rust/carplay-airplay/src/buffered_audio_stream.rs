// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::screen_stream::{accept_one, invalid, read_exact, Worker};
use crate::*;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct BufferedAudioStream {
    inner: Arc<Inner>,
}
struct Inner {
    server: Arc<dyn StreamListener>,
    port: u16,
    worker: Arc<Worker>,
    started: AtomicBool,
    format: AudioFormat,
    key: [u8; 32],
    listener: Arc<dyn AudioListener>,
    state: Mutex<State>,
    changed: Condvar,
    delivery: DeliveryGate,
    diagnostic: Mutex<Arc<dyn Fn(String) + Send + Sync>>,
}
#[derive(Default)]
struct State {
    queue: VecDeque<BufferedFrame>,
    fed: VecDeque<BufferedFrame>,
    bytes: usize,
    rate: i32,
    flush_until: Option<u32>,
    anchor_rtp: Option<u32>,
    anchor_ntp: Option<u64>,
    feed_start: Option<(u32, Instant)>,
    fed_samples: u64,
    generation: u64,
    closed: bool,
}
#[derive(Clone, Debug)]
pub struct BufferedFrame {
    pub sequence: u16,
    pub timestamp: u32,
    pub rtp: Vec<u8>,
}
/// Serialize delivery against controls, allowing a sink to synchronously reenter its owner.
#[derive(Default)]
pub(crate) struct DeliveryGate {
    state: Mutex<(Option<std::thread::ThreadId>, usize)>,
    changed: Condvar,
}
pub(crate) struct DeliveryGuard<'a>(&'a DeliveryGate);
impl DeliveryGate {
    pub(crate) fn enter(&self) -> DeliveryGuard<'_> {
        let id = std::thread::current().id();
        let mut s = self.state.lock().unwrap();
        while s.0.is_some_and(|owner| owner != id) {
            s = self.changed.wait(s).unwrap();
        }
        s.0 = Some(id);
        s.1 += 1;
        DeliveryGuard(self)
    }
}
impl Drop for DeliveryGuard<'_> {
    fn drop(&mut self) {
        let mut s = self.0.state.lock().unwrap();
        s.1 -= 1;
        if s.1 == 0 {
            s.0 = None;
            self.0.changed.notify_all();
        }
    }
}
impl BufferedAudioStream {
    pub fn new(
        transport: Arc<dyn StreamTransport>,
        format: AudioFormat,
        key: Vec<u8>,
        listener: Arc<dyn AudioListener>,
    ) -> Result<Self> {
        let key = key
            .try_into()
            .map_err(|_| invalid("buffered audio key must be 32 bytes"))?;
        if format.sample_rate == 0 {
            return Err(invalid("buffered audio sample rate must be positive"));
        }
        let server = transport.listen("::".parse().unwrap())?;
        let port = server.port()?;
        let worker = Arc::new(Worker::default());
        worker.add_server(server.clone())?;
        Ok(Self {
            inner: Arc::new(Inner {
                server,
                port,
                worker,
                started: AtomicBool::new(false),
                format,
                key,
                listener,
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
                delivery: DeliveryGate::default(),
                diagnostic: Mutex::new(Arc::new(|line| eprintln!("{line}"))),
            }),
        })
    }
    pub fn with_diagnostic(self, diagnostic: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        *self.inner.diagnostic.lock().unwrap() = diagnostic;
        self
    }
    pub fn port(&self) -> u16 {
        self.inner.port
    }
    pub fn start(&self) -> Result<()> {
        if self.inner.worker.closed() || self.inner.started.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let this = self.clone();
        self.inner.worker.spawn("airplay-buffered-rx", move || {
            let _ = this.receive();
            this.inner.changed.notify_all();
        })?;
        let this = self.clone();
        self.inner.worker.spawn("airplay-buffered-feed", move || {
            if this.feed().is_err() {
                let _ = this.close();
            }
        })
    }
    fn receive(&self) -> Result<()> {
        let i = &self.inner;
        let Some(socket) = accept_one(&i.worker, &i.server)? else {
            return Ok(());
        };
        i.server.close()?;
        let diagnostic = i.diagnostic.lock().unwrap().clone();
        diagnostic("Buffered audio: connected".into());
        let mut failures = 0;
        let mut received_bytes = 0u64;
        let mut window = Instant::now();
        loop {
            let Some(prefix) = read_exact(&i.worker, &socket, 2)? else {
                return Ok(());
            };
            let length = u16::from_be_bytes(prefix.try_into().unwrap()) as usize;
            if length < 38 {
                diagnostic(format!(
                    "Buffered audio: invalid frame length {length}; closed"
                ));
                self.close()?;
                return Ok(());
            }
            let Some(body) = read_exact(&i.worker, &socket, length - 2)? else {
                return Ok(());
            };
            let Some(frame) = Self::open(&i.key, &body) else {
                if failures < 3 {
                    diagnostic(format!(
                        "Buffered audio: frame did not open ({} bytes)",
                        body.len()
                    ));
                }
                failures += 1;
                continue;
            };
            received_bytes += length as u64;
            if window.elapsed() >= Duration::from_secs(10) {
                let millis = i.state.lock().unwrap().queue.len() as u64 * 1024 * 1000
                    / i.format.sample_rate as u64;
                diagnostic(format!(
                    "Buffered audio: queued {} s, received {} kbit/s",
                    millis / 1000,
                    received_bytes * 8 * 1000 / window.elapsed().as_micros().max(1) as u64
                ));
                window = Instant::now();
                received_bytes = 0;
            }
            if frame.rtp.len() <= 12 {
                continue;
            }
            let mut s = i.state.lock().unwrap();
            let cap = (i.format.sample_rate as u64 * MAX_BUFFER_MILLIS / (1024 * 1000)) as usize;
            while !s.closed
                && !s
                    .flush_until
                    .is_some_and(|ts| Self::before(frame.timestamp, ts))
                && (s.bytes + frame.rtp.len() > AUDIO_BUFFER_BYTES as usize
                    || s.queue.len() + s.fed.len() >= cap)
            {
                s = i.changed.wait(s).unwrap();
            }
            if s.closed {
                return Ok(());
            }
            if s.flush_until
                .is_some_and(|ts| Self::before(frame.timestamp, ts))
            {
                continue;
            }
            s.bytes += frame.rtp.len();
            s.queue.push_back(frame);
            let pace = s.queue.len() + s.fed.len()
                >= (i.format.sample_rate as u64 * PACED_FILL_FLOOR_MILLIS / (1024 * 1000)) as usize;
            i.changed.notify_all();
            drop(s);
            if pace {
                let s = i.state.lock().unwrap();
                let _ = i
                    .changed
                    .wait_timeout_while(
                        s,
                        Duration::from_millis(
                            (1024 * 1000 / (i.format.sample_rate as u64 * 4)).max(1),
                        ),
                        |s| !s.closed,
                    )
                    .unwrap();
            }
        }
    }
    fn feed(&self) -> Result<()> {
        let i = &self.inner;
        loop {
            {
                let mut s = i.state.lock().unwrap();
                if s.closed {
                    return Ok(());
                }
                Self::retire(&mut s, i.format.sample_rate);
                i.changed.notify_all();
                if s.rate == 0 || s.queue.is_empty() || Self::lead(&s, i.format.sample_rate) >= 1000
                {
                    drop(
                        i.changed
                            .wait_timeout(s, Duration::from_millis(20))
                            .unwrap(),
                    );
                    continue;
                }
            }
            let _delivery = i.delivery.enter();
            let (frame, first, generation) = {
                let mut s = i.state.lock().unwrap();
                if s.closed {
                    return Ok(());
                }
                if s.rate == 0 || s.queue.is_empty() {
                    continue;
                }
                let frame = s.queue.pop_front().unwrap();
                s.fed.push_back(frame.clone());
                let first = s.feed_start.is_none();
                if first {
                    s.feed_start = Some((frame.timestamp, Instant::now()));
                    s.fed_samples = 0;
                }
                (frame, first, s.generation)
            };
            if first {
                i.listener.on_started(&i.format, frame.timestamp)?;
            }
            {
                let mut s = i.state.lock().unwrap();
                if s.closed || s.rate == 0 || s.generation != generation {
                    continue;
                }
                s.fed_samples += 1024;
            }
            i.listener.on_rtp(&i.format, &frame.rtp, frame.timestamp)?;
        }
    }
    fn lead(s: &State, rate: u32) -> i64 {
        s.feed_start.map_or(0, |(_, time)| {
            (s.fed_samples * 1000 / rate as u64) as i64 - time.elapsed().as_millis() as i64
        })
    }
    fn position(s: &State, rate: u32) -> Option<u32> {
        if s.rate > 0 {
            if let Some((start, time)) = s.feed_start {
                return Some(start.wrapping_add(
                    (time.elapsed().as_millis().saturating_sub(400) * rate as u128 / 1000) as u32,
                ));
            }
        }
        s.anchor_rtp
    }
    fn restore(s: &mut State) {
        while let Some(f) = s.fed.pop_back() {
            s.queue.push_front(f);
        }
    }
    fn drop_before(s: &mut State, timestamp: u32) {
        s.flush_until = Some(timestamp);
        while s
            .queue
            .front()
            .is_some_and(|f| Self::before(f.timestamp, timestamp))
        {
            s.bytes -= s.queue.pop_front().unwrap().rtp.len();
        }
    }
    fn retire(s: &mut State, rate: u32) {
        if let Some(position) = Self::position(s, rate) {
            while s.fed.front().is_some_and(|f| {
                let end = f.timestamp.wrapping_add(1024);
                end == position || Self::before(end, position)
            }) {
                s.bytes -= s.fed.pop_front().unwrap().rtp.len();
            }
        }
    }
    fn restart(s: &mut State) -> bool {
        s.generation = s.generation.wrapping_add(1);
        let stop = s.feed_start.take().is_some();
        s.fed_samples = 0;
        stop
    }
    pub fn set_rate(
        &self,
        rtp_time: Option<u32>,
        new_rate: i32,
        now_ntp: u64,
    ) -> Result<Option<BplistValue>> {
        let i = &self.inner;
        let _delivery = i.delivery.enter();
        let (stop, anchor) = {
            let mut s = i.state.lock().unwrap();
            if s.closed {
                return Ok(None);
            }
            if new_rate > 0 {
                let start = rtp_time
                    .or_else(|| Self::position(&s, i.format.sample_rate))
                    .or_else(|| s.queue.front().map(|f| f.timestamp));
                Self::restore(&mut s);
                if let Some(start) = start {
                    Self::drop_before(&mut s, start);
                }
                s.anchor_rtp = start;
                s.anchor_ntp = Some(now_ntp.wrapping_add(Self::millis_to_ntp(400)));
            } else {
                let position = Self::position(&s, i.format.sample_rate);
                Self::restore(&mut s);
                if let Some(position) = position {
                    s.anchor_rtp = Some(position);
                    Self::drop_before(&mut s, position);
                }
                s.anchor_ntp = Some(now_ntp);
            }
            let stop = Self::restart(&mut s);
            s.rate = new_rate.clamp(0, 1);
            (stop, Self::anchor_locked(&s))
        };
        i.changed.notify_all();
        if stop {
            i.listener.on_stopped()?;
        }
        Ok(anchor)
    }
    fn anchor_locked(s: &State) -> Option<BplistValue> {
        Some(Self::anchor_plist(s.anchor_rtp?, s.anchor_ntp?, s.rate))
    }
    pub fn anchor(&self) -> Result<Option<BplistValue>> {
        Ok(Self::anchor_locked(&self.inner.state.lock().unwrap()))
    }
    pub fn flush(&self, until_timestamp: Option<u32>) -> Result<()> {
        let i = &self.inner;
        let _delivery = i.delivery.enter();
        let stop = {
            let mut s = i.state.lock().unwrap();
            if s.closed {
                return Ok(());
            }
            Self::restore(&mut s);
            if let Some(ts) = until_timestamp {
                Self::drop_before(&mut s, ts);
            }
            Self::restart(&mut s)
        };
        i.changed.notify_all();
        if stop {
            i.listener.on_stopped()?;
        }
        Ok(())
    }
    pub fn feedback(&self, now_ntp: u64, connection_id: u64) -> Result<BplistValue> {
        let i = &self.inner;
        let s = i.state.lock().unwrap();
        let mut fields = vec![
            ("type".into(), integer(i.format.payload_type)),
            ("sampleRate".into(), integer(i.format.sample_rate)),
        ];
        if let Some(position) = Self::position(&s, i.format.sample_rate) {
            fields.extend([
                ("streamConnectionID".into(), integer(connection_id)),
                ("timestamp".into(), integer(now_ntp)),
                (
                    "timestampRawNs".into(),
                    integer(crate::stream_receive_stats::monotonic_nanos()),
                ),
                ("sampleTime".into(), integer(position)),
            ]);
        }
        Ok(BplistValue::Dictionary(fields))
    }
    pub fn close(&self) -> Result<()> {
        let i = &self.inner;
        {
            let _delivery = i.delivery.enter();
            let stop = {
                let mut s = i.state.lock().unwrap();
                if s.closed {
                    return Ok(());
                }
                s.closed = true;
                s.queue.clear();
                s.fed.clear();
                s.bytes = 0;
                s.rate = 0;
                Self::restart(&mut s)
            };
            i.changed.notify_all();
            if stop {
                let _ = i.listener.on_stopped();
            }
        }
        i.worker.close()
    }
    pub fn open(key: &[u8; 32], body: &[u8]) -> Option<BufferedFrame> {
        if body.first() != Some(&0x80) {
            return None;
        }
        let rtp = AudioStreamCodec::open(key, body).ok()?;
        Some(BufferedFrame {
            sequence: u16::from_be_bytes([rtp[2], rtp[3]]),
            timestamp: u32::from_be_bytes(rtp[4..8].try_into().unwrap()),
            rtp,
        })
    }
    pub fn before(a: u32, b: u32) -> bool {
        a.wrapping_sub(b) >= 0x80000000
    }
    pub fn millis_to_ntp(ms: u64) -> u64 {
        ((ms as u128) << 32).wrapping_div(1000) as u64
    }
    pub fn anchor_plist(rtp: u32, ntp: u64, rate: i32) -> BplistValue {
        BplistValue::Dictionary(vec![
            ("rtpTime".into(), integer(rtp)),
            (
                "networkTimeSecs".into(),
                BplistValue::Integer(((ntp >> 32) as i64 - 2_208_988_800).into()),
            ),
            ("networkTimeFrac".into(), integer((ntp & 0xffffffff) << 32)),
            ("rate".into(), BplistValue::Integer(rate.into())),
        ])
    }
}
fn integer(v: impl Into<num_bigint::BigInt>) -> BplistValue {
    BplistValue::Integer(v.into())
}

/// Protocol message, field, or framing constant.
pub const TAG: &str = "xcertplay-usb";
/// Protocol message, field, or framing constant.
pub const STREAM_TYPE: u64 = 103;
/// Protocol message, field, or framing constant.
pub const MAX_BUFFER_MILLIS: u64 = 120_000;
/// Protocol message, field, or framing constant.
pub const PACED_FILL_FLOOR_MILLIS: u64 = 15_000;
/// Protocol message, field, or framing constant.
pub const FILL_RATE_MULTIPLE: u64 = 4;
/// Protocol message, field, or framing constant.
pub const LEAD_MILLIS: u64 = 1_000;
/// Protocol message, field, or framing constant.
pub const START_LATENCY_MILLIS: u64 = 400;
/// Protocol message, field, or framing constant.
pub const FEED_POLL_MILLIS: u64 = 20;
/// Protocol message, field, or framing constant.
pub const STATS_WINDOW_NS: u64 = 10_000_000_000;
/// Protocol message, field, or framing constant.
pub const SAMPLES_PER_FRAME: u64 = 1024;
/// Protocol message, field, or framing constant.
pub const LENGTH_BYTES: u64 = 2;
/// Protocol message, field, or framing constant.
pub const RTP_HEADER: u64 = 12;
/// Protocol message, field, or framing constant.
pub const NONCE_BYTES: u64 = 8;
/// Protocol message, field, or framing constant.
pub const TAG_BYTES: u64 = 16;
/// Protocol message, field, or framing constant.
pub const U32: u64 = 0xffff_ffff;
/// Protocol message, field, or framing constant.
pub const NTP_UNIX_OFFSET: u64 = 2_208_988_800;

/// Constant expression: `8 * 1024 * 1024`.
pub const AUDIO_BUFFER_BYTES: u64 = 8388608;
