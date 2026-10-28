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
#[path = "carlink_lifecycle.rs"]
mod carlink_lifecycle;
use carlink_lifecycle::Lifecycle;
pub type EventSink = Arc<dyn Fn(Value) + Send + Sync>;
#[path = "receiver_tls_control.rs"]
mod receiver_tls_control;
use receiver_tls_control::WebTlsControl;
#[cfg(test)]
#[path = "receiver_tls_tests.rs"]
pub(crate) mod receiver_tls_tests;
#[derive(Clone, Debug)]
pub struct CarlinkSettings {
    pub name: String,
    pub model: String,
    pub hevc: String,
    pub trace: bool,
    pub vehicle_data: bool,
    pub picture: Option<crate::display::Size>,
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
    /// The selected local adapter address when the saved override is empty.
    pub default_phone_mac: Option<String>,
}
impl BleCarlinkSettings {
    /// Resolve a saved experiment snapshot only when starting a new Carlink connection.
    pub fn phone_mac_for_connection(&self, settings: &crate::settings::Settings) -> Option<String> {
        if !settings.experimental_active || !settings.experimental_send_bluetooth_address {
            return None;
        }
        if settings.experimental_bluetooth_address.is_empty() {
            return self.default_phone_mac.clone();
        }
        crate::carlink_policy::bluetooth_address(&settings.experimental_bluetooth_address)
    }
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
    pub web_transcode: bool,
    pub web_root: PathBuf,
    pub plugins: Option<crate::plugins::OutputPlugins>,
    pub state_dir: PathBuf,
    pub addresses: Vec<Ipv4Addr>,
    pub pairing: bool,
    pub carlink: Option<CarlinkSettings>,
    /// Serve this certificate for its SNI name while both files exist.
    pub pem_tls: Option<PemTls>,
    pub default_tls: Option<crate::identity::tls::DefaultTls>,
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
                "--control-stdin".into(),
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
            if let Some(base) = &self.default_tls {
                args.extend([
                    "--tls-default-cert".into(),
                    base.cert.as_os_str().into(),
                    "--tls-default-key".into(),
                    base.key.as_os_str().into(),
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
        if !args.iter().any(|a| a == "--control-stdin") {
            args.push("--control-stdin".into());
        }
        if self.web_port != 0 {
            args.extend([
                "--web-transcode".into(),
                if self.web_transcode { "auto" } else { "off" }.into(),
            ]);
        }
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
                "--clink-vehicle-data".into(),
                if c.vehicle_data { "on" } else { "off" }.into(),
            ]);
            args.extend([
                "--clink-state-dir".into(),
                self.state_dir.as_os_str().into(),
            ]);
            if let Some(ble) = &c.ble {
                args.extend(["--clink-scan".into(), "continuous".into()]);
                let phone_mac = self.phone_mac_for_connection();
                if let Some(mac) = &phone_mac {
                    args.extend(["--clink-phone-mac".into(), mac.into()]);
                }
                args.extend([
                    "--clink-identity".into(),
                    "ble".into(),
                    "--clink-link".into(),
                    "ble".into(),
                    "--clink-interface".into(),
                    ble.interface.clone().into(),
                    "--clink-id".into(),
                    ble.id.clone().into(),
                    "--clink-band".into(),
                    ble.band.to_string().into(),
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
    fn phone_mac_for_connection(&self) -> Option<String> {
        let ble = self.carlink.as_ref()?.ble.as_ref()?;
        let saved = std::fs::read(self.state_dir.join("settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<crate::settings::Settings>(&bytes).ok());
        saved
            .as_ref()
            .map(|settings| ble.phone_mac_for_connection(settings))
            .unwrap_or_else(|| ble.phone_mac.clone())
    }
    /// Each retained-child replacement captures the next connection's experiment snapshot.
    /// The child must treat phoneMac:null as clearing the old override, never as omission.
    fn connection_options(&self, value: &mut Value) {
        if self.carlink.as_ref().is_some_and(|c| c.ble.is_some()) {
            value["phoneMac"] = json!(self.phone_mac_for_connection());
        }
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
pub fn has_trusted_carlink_for(state: &Path, ble: bool) -> bool {
    trusted_carlink(state, Some(ble))
}
pub fn has_trusted_carlink(state: &Path) -> bool {
    trusted_carlink(state, None)
}
fn trusted_carlink(state: &Path, ble: Option<bool>) -> bool {
    std::fs::read(state.join("carlink/trust.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["peers"].as_object().cloned())
        .is_some_and(|peers| {
            peers.iter().any(|(id, key)| {
                if let Some(ble) = ble {
                    let is_ble = serde_json::from_str::<Vec<String>>(id)
                        .ok()
                        .is_some_and(|parts| parts.len() == 2 && parts[0] == "ble");
                    if ble != is_ble {
                        return false;
                    }
                }
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
        let _ = attempt;
        self.arguments()
    }
}
/// One nonblocking write, never waiting for a reader or another request.
#[derive(Clone)]
pub struct ControlPipe(Arc<Mutex<Option<ControlWriter>>>);
#[cfg(test)]
type FakeControlSink = dyn Fn(&Value) -> io::Result<()> + Send + Sync;
enum ControlWriter {
    Process(ChildStdin),
    #[cfg(test)]
    Fake(Arc<FakeControlSink>),
}
impl ControlPipe {
    fn new(stdin: ChildStdin) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let fd = stdin.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(Arc::new(Mutex::new(Some(ControlWriter::Process(
            stdin,
        ))))))
    }
    #[cfg(test)]
    pub(crate) fn fake(send: impl Fn(&Value) -> io::Result<()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(Mutex::new(Some(ControlWriter::Fake(Arc::new(
            send,
        ))))))
    }
    pub fn send_control(&self, value: &Value) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        let mut bytes = serde_json::to_vec(value)?;
        if bytes.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "control too long",
            ));
        }
        bytes.push(b'\n');
        let mut pipe = self
            .0
            .try_lock()
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        let written = match pipe.as_ref().ok_or(io::ErrorKind::BrokenPipe)? {
            ControlWriter::Process(stdin) => unsafe {
                libc::write(stdin.as_raw_fd(), bytes.as_ptr().cast(), bytes.len())
            },
            #[cfg(test)]
            ControlWriter::Fake(send) => return send(value),
        };
        if written < 0 {
            return Err(io::Error::last_os_error());
        }
        if written as usize != bytes.len() {
            // Larger W2 requests may exceed PIPE_BUF on non-Linux hosts. Close on a partial
            // write so a following capture/clear command cannot complete malformed JSON.
            *pipe = None;
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
    pub poll: Duration,
}
impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            maximum: Duration::from_secs(15),
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
        if event["type"] == "state"
            && event["state"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("playing"))
        {
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
/// Pause after a head unit rejects the relay's Wi-Fi band, using the 1–15 s retry cap. It
/// outlives child and supervisor restarts: share one between the supervisors a relay creates.
/// Only current-generation Playing resets the escalation; Connect skips the current wait once.
#[derive(Clone)]
pub struct CarlinkBackoff(Arc<Mutex<BandBackoff>>);
impl Default for CarlinkBackoff {
    fn default() -> Self {
        Self::with_delays(Duration::from_secs(1), Duration::from_secs(15))
    }
}
impl CarlinkBackoff {
    pub fn with_delays(initial: Duration, maximum: Duration) -> Self {
        Self(Arc::new(Mutex::new(BandBackoff::new(initial, maximum))))
    }
    fn observe(&self, event: &Value, now: Instant) -> Option<Duration> {
        self.0.lock().unwrap().observe(event, now)
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
    band_backoff: CarlinkBackoff,
    hevc_disabled: Arc<AtomicBool>,
    control: Arc<Mutex<Option<ControlPipe>>>,
    tracing: Arc<AtomicBool>,
    web_tls: Arc<WebTlsControl>,
    carlink_lifecycle: Arc<Mutex<Lifecycle>>,
    carlink_options: Arc<Mutex<Value>>,
}
impl RelaySupervisor {
    pub fn new(
        settings: SupervisorSettings,
        factory: Arc<dyn ChildFactory>,
        events: EventSink,
    ) -> Self {
        Self {
            web_tls: Arc::new(WebTlsControl::new(&settings)),
            carlink_options: Arc::new(Mutex::new(settings.carlink.as_ref().map_or(
                Value::Null,
                |c| json!({"picture":c.picture,"hevc":c.hevc,"vehicleData":c.vehicle_data}),
            ))),
            carlink_lifecycle: Arc::new(Mutex::new(Lifecycle::new(
                settings.carlink.as_ref().is_some_and(|c| c.auto_reconnect),
            ))),
            settings,
            factory,
            events,
            policy: RestartPolicy::default(),
            stop: Arc::new((Mutex::new(false), Condvar::new())),
            running: Arc::new(AtomicBool::new(false)),
            worker: None,
            pairing_fragment: Arc::new(Mutex::new(None)),
            connect: Arc::new(AtomicBool::new(false)),
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
        let mut settings = self.settings.clone();
        settings.addresses = self.web_tls.settings().sans;
        let mut urls = settings.receiver_urls(host);
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
    pub fn with_control_handle(mut self, control: Arc<Mutex<Option<ControlPipe>>>) -> Self {
        self.control = control;
        self
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
    /// Wait only on the action worker. Reader callbacks acknowledge W2's matching request;
    /// subsequent automatic child retries use only the last acknowledged complete set.
    pub fn update_web_tls(
        &self,
        pem: Option<PemTls>,
        sans: Vec<Ipv4Addr>,
        timeout: Duration,
    ) -> io::Result<()> {
        self.update_web_tls_with_default(pem, sans, self.web_tls.settings().default_tls, timeout)
    }
    pub fn update_web_tls_with_default(
        &self,
        pem: Option<PemTls>,
        sans: Vec<Ipv4Addr>,
        default_tls: Option<crate::identity::tls::DefaultTls>,
        timeout: Duration,
    ) -> io::Result<()> {
        let control = self.control.lock().unwrap();
        let pipe = control.as_ref().ok_or(io::ErrorKind::NotConnected)?;
        let id = self.web_tls.send(
            pipe,
            receiver_tls_control::Settings {
                pem,
                sans,
                default_tls,
            },
        )?;
        drop(control);
        self.web_tls.wait(&id, timeout)
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
        self.band_backoff.allow_attempt();
        let options = self.carlink_options.lock().unwrap().clone();
        if self.control.lock().unwrap().is_some() {
            let picture = serde_json::from_value(options["picture"].clone()).ok();
            let _ = self.restart_carlink(
                picture,
                if self.hevc_disabled.load(Ordering::Acquire) {
                    "off"
                } else {
                    options["hevc"].as_str().unwrap_or("off")
                },
                options["vehicleData"] == true,
            );
        } else {
            self.connect.store(true, Ordering::Release);
            (self.events)(json!({"type":"carlink-starting"}));
            self.stop.1.notify_all();
        }
    }
    /// Replaces the output only. The web receiver and media wire remain connected.
    pub fn restart_carlink(
        &self,
        picture: Option<crate::display::Size>,
        hevc: &str,
        vehicle: bool,
    ) -> io::Result<()> {
        let mut lifecycle = self.carlink_lifecycle.lock().unwrap();
        let request = lifecycle.replacement();
        *self.carlink_options.lock().unwrap() =
            json!({"picture":picture,"hevc":hevc,"vehicleData":vehicle});
        let mut value = json!({"type":"clink-restart","request":request,"picture":picture,"hevc":hevc,"vehicleData":vehicle});
        self.settings.connection_options(&mut value);
        let result = self.send_control(&value);
        if result.is_err() {
            lifecycle.control_failed(Instant::now());
        }
        (self.events)(json!({"type":"carlink-starting"}));
        drop(lifecycle);
        result
    }
    pub fn pairing(&self, request: u64, action: &str, pin: Option<&str>) -> io::Result<()> {
        if action == "cancel" {
            self.carlink_lifecycle.lock().unwrap().cancel();
        }
        self.send_control(
            &json!({"type":"clink-pairing","request":request,"action":action,"pin":pin}),
        )
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
        let web_tls = self.web_tls.clone();
        let tracing = self.tracing.clone();
        let connect = self.connect.clone();
        let fragment = self.pairing_fragment.clone();
        let lifecycle = self.carlink_lifecycle.clone();
        let carlink_options = self.carlink_options.clone();
        let band = self.band_backoff.clone();
        let hevc_disabled = self.hevc_disabled.clone();
        self.worker = Some(thread::Builder::new().name("receiver-supervisor".into()).stack_size(256 * 1024).spawn(move || {
            let wait = |duration| {
                let state = stop.0.lock().unwrap();
                *stop.1.wait_timeout_while(state, duration, |stopped| !*stopped && !connect.load(Ordering::Acquire)).unwrap().0
            };
            let mut failures = 0;
            while !*stop.0.lock().unwrap() {
                *fragment.lock().unwrap() = None;
                // A fresh child must prove its encoder before using HEVC beside the browser.
                if settings.web_port != 0 && settings.carlink.is_some() {
                    carlink_options.lock().unwrap()["hevc"] = "off".into();
                }
                let requested = carlink_options.lock().unwrap().clone();
                let initial_picture = requested["picture"].is_object();
                lifecycle.lock().unwrap().begin_child(initial_picture);
                remove_stale_socket(&settings.socket);
                let (process_generation, tls) = web_tls.begin_child();
                let mut launch = settings.clone();
                launch.pem_tls = tls.pem;
                launch.default_tls = tls.default_tls;
                launch.addresses = tls.sans;
                if let Some(c) = launch.carlink.as_mut() {
                    c.hevc=requested["hevc"].as_str().unwrap_or("off").into();
                    c.vehicle_data=requested["vehicleData"] == true;
                    c.picture=serde_json::from_value(requested["picture"].clone()).ok();
                    if hevc_disabled.load(Ordering::Acquire) { c.hevc="off".into(); }
                }
                let arguments = launch.arguments();
                events(json!({"type":"receiver-starting"}));
                // A retired process may still drain buffered stdout. Fence every callback.
                let active = Arc::new(Mutex::new(true));
                let child_events: EventSink = Arc::new({
                    let (active, lifecycle, events, fragment, web_tls, band, hevc_disabled) = (active.clone(), lifecycle.clone(), events.clone(), fragment.clone(), web_tls.clone(), band.clone(), hevc_disabled.clone());
                    let hevc_auto = settings.carlink.as_ref().is_some_and(|c| c.hevc == "auto");
                    move |mut event| {
                        let live = active.lock().unwrap();
                        if !*live { return; }
                        web_tls.observe(process_generation, &event);
                        let mut life = lifecycle.lock().unwrap();
                        if !life.accept(&event) { return; }
                        life.observe(&event, Instant::now());
                        if event.get("generation").is_none() && matches!(event["type"].as_str(), Some("state" | "error" | "diagnostic" | "clink-view-size" | "media-stats" | "clink-pin" | "pin-timeout")) { event["generation"] = json!(life.generation()); }
                        if event["type"] == "diagnostic" && hevc_auto && event["message"].as_str().is_some_and(|m| m.starts_with("codec mismatch expected=H265")) && !hevc_disabled.swap(true, Ordering::AcqRel) {
                            life.retry_now();
                        }
                        let delay = band.observe(&event, Instant::now());
                        if let Some(delay) = delay { life.defer(Instant::now() + delay); }
                        drop(life);
                        if event["type"] == "pairing-url" {
                            *fragment.lock().unwrap() = event["url"].as_str().and_then(|url| url.split_once('#').map(|(_, v)| v.to_owned()));
                        }
                        events(event);
                        if let Some(delay) = delay {
                            events(json!({"type":"carlink-backoff","delayMillis":delay.as_millis() as u64,"nextAttempt":crate::platform::wall_clock().as_secs().saturating_add(delay.as_secs())}));
                        }
                    }
                });
                match factory.spawn(&settings.binary, &arguments, child_events) {
                    Ok(mut child) => {
                        { let mut pipe = control.lock().unwrap(); *pipe = child.control(); }
                        tracing.store(control.lock().unwrap().is_some() && arguments.iter().any(|a| a == "--clink-trace"), Ordering::Release);
                        running.store(true, Ordering::Release);
                        events(json!({"type":"receiver-started"}));
                        if initial_picture { connect.store(true, Ordering::Release); }
                        loop {
                            if wait(policy.poll) { break; }
                            if child.exited().unwrap_or(true) { break; }
                            let manual = connect.swap(false, Ordering::AcqRel);
                            let mut life = lifecycle.lock().unwrap();
                            if settings.carlink.is_some() && (manual || life.due(Instant::now())) {
                                let request = life.replacement();
                                let mut value = carlink_options.lock().unwrap().clone();
                                value["type"] = json!("clink-restart"); value["request"] = json!(request);
                                settings.connection_options(&mut value);
                                if hevc_disabled.load(Ordering::Acquire) { value["hevc"] = json!("off"); }
                                let result = control.lock().unwrap().as_ref().ok_or(io::ErrorKind::NotConnected).and_then(|p| p.send_control(&value).map_err(|e| e.kind()));
                                if result.is_err() { life.control_failed(Instant::now()); }
                                events(json!({"type":"carlink-starting"}));
                                drop(life);
                            }
                        }
                        *active.lock().unwrap() = false;
                        *control.lock().unwrap() = None;
                        web_tls.disconnect(process_generation);
                        running.store(false, Ordering::Release);
                        if let Err(e) = child.stop() { events(json!({"type":"receiver-error","message":e.to_string()})); }
                        events(json!({"type":"receiver-stopped"}));
                    }
                    Err(e) => {
                        *active.lock().unwrap() = false;
                        web_tls.disconnect(process_generation);
                        events(json!({"type":"receiver-error","message":e.to_string()}));
                    }
                }
                tracing.store(false, Ordering::Release);
                if *stop.0.lock().unwrap() { break; }
                if lifecycle.lock().unwrap().played { failures = 0; }
                let delay = restart_delay(policy, failures);
                failures = failures.saturating_add(1);
                events(json!({"type":"receiver-restarting","delayMillis":delay.as_millis() as u64}));
                if wait(delay) { break; }
            }
            *fragment.lock().unwrap() = None;
            running.store(false, Ordering::Release);
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
        backoff.observe(&json!({"type":"state","state":"Playing"}), now);
        assert_eq!(backoff.failures, 0);
        assert_eq!(
            backoff.observe(&failure, now),
            Some(Duration::from_secs(60))
        );
    }

    struct Child(Sender<()>, ControlPipe, Arc<AtomicBool>);
    impl RelayChild for Child {
        fn control(&self) -> Option<ControlPipe> {
            Some(self.1.clone())
        }
        fn exited(&mut self) -> io::Result<bool> {
            Ok(self.2.swap(false, Ordering::AcqRel))
        }
        fn stop(&mut self) -> io::Result<()> {
            let _ = self.0.send(());
            Ok(())
        }
    }
    struct Factory(
        Sender<(Vec<OsString>, EventSink)>,
        Sender<()>,
        Sender<Value>,
        Arc<AtomicBool>,
    );
    impl ChildFactory for Factory {
        fn spawn(
            &self,
            _: &Path,
            args: &[OsString],
            events: EventSink,
        ) -> io::Result<Box<dyn RelayChild>> {
            self.0.send((args.to_vec(), events)).unwrap();
            let tx = self.2.clone();
            Ok(Box::new(Child(
                self.1.clone(),
                ControlPipe::fake(move |v| tx.send(v.clone()).map_err(io::Error::other)),
                self.3.clone(),
            )))
        }
    }
    struct Fixture {
        dir: PathBuf,
        settings: SupervisorSettings,
        spawns: Receiver<(Vec<OsString>, EventSink)>,
        stops: Receiver<()>,
        controls: Receiver<Value>,
        exited: Arc<AtomicBool>,
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
                web_transcode: true,
                plugins: None,
                state_dir: dir.clone(),
                addresses: vec![],
                pairing: false,
                pem_tls: None,
                default_tls: None,
                carlink: Some(CarlinkSettings {
                    ble: None,
                    head_unit: Ipv4Addr::LOCALHOST,
                    local: Ipv4Addr::LOCALHOST,
                    pin: None,
                    name: crate::branding::DEVICE_NAME.into(),
                    model: crate::branding::MODEL.into(),
                    hevc: "auto".into(),
                    trace: false,
                    vehicle_data: false,
                    picture: None,
                    auto_reconnect: true,
                    ports: BTreeMap::new(),
                }),
            };
            let (spawn_tx, spawns) = channel();
            let (stop_tx, stops) = channel();
            let (control_tx, controls) = channel();
            let exited = Arc::new(AtomicBool::new(false));
            let (event_tx, events) = channel();
            let event_tx = Mutex::new(event_tx);
            Self {
                dir,
                settings,
                spawns,
                stops,
                controls,
                exited: exited.clone(),
                events,
                factory: Arc::new(Factory(spawn_tx, stop_tx, control_tx, exited)),
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
            self.wait_for_control();
            let has = |flag: &str| args.iter().any(|arg| arg == flag);
            (has("--clink-head-unit"), has("--web-port"), sink)
        }
        fn wait_for_control(&self) {
            // The supervisor emits receiver-started after installing the child's control pipe.
            loop {
                let event = self.events.recv_timeout(Duration::from_secs(2)).unwrap();
                if event["type"] == "receiver-started" {
                    break;
                }
            }
        }
        fn backoff_delays(&self) -> Vec<u64> {
            self.events
                .try_iter()
                .filter(|v| v["type"] == "carlink-backoff")
                .map(|v| v["delayMillis"].as_u64().unwrap())
                .collect()
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
    fn retained_child_reconnects_capture_phone_mac_for_each_connection() {
        let mut f = Fixture::new("phone-mac-snapshot", 8443);
        f.settings.carlink.as_mut().unwrap().ble = Some(BleCarlinkSettings {
            adapter: None,
            interface: "wlan1".into(),
            address: None,
            id: "relay".into(),
            band: 1,
            phone_mac: Some("02:aa:bb:cc:dd:ee".into()),
            default_phone_mac: Some("02:aa:bb:cc:dd:ee".into()),
        });
        let mut saved = crate::settings::Settings::default();
        saved.save(&f.dir).unwrap();
        let mut relay = f.supervisor(&CarlinkBackoff::default());
        relay.start().unwrap();
        let (args, sink) = f.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(args
            .windows(2)
            .any(|p| p[0] == "--clink-phone-mac" && p[1] == "02:aa:bb:cc:dd:ee"));
        f.wait_for_control();
        saved.experimental_active = false;
        saved.save(&f.dir).unwrap();
        relay.restart_carlink(None, "auto", false).unwrap();
        let value = f.controls.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(value.as_object().unwrap().contains_key("phoneMac"));
        assert_eq!(value["phoneMac"], Value::Null);
        sink(json!({"type":"clink-restart","request":value["request"],"generation":2}));
        saved.experimental_active = true;
        saved.experimental_bluetooth_address = "02:11:22:33:44:55".into();
        saved.save(&f.dir).unwrap();
        // The retry worker must resolve the same saved snapshot as manual replacements.
        relay.carlink_lifecycle.lock().unwrap().retry_now();
        let value = f.controls.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(value["phoneMac"], "02:11:22:33:44:55");
        sink(json!({"type":"clink-restart","request":value["request"],"generation":3}));
        saved.experimental_bluetooth_address.clear();
        saved.save(&f.dir).unwrap();
        relay.restart_carlink(None, "auto", false).unwrap();
        assert_eq!(
            f.controls.recv_timeout(Duration::from_secs(2)).unwrap()["phoneMac"],
            "02:aa:bb:cc:dd:ee"
        );
        assert!(f.spawns.try_recv().is_err());
        assert!(f.stops.try_recv().is_err());
        relay.stop();
    }
    #[test]
    fn codec_mismatch_replaces_only_carlink_and_disables_hevc_once() {
        let f = Fixture::new("hevc-mismatch", 8443);
        let mut relay = f.supervisor(&CarlinkBackoff::default());
        relay.start().unwrap();
        let (_, _, sink) = f.spawned();
        sink(json!({"type":"diagnostic","generation":1,"message":"codec mismatch expected=H265"}));
        let command = f.controls.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(command["type"], "clink-restart");
        assert_eq!(command["hevc"], "off");
        sink(
            json!({"type":"clink-restart","request":command["request"],"generation":2,"result":"started"}),
        );
        sink(json!({"type":"diagnostic","generation":2,"message":"codec mismatch expected=H265"}));
        assert!(f.controls.recv_timeout(Duration::from_millis(20)).is_err());
        assert!(f.spawns.try_recv().is_err());
        assert!(f.stops.try_recv().is_err());
        relay.stop();
    }
    #[test]
    fn current_generation_only_and_browser_survives_manual_resize_and_trial() {
        let f = Fixture::new("generations", 8443);
        let mut relay = f.supervisor(&CarlinkBackoff::default());
        relay.start().unwrap();
        let (carlink, web, sink) = f.spawned();
        assert!(carlink && web);
        for generation in [2, 3] {
            relay
                .restart_carlink(
                    Some(crate::display::Size {
                        width: 1920,
                        height: 1080,
                    }),
                    "signal",
                    true,
                )
                .unwrap();
            let command = f.controls.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(command["picture"]["width"], 1920);
            assert_eq!(command["vehicleData"], true);
            sink(json!({"type":"state","state":"Playing","generation":generation-1}));
            sink(
                json!({"type":"clink-summary","generation":generation-1,"fields":{"name":"retired"}}),
            );
            sink(
                json!({"type":"clink-restart","request":command["request"],"generation":generation,"result":"started"}),
            );
            sink(json!({"type":"state","state":"Playing","generation":generation}));
        }
        let events = f.events.try_iter().collect::<Vec<_>>();
        assert_eq!(events.iter().filter(|v| v["type"] == "state").count(), 2);
        assert!(!events.iter().any(|v| v["fields"]["name"] == "retired"));
        assert!(f.spawns.try_recv().is_err());
        assert!(f.stops.try_recv().is_err());
        relay.stop();
        sink(json!({"type":"state","state":"Playing","generation":3}));
        assert!(!f.events.try_iter().any(|v| v["type"] == "state"));
    }
    #[test]
    fn live_options_survive_a_real_child_exit_and_old_process_events_stay_fenced() {
        let f = Fixture::new("retained-options", 8443);
        let mut relay = f.supervisor(&CarlinkBackoff::default());
        relay.start().unwrap();
        let (_, _, old) = f.spawned();
        relay
            .restart_carlink(
                Some(crate::display::Size {
                    width: 1920,
                    height: 1080,
                }),
                "signal",
                true,
            )
            .unwrap();
        let command = f.controls.recv_timeout(Duration::from_secs(2)).unwrap();
        old(
            json!({"type":"clink-restart","request":command["request"],"generation":2,"result":"started"}),
        );
        f.exited.store(true, Ordering::Release);
        let (args, new) = f.spawns.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(args
            .windows(2)
            .any(|a| a[0] == "--clink-hevc" && a[1] == "off"));
        // The new process must report its encoder before HEVC can be restored beside web.
        assert!(args
            .windows(2)
            .any(|a| a[0] == "--clink-vehicle-data" && a[1] == "on"));
        let command = f.controls.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(command["picture"], json!({"width":1920,"height":1080}));
        old(json!({"type":"state","state":"Playing","generation":2}));
        new(
            json!({"type":"clink-restart","request":command["request"],"generation":2,"result":"started"}),
        );
        new(json!({"type":"state","state":"Playing","generation":2}));
        assert_eq!(
            f.events.try_iter().filter(|v| v["type"] == "state").count(),
            1
        );
        relay.stop();
    }
    #[test]
    fn manual_reconnect_waits_after_session_end_and_connect_still_works() {
        let mut f = Fixture::new("manual", 8443);
        f.settings.carlink.as_mut().unwrap().auto_reconnect = false;
        let mut relay = f.supervisor(&CarlinkBackoff::default());
        relay.start().unwrap();
        let (_, _, sink) = f.spawned();
        sink(json!({"type":"state","state":"Ended","generation":1}));
        assert!(f.controls.recv_timeout(Duration::from_millis(20)).is_err());
        relay.connect_carlink();
        assert_eq!(
            f.controls.recv_timeout(Duration::from_secs(2)).unwrap()["type"],
            "clink-restart"
        );
        assert!(f.spawns.try_recv().is_err());
        relay.stop();
    }
    #[test]
    fn both_outputs_keep_the_same_web_child_during_band_backoff() {
        let f = Fixture::new("both", 8443);
        let backoff =
            CarlinkBackoff::with_delays(Duration::from_millis(40), Duration::from_secs(1));
        let mut relay = f.supervisor(&backoff);
        relay.start().unwrap();
        let (_, _, sink) = f.spawned();
        let now = Instant::now();
        sink(band_failure());
        let command = f.controls.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(command["type"], "clink-restart");
        assert!(now.elapsed() >= Duration::from_millis(40));
        assert_eq!(f.backoff_delays(), [40]);
        assert!(f.spawns.try_recv().is_err());
        assert!(f.stops.try_recv().is_err());
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
            web_transcode: true,
            plugins: None,
            state_dir: dir.into(),
            addresses: vec![Ipv4Addr::new(10, 77, 0, 1), Ipv4Addr::new(192, 168, 1, 2)],
            pairing: false,
            carlink: None,
            pem_tls: None,
            default_tls: None,
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
                vehicle_data: false,
                picture: None,
                auto_reconnect: false,
                ble: ble.then(|| BleCarlinkSettings {
                    adapter: None,
                    interface: "wlan0".into(),
                    address: None,
                    id: "id".into(),
                    band: 1,
                    phone_mac: None,
                    default_phone_mac: None,
                }),
                head_unit: Ipv4Addr::new(192, 168, 43, 1),
                local: Ipv4Addr::new(192, 168, 43, 2),
                pin: None,
                ports: BTreeMap::new(),
            });
            let mut expected = vec![
                "--unix",
                "/tmp/oxp-relay.sock",
                "--control-stdin",
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
                "--clink-vehicle-data",
                "off",
                "--clink-state-dir",
                "/state",
            ];
            expected.extend(if ble {
                vec![
                    "--clink-scan",
                    "continuous",
                    "--clink-identity",
                    "ble",
                    "--clink-link",
                    "ble",
                    "--clink-interface",
                    "wlan0",
                    "--clink-id",
                    "id",
                    "--clink-band",
                    "1",
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
            expected.splice(8..8, ["--clink-trace", "/state/carlink-trace"]);
            assert_eq!(strings(settings.arguments()), expected);
            settings.carlink = None;
            assert!(strings(settings.arguments())
                .iter()
                .any(|arg| arg == "--control-stdin"));
        }
    }
    #[test]
    fn web_arguments_always_enable_control_and_keep_optional_trusted_material() {
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
            "--control-stdin",
            "--tls-self-signed",
            &format!("{state}/receiver-tls"),
            "--tls-san",
            "openxplay-linux.local",
            "--web-transcode",
            "auto",
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
            default_tls: None,
            ..settings(&dir)
        };
        assert_eq!(strings(with.arguments()), legacy);
        std::fs::create_dir_all(dir.join("public-name")).unwrap();
        std::fs::write(&pem.cert, "chain").unwrap();
        std::fs::write(&pem.key, "key").unwrap();
        let args = strings(with.arguments());
        let mut expected: Vec<String> = legacy.iter().map(|s| s.to_string()).collect();
        expected.splice(
            9..9,
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
    fn pipe_is_created_only_with_control_and_dead_or_stuck_children_never_block() {
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
