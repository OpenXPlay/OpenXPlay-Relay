// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};
pub(crate) fn monotonic_nanos() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}
pub struct StreamReceiveStats {
    label: String,
    state: Mutex<State>,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
    report: Arc<dyn Fn(String) + Send + Sync>,
}
#[derive(Default)]
struct State {
    window: u64,
    read: u64,
    processing: u64,
    packets: u64,
    bytes: u64,
    max_read: u64,
    long_reads: u64,
    gap_after_long: u64,
    missing_after_long: u64,
    last_received: u64,
    inter: u64,
    process: u64,
    last_timestamp: Option<u32>,
    next_sequence: Option<u16>,
    gaps: u64,
    gap_events: u64,
    gap_max: u16,
    gap_last: String,
    gap_at: i64,
    late: u64,
    decrypt_samples: u64,
    decrypt_bytes: u64,
    decrypt_sum: u64,
    decrypt_max: u64,
}
impl State {
    fn window(now: u64) -> Self {
        Self {
            window: now,
            read: now,
            processing: now,
            gap_last: "none".into(),
            gap_at: -1,
            ..Self::default()
        }
    }
}
impl StreamReceiveStats {
    pub fn new(label: &str) -> Self {
        Self::with_clock(
            label,
            Arc::new(monotonic_nanos),
            Arc::new(|line| eprintln!("{line}")),
        )
    }
    pub fn with_clock(
        label: &str,
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
        report: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        Self {
            label: label.into(),
            state: Mutex::new(State::window(now())),
            now,
            report,
        }
    }
    pub fn reading(&self) {
        self.reading_at((self.now)());
    }
    pub fn reading_at(&self, ns: u64) {
        self.state.lock().unwrap().read = ns;
    }
    pub fn received(&self, bytes: usize, now_nanos: u64) {
        self.received_rtp(bytes, None, None, now_nanos);
    }
    pub fn received_rtp(
        &self,
        bytes: usize,
        sequence: Option<u16>,
        timestamp: Option<u32>,
        now: u64,
    ) {
        let mut s = self.state.lock().unwrap();
        s.processing = now;
        let read = now.saturating_sub(s.read);
        s.max_read = s.max_read.max(read);
        if read >= 250_000_000 {
            s.long_reads += 1;
        }
        if s.last_received != 0 {
            s.inter = s.inter.max(now.saturating_sub(s.last_received));
        }
        s.last_received = now;
        s.packets += 1;
        s.bytes += bytes as u64;
        if let Some(seq) = sequence {
            let delta = s
                .next_sequence
                .map_or(0, |expected| seq.wrapping_sub(expected));
            if delta < 0x8000 {
                s.gaps += delta as u64;
                if delta > 0 {
                    s.gap_events += 1;
                    s.gap_max = s.gap_max.max(delta);
                    if read >= 250_000_000 {
                        s.gap_after_long += 1;
                        s.missing_after_long += delta as u64;
                    }
                    s.gap_last = format!("expected={} received={seq} missing={delta} previousRtpTs={} receivedRtpTs={}", s.next_sequence.unwrap(), s.last_timestamp.map_or("unknown".into(), |v| v.to_string()), timestamp.map_or("unknown".into(), |v| v.to_string()));
                    s.gap_at = (now.saturating_sub(s.window) / 1_000_000) as i64;
                }
                s.next_sequence = Some(seq.wrapping_add(1));
            } else {
                s.late += 1;
            }
        }
        if timestamp.is_some() {
            s.last_timestamp = timestamp;
        }
    }
    pub fn decrypted(&self, duration_ns: u64, size: usize) {
        let mut s = self.state.lock().unwrap();
        s.decrypt_samples += 1;
        s.decrypt_bytes += size as u64;
        s.decrypt_sum += duration_ns;
        s.decrypt_max = s.decrypt_max.max(duration_ns);
    }
    pub fn processed(&self) {
        let now = (self.now)();
        {
            let mut s = self.state.lock().unwrap();
            s.process = s.process.max(now.saturating_sub(s.processing));
        }
        self.flush(false);
    }
    fn line(&self, s: &State, now: u64, ended: bool) -> String {
        let mut line = format!("Receive: {} packets={} bytes={} readMaxMs={} processMaxUs={} seqForwardGaps={} lateOrDuplicate={} interArrivalMaxMs={} seqGapEvents={} seqGapMax={} seqGapLast=[{}] seqGapAtMs={} ended={} windowMs={} readsOver250Ms={} seqGapAfterReadOver250Ms={} seqMissingAfterReadOver250Ms={}", self.label, s.packets, s.bytes, s.max_read/1_000_000, s.process/1000, s.gaps, s.late, s.inter/1_000_000, s.gap_events, s.gap_max, s.gap_last, s.gap_at, ended, now.saturating_sub(s.window)/1_000_000, s.long_reads, s.gap_after_long, s.missing_after_long);
        if s.decrypt_samples > 0 {
            line += &format!(
                " decryptAvgUs={} decryptMaxUs={} decryptMBps={}",
                s.decrypt_sum / s.decrypt_samples / 1000,
                s.decrypt_max / 1000,
                if s.decrypt_sum == 0 {
                    0
                } else {
                    s.decrypt_bytes * 1000 / s.decrypt_sum
                }
            );
        }
        line
    }
    pub fn summary(&self, now_nanos: u64) -> String {
        self.line(&self.state.lock().unwrap(), now_nanos, false)
    }
    pub fn flush(&self, ended: bool) {
        let now = (self.now)();
        let line = {
            let mut s = self.state.lock().unwrap();
            if !ended && now.saturating_sub(s.window) < 5_000_000_000 {
                return;
            }
            let line = self.line(&s, now, ended);
            let next = State {
                next_sequence: s.next_sequence,
                last_timestamp: s.last_timestamp,
                last_received: s.last_received,
                ..State::window(now)
            };
            *s = next;
            line
        };
        (self.report)(line);
    }
}
