// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use carplay_airplay::{AudioCodecKind, MicrophoneConfig, MicrophonePacketizer};
use std::{
    net::UdpSocket,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
pub trait MicrophoneUplink: Send + Sync {
    fn start(&self) -> Result<()>;
    fn offer(&self, encoded: &[u8]) -> Result<bool>;
    fn close(&self) -> Result<()>;
}
struct FrameState {
    queue: std::collections::VecDeque<Vec<u8>>,
    pending: Vec<u8>,
    packetizer: Option<MicrophonePacketizer>,
}
pub struct MicrophoneFrames {
    config: MicrophoneConfig,
    state: Mutex<FrameState>,
}
impl MicrophoneFrames {
    pub fn new(config: MicrophoneConfig) -> Self {
        Self {
            config,
            state: Mutex::new(FrameState {
                queue: Default::default(),
                pending: Vec::new(),
                packetizer: None,
            }),
        }
    }
    pub fn offer(&self, encoded: &[u8]) -> Result<bool> {
        if self.config.codec == AudioCodecKind::Lpcm
            && encoded.len() != self.config.sample_rate as usize / 50 * 2
        {
            return Ok(false);
        }
        if encoded.len() > 65536 {
            return Ok(false);
        }
        let mut s = self.state.lock().unwrap();
        if s.queue.len() == 5 {
            s.queue.pop_front();
        }
        s.queue.push_back(encoded.to_vec());
        Ok(true)
    }
    pub fn tick(&self) -> Result<Vec<Vec<u8>>> {
        let mut s = self.state.lock().unwrap();
        if s.packetizer.is_none() {
            s.packetizer = Some(MicrophonePacketizer::new(self.config.clone())?);
        }
        let mono = s.queue.pop_front();
        if self.config.codec == AudioCodecKind::Opus {
            return mono
                .map(|b| {
                    s.packetizer
                        .as_mut()
                        .unwrap()
                        .packetize(&b)
                        .map(|p| vec![p])
                })
                .unwrap_or_else(|| Ok(Vec::new()));
        }
        let mono = mono.unwrap_or_else(|| vec![0; self.config.sample_rate as usize / 50 * 2]);
        for sample in mono.chunks_exact(2) {
            for _ in 0..self.config.channels {
                s.pending.extend_from_slice(sample);
            }
        }
        let frame = self.config.sample_rate as usize * self.config.frame_millis as usize / 1000
            * self.config.channels as usize
            * 2;
        if frame == 0 {
            return Err(std::io::Error::other("Invalid microphone frame size"));
        }
        let mut result = Vec::new();
        while s.pending.len() >= frame {
            let body: Vec<_> = s.pending.drain(..frame).collect();
            result.push(s.packetizer.as_mut().unwrap().packetize(&body)?);
        }
        Ok(result)
    }
}
pub struct LinuxMicrophoneUplink {
    config: MicrophoneConfig,
    frames: Arc<MicrophoneFrames>,
    running: Arc<AtomicBool>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl LinuxMicrophoneUplink {
    pub fn new(config: MicrophoneConfig) -> Result<Self> {
        MicrophonePacketizer::new(config.clone())?;
        Ok(Self {
            frames: Arc::new(MicrophoneFrames::new(config.clone())),
            config,
            running: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        })
    }
}
impl MicrophoneUplink for LinuxMicrophoneUplink {
    fn start(&self) -> Result<()> {
        if self.running.swap(true, Ordering::AcqRel) {
            return Err(std::io::Error::other("Microphone already running"));
        }
        let config = self.config.clone();
        let frames = self.frames.clone();
        let running = self.running.clone();
        let socket = UdpSocket::bind(if config.host.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        })?;
        socket.connect((config.host, config.port))?;
        *self.worker.lock().unwrap() = Some(crate::platform::spawn_worker(
            "linux-microphone",
            Box::new(move || {
                let period = Duration::from_millis(20);
                let mut next = Instant::now();
                while running.load(Ordering::Acquire) {
                    match frames.tick() {
                        Ok(packets) => {
                            for p in packets {
                                if socket.send(&p).is_err() {
                                    running.store(false, Ordering::Release);
                                    break;
                                }
                            }
                        }
                        Err(_) => {
                            running.store(false, Ordering::Release);
                            break;
                        }
                    }
                    next += period;
                    let wait = next.saturating_duration_since(Instant::now());
                    if wait.is_zero() {
                        next = Instant::now();
                    } else {
                        std::thread::sleep(wait)
                    }
                }
            }),
        )?);
        Ok(())
    }
    fn offer(&self, b: &[u8]) -> Result<bool> {
        if !self.running.load(Ordering::Acquire) {
            return Ok(false);
        }
        self.frames.offer(b)
    }
    fn close(&self) -> Result<()> {
        self.running.store(false, Ordering::Release);
        if let Some(w) = self.worker.lock().unwrap().take() {
            let _ = w.join();
        }
        Ok(())
    }
}
impl Drop for LinuxMicrophoneUplink {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
