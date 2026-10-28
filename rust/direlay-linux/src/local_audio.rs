// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Optional ALSA endpoints. External codecs never run on the media delivery thread.
use crate::{MicrophoneUplink, Result};
use carplay_airplay::{AudioCodecKind, AudioFormat, MicrophoneConfig};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Devices {
    pub playback: Vec<String>,
    pub capture: Vec<String>,
    pub playback_available: bool,
    pub capture_available: bool,
}
fn executable(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}
pub fn devices(root: &Path) -> Devices {
    inventory(
        &std::fs::read_to_string(root.join("proc/asound/pcm")).unwrap_or_default(),
        executable("ffmpeg").is_some(),
        executable("arecord").is_some(),
    )
}
fn inventory(pcm: &str, decoder: bool, recorder: bool) -> Devices {
    let mut result = Devices::default();
    for line in pcm.lines().take(256) {
        let Some((id, rest)) = line.split_once(':') else {
            continue;
        };
        let Some((card, device)) = id.split_once('-') else {
            continue;
        };
        let (Ok(card), Ok(device)) = (card.parse::<u16>(), device.parse::<u16>()) else {
            continue;
        };
        let name = format!("plughw:{card},{device}");
        if rest.contains("playback ") {
            result.playback.push(name.clone());
        }
        if rest.contains("capture ") {
            result.capture.push(name);
        }
    }
    result.playback_available = decoder && !result.playback.is_empty();
    result.capture_available = recorder && !result.capture.is_empty();
    result
}
fn nonblocking(fd: &impl std::os::fd::AsRawFd) -> Result<()> {
    let fd = fd.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub struct Playback {
    queue: SyncSender<Vec<u8>>,
    worker: Option<JoinHandle<()>>,
    running: Arc<AtomicBool>,
}
impl Playback {
    pub fn open(format: &AudioFormat, devices: &Devices) -> Result<Self> {
        Self::open_program(
            format,
            devices,
            &executable("ffmpeg").ok_or(std::io::ErrorKind::NotFound)?,
        )
    }
    fn open_program(format: &AudioFormat, devices: &Devices, program: &Path) -> Result<Self> {
        if !devices.playback_available {
            return Err(std::io::ErrorKind::Unsupported.into());
        }
        let mut args = vec![
            "-nostdin".to_owned(),
            "-loglevel".into(),
            "quiet".into(),
            "-probesize".into(),
            "32".into(),
            "-analyzeduration".into(),
            "0".into(),
            "-fflags".into(),
            "nobuffer".into(),
            "-f".into(),
        ];
        match format.codec {
            AudioCodecKind::Lpcm => args.extend([
                "s16le".into(),
                "-ar".into(),
                format.sample_rate.to_string(),
                "-ac".into(),
                format.channels.to_string(),
            ]),
            AudioCodecKind::AacLc => args.push("aac".into()),
            _ => return Err(std::io::ErrorKind::Unsupported.into()),
        }
        args.extend([
            "-i".into(),
            "pipe:0".into(),
            "-f".into(),
            "alsa".into(),
            devices.playback[0].clone(),
        ]);
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        if let Err(e) = nonblocking(&stdin) {
            stop(&mut child);
            return Err(e);
        }
        // At most 32 normalized packets, with the same 64 KiB packet bound as the wire.
        let (queue, rx) = sync_channel::<Vec<u8>>(32);
        let running = Arc::new(AtomicBool::new(true));
        let active = running.clone();
        let aac = format.codec == AudioCodecKind::AacLc;
        let worker = std::thread::spawn(move || {
            while active.load(Ordering::Acquire) {
                let packet = match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(p) => p,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                let packet = if aac { aac_adts(&packet) } else { packet };
                let mut remaining = packet.as_slice();
                let deadline = std::time::Instant::now() + Duration::from_millis(100);
                while !remaining.is_empty() && active.load(Ordering::Acquire) {
                    match stdin.write(remaining) {
                        Ok(0) => break,
                        Ok(n) => remaining = &remaining[n..],
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => {
                            active.store(false, Ordering::Release);
                            break;
                        }
                    }
                }
            }
            stop(&mut child);
        });
        Ok(Self {
            queue,
            worker: Some(worker),
            running,
        })
    }
    pub fn offer(&self, packet: &[u8]) {
        if packet.len() <= 65536 {
            let _ = self.queue.try_send(packet.to_vec());
        }
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
// Linux's media offer is AAC-LC 48 kHz stereo (ASC 0x1190).
fn aac_adts(packet: &[u8]) -> Vec<u8> {
    let size = packet.len() + 7;
    if size > 8191 {
        return Vec::new();
    }
    let mut data = vec![
        0xff,
        0xf1,
        0x4c,
        0x80 | ((size >> 11) as u8),
        (size >> 3) as u8,
        ((size & 7) as u8) << 5 | 0x1f,
        0xfc,
    ];
    data.extend_from_slice(packet);
    data
}

pub struct Capture {
    uplink: Arc<crate::LinuxMicrophoneUplink>,
    child: Mutex<Option<Child>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    running: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
}
impl Capture {
    pub fn open(config: MicrophoneConfig, devices: &Devices) -> Result<Self> {
        Self::open_program(
            config,
            devices,
            &executable("arecord").ok_or(std::io::ErrorKind::NotFound)?,
        )
    }
    fn open_program(config: MicrophoneConfig, devices: &Devices, program: &Path) -> Result<Self> {
        if !devices.capture_available || config.codec != AudioCodecKind::Lpcm {
            return Err(std::io::ErrorKind::Unsupported.into());
        }
        let uplink = Arc::new(crate::LinuxMicrophoneUplink::new(config.clone())?);
        let mut child = Command::new(program)
            .args([
                "-q",
                "--period-time=20000",
                "--buffer-time=80000",
                "-D",
                &devices.capture[0],
                "-t",
                "raw",
                "-f",
                "S16_LE",
                "-c",
                "1",
                "-r",
                &config.sample_rate.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdout = child.stdout.take().unwrap();
        if let Err(e) = nonblocking(&stdout) {
            stop(&mut child);
            return Err(e);
        }
        let running = Arc::new(AtomicBool::new(true));
        let started = Arc::new(AtomicBool::new(false));
        let ready = started.clone();
        let active = running.clone();
        let target = uplink.clone();
        let length = config.sample_rate as usize / 50 * 2;
        let worker = std::thread::spawn(move || {
            let mut packet = vec![0; length];
            let mut offset = 0;
            // Wait until the session's UDP uplink has started.
            while !ready.load(Ordering::Acquire) && active.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(2));
            }
            while active.load(Ordering::Acquire) {
                match stdout.read(&mut packet[offset..]) {
                    Ok(0) => break,
                    Ok(n) => {
                        offset += n;
                        if offset == length {
                            let _ = target.offer(&packet);
                            offset = 0;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            uplink,
            child: Mutex::new(Some(child)),
            worker: Mutex::new(Some(worker)),
            running,
            started,
        })
    }
}
impl MicrophoneUplink for Capture {
    fn start(&self) -> Result<()> {
        self.uplink.start()?;
        self.started.store(true, Ordering::Release);
        Ok(())
    }
    // A head-unit packet cannot enter a microphone configured for this relay.
    fn offer(&self, _: &[u8]) -> Result<bool> {
        Ok(false)
    }
    fn close(&self) -> Result<()> {
        self.running.store(false, Ordering::Release);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            stop(&mut child);
        }
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
        self.uplink.close()
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_microphone_rejects_receiver_packets_and_closes_before_start() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "w16r-mic-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let program = root.join("capture");
        std::fs::write(&program, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = MicrophoneConfig {
            audio_type: "speechrecognition".into(),
            sample_rate: 16000,
            channels: 1,
            payload_type: 96,
            frame_millis: 20,
            host: "127.0.0.1".parse().unwrap(),
            port: 9,
            key: vec![0; 32],
            codec: AudioCodecKind::Lpcm,
            bitrate: None,
            opus_clock_rate: 16000,
        };
        let devices = inventory("00-00: Test : capture 1", false, true);
        let capture = Capture::open_program(config, &devices, &program).unwrap();
        assert!(!capture.offer(&vec![0; 640]).unwrap());
        let start = std::time::Instant::now();
        capture.close().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
        drop(capture);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn playback_uses_a_bounded_worker_and_reaps_the_fake_child() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "w16r-audio-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let program = root.join("playback");
        std::fs::write(&program, "#!/bin/sh\nexec cat >/dev/null\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let format = AudioFormat {
            codec: AudioCodecKind::Lpcm,
            sample_rate: 48000,
            channels: 2,
            payload_type: 96,
            audio_type: "alert".into(),
            rtp_clock_rate: 48000,
        };
        let devices = inventory("00-00: Test : playback 1", true, false);
        let playback = Playback::open_program(&format, &devices, &program).unwrap();
        for _ in 0..100 {
            playback.offer(&[0; 4096]);
        }
        let start = std::time::Instant::now();
        drop(playback);
        assert!(start.elapsed() < Duration::from_secs(1));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn alsa_endpoints_require_both_a_device_and_the_optional_tool() {
        let pcm = "00-00: gx-sound-card : playback 1\n02-01: USB microphone : capture 1\nnot-a-device: playback 1";
        let d = inventory(pcm, true, true);
        assert_eq!(d.playback, ["plughw:0,0"]);
        assert_eq!(d.capture, ["plughw:2,1"]);
        assert!(d.playback_available && d.capture_available);
        let d = inventory(pcm, false, false);
        assert!(!d.playback_available && !d.capture_available);
        let d = inventory("", true, true);
        assert!(!d.playback_available && !d.capture_available);
    }
    #[test]
    fn aac_packets_get_the_offered_lc_48khz_stereo_header() {
        let frame = aac_adts(&[1, 2, 3]);
        assert_eq!(&frame[..4], &[0xff, 0xf1, 0x4c, 0x80]);
        assert_eq!(
            (usize::from(frame[3] & 3) << 11)
                | (usize::from(frame[4]) << 3)
                | usize::from(frame[5] >> 5),
            10
        );
        assert_eq!(&frame[7..], &[1, 2, 3]);
        assert!(aac_adts(&vec![0; 8192]).is_empty());
    }
}
