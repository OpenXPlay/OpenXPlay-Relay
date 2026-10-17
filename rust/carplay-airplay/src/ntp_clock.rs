// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::screen_stream::{invalid, Worker};
use crate::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub const PT_REQUEST: u64 = 210;
pub const PT_RESPONSE: u64 = 211;
pub const NTP_PACKET_BYTES: u64 = 32;
pub const NTP_ORIGINATE_OFFSET: u64 = 8;
pub const NTP_RECEIVE_OFFSET: u64 = 16;
pub const NTP_TRANSMIT_OFFSET: u64 = 24;
pub const REQUEST_INTERVAL_MS: u64 = 1000;
pub const RECEIVE_BUFFER_BYTES: u64 = 2048;
pub const DELAY_WINDOW: u64 = 8;
pub const PICK_COUNT: u64 = 2;
pub const NTP_EPOCH_OFFSET: u64 = 2_208_988_800;
pub const STEP_THRESHOLD_SEC: f64 = 0.128;
pub const SLEW_GAIN: f64 = 0.125;
pub const TWO32_DOUBLE: u64 = 4294967296;
#[derive(Clone)]
pub struct NtpClock {
    inner: Arc<Inner>,
}
struct Inner {
    transport: Arc<dyn DatagramTransport>,
    worker: Arc<Worker>,
    receiving: AtomicBool,
    sending: AtomicBool,
    origin: Instant,
    epoch_ns: u128,
    clock: Mutex<Clock>,
}
struct Clock {
    offset_ns: i128,
    delays: [f64; 8],
    delay_index: usize,
    pick_count: u8,
    pick_rtt: f64,
    pick_offset: f64,
    pending: Option<u64>,
    synced: bool,
}
impl NtpClock {
    pub fn new(transport: Arc<dyn DatagramTransport>) -> Self {
        Self {
            inner: Arc::new(Inner {
                transport,
                worker: Arc::new(Worker::default()),
                receiving: AtomicBool::new(false),
                sending: AtomicBool::new(false),
                origin: Instant::now(),
                epoch_ns: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                clock: Mutex::new(Clock {
                    offset_ns: 0,
                    delays: [f64::INFINITY; 8],
                    delay_index: 0,
                    pick_count: 2,
                    pick_rtt: f64::INFINITY,
                    pick_offset: 0.0,
                    pending: None,
                    synced: false,
                }),
            }),
        }
    }
    pub fn to_ntp(unix_nanos: u64) -> u64 {
        (((unix_nanos / 1_000_000_000).wrapping_add(NTP_EPOCH_OFFSET)) << 32)
            | (((unix_nanos % 1_000_000_000) as u128 * (1u128 << 32) / 1_000_000_000) as u64)
    }
    pub fn synced_ntp(&self) -> u64 {
        let offset = self.inner.clock.lock().unwrap().offset_ns;
        let ns =
            (self.inner.epoch_ns as i128 + self.inner.origin.elapsed().as_nanos() as i128 + offset)
                .max(0) as u64;
        Self::to_ntp(ns)
    }
    pub fn listen(&self) -> Result<u16> {
        if self.inner.receiving.swap(true, Ordering::AcqRel) {
            return Err(invalid("NtpClock is already running"));
        }
        self.inner
            .worker
            .add_datagram(self.inner.transport.clone())?;
        let this = self.clone();
        self.inner.worker.spawn("airplay-ntp-rx", move || {
            while !this.inner.worker.closed() {
                match this
                    .inner
                    .transport
                    .recv_from(2048, Duration::from_millis(100))
                {
                    Ok(Some((message, source))) if message.len() >= 32 => match message[1] {
                        210 => {
                            let mut reply = [0u8; 32];
                            reply[..4].copy_from_slice(&[0x80, 211, 0, 7]);
                            reply[8..16].copy_from_slice(&message[24..32]);
                            reply[16..24].copy_from_slice(&this.synced_ntp().to_be_bytes());
                            reply[24..].copy_from_slice(&this.synced_ntp().to_be_bytes());
                            let _ = this.inner.transport.send_to(&reply, source);
                        }
                        211 => this.handle_response(&message),
                        _ => {}
                    },
                    _ => {}
                }
            }
        })?;
        self.inner.transport.local_port()
    }
    pub fn start(&self) -> Result<()> {
        if !self.inner.receiving.load(Ordering::Acquire) {
            self.listen()?;
        }
        if self.inner.sending.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let this = self.clone();
        self.inner.worker.spawn("airplay-ntp-tx", move || {
            while !this.inner.worker.closed() {
                let mut request = [0u8; 32];
                request[..4].copy_from_slice(&[0x80, 210, 0, 7]);
                let t1 = this.synced_ntp();
                this.inner.clock.lock().unwrap().pending = Some(t1);
                request[24..].copy_from_slice(&t1.to_be_bytes());
                let _ = this.inner.transport.send(&request);
                for _ in 0..10 {
                    if this.inner.worker.closed() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })
    }
    pub fn close(&self) -> Result<()> {
        self.inner.worker.close()
    }
    fn handle_response(&self, message: &[u8]) {
        self.handle_response_at(message, self.synced_ntp());
    }
    fn handle_response_at(&self, message: &[u8], t4: u64) {
        let t1 = u64::from_be_bytes(message[8..16].try_into().unwrap());
        let t2 = u64::from_be_bytes(message[16..24].try_into().unwrap());
        let t3 = u64::from_be_bytes(message[24..32].try_into().unwrap());
        let mut c = self.inner.clock.lock().unwrap();
        if c.pending != Some(t1) {
            return;
        }
        c.pending = None;
        let offset = ((t2 as i128 - t1 as i128) + (t3 as i128 - t4 as i128)) as f64 * 0.5
            / TWO32_DOUBLE as f64;
        let rtt =
            ((t4 as i128 - t1 as i128) - (t3 as i128 - t2 as i128)) as f64 / TWO32_DOUBLE as f64;
        if rtt < 0.0 {
            return;
        }
        if rtt < c.pick_rtt {
            c.pick_rtt = rtt;
            c.pick_offset = offset;
        }
        c.pick_count -= 1;
        if c.pick_count > 0 {
            return;
        }
        let rtt = c.pick_rtt;
        let offset = c.pick_offset;
        c.pick_count = 2;
        c.pick_rtt = f64::INFINITY;
        let use_sample = c.delays.iter().all(|d| rtt <= *d);
        let index = c.delay_index;
        c.delays[index] = rtt;
        c.delay_index = (index + 1) % 8;
        if use_sample {
            let step = !c.synced || offset.abs() > STEP_THRESHOLD_SEC;
            c.offset_ns += (offset * if step { 1.0 } else { SLEW_GAIN } * 1e9).round() as i128;
            if step {
                c.delays.fill(f64::INFINITY);
                c.delay_index = 0;
                c.pending = None;
                c.synced = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carplay_iap2::Receive;
    struct Datagram;
    impl DatagramTransport for Datagram {
        fn local_port(&self) -> Result<u16> {
            Ok(1)
        }
        fn recv(&self, _: usize, _: Duration) -> Result<Receive> {
            Ok(Receive::Timeout)
        }
        fn recv_from(
            &self,
            _: usize,
            _: Duration,
        ) -> Result<Option<(Vec<u8>, std::net::SocketAddr)>> {
            Ok(None)
        }
        fn send(&self, _: &[u8]) -> Result<()> {
            Ok(())
        }
        fn send_to(&self, _: &[u8], _: std::net::SocketAddr) -> Result<()> {
            Ok(())
        }
        fn close(&self) -> Result<()> {
            Ok(())
        }
    }
    fn sample(clock: &NtpClock, rtt_ms: u64, offset_ms: u64) {
        let now = clock.synced_ntp();
        let rtt = ((rtt_ms as u128) << 32) / 1000;
        let t1 = now.wrapping_sub(rtt as u64);
        let t2 = t1
            .wrapping_add((rtt / 2) as u64)
            .wrapping_add((((offset_ms as u128) << 32) / 1000) as u64);
        clock.inner.clock.lock().unwrap().pending = Some(t1);
        let mut message = [0; 32];
        message[8..16].copy_from_slice(&t1.to_be_bytes());
        message[16..24].copy_from_slice(&t2.to_be_bytes());
        message[24..].copy_from_slice(&t2.to_be_bytes());
        clock.handle_response_at(&message, now);
    }
    #[test]
    fn picks_best_of_two_and_slews_small_offsets_by_one_eighth() {
        let clock = NtpClock::new(Arc::new(Datagram));
        sample(&clock, 100, 5000);
        assert_eq!(clock.inner.clock.lock().unwrap().offset_ns, 0);
        sample(&clock, 10, 1000);
        let before = clock.inner.clock.lock().unwrap().offset_ns;
        assert!((999_000_000..=1_001_000_000).contains(&before));
        sample(&clock, 1, 64);
        sample(&clock, 1, 64);
        let after = clock.inner.clock.lock().unwrap().offset_ns;
        assert!((7_000_000..=9_000_000).contains(&(after - before)));
    }
    #[test]
    fn large_offsets_step_and_delay_window_rejects_worse_samples() {
        let clock = NtpClock::new(Arc::new(Datagram));
        sample(&clock, 1, 1000);
        sample(&clock, 1, 1000);
        sample(&clock, 1, 200);
        sample(&clock, 1, 200);
        let stepped = clock.inner.clock.lock().unwrap().offset_ns;
        assert!((1_198_000_000..=1_202_000_000).contains(&stepped));
        sample(&clock, 1, 0);
        sample(&clock, 1, 0);
        let before = clock.inner.clock.lock().unwrap().offset_ns;
        sample(&clock, 50, 5000);
        sample(&clock, 50, 5000);
        assert_eq!(clock.inner.clock.lock().unwrap().offset_ns, before);
    }
    #[test]
    fn mismatched_origin_and_negative_round_trip_do_not_steer_clock() {
        let clock = NtpClock::new(Arc::new(Datagram));
        clock.handle_response(&[0; 32]);
        assert_eq!(clock.inner.clock.lock().unwrap().pick_count, 2);
        let t1 = clock.synced_ntp();
        clock.inner.clock.lock().unwrap().pending = Some(t1);
        let mut message = [0; 32];
        message[8..16].copy_from_slice(&t1.to_be_bytes());
        message[16..24].copy_from_slice(&t1.to_be_bytes());
        message[24..].copy_from_slice(&(t1 + (1u64 << 32)).to_be_bytes());
        clock.handle_response(&message);
        assert_eq!(clock.inner.clock.lock().unwrap().offset_ns, 0);
        assert_eq!(clock.inner.clock.lock().unwrap().pick_count, 2);
    }
}
