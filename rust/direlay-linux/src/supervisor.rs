// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// OpenXPlay-Stream oxp-relay CLI contract tests.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{self, BufRead, BufReader, Read},
    net::Ipv4Addr,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub type EventSink = Arc<dyn Fn(Value) + Send + Sync>;
#[derive(Clone, Debug)]
pub struct CarlinkSettings {
    pub name: String,
    pub model: String,
    pub hevc: String,
    pub trace: bool,
    pub auto_reconnect: bool,
    pub ble: Option<BleCarlinkSettings>,
    pub head_unit: Ipv4Addr,
    pub local: Ipv4Addr,
    pub pin: Option<String>,
    pub ports: BTreeMap<String, u16>,
}
#[derive(Clone, Debug)]
pub struct BleCarlinkSettings {
    pub adapter: Option<String>,
    pub interface: String,
    pub address: Option<String>,
    pub id: String,
    /// Carlink band the head unit creates its group on: 0 is 2.4 GHz, 1 allows 5 GHz.
    pub band: u8,
    pub phone_mac: Option<String>,
}
/// A certificate for the head-unit page from PEM files (the trusted-HTTPS name's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PemTls {
    pub name: String,
    pub cert: PathBuf,
    pub key: PathBuf,
}
#[derive(Clone, Debug)]
pub struct SupervisorSettings {
    pub binary: PathBuf,
    pub socket: PathBuf,
    pub web_port: u16,
    pub web_root: PathBuf,
    pub plugins: Option<crate::plugins::OutputPlugins>,
    pub state_dir: PathBuf,
    pub addresses: Vec<Ipv4Addr>,
    pub pairing: bool,
    pub carlink: Option<CarlinkSettings>,
    /// Serve this certificate for its SNI name while both files exist.
    pub pem_tls: Option<PemTls>,
}
impl SupervisorSettings {
    pub fn arguments(&self) -> Vec<OsString> {
        // Checked at every spawn: a respawn after the files were removed serves self-signed.
        let pem = self
            .pem_tls
            .as_ref()
            .filter(|t| t.cert.is_file() && t.key.is_file());
        let mut args: Vec<OsString> = if self.web_port == 0 {
            vec!["--unix".into(), self.socket.as_os_str().into()]
        } else {
            let mut args: Vec<OsString> = vec![
                "--unix".into(),
                self.socket.as_os_str().into(),
                "--web-port".into(),
                self.web_port.to_string().into(),
                "--web-root".into(),
                self.web_root.as_os_str().into(),
                "--web-bind".into(),
                "0.0.0.0".into(),
            ];
            if let Some(pem) = pem {
                args.extend([
                    "--tls-cert".into(),
                    pem.cert.as_os_str().into(),
                    "--tls-key".into(),
                    pem.key.as_os_str().into(),
                    "--tls-name".into(),
                    pem.name.clone().into(),
                ]);
            }
            args.extend([
                "--tls-self-signed".into(),
                self.state_dir.join("receiver-tls").into_os_string(),
                "--tls-san".into(),
                "openxplay-linux.local".into(),
            ]);
            args
        };
        args.push("--host-ui".into());
        args.extend([
            "--relay-name".into(),
            crate::branding::DEVICE_NAME.into(),
            "--relay-version".into(),
            env!("CARGO_PKG_VERSION").into(),
        ]);
        let mut ips = self.addresses.clone();
        ips.sort();
        ips.dedup();
        for ip in ips.into_iter().filter(|_| self.web_port != 0) {
            args.extend(["--tls-san".into(), ip.to_string().into()]);
        }
        if self.pairing && self.web_port != 0 {
            args.push("--pairing".into());
        }
        if let Some(c) = &self.carlink {
            if c.trace {
                args.extend([
                    "--clink-trace".into(),
                    self.state_dir.join("carlink-trace").into_os_string(),
                    "--control-stdin".into(),
                ]);
            }
            args.extend([
                "--state-dir".into(),
                self.state_dir.join("carlink").into_os_string(),
            ]);
            args.extend([
                "--clink-name".into(),
                c.name.clone().into(),
                "--clink-model".into(),
                c.model.clone().into(),
                "--clink-hevc".into(),
                c.hevc.clone().into(),
            ]);
            args.extend([
                "--clink-state-dir".into(),
                self.state_dir.as_os_str().into(),
            ]);
            if let Some(ble) = &c.ble {
                args.extend(["--clink-scan".into(), "continuous".into()]);
                if let Some(mac) = &ble.phone_mac {
                    args.extend(["--clink-phone-mac".into(), mac.into()]);
                }
                args.extend([
                    "--clink-link".into(),
                    "ble".into(),
                    "--clink-interface".into(),
                    ble.interface.clone().into(),
                    "--clink-id".into(),
                    ble.id.clone().into(),
                    "--clink-band".into(),
                    ble.band.to_string().into(),
                    "--pin-file".into(),
                    self.state_dir.join("carlink/pin").into_os_string(),
                ]);
                if let Some(adapter) = &ble.adapter {
                    args.extend(["--clink-adapter".into(), adapter.into()]);
                }
                if let Some(address) = &ble.address {
                    args.extend(["--clink-address".into(), address.into()]);
                }
            } else {
                args.extend([
                    "--clink-head-unit".into(),
                    c.head_unit.to_string().into(),
                    "--clink-local".into(),
                    c.local.to_string().into(),
                ]);
                if let Some(pin) = &c.pin {
                    args.extend(["--pin".into(), pin.into()]);
                }
            }
            if !c.ports.is_empty() {
                args.extend([
                    "--port".into(),
                    c.ports
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(",")
                        .into(),
                ]);
            }
        }
        if let Some(plugins) = &self.plugins {
            let enabled = plugins.present_ids();
            if !enabled.is_empty() {
                args.extend([
                    "--plugins-root".into(),
                    plugins.root.as_os_str().into(),
                    "--plugins-enabled".into(),
                    enabled.join(",").into(),
                    // Stream adds plugins/storage below this state directory.
                    "--plugins-state-dir".into(),
                    self.state_dir.as_os_str().into(),
                ]);
            }
        }
        args
    }
    pub fn receiver_urls(&self, host: &str) -> Vec<String> {
        if self.web_port == 0 {
            return Vec::new();
        }
        let mut urls = vec![format!("https://{host}:{}/", self.web_port)];
        for ip in &self.addresses {
            let url = format!("https://{ip}:{}/", self.web_port);
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        urls
    }
}
pub fn carlink_attempt_failed(event: &Value) -> bool {
    (event["type"] == "link" && event["state"] == "lost")
        || event["type"] == "error"
        || event["type"] == "pin-timeout"
        || (event["type"] == "state"
            && matches!(
                event["state"].as_str(),
                Some("ended" | "disconnected" | "Ended" | "Disconnected")
            ))
}
// oxp-bin FileTrust stores base64 64-byte peer public keys separately from its own identity.
pub fn has_trusted_carlink(state: &Path) -> bool {
    std::fs::read(state.join("carlink/trust.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["peers"].as_object().cloned())
        .is_some_and(|peers| {
            peers.values().any(|key| {
                key.as_str().is_some_and(|s| {
                    s.len() == 88
                        && s.ends_with("==")
                        && s.bytes()
                            .take(86)
                            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
                })
            })
        })
}
impl SupervisorSettings {
    pub fn launch_arguments(&self, attempt: bool) -> Vec<OsString> {
        let mut settings = self.clone();
        if !attempt && !has_trusted_carlink(&self.state_dir) {
            settings.carlink = None;
        }
        settings.arguments()
    }
}
/// A nonblocking pipe: one small atomic write, never waiting for a reader or another request.
#[derive(Clone)]
pub struct ControlPipe(Arc<Mutex<ChildStdin>>);
impl ControlPipe {
    fn new(stdin: ChildStdin) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let fd = stdin.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(Arc::new(Mutex::new(stdin))))
    }
    pub fn send_control(&self, value: &Value) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        let mut bytes = serde_json::to_vec(value)?;
        bytes.push(b'\n');
        // POSIX guarantees atomic pipe writes of at least 512 bytes; our commands are much smaller.
        if bytes.len() > 512 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "control too long",
            ));
        }
        let pipe = self
            .0
            .try_lock()
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        let written = unsafe { libc::write(pipe.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
        if written < 0 {
            return Err(io::Error::last_os_error());
        }
        if written as usize != bytes.len() {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        Ok(())
    }
}
pub trait RelayChild: Send {
    fn control(&self) -> Option<ControlPipe> {
        None
    }

    fn exited(&mut self) -> io::Result<bool>;
    fn stop(&mut self) -> io::Result<()>;
}
pub trait ChildFactory: Send + Sync + 'static {
    fn spawn(
        &self,
        binary: &Path,
        args: &[OsString],
        events: EventSink,
    ) -> io::Result<Box<dyn RelayChild>>;
}
pub struct ProcessChildFactory;
struct ProcessChild {
    child: Child,
    readers: Vec<JoinHandle<()>>,
    control: Option<ControlPipe>,
}
/// Ignore invalid/oversized event lines while keeping a bounded buffer.
pub fn forward_json_lines(input: impl Read, events: &EventSink) -> io::Result<()> {
    let mut input = BufReader::new(input);
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let buffer = input.fill_buf()?;
        if buffer.is_empty() {
            if !oversized && !line.is_empty() {
                if let Ok(Value::Object(value)) = serde_json::from_slice::<Value>(&line) {
                    events(Value::Object(value));
                }
            }
            break;
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let count = end.map_or(buffer.len(), |p| p + 1);
        if line.len() + count > 64 * 1024 {
            oversized = true;
        }
        if !oversized {
            line.extend_from_slice(&buffer[..count]);
        }
        input.consume(count);
        if end.is_some() {
            if !oversized {
                if let Ok(Value::Object(value)) = serde_json::from_slice::<Value>(&line) {
                    events(Value::Object(value));
                }
            }
            line.clear();
            oversized = false;
        }
    }
    Ok(())
}
impl ChildFactory for ProcessChildFactory {
    fn spawn(
        &self,
        binary: &Path,
        args: &[OsString],
        events: EventSink,
    ) -> io::Result<Box<dyn RelayChild>> {
        let mut child = Command::new(binary)
            .args(args)
            .stdin(if args.iter().any(|a| a == "--control-stdin") {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let control = match child.stdin.take().map(ControlPipe::new).transpose() {
            Ok(control) => control,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let copy = events.clone();
        let readers = vec![
            thread::spawn(move || {
                let _ = forward_json_lines(stdout, &copy);
            }),
            thread::spawn(move || {
                let _ = forward_json_lines(stderr, &events);
            }),
        ];
        Ok(Box::new(ProcessChild {
            child,
            readers,
            control,
        }))
    }
}
impl RelayChild for ProcessChild {
    fn control(&self) -> Option<ControlPipe> {
        self.control.clone()
    }
    fn exited(&mut self) -> io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
    fn stop(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_none() {
            // The child PID stays ours until it is reaped.
            let result = unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM) };
            if result != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                return Err(io::Error::last_os_error());
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.child.try_wait()?.is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            if self.child.try_wait()?.is_none() {
                self.child.kill()?;
            }
        }
        self.child.wait()?;
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
        Ok(())
    }
}
impl Drop for ProcessChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RestartPolicy {
    pub initial: Duration,
    pub maximum: Duration,
    pub stable_after: Duration,
    pub poll: Duration,
}
impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(250),
            maximum: Duration::from_secs(30),
            stable_after: Duration::from_secs(30),
            poll: Duration::from_millis(100),
        }
    }
}
pub fn restart_delay(policy: RestartPolicy, failures: u32) -> Duration {
    policy
        .initial
        .saturating_mul(1u32.checked_shl(failures.min(31)).unwrap_or(u32::MAX))
        .min(policy.maximum)
}
struct BandBackoff {
    initial: Duration,
    maximum: Duration,
    failures: u32,
    until: Option<Instant>,
}
impl BandBackoff {
    fn new(initial: Duration, maximum: Duration) -> Self {
        Self {
            initial,
            maximum,
            failures: 0,
            until: None,
        }
    }
    fn observe(&mut self, event: &Value, now: Instant) -> Option<Duration> {
        if event["type"] == "link" && event["state"] == "joined" {
            self.failures = 0;
            self.until = None;
        }
        let reason = event["reason"]
            .as_str()
            .or_else(|| event["message"].as_str());
        if !carlink_attempt_failed(event)
            || !reason.is_some_and(|r| r.starts_with("head_unit_band_unsupported"))
            || self.until.is_some_and(|until| until > now)
        {
            return None;
        }
        let delay = self
            .initial
            .saturating_mul(1u32.checked_shl(self.failures.min(31)).unwrap_or(u32::MAX))
            .min(self.maximum);
        self.failures = self.failures.saturating_add(1);
        self.until = Some(now + delay);
        Some(delay)
    }
}
/// Pause after a head unit rejects the relay's Wi-Fi band, escalating from 60 s to 300 s. It
/// outlives child and supervisor restarts: share one between the supervisors a relay creates.
/// Only a successful Carlink join resets the escalation; Connect skips the current wait once.
#[derive(Clone)]
pub struct CarlinkBackoff(Arc<Mutex<BandBackoff>>);
impl Default for CarlinkBackoff {
    fn default() -> Self {
        Self::with_delays(Duration::from_secs(60), Duration::from_secs(300))
    }
}
impl CarlinkBackoff {
    pub fn with_delays(initial: Duration, maximum: Duration) -> Self {
        Self(Arc::new(Mutex::new(BandBackoff::new(initial, maximum))))
    }
    fn observe(&self, event: &Value, now: Instant) -> Option<Duration> {
        self.0.lock().unwrap().observe(event, now)
    }
    /// Time left in the current backoff; an expired one is cleared.
    fn remaining(&self, now: Instant) -> Option<Duration> {
        let mut backoff = self.0.lock().unwrap();
        let remaining = backoff
            .until
            .map(|until| until.saturating_duration_since(now))
            .filter(|left| !left.is_zero());
        if remaining.is_none() {
            backoff.until = None;
        }
        remaining
    }
    /// An explicit Connect may try at once; the next failure still escalates.
    fn allow_attempt(&self) {
        self.0.lock().unwrap().until = None;
    }
}
pub struct RelaySupervisor {
    settings: SupervisorSettings,
    factory: Arc<dyn ChildFactory>,
    events: EventSink,
    policy: RestartPolicy,
    stop: Arc<(Mutex<bool>, Condvar)>,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    pairing_fragment: Arc<Mutex<Option<String>>>,
    connect: Arc<AtomicBool>,
    restart: Arc<AtomicBool>,
    band_backoff: CarlinkBackoff,
    hevc_disabled: Arc<AtomicBool>,
    control: Arc<Mutex<Option<ControlPipe>>>,
    tracing: Arc<AtomicBool>,
}
impl RelaySupervisor {
    pub fn new(
        settings: SupervisorSettings,
        factory: Arc<dyn ChildFactory>,
        events: EventSink,
    ) -> Self {
        Self {
            settings,
            factory,
            events,
            policy: RestartPolicy::default(),
            stop: Arc::new((Mutex::new(false), Condvar::new())),
            running: Arc::new(AtomicBool::new(false)),
            worker: None,
            pairing_fragment: Arc::new(Mutex::new(None)),
            connect: Arc::new(AtomicBool::new(false)),
            restart: Arc::new(AtomicBool::new(false)),
            band_backoff: CarlinkBackoff::default(),
            hevc_disabled: Arc::new(AtomicBool::new(false)),
            control: Arc::default(),
            tracing: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Shares the band backoff with earlier and later supervisors of the same relay.
    pub fn with_carlink_backoff(mut self, backoff: CarlinkBackoff) -> Self {
        self.band_backoff = backoff;
        self
    }
    /// Keeps the codec fallback across supervisor replacements within one service run.
    pub fn with_hevc_disabled(mut self, disabled: Arc<AtomicBool>) -> Self {
        self.hevc_disabled = disabled;
        self
    }
    pub fn set_restart_policy(&mut self, policy: RestartPolicy) {
        self.policy = policy;
    }
    pub fn receiver_urls(&self, host: &str) -> Vec<String> {
        let mut urls = self.settings.receiver_urls(host);
        if let Some(fragment) = self.pairing_fragment.lock().unwrap().as_ref() {
            for url in &mut urls {
                url.push('#');
                url.push_str(fragment);
            }
        }
        urls
    }
    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
    pub fn trace_running(&self) -> bool {
        self.tracing.load(Ordering::Acquire)
    }
    pub fn send_control(&self, value: &Value) -> io::Result<()> {
        let control = self
            .control
            .try_lock()
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        control
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
            .send_control(value)
    }
    /// Serialize offline clearing with child startup, without waiting for the lifecycle worker.
    pub fn clear_trace(&self, when_stopped: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
        let control = self
            .control
            .try_lock()
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        if self.trace_running() {
            let pipe = control
                .as_ref()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?;
            pipe.send_control(&json!({"type":"clink-capture","action":"stop"}))?;
            pipe.send_control(&json!({"type":"clink-trace","action":"clear"}))
        } else {
            when_stopped()
        }
    }
    pub fn connect_carlink(&self) {
        // Restart the child with Carlink arguments for one explicit attempt, also during a band
        // backoff. Lift the backoff first: the worker reads it after taking `connect`.
        self.band_backoff.allow_attempt();
        let stop = self.stop.0.lock().unwrap();
        self.connect.store(true, Ordering::Release);
        self.restart.store(true, Ordering::Release);
        drop(stop);
        self.stop.1.notify_all();
    }
    pub fn start(&mut self) -> io::Result<()> {
        if self.worker.is_some() || (self.settings.web_port == 0 && self.settings.carlink.is_none())
        {
            return Ok(());
        }
        *self.stop.0.lock().unwrap() = false;
        let (settings, factory, events, policy, stop, running) = (
            self.settings.clone(),
            self.factory.clone(),
            self.events.clone(),
            self.policy,
            self.stop.clone(),
            self.running.clone(),
        );
        let control = self.control.clone();
        let tracing = self.tracing.clone();
        let connect = self.connect.clone();
        let restart = self.restart.clone();
        let fragment = self.pairing_fragment.clone();
        let linking = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let band_backoff = self.band_backoff.clone();
        let hevc_disabled = self.hevc_disabled.clone();
        let child_events: EventSink = Arc::new({
            let events = events.clone();
            let fragment = fragment.clone();
            let restart = restart.clone();
            let state_dir = settings.state_dir.clone();
            let paused = paused.clone();
            let auto_reconnect = settings.carlink.as_ref().is_none_or(|c| c.auto_reconnect);
            let linking = linking.clone();
            let band_backoff = band_backoff.clone();
            let hevc_disabled = hevc_disabled.clone();
            let hevc_auto = settings.carlink.as_ref().is_some_and(|c| c.hevc == "auto");
            move |event| {
                if event["type"] == "diagnostic" {
                    if let Some(message) = event["message"].as_str() {
                        if message.starts_with("codec mismatch expected=H265")
                            && hevc_auto
                            && !hevc_disabled.swap(true, Ordering::AcqRel)
                        {
                            eprintln!("oxp-relay: H.265 codec mismatch; restarting with Carlink H.265 off until settings change or service restart");
                            restart.store(true, Ordering::Release);
                        }
                        if message.starts_with("oxp-relay:") {
                            eprintln!("{message}");
                        } else {
                            eprintln!("oxp-relay: {message}");
                        }
                    }
                }
                if !auto_reconnect
                    && linking.load(Ordering::Acquire)
                    && carlink_attempt_failed(&event)
                {
                    paused.store(true, Ordering::Release);
                    restart.store(true, Ordering::Release);
                }
                if event["type"] == "pairing-url" {
                    *fragment.lock().unwrap() = event["url"]
                        .as_str()
                        .and_then(|url| url.split_once('#').map(|(_, value)| value.to_owned()));
                }
                if linking.load(Ordering::Acquire)
                    && carlink_attempt_failed(&event)
                    && !has_trusted_carlink(&state_dir)
                {
                    restart.store(true, Ordering::Release);
                }
                let delay = if linking.load(Ordering::Acquire) {
                    band_backoff.observe(&event, Instant::now())
                } else {
                    None
                };
                let reason = event["reason"]
                    .as_str()
                    .or_else(|| event["message"].as_str())
                    .map(str::to_owned);
                events(event);
                if let Some(delay) = delay {
                    // For display only: the backoff itself runs on the monotonic clock.
                    let next = crate::platform::wall_clock()
                        .as_secs()
                        .saturating_add(delay.as_secs());
                    events(json!({"type":"carlink-backoff", "reason":reason,
                        "delayMillis":delay.as_millis() as u64, "nextAttempt":next}));
                    // Stop internal link retries as well as supervisor respawns.
                    restart.store(true, Ordering::Release);
                }
            }
        });
        self.worker = Some(thread::Builder::new()
            .name("receiver-supervisor".into()).stack_size(256 * 1024)
            .spawn(move || {
                let wait = |duration| {
                    let (lock, wake) = &*stop;
                    let state = lock.lock().unwrap();
                    *wake.wait_timeout_while(state, duration, |stopped| !*stopped && !connect.load(Ordering::Acquire)).unwrap().0
                };
                let mut failures = 0;
                let mut unpaired_reported = false;
                while !*stop.0.lock().unwrap() {
                    *fragment.lock().unwrap() = None;
                    let attempt = connect.swap(false, Ordering::AcqRel);
                    if attempt { paused.store(false, Ordering::Release); }
                    let manual_pause = paused.load(Ordering::Acquire);
                    // Connect lifted the backoff for this one attempt.
                    let backoff = band_backoff
                        .remaining(Instant::now())
                        .filter(|_| !attempt && settings.carlink.is_some());
                    if let Some(remaining) = backoff.filter(|_| settings.web_port == 0 && !manual_pause) {
                        // Carlink only: nothing to serve until the backoff ends or Connect.
                        let (lock, wake) = &*stop;
                        let state = lock.lock().unwrap();
                        if *wake.wait_timeout_while(state, remaining, |stopped| !*stopped && !connect.load(Ordering::Acquire)).unwrap().0 { break; }
                        continue;
                    }
                    // With the web receiver in the same child, keep it up without Carlink.
                    let backing_off = backoff.is_some() || manual_pause;
                    let enabled = settings.carlink.is_some() && !backing_off && (attempt || has_trusted_carlink(&settings.state_dir));
                    linking.store(enabled, Ordering::Release);
                    if settings.carlink.is_some() && !enabled && (!backing_off || manual_pause) {
                        if !unpaired_reported {
                            events(json!({"type":if manual_pause { "carlink-paused" } else { "carlink-unpaired" }}));
                            unpaired_reported = true;
                        }
                        if settings.web_port == 0 {
                            let (lock, wake) = &*stop;
                            let state = lock.lock().unwrap();
                            let _state = wake.wait_while(state, |stopped| !*stopped && !connect.load(Ordering::Acquire)).unwrap();
                            continue;
                        }
                    } else if !backing_off {
                        unpaired_reported = false;
                    }
                    let started = Instant::now();
                    remove_stale_socket(&settings.socket);
                    restart.store(false, Ordering::Release);
                    let mut launch = settings.clone();
                    if hevc_disabled.load(Ordering::Acquire) {
                        if let Some(carlink) = launch.carlink.as_mut() {
                            carlink.hevc = "off".into();
                        }
                    }
                    let arguments = if backing_off {
                        SupervisorSettings { carlink: None, ..launch }.arguments()
                    } else {
                        launch.launch_arguments(attempt)
                    };
                    {
                        let _guard = control.lock().unwrap();
                        tracing.store(arguments.iter().any(|a| a == "--control-stdin"), Ordering::Release);
                    }
                    // Reset session observations before the child can emit its first event.
                    events(json!({"type":"receiver-starting"}));
                    match factory.spawn(&settings.binary, &arguments, child_events.clone()) {
                        Ok(mut child) => {
                            let pipe = child.control();
                            {
                                let mut available = control.lock().unwrap();
                                tracing.store(pipe.is_some(), Ordering::Release);
                                *available = pipe;
                            }
                            running.store(true, Ordering::Relaxed);
                            events(json!({"type": "receiver-started"}));
                            loop {
                                if wait(policy.poll) || restart.load(Ordering::Acquire) { break; }
                                // The web-only child makes way for Carlink when the backoff ends.
                                if backing_off && !manual_pause && band_backoff.remaining(Instant::now()).is_none() { break; }
                                match child.exited() {
                                    Ok(false) => {},
                                    Ok(true) => {
                                        if enabled && settings.carlink.as_ref().is_some_and(|c| !c.auto_reconnect) {
                                            paused.store(true, Ordering::Release);
                                        }
                                        break;
                                    },
                                    Err(e) => {
                                        events(json!({"type": "receiver-error", "message": e.to_string()}));
                                        break;
                                    }
                                }
                            }
                            *control.lock().unwrap() = None;
                            running.store(false, Ordering::Relaxed);
                            if let Err(e) = child.stop() {
                                events(json!({"type": "receiver-error", "message": e.to_string()}));
                            }
                            tracing.store(false, Ordering::Release);
                            events(json!({"type":"receiver-stopped"}));
                        }
                        Err(e) => {
                            tracing.store(false, Ordering::Release);
                            events(json!({"type": "receiver-error", "message": e.to_string()}));
                        },
                    }
                    *fragment.lock().unwrap() = None;
                    if *stop.0.lock().unwrap() { break; }
                    if started.elapsed() >= policy.stable_after { failures = 0; }
                    // Entering or leaving a backoff swaps the child's arguments at once.
                    let switched = backing_off != band_backoff.remaining(Instant::now()).is_some();
                    let delay = if switched || connect.load(Ordering::Acquire) { Duration::ZERO } else { restart_delay(policy, failures) };
                    failures = failures.saturating_add(1);
                    events(json!({"type": "receiver-restarting", "delayMillis": delay.as_millis() as u64}));
                    if wait(delay) { break; }
                }
                running.store(false, Ordering::Relaxed);
            })?);
        Ok(())
    }
    pub fn stop(&mut self) {
        *self.stop.0.lock().unwrap() = true;
        self.stop.1.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for RelaySupervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A socket file left by a relay that died (or a previous service run) makes the next bind fail
/// with "Address in use"; remove it when nothing accepts connections on it.
fn remove_stale_socket(path: &Path) {
    use std::os::unix::fs::FileTypeExt;
    let is_socket = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_socket());
    if is_socket && std::os::unix::net::UnixStream::connect(path).is_err() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod band_tests {
    use super::*;
    use std::sync::mpsc::{channel, Receiver, Sender};

    #[test]
    fn missing_carlink_without_web_output_starts_no_child() {
        let mut f = Fixture::new("missing-interface", 0);
        f.settings.carlink = None;
        let mut relay = f.supervisor(&CarlinkBackoff::default());
        relay.start().unwrap();
        relay.connect_carlink();
        relay.start().unwrap();
        assert!(f.spawns.try_recv().is_err());
        assert!(!relay.running());
        relay.stop();
    }
    #[test]
    fn band_backoff_sequence_duplicate_failure_and_success_reset() {
        let mut backoff = BandBackoff::new(Duration::from_secs(60), Duration::from_secs(300));
        let mut now = Instant::now();
        let failure =
            json!({"type":"link","state":"lost","reason":"head_unit_band_unsupported:5180"});
        for seconds in [60, 120, 240, 300, 300] {
            assert_eq!(
                backoff.observe(&failure, now),
                Some(Duration::from_secs(seconds))
            );
            assert_eq!(backoff.until, Some(now + Duration::from_secs(seconds)));
            assert!(backoff
                .observe(
                    &json!({"type":"error","message":"head_unit_band_unsupported:5180"}),
                    now
                )
                .is_none());
            now += Duration::from_secs(seconds);
        }
        backoff.observe(&json!({"type":"link","state":"joined"}), now);
        assert_eq!(backoff.failures, 0);
        assert_eq!(
            backoff.observe(&failure, now),
            Some(Duration::from_secs(60))
        );
    }

    struct Child(Sender<()>);
    impl RelayChild for Child {
        fn exited(&mut self) -> io::Result<bool> {
            Ok(false)
        }
        fn stop(&mut self) -> io::Result<()> {
            let _ = self.0.send(());
            Ok(())
        }
    }
    struct Factory(Sender<(Vec<OsString>, EventSink)>, Sender<()>);
    impl ChildFactory for Factory {
        fn spawn(
            &self,
            _: &Path,
            args: &[OsString],
            events: EventSink,
        ) -> io::Result<Box<dyn RelayChild>> {
            self.0.send((args.to_vec(), events)).unwrap();
            Ok(Box::new(Child(self.1.clone())))
        }
    }
    struct Fixture {
        dir: PathBuf,
        settings: SupervisorSettings,
        spawns: Receiver<(Vec<OsString>, EventSink)>,
        stops: Receiver<()>,
        events: Receiver<Value>,
        factory: Arc<Factory>,
        sink: EventSink,
    }
    impl Fixture {
        fn new(name: &str, web_port: u16) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "band-backoff-{name}-{}-{:?}",
                std::process::id(),
                thread::current().id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let settings = SupervisorSettings {
                binary: "fake".into(),
                socket: dir.join("socket"),
                web_port,
                web_root: dir.clone(),
                plugins: None,
                state_dir: dir.clone(),
                addresses: vec![],
                pairing: false,
                pem_tls: None,
                carlink: Some(CarlinkSettings {
                    ble: None,
                    head_unit: Ipv4Addr::LOCALHOST,
                    local: Ipv4Addr::LOCALHOST,
                    pin: None,
                    name: crate::branding::DEVICE_NAME.into(),
                    model: crate::branding::MODEL.into(),
                    hevc: "auto".into(),
                    trace: false,
                    auto_reconnect: true,
                    ports: BTreeMap::new(),
                }),
            };
            let (spawn_tx, spawns) = channel();
            let (stop_tx, stops) = channel();
            let (event_tx, events) = channel();
            let event_tx = Mutex::new(event_tx);
            Self {
                dir,
                settings,
                spawns,
                stops,
                events,
                factory: Arc::new(Factory(spawn_tx, stop_tx)),
                sink: Arc::new(move |event| {
                    let _ = event_tx.lock().unwrap().send(event);
                }),
            }
        }
        fn supervisor(&self, backoff: &CarlinkBackoff) -> RelaySupervisor {
            let mut relay = RelaySupervisor::new(
                self.settings.clone(),
                self.factory.clone(),
                self.sink.clone(),
            )
            .with_carlink_backoff(backoff.clone());
            relay.set_restart_policy(RestartPolicy {
                poll: Duration::from_millis(1),
                initial: Duration::ZERO,
                ..RestartPolicy::default()
            });
            relay
        }
        /// The next child: whether it links Carlink, and its event sink.
        fn spawned(&self) -> (bool, bool, EventSink) {
            let (args, sink) = self.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
            let has = |flag: &str| args.iter().any(|arg| arg == flag);
            (has("--clink-head-unit"), has("--web-port"), sink)
        }
        fn backoff_delays(&self) -> Vec<u64> {
            self.events
                .try_iter()
                .filter(|v| v["type"] == "carlink-backoff")
                .map(|v| v["delayMillis"].as_u64().unwrap())
                .collect()
        }
        fn trust(&self) {
            std::fs::create_dir_all(self.dir.join("carlink")).unwrap();
            std::fs::write(
                self.dir.join("carlink/trust.json"),
                serde_json::to_vec(&json!({"peers":{"unit":format!("{}==", "A".repeat(86))}}))
                    .unwrap(),
            )
            .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn band_failure() -> Value {
        json!({"type":"link","state":"lost","reason":"head_unit_band_unsupported:5180"})
    }

    #[test]
    fn codec_mismatch_disables_hevc_once_until_settings_rebuild() {
        let mut f = Fixture::new("hevc-mismatch", 0);
        f.trust();
        let backoff = CarlinkBackoff::default();
        let mut relay = f.supervisor(&backoff);
        relay.start().unwrap();
        let (args, sink) = f.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
        let has_hevc = |args: &[OsString], value: &str| {
            args.windows(2)
                .any(|a| a[0] == "--clink-hevc" && a[1] == value)
        };
        assert!(has_hevc(&args, "auto"));
        sink(json!({"type":"diagnostic","message":"unrelated diagnostic"}));
        assert!(f.spawns.recv_timeout(Duration::from_millis(30)).is_err());
        let mismatch =
            json!({"type":"diagnostic","message":"codec mismatch expected=H265 actual=Some(H264)"});
        sink(mismatch.clone());
        let (args, sink) = f.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(has_hevc(&args, "off"));
        sink(mismatch.clone());
        assert!(f.spawns.recv_timeout(Duration::from_millis(30)).is_err());
        relay.connect_carlink();
        let (args, _) = f.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(has_hevc(&args, "off"));
        relay.stop();
        // Settings changes replace the supervisor and re-evaluate the configured HEVC policy.
        f.settings.carlink.as_mut().unwrap().name = "Changed name".into();
        let mut relay = f.supervisor(&backoff);
        relay.start().unwrap();
        let (args, sink) = f.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(has_hevc(&args, "auto"));
        sink(mismatch);
        let (args, _) = f.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(has_hevc(&args, "off"));
        relay.stop();
    }

    #[test]
    fn carlink_only_band_failure_blocks_respawn_until_connect_and_survives_restarts() {
        let f = Fixture::new("carlink-only", 0);
        let backoff = CarlinkBackoff::default();
        let mut relay = f.supervisor(&backoff);
        relay.connect_carlink();
        relay.start().unwrap();
        let (carlink, _, sink) = f.spawned();
        assert!(carlink);
        sink(band_failure());
        f.stops.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(f.spawns.recv_timeout(Duration::from_millis(100)).is_err());
        let backoff_event = f
            .events
            .try_iter()
            .find(|v| v["type"] == "carlink-backoff")
            .unwrap();
        assert_eq!(backoff_event["delayMillis"], 60000);
        assert_eq!(backoff_event["reason"], "head_unit_band_unsupported:5180");
        assert!(backoff_event["nextAttempt"].as_u64().is_some());
        // Restarting the supervisor, or replacing it, keeps the backoff.
        relay.stop();
        relay.start().unwrap();
        assert!(f.spawns.recv_timeout(Duration::from_millis(100)).is_err());
        relay.stop();
        let mut relay = f.supervisor(&backoff);
        relay.start().unwrap();
        assert!(f.spawns.recv_timeout(Duration::from_millis(100)).is_err());
        // Connect tries at once, and the next failure escalates.
        relay.connect_carlink();
        let (carlink, _, sink) = f.spawned();
        assert!(carlink);
        sink(band_failure());
        f.stops.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(f.spawns.recv_timeout(Duration::from_millis(100)).is_err());
        assert_eq!(f.backoff_delays(), [120000]);
        relay.stop();
    }

    #[test]
    fn manual_reconnect_waits_after_session_end_and_connect_still_works() {
        for port in [0, 8443] {
            let mut f = Fixture::new("manual", port);
            f.settings.carlink.as_mut().unwrap().auto_reconnect = false;
            f.trust();
            let mut relay = f.supervisor(&CarlinkBackoff::default());
            relay.start().unwrap();
            let (carlink, _, sink) = f.spawned();
            assert!(carlink);
            sink(json!({"type":"state","state":"Ended"}));
            f.stops.recv_timeout(Duration::from_secs(2)).unwrap();
            if port != 0 {
                let (carlink, web, _) = f.spawned();
                assert!(!carlink && web);
            }
            assert!(f.spawns.recv_timeout(Duration::from_millis(100)).is_err());
            relay.connect_carlink();
            let (carlink, web, _) = f.spawned();
            assert!(carlink);
            assert_eq!(web, port != 0);
            relay.stop();
        }
    }

    #[test]
    fn both_outputs_keep_the_web_receiver_during_band_backoff() {
        let f = Fixture::new("both", 8443);
        f.trust();
        let backoff =
            CarlinkBackoff::with_delays(Duration::from_millis(400), Duration::from_secs(60));
        let mut relay = f.supervisor(&backoff);
        relay.start().unwrap();
        let (carlink, web, sink) = f.spawned();
        assert!(carlink && web);
        let failed = Instant::now();
        sink(band_failure());
        // The web receiver comes back at once without Carlink.
        let (carlink, web, _) = f.spawned();
        assert!(!carlink && web);
        assert!(failed.elapsed() < Duration::from_millis(350));
        assert_eq!(f.backoff_delays(), [400]);
        // When the backoff ends the child relinks Carlink.
        let (carlink, web, sink) = f.spawned();
        assert!(carlink && web);
        assert!(failed.elapsed() >= Duration::from_millis(400));
        sink(band_failure());
        let (carlink, _, _) = f.spawned();
        assert!(!carlink);
        assert_eq!(f.backoff_delays(), [800]);
        // A supervisor restart keeps the web-only child.
        relay.stop();
        relay.start().unwrap();
        let (carlink, web, _) = f.spawned();
        assert!(!carlink && web);
        // Connect relinks at once; a successful join resets the escalation.
        relay.connect_carlink();
        let (carlink, _, sink) = f.spawned();
        assert!(carlink);
        sink(json!({"type":"link","state":"joined"}));
        sink(band_failure());
        let (carlink, _, _) = f.spawned();
        assert!(!carlink);
        assert_eq!(f.backoff_delays(), [400]);
        relay.stop();
    }
}

#[cfg(test)]
mod tls_tests {
    use super::*;
    fn settings(dir: &Path) -> SupervisorSettings {
        SupervisorSettings {
            binary: "oxp-relay".into(),
            socket: "/tmp/oxp-relay.sock".into(),
            web_port: 8443,
            web_root: "/opt/openxplay/share/web".into(),
            plugins: None,
            state_dir: dir.into(),
            addresses: vec![Ipv4Addr::new(10, 77, 0, 1), Ipv4Addr::new(192, 168, 1, 2)],
            pairing: false,
            carlink: None,
            pem_tls: None,
        }
    }
    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }
    #[test]
    fn carlink_name_model_hevc_argument_lists_are_exact_for_both_links() {
        for ble in [false, true] {
            let mut settings = settings(Path::new("/state"));
            settings.web_port = 0;
            settings.carlink = Some(CarlinkSettings {
                name: "My 车".into(),
                model: "Model 1".into(),
                hevc: "off".into(),
                trace: false,
                auto_reconnect: false,
                ble: ble.then(|| BleCarlinkSettings {
                    adapter: None,
                    interface: "wlan0".into(),
                    address: None,
                    id: "id".into(),
                    band: 1,
                    phone_mac: None,
                }),
                head_unit: Ipv4Addr::new(192, 168, 43, 1),
                local: Ipv4Addr::new(192, 168, 43, 2),
                pin: None,
                ports: BTreeMap::new(),
            });
            let mut expected = vec![
                "--unix",
                "/tmp/oxp-relay.sock",
                "--host-ui",
                "--relay-name",
                crate::branding::DEVICE_NAME,
                "--relay-version",
                env!("CARGO_PKG_VERSION"),
                "--state-dir",
                "/state/carlink",
                "--clink-name",
                "My 车",
                "--clink-model",
                "Model 1",
                "--clink-hevc",
                "off",
                "--clink-state-dir",
                "/state",
            ];
            expected.extend(if ble {
                vec![
                    "--clink-scan",
                    "continuous",
                    "--clink-link",
                    "ble",
                    "--clink-interface",
                    "wlan0",
                    "--clink-id",
                    "id",
                    "--clink-band",
                    "1",
                    "--pin-file",
                    "/state/carlink/pin",
                ]
            } else {
                vec![
                    "--clink-head-unit",
                    "192.168.43.1",
                    "--clink-local",
                    "192.168.43.2",
                ]
            });
            assert_eq!(strings(settings.arguments()), expected);
            settings.carlink.as_mut().unwrap().trace = true;
            expected.splice(
                7..7,
                ["--clink-trace", "/state/carlink-trace", "--control-stdin"],
            );
            assert_eq!(strings(settings.arguments()), expected);
            settings.carlink = None;
            assert!(!strings(settings.arguments())
                .iter()
                .any(|arg| arg == "--control-stdin"));
        }
    }
    #[test]
    fn without_a_trusted_certificate_the_arguments_are_exactly_as_before() {
        let dir = std::env::temp_dir().join(format!(
            "supervisor-tls-{}",
            crate::admin::auth::random_hex(6).unwrap()
        ));
        let state = dir.to_string_lossy().into_owned();
        let legacy = [
            "--unix",
            "/tmp/oxp-relay.sock",
            "--web-port",
            "8443",
            "--web-root",
            "/opt/openxplay/share/web",
            "--web-bind",
            "0.0.0.0",
            "--tls-self-signed",
            &format!("{state}/receiver-tls"),
            "--tls-san",
            "openxplay-linux.local",
            "--host-ui",
            "--relay-name",
            crate::branding::DEVICE_NAME,
            "--relay-version",
            env!("CARGO_PKG_VERSION"),
            "--tls-san",
            "10.77.0.1",
            "--tls-san",
            "192.168.1.2",
        ];
        assert_eq!(strings(settings(&dir).arguments()), legacy);
        // Configured but missing files (removed, or never issued) change nothing either.
        let pem = PemTls {
            name: "myrelay.duckdns.org".into(),
            cert: dir.join("public-name/cert.pem"),
            key: dir.join("public-name/key.pem"),
        };
        let with = SupervisorSettings {
            pem_tls: Some(pem.clone()),
            ..settings(&dir)
        };
        assert_eq!(strings(with.arguments()), legacy);
        std::fs::create_dir_all(dir.join("public-name")).unwrap();
        std::fs::write(&pem.cert, "chain").unwrap();
        std::fs::write(&pem.key, "key").unwrap();
        let args = strings(with.arguments());
        let mut expected: Vec<String> = legacy.iter().map(|s| s.to_string()).collect();
        expected.splice(
            8..8,
            [
                "--tls-cert".into(),
                format!("{state}/public-name/cert.pem"),
                "--tls-key".into(),
                format!("{state}/public-name/key.pem"),
                "--tls-name".into(),
                "myrelay.duckdns.org".into(),
            ],
        );
        assert_eq!(args, expected);
        // Carlink-only children take no TLS options at all.
        let carlink = SupervisorSettings {
            web_port: 0,
            ..with
        };
        assert!(!strings(carlink.arguments())
            .iter()
            .any(|a| a.starts_with("--tls")));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod trace_tests {
    use super::*;
    #[test]
    fn pipe_is_only_created_with_trace_and_dead_or_stuck_children_never_block() {
        let factory = ProcessChildFactory;
        let sink: EventSink = Arc::new(|_| {});
        let mut off = factory
            .spawn(Path::new("/bin/cat"), &[], sink.clone())
            .unwrap();
        assert!(off.control().is_none());
        off.stop().unwrap();
        // sh ignores the flag and sleeps without reading stdin.
        let args = ["-c", "exec sleep 30", "--control-stdin"].map(OsString::from);
        let mut stuck = factory
            .spawn(Path::new("/bin/sh"), &args, sink.clone())
            .unwrap();
        let pipe = stuck.control().unwrap();
        let start = Instant::now();
        let value = json!({"type":"clink-capture","action":"start"});
        let mut full = false;
        for _ in 0..100_000 {
            if let Err(e) = pipe.send_control(&value) {
                assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
                full = true;
                break;
            }
        }
        assert!(full);
        assert!(start.elapsed() < Duration::from_secs(2));
        stuck.stop().unwrap();
        let start = Instant::now();
        assert_eq!(
            pipe.send_control(&value).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        let mut echo = factory
            .spawn(
                Path::new("/bin/sh"),
                &[
                    "-c".into(),
                    "read line; printf '%s\\n' \"$line\"".into(),
                    "--control-stdin".into(),
                ],
                sink,
            )
            .unwrap();
        echo.control().unwrap().send_control(&value).unwrap();
        echo.stop().unwrap();
    }
}
