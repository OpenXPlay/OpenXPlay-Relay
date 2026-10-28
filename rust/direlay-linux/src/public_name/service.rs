// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! The public-name worker: one thread that keeps the name's address and certificate current,
//! reports the status line and says which certificate the relay should serve.
use super::{
    acme::{self, AcmeError, Dns01, IssueError},
    cert,
    config::{self, AcmeEnvironment, Config, Provider, Stamp},
    dns::{self, Propagation, WaitError},
    http::{Http, UreqHttp},
    local_dns, privacy,
    providers::{self, Endpoints, ProviderClient, ProviderError},
    schedule,
    store::{self, ErrorRecord, Meta, Store},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// What the worker needs from the relay.
pub trait Host: Send + Sync {
    /// The head-unit-facing IPv4 address now, if there is one.
    fn address(&self) -> Option<Ipv4Addr>;
    /// Whether the relay has a route towards the internet.
    fn online(&self) -> bool {
        default_route()
    }
    /// The wall clock in Unix seconds, `None` while it is clearly not synchronised.
    fn clock(&self) -> Option<u64> {
        trusted_clock()
    }
}

/// The wall clock, unless the kernel says no time service keeps it (the signal behind
/// `timedatectl`'s `NTPSynchronized`, which the clock guard reads) or it reads earlier than this
/// build: a relay without an RTC boots with the time it was switched off.
pub fn trusted_clock() -> Option<u64> {
    let now = crate::platform::wall_clock().as_secs();
    (super::time::kernel_synchronized() && now >= super::time::CLOCK_FLOOR).then_some(now)
}

/// Whether the kernel has an IPv4 default route.
#[cfg(target_os = "linux")]
pub fn default_route() -> bool {
    std::fs::read_to_string("/proc/net/route").is_ok_and(|routes| {
        routes.lines().skip(1).any(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.len() > 3
                && fields[1] == "00000000"
                && u16::from_str_radix(fields[3], 16).is_ok_and(|flags| flags & 1 != 0)
        })
    })
}
/// Development hosts are taken to be online.
#[cfg(not(target_os = "linux"))]
pub fn default_route() -> bool {
    true
}

type NameServers = dyn Fn(Provider) -> Vec<Vec<SocketAddr>> + Send + Sync;
/// Everything the worker talks to; tests point it at local fakes.
pub struct Network {
    pub http: Arc<dyn Http>,
    pub endpoints: Endpoints,
    pub production: String,
    pub staging: String,
    pub name_servers: Arc<NameServers>,
    pub propagation: Propagation,
    pub acme: acme::Timing,
    /// How often the file and the address are checked.
    pub tick: Duration,
    /// The pause after a configuration change before requests start, so that a page saving
    /// field by field does not send one request per field.
    pub settle: Duration,
    /// Where NetworkManager's shared-mode dnsmasq reads extra configuration (the local DNS
    /// answer in device-hotspot mode); `None` writes nothing.
    pub local_dns: Option<local_dns::Paths>,
}
impl Network {
    pub fn internet(local_dns: Option<local_dns::Paths>) -> Self {
        Self {
            http: Arc::new(UreqHttp::new(true)),
            endpoints: Endpoints::production(),
            production: acme::PRODUCTION.into(),
            staging: acme::STAGING.into(),
            name_servers: Arc::new(|provider| {
                dns::resolve_servers(providers::name_servers(provider))
            }),
            propagation: Propagation::default(),
            acme: acme::Timing::default(),
            tick: Duration::from_secs(5),
            settle: Duration::from_secs(3),
            local_dns,
        }
    }
}

/// The ACME certificate to serve for the name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Served {
    pub name: String,
    pub cert: PathBuf,
    pub key: PathBuf,
    pub not_after: u64,
    /// Identifies the actual chain, including renewals with unchanged names and dates.
    pub fingerprint: String,
}

/// Whether the head-unit page should be restarted now to serve `desired` instead of `current`:
/// only while no CarPlay session runs, or at once when the served certificate expires within
/// three days.
pub fn should_restart(
    current: Option<&Served>,
    desired: Option<&Served>,
    session: bool,
    now: Option<u64>,
) -> bool {
    if current.map(|s| (&s.name, &s.fingerprint)) == desired.map(|s| (&s.name, &s.fingerprint)) {
        return false;
    }
    let urgent = current
        .zip(now)
        .is_some_and(|(c, now)| c.not_after <= now + schedule::URGENT);
    !session || urgent
}

/// The status line: `off`, `incomplete`, `waiting-network`, `updating-address`,
/// `requesting-certificate`, `ready` or `error` (with `error`, `nextAttempt` and, for
/// `acme-rejected`, `errorDetail`). Times are wall-clock Unix seconds, for display only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicNameStatus {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_after: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_attempt: Option<u64>,
    /// Problems in `public-name.json` by file key (`missing` keys left out).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub file_errors: BTreeMap<String, String>,
    /// The name, while its certificate is valid and the published address is the current one:
    /// the head-unit address to show first, also while the status line reports a problem (a
    /// failing renewal does not hide a name that still works).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_name: Option<String>,
}
impl PublicNameStatus {
    fn state(state: &str) -> Self {
        Self {
            state: state.into(),
            ..Default::default()
        }
    }
}

struct Shared {
    config: Option<Config>,
    stamp: Option<Stamp>,
    /// Bumped whenever the configuration's effect changes; a running attempt then stops.
    version: u64,
    status: PublicNameStatus,
    served: Option<Served>,
    /// The CarPlay hotspot's address at its last activation.
    hotspot: Option<Ipv4Addr>,
    /// Established Tesla address; only name modes set this override.
    address_override: Option<Ipv4Addr>,
    renew: bool,
    stop: bool,
}
struct Inner {
    path: PathBuf,
    store: Store,
    network: Network,
    shared: Mutex<Shared>,
    wake: Condvar,
    /// Bumped whenever [`Shared::served`] changes.
    generation: AtomicU64,
    /// Serialises writes of the configuration and the store against removal.
    files: Mutex<()>,
}

pub struct PublicNameService {
    inner: Arc<Inner>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
pub enum ImportError {
    Invalid(BTreeMap<String, String>),
    Storage,
}
impl PublicNameService {
    pub fn new(state_dir: &Path, network: Network) -> Self {
        let inner = Arc::new(Inner {
            path: state_dir.join(config::FILE),
            store: Store::new(state_dir),
            network,
            shared: Mutex::new(Shared {
                config: None,
                stamp: None,
                version: 0,
                status: PublicNameStatus::state("off"),
                served: None,
                hotspot: None,
                address_override: None,
                renew: false,
                stop: false,
            }),
            wake: Condvar::new(),
            generation: AtomicU64::new(0),
            files: Mutex::new(()),
        });
        inner.reload(true);
        inner.refresh_served(trusted_clock());
        let idle = inner.idle_status();
        inner.shared.lock().unwrap().status =
            idle.unwrap_or_else(|| PublicNameStatus::state("waiting-network"));
        Self {
            inner,
            worker: Mutex::new(None),
        }
    }
    pub fn start(&self, host: Arc<dyn Host>) -> std::io::Result<()> {
        let mut worker = self.worker.lock().unwrap();
        if worker.is_some() {
            return Ok(());
        }
        self.inner.shared.lock().unwrap().stop = false;
        let inner = self.inner.clone();
        *worker = Some(crate::platform::spawn_worker(
            "public-name",
            Box::new(move || inner.run(host.as_ref())),
        )?);
        Ok(())
    }
    pub fn stop(&self) {
        self.inner.shared.lock().unwrap().stop = true;
        self.inner.wake.notify_all();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            // The worker may drop the last owner itself; it then ends on its own.
            if worker.thread().id() != std::thread::current().id() {
                let _ = worker.join();
            }
        }
    }
    pub fn status(&self) -> PublicNameStatus {
        self.inner.shared.lock().unwrap().status.clone()
    }
    /// The ACME certificate to serve, if any; `None` means the relay's own certificates.
    pub fn served(&self) -> Option<Served> {
        self.inner.shared.lock().unwrap().served.clone()
    }
    /// Freeze the pair under the worker's write gate; automatic child respawns keep this
    /// material until the relay explicitly applies a newer certificate.
    pub fn certificate_snapshot(&self, state_dir: &Path) -> std::io::Result<Option<Served>> {
        let _files = self.inner.files.lock().unwrap();
        self.inner.refresh_served(trusted_clock());
        let mut served = self.served();
        if let Some(material) = served.as_mut() {
            let chain = std::fs::read(&material.cert)?;
            let key = std::fs::read(&material.key)?;
            // Immutable generations keep retries on the acknowledged pair even if a later
            // replacement is rejected or its acknowledgement is lost.
            let dir = state_dir
                .join("receiver-tls/named")
                .join(&material.fingerprint);
            material.cert = dir.join("named-cert.pem");
            material.key = dir.join("named-key.pem");
            crate::admin::auth::private_file(&material.key, &key)?;
            crate::admin::auth::private_file(&material.cert, &chain)?;
        }
        Ok(served)
    }
    /// Changes whenever [`Self::served`] does.
    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }
    /// A secret-bearing snapshot for configuration export only; never include it in status.
    pub fn privacy_export(&self) -> std::io::Result<Value> {
        let _files = self.inner.files.lock().unwrap();
        self.inner.reload_locked(false);
        let config = self.inner.shared.lock().unwrap().config.clone();
        privacy::export(&self.inner.store, config.as_ref())
    }
    pub fn config(&self) -> Option<Config> {
        let _files = self.inner.files.lock().unwrap();
        self.inner.reload_locked(false);
        self.inner.shared.lock().unwrap().config.clone()
    }
    /// Installs every validated privacy section before releasing the worker's write gate.
    /// Bumping the version also fences an ACME order already in flight for the same name.
    pub fn import_privacy(&self, import: privacy::Import) -> std::io::Result<()> {
        let files = self.inner.files.lock().unwrap();
        self.inner.reload_locked(false);
        let config = import
            .config
            .as_ref()
            .cloned()
            .or_else(|| self.inner.shared.lock().unwrap().config.clone());
        if import.certificate.as_ref().is_some_and(|material| {
            config.as_ref().and_then(|c| c.name.as_deref()) != Some(material.name.as_str())
        }) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "public-name configuration changed",
            ));
        }
        self.inner.shared.lock().unwrap().version += 1;
        if let Some(material) = import.certificate {
            self.inner
                .store
                .save_certificate(material.chain.as_bytes(), &material.key)?;
            let mut meta = self.inner.store.meta();
            meta.name = Some(material.name);
            meta.provider = config
                .as_ref()
                .and_then(|c| c.provider)
                .map(|p| p.id().into());
            meta.acme = config.as_ref().map(|c| c.acme.id().into());
            meta.not_before = Some(material.leaf.not_before);
            meta.not_after = Some(material.leaf.not_after);
            meta.issued_at = Some(material.leaf.not_before);
            meta.cert_id = material.leaf.cert_id;
            meta.ari_window = None;
            meta.renew_at = None;
            meta.ari_next_poll = None;
            self.inner.store.save_meta(&meta)?;
        }
        if let Some(account) = import.account {
            self.inner.store.save_account(
                &account.key,
                &account.directory,
                account.url.as_deref(),
            )?;
        }
        if let Some(config) = import.config {
            self.inner.save_config(&config)?;
        }
        if let Some(providers) = import.providers {
            self.inner.store.save_providers(&providers)?;
        }
        self.inner.reload_locked(true);
        drop(files);
        self.inner.wake.notify_all();
        Ok(())
    }
    /// The admin page's values; the token never leaves the relay.
    pub fn admin_values(&self) -> Map<String, Value> {
        let _files = self.inner.files.lock().unwrap();
        self.inner.reload_locked(false);
        let active = self.inner.shared.lock().unwrap().config.clone();
        let mut values = config::admin_values(active.as_ref());
        let providers = self
            .inner
            .store
            .providers(active.as_ref())
            .unwrap_or_default();
        values.insert(
            "publicNameProviders".into(),
            store::provider_values(&providers),
        );
        values.insert(
            "privacyConfigAvailable".into(),
            (active.is_some()
                || !providers.is_empty()
                || self.inner.store.certificate().is_some()
                || self
                    .inner
                    .store
                    .account(active.as_ref().map(|c| c.acme))
                    .is_some())
            .into(),
        );
        values
    }
    /// Applies the admin page's partial update and writes the file.
    pub fn update(&self, values: &Map<String, Value>) -> Result<(), BTreeMap<String, String>> {
        let files = self.inner.files.lock().unwrap();
        self.inner.reload_locked(false);
        let mut current = self.inner.shared.lock().unwrap().config.clone();
        let storage_error = || {
            values
                .keys()
                .map(|k| (k.clone(), "error.request".to_string()))
                .collect::<BTreeMap<_, _>>()
        };
        let providers = self
            .inner
            .store
            .providers(current.as_ref())
            .map_err(|_| storage_error())?;
        if let Some(provider) = values
            .get("publicNameProvider")
            .and_then(Value::as_str)
            .and_then(Provider::parse)
        {
            if current.as_ref().and_then(|c| c.provider) != Some(provider) {
                let setup = providers.get(provider.id()).cloned().unwrap_or_default();
                let base = current.get_or_insert_with(|| Config {
                    enabled: false,
                    ..Default::default()
                });
                base.provider = Some(provider);
                base.name = (!setup.name.is_empty()).then_some(setup.name);
                base.token = (!setup.token.is_empty()).then_some(setup.token);
            }
        }
        let next = config::apply_admin(current.as_ref(), values)?;
        self.inner.save_config(&next).map_err(|_| storage_error())?;
        self.inner.reload_locked(false);
        drop(files);
        self.inner.wake.notify_all();
        Ok(())
    }
    /// Uses the hand-written file parser, including its size cap and per-key error codes.
    pub fn import(&self, content: &str) -> Result<(String, String), ImportError> {
        let config = config::parse(content.as_bytes());
        if !config.errors.is_empty() {
            return Err(ImportError::Invalid(config.errors));
        }
        let files = self.inner.files.lock().unwrap();
        self.inner.reload_locked(false);
        self.inner
            .save_config(&config)
            .map_err(|_| ImportError::Storage)?;
        self.inner.reload_locked(false);
        drop(files);
        self.inner.wake.notify_all();
        Ok((config.provider.unwrap().id().into(), config.name.unwrap()))
    }
    /// Runs just before the CarPlay hotspot is activated with `address` (device-hotspot mode):
    /// the head unit then resolves the name to it locally, also without internet.
    pub fn before_hotspot(&self, address: Ipv4Addr) {
        self.head_unit_address(Some(address));
    }
    /// Follow the selected car-browser network, including loss of its address.
    pub fn head_unit_address(&self, address: Option<Ipv4Addr>) {
        let _files = self.inner.files.lock().unwrap();
        let mut shared = self.inner.shared.lock().unwrap();
        if shared.hotspot == address {
            return;
        }
        shared.hotspot = address;
        drop(shared);
        self.inner.sync_local_dns();
        self.inner.wake.notify_all();
    }
    /// Established Tesla addresses override publishing and the local name answer together.
    pub fn published_address_override(&self, address: Option<Ipv4Addr>) {
        let _files = self.inner.files.lock().unwrap();
        let mut shared = self.inner.shared.lock().unwrap();
        if shared.address_override == address {
            return;
        }
        shared.address_override = address;
        drop(shared);
        self.inner.sync_local_dns();
        self.inner.wake.notify_all();
    }
    pub fn has_configuration(&self) -> bool {
        self.inner
            .shared
            .lock()
            .unwrap()
            .config
            .as_ref()
            .is_some_and(|c| c.enabled && c.complete())
    }
    /// "Request certificate now": the backoff starts over and the address update and the
    /// certificate check run at once, still within the order guard and the CA's `Retry-After`.
    pub fn renew(&self) {
        self.inner.shared.lock().unwrap().renew = true;
        self.inner.wake.notify_all();
    }
    /// Deletes the configuration, the certificate, its key and the account key.
    pub fn remove(&self) -> std::io::Result<()> {
        let files = self.inner.files.lock().unwrap();
        match std::fs::remove_file(&self.inner.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        self.inner.store.remove_all()?;
        self.inner.reload_locked(false);
        drop(files);
        self.inner.wake.notify_all();
        if self.inner.store.exists() {
            return Err(std::io::Error::other("public-name files remain"));
        }
        Ok(())
    }
}
impl Drop for PublicNameService {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The effective part of a configuration: a change here restarts the work.
#[derive(PartialEq)]
struct Effect {
    enabled: bool,
    provider: Option<Provider>,
    name: Option<String>,
    token: Option<String>,
    agree_terms: bool,
    acme: AcmeEnvironment,
}
fn effect(config: &Option<Config>) -> Option<Effect> {
    config.as_ref().map(|c| Effect {
        enabled: c.enabled,
        provider: c.provider,
        name: c.name.clone(),
        token: c.token.clone(),
        agree_terms: c.agree_terms,
        acme: c.acme,
    })
}

/// One run of the worker's loop for one configuration version, local to its thread.
struct Runtime {
    version: u64,
    failures: u32,
    /// The backoff after a failure: nothing is sent before it.
    next_attempt: Option<Instant>,
    /// The last failure found the relay without a default route: retry once it has one.
    offline: bool,
    /// Let's Encrypt's `Retry-After` or the local order guard: only certificate requests wait
    /// (address updates go on), until this time (also as wall-clock seconds), showing this error.
    cert_hold: Option<(Instant, u64, ErrorRecord)>,
    /// The address published in this process for this configuration, and when.
    published: Option<(Ipv4Addr, Instant)>,
    /// "Request certificate now": publish again even if unchanged.
    republish: bool,
    last_address: Option<Ipv4Addr>,
    last_check: Option<Instant>,
    check_at: Option<u64>,
    check_due: bool,
    settle_until: Option<Instant>,
}
impl Runtime {
    fn new(version: u64) -> Self {
        Self {
            version,
            failures: 0,
            next_attempt: None,
            offline: false,
            cert_hold: None,
            published: None,
            republish: false,
            last_address: None,
            last_check: None,
            check_at: None,
            check_due: true,
            settle_until: None,
        }
    }
    fn reset_backoff(&mut self) {
        self.failures = 0;
        self.next_attempt = None;
        self.offline = false;
    }
}
/// A provider's `Retry-After`: every request to that provider waits until then (instant and
/// wall-clock seconds), also across configuration changes and "Request certificate now".
type ProviderHold = Option<(Provider, Instant, u64)>;
enum Check {
    /// Nothing to order, or a new certificate is stored.
    Done,
    /// The CA's `Retry-After` or the local guard holds the order back until this time.
    Blocked(u64, ErrorRecord),
    Failed(IssueError),
}

/// The status line's error code, detail and `Retry-After` for a failure.
fn describe(error: &IssueError) -> (&'static str, Option<String>, Option<u64>) {
    match error {
        IssueError::Provider(e) => (
            e.code(),
            None,
            match e {
                ProviderError::RateLimited(s) => *s,
                _ => None,
            },
        ),
        IssueError::DnsNotUpdated => ("dns-not-updated", None, None),
        IssueError::Acme(AcmeError::RateLimited(s)) => ("acme-rate-limited", None, *s),
        IssueError::Acme(AcmeError::Rejected(detail)) => {
            ("acme-rejected", Some(detail.clone()), None)
        }
        IssueError::Acme(AcmeError::AccountMissing) => {
            ("acme-rejected", Some("the account is unknown".into()), None)
        }
        // There is no label for Let's Encrypt being unreachable; the provider one says it.
        IssueError::Acme(AcmeError::Unreachable) | IssueError::Interrupted => {
            ("provider-unreachable", None, None)
        }
    }
}

fn rate_limited() -> ErrorRecord {
    ErrorRecord {
        code: "acme-rate-limited".into(),
        detail: None,
    }
}

/// The provider side of DNS-01.
struct ProviderDns<'a> {
    client: ProviderClient<'a>,
    servers: &'a NameServers,
    provider: Provider,
    challenge: String,
    timing: Propagation,
    pause: &'a dyn Fn(Duration) -> bool,
}
impl Dns01 for ProviderDns<'_> {
    fn set(&mut self, value: &str) -> Result<(), IssueError> {
        self.client.set_txt(value).map_err(IssueError::Provider)
    }
    fn propagate(&mut self, value: &str) -> Result<(), IssueError> {
        let servers = (self.servers)(self.provider);
        dns::wait_for_txt(&servers, &self.challenge, value, self.timing, &mut |d| {
            (self.pause)(d)
        })
        .map_err(|e| match e {
            WaitError::NotUpdated => IssueError::DnsNotUpdated,
            WaitError::Interrupted => IssueError::Interrupted,
        })
    }
    fn clear(&mut self) {
        let _ = self.client.clear_txt();
    }
}

impl Inner {
    /// Called with `files` held: retain the other provider before activating this setup.
    fn save_config(&self, config: &Config) -> std::io::Result<()> {
        let active = self.shared.lock().unwrap().config.clone();
        let mut providers = self.store.providers(active.as_ref())?;
        store::remember_provider(&mut providers, config);
        self.store.save_providers(&providers)?;
        crate::admin::auth::private_file(&self.path, &config.to_json())
    }
    /// Re-reads the file when it changed (or `force`). Without a file, everything kept for the
    /// name is deleted: removing `public-name.json` by hand removes the certificate too.
    fn reload(&self, force: bool) {
        let _files = self.files.lock().unwrap();
        self.reload_locked(force);
    }
    /// [`Self::reload`] with `files` held.
    fn reload_locked(&self, force: bool) {
        let stamp = config::stamp_of(&self.path);
        let changed = {
            let shared = self.shared.lock().unwrap();
            force || shared.stamp != stamp || (stamp.is_none() && shared.config.is_some())
        };
        if !changed && (stamp.is_some() || !self.store.exists()) {
            return;
        }
        let loaded = match config::load(&self.path) {
            Ok(Some((config, stamp))) => Some((config, stamp)),
            Ok(None) => None,
            // Unreadable for now: keep what was read last.
            Err(_) => return,
        };
        if loaded.is_none() && self.shared.lock().unwrap().config.is_some() && self.store.exists() {
            let _ = self.store.remove_all();
        }
        if let Some((config, _)) = &loaded {
            let _ = self.store.providers(Some(config));
        }
        let mut shared = self.shared.lock().unwrap();
        let (config, stamp) = match loaded {
            Some((config, stamp)) => (Some(config), Some(stamp)),
            None => (None, None),
        };
        if effect(&config) != effect(&shared.config) || force {
            shared.version += 1;
        }
        shared.config = config;
        shared.stamp = stamp;
        drop(shared);
        self.sync_local_dns();
        self.refresh_served(trusted_clock());
        if let Some(idle) = self.idle_status() {
            self.shared.lock().unwrap().status = idle;
        }
    }
    /// The local DNS answer follows the configuration: written for an enabled, complete
    /// configuration once the hotspot address is known, removed otherwise. Needs `files`.
    fn sync_local_dns(&self) {
        let Some(dir) = &self.network.local_dns else {
            return;
        };
        let (name, hotspot, address_override) = {
            let shared = self.shared.lock().unwrap();
            let name = shared
                .config
                .as_ref()
                .filter(|c| c.enabled && c.complete())
                .and_then(|c| c.name.clone());
            (name, shared.hotspot, shared.address_override)
        };
        let _ = match (name, hotspot) {
            (Some(name), address) => {
                local_dns::public_name_override(dir, &name, address, address_override)
            }
            (None, _) => local_dns::remove(dir),
        };
    }
    /// The certificate to serve: an enabled configuration's name with a stored certificate that
    /// passes the checks (its expiry only while the clock can be trusted).
    fn refresh_served(&self, now: Option<u64>) {
        let config = self.shared.lock().unwrap().config.clone();
        let served = config
            .filter(|c| c.enabled)
            .and_then(|c| c.name)
            .and_then(|name| {
                let (chain, key) = self.store.certificate()?;
                let leaf = cert::check(&chain, &key, &name, now).ok()?;
                Some(Served {
                    name,
                    cert: self.store.cert_path(),
                    key: self.store.key_path(),
                    not_after: leaf.not_after,
                    fingerprint: {
                        use sha2::{Digest, Sha256};
                        super::jws::b64(&Sha256::digest(&chain))
                    },
                })
            });
        let mut shared = self.shared.lock().unwrap();
        if shared.served != served {
            shared.served = served;
            self.generation.fetch_add(1, Ordering::AcqRel);
        }
    }
    /// The status while there is nothing to do: off, or incomplete with the file's problems.
    fn idle_status(&self) -> Option<PublicNameStatus> {
        let shared = self.shared.lock().unwrap();
        let config = match &shared.config {
            Some(c) if c.enabled => c,
            _ => return Some(PublicNameStatus::state("off")),
        };
        if config.complete() {
            return None;
        }
        Some(PublicNameStatus {
            name: config.name.clone(),
            file_errors: config
                .errors
                .iter()
                .filter(|(_, code)| *code != "missing")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ..PublicNameStatus::state("incomplete")
        })
    }
    fn set_status(&self, status: PublicNameStatus) {
        self.shared.lock().unwrap().status = status;
    }
    /// Sets the status a run of `version` found, unless the configuration changed meanwhile
    /// (a removal must not be followed by a stale line).
    fn report(&self, version: u64, status: PublicNameStatus) {
        let mut shared = self.shared.lock().unwrap();
        if shared.version == version {
            shared.status = status;
        }
    }
    /// Waits up to `d`; `false` when stopped or the configuration changed meanwhile.
    fn pause(&self, version: u64, d: Duration) -> bool {
        let deadline = Instant::now() + d;
        loop {
            {
                let shared = self.shared.lock().unwrap();
                if shared.stop || shared.version != version {
                    return false;
                }
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return true;
                }
                let _ = self
                    .wake
                    .wait_timeout(shared, left.min(self.network.tick))
                    .unwrap();
            }
            self.reload(false);
        }
    }
    /// Waits for the next tick, a change or a request; `false` when stopped.
    fn idle(&self, d: Duration) -> bool {
        let shared = self.shared.lock().unwrap();
        if shared.stop || shared.renew {
            return !shared.stop;
        }
        let shared = self.wake.wait_timeout(shared, d).unwrap().0;
        !shared.stop
    }
    /// Writes to the store unless the configuration changed since `version` (a removal or
    /// another name): nothing comes back after Remove.
    fn write(&self, version: u64, write: impl FnOnce(&Store) -> std::io::Result<()>) -> bool {
        let _files = self.files.lock().unwrap();
        if self.shared.lock().unwrap().version != version || !self.path.exists() {
            return false;
        }
        write(&self.store).is_ok()
    }

    fn run(&self, host: &dyn Host) {
        let mut rt = Runtime::new(u64::MAX);
        let mut provider_hold: ProviderHold = None;
        loop {
            self.reload(false);
            let (config, version, renew) = {
                let mut shared = self.shared.lock().unwrap();
                if shared.stop {
                    return;
                }
                let renew = std::mem::take(&mut shared.renew);
                (shared.config.clone(), shared.version, renew)
            };
            if version != rt.version {
                let first = rt.version == u64::MAX;
                rt = Runtime::new(version);
                if !first {
                    rt.settle_until = Some(Instant::now() + self.network.settle);
                }
            }
            if renew {
                // Retry now: the backoff starts over; the holds stay.
                rt.reset_backoff();
                rt.republish = true;
                rt.check_due = true;
            }
            self.refresh_served(host.clock());
            let Some(config) = config.filter(|c| c.enabled && c.complete()) else {
                if let Some(idle) = self.idle_status() {
                    self.set_status(idle);
                }
                if !self.idle(self.network.tick) {
                    return;
                }
                continue;
            };
            let address_override = self.shared.lock().unwrap().address_override;
            let address = address_override.or_else(|| host.address().filter(Ipv4Addr::is_private));
            if address != rt.last_address {
                // A new address starts the backoff over.
                if rt.last_address.is_some() || address.is_some() {
                    rt.reset_backoff();
                }
                rt.last_address = address;
            }
            if rt.offline && host.online() {
                // The network is back: try at once instead of waiting out the backoff.
                rt.reset_backoff();
            }
            provider_hold = provider_hold
                .filter(|(p, until, _)| Some(*p) == config.provider && *until > Instant::now());
            let now = Instant::now();
            let wait = [rt.settle_until, rt.next_attempt]
                .into_iter()
                .flatten()
                .filter(|at| *at > now)
                .min();
            if let Some(at) = wait {
                if !self.idle((at - now).min(self.network.tick)) {
                    return;
                }
                continue;
            }
            rt.settle_until = None;
            self.step(host, &config, address, &mut rt, &mut provider_hold);
            let wait = rt
                .check_at
                .zip(host.clock())
                .filter(|(at, now)| at > now)
                .map(|(at, now)| Duration::from_secs(at - now).min(self.network.tick))
                .unwrap_or(self.network.tick);
            if !self.idle(wait) {
                return;
            }
        }
    }
    fn base_status(
        &self,
        config: &Config,
        address: Option<Ipv4Addr>,
        state: &str,
        rt: &Runtime,
    ) -> PublicNameStatus {
        let not_after = self
            .shared
            .lock()
            .unwrap()
            .served
            .as_ref()
            .map(|s| s.not_after);
        let published = rt.published.map(|(a, _)| a);
        PublicNameStatus {
            name: config.name.clone(),
            address: address.map(|a| a.to_string()),
            not_after,
            trusted_name: config
                .name
                .clone()
                .filter(|_| not_after.is_some() && address.is_some() && published == address),
            ..PublicNameStatus::state(state)
        }
    }
    fn fail(
        &self,
        host: &dyn Host,
        config: &Config,
        address: Option<Ipv4Addr>,
        rt: &mut Runtime,
        error: &IssueError,
        now: u64,
    ) {
        let (code, detail, retry_after) = describe(error);
        rt.failures = rt.failures.saturating_add(1);
        let delay = schedule::retry_delay(rt.failures, retry_after);
        rt.next_attempt = Some(Instant::now() + Duration::from_secs(delay));
        // No separate internet check: an unanswered request while the relay has no default
        // route means it is offline.
        rt.offline = matches!(
            error,
            IssueError::Provider(ProviderError::Unreachable)
                | IssueError::Acme(AcmeError::Unreachable)
        ) && !host.online();
        if rt.offline {
            self.report(
                rt.version,
                self.base_status(config, address, "waiting-network", rt),
            );
            return;
        }
        let record = ErrorRecord {
            code: code.into(),
            detail: detail.clone(),
        };
        self.write(rt.version, |store| {
            let mut meta = store.meta();
            meta.last_error = Some(record);
            store.save_meta(&meta)
        });
        self.report(
            rt.version,
            PublicNameStatus {
                error: Some(code.into()),
                error_detail: detail,
                next_attempt: Some(now + delay),
                ..self.base_status(config, address, "error", rt)
            },
        );
    }
    fn held(
        &self,
        config: &Config,
        address: Ipv4Addr,
        rt: &Runtime,
        until: u64,
        error: &ErrorRecord,
    ) {
        self.report(
            rt.version,
            PublicNameStatus {
                error: Some(error.code.clone()),
                error_detail: error.detail.clone(),
                next_attempt: Some(until),
                ..self.base_status(config, Some(address), "error", rt)
            },
        );
    }
    fn step(
        &self,
        host: &dyn Host,
        config: &Config,
        address: Option<Ipv4Addr>,
        rt: &mut Runtime,
        provider_hold: &mut ProviderHold,
    ) {
        let (Some(provider), Some(name), Some(token)) = (
            config.provider,
            config.name.as_deref(),
            config.token.as_deref(),
        ) else {
            return;
        };
        // HTTPS needs a sane clock, and expiry is never judged without one.
        let Some(now) = host.clock() else {
            self.report(
                rt.version,
                self.base_status(config, address, "waiting-network", rt),
            );
            return;
        };
        let Some(address) = address else {
            self.report(
                rt.version,
                PublicNameStatus {
                    error: Some("no-address".into()),
                    next_attempt: Some(now + self.network.tick.as_secs().max(1)),
                    ..self.base_status(config, None, "error", rt)
                },
            );
            return;
        };
        if let Some((_, _, until)) = provider_hold {
            let error = ErrorRecord {
                code: ProviderError::RateLimited(None).code().into(),
                detail: None,
            };
            self.held(config, address, rt, *until, &error);
            return;
        }
        let hold_provider = |hold: &mut ProviderHold, error: &IssueError| {
            if let IssueError::Provider(ProviderError::RateLimited(seconds)) = error {
                let wait = seconds.unwrap_or(0).min(schedule::MAX_RETRY_AFTER);
                if wait > 0 {
                    *hold = Some((
                        provider,
                        Instant::now() + Duration::from_secs(wait),
                        now + wait,
                    ));
                }
            }
        };
        let http = self.network.http.as_ref();
        let republish = rt.republish
            || rt.published.is_none_or(|(published, at)| {
                published != address || at.elapsed().as_secs() >= schedule::REPUBLISH_AFTER
            });
        if republish {
            self.report(
                rt.version,
                self.base_status(config, Some(address), "updating-address", rt),
            );
            let mut client =
                ProviderClient::new(http, &self.network.endpoints, provider, name, token, now);
            if let Err(e) = client.set_address(address) {
                let error = IssueError::Provider(e);
                hold_provider(provider_hold, &error);
                self.fail(host, config, Some(address), rt, &error, now);
                return;
            }
            rt.published = Some((address, Instant::now()));
            rt.republish = false;
            rt.check_due = true;
            self.write(rt.version, |store| {
                let mut meta = store.meta();
                meta.address = Some(address.to_string());
                meta.published_at = Some(now);
                store.save_meta(&meta)
            });
        }
        if let Some((until, wall, error)) = &rt.cert_hold {
            if *until > Instant::now() {
                self.held(config, address, rt, *wall, error);
                return;
            }
            rt.cert_hold = None;
        }
        let check = rt.check_due
            || rt.check_at.is_some_and(|at| now >= at)
            || self.shared.lock().unwrap().served.is_none()
            || rt
                .last_check
                .is_none_or(|at| at.elapsed().as_secs() >= schedule::CHECK_EVERY);
        if check {
            match self.check_certificate(config, provider, name, token, now, rt) {
                Check::Done => {
                    rt.check_due = false;
                    rt.last_check = Some(Instant::now());
                    let meta = self.store.meta();
                    rt.check_at = schedule::next_check(
                        now,
                        meta.not_before,
                        meta.not_after,
                        meta.renew_at,
                        meta.ari_next_poll,
                    );
                    rt.reset_backoff();
                }
                Check::Blocked(until, error) => {
                    let wait = Duration::from_secs(until.saturating_sub(now));
                    rt.cert_hold = Some((Instant::now() + wait, until, error.clone()));
                    self.held(config, address, rt, until, &error);
                    return;
                }
                Check::Failed(IssueError::Interrupted) => return,
                Check::Failed(error) => {
                    hold_provider(provider_hold, &error);
                    self.fail(host, config, Some(address), rt, &error, now);
                    return;
                }
            }
        }
        self.refresh_served(Some(now));
        if self.shared.lock().unwrap().served.is_some() {
            self.report(
                rt.version,
                self.base_status(config, Some(address), "ready", rt),
            );
        } else {
            // Stored, yet not servable: try again like after a failure.
            let error = IssueError::Acme(AcmeError::Rejected(
                "the certificate cannot be served".into(),
            ));
            self.fail(host, config, Some(address), rt, &error, now);
        }
    }
    fn check_certificate(
        &self,
        config: &Config,
        provider: Provider,
        name: &str,
        token: &str,
        now: u64,
        rt: &mut Runtime,
    ) -> Check {
        let version = rt.version;
        let mut meta = self.store.meta();
        // A certificate from the other CA environment (staging and production) is not this
        // configuration's: it is replaced like a missing one.
        let current = self
            .store
            .certificate()
            .and_then(|(chain, key)| cert::check(&chain, &key, name, Some(now)).ok())
            .filter(|_| meta.acme.as_deref() == Some(config.acme.id()));
        let directory = match config.acme {
            AcmeEnvironment::Production => self.network.production.as_str(),
            AcmeEnvironment::Staging => self.network.staging.as_str(),
        };
        let account = {
            let _files = self.files.lock().unwrap();
            if self.shared.lock().unwrap().version != version || !self.path.exists() {
                return Check::Failed(IssueError::Interrupted);
            }
            match self.store.account_key() {
                Ok(key) => key,
                Err(_) => {
                    return Check::Failed(IssueError::Acme(AcmeError::Rejected(
                        "the account key cannot be stored".into(),
                    )))
                }
            }
        };
        let http = self.network.http.as_ref();
        let mut client = None;
        let mut due = false;
        if let Some(leaf) = &current {
            // Keep the previous renewal plan until ARI's next poll; a failure never erases it.
            if meta.cert_id != leaf.cert_id {
                meta.cert_id = leaf.cert_id.clone();
                meta.ari_window = None;
                meta.renew_at = None;
                meta.ari_next_poll = None;
            }
            if leaf.cert_id.is_some() && meta.ari_next_poll.is_none_or(|at| now >= at) {
                let result = acme::Client::connect(
                    http,
                    directory,
                    &account,
                    self.store.account_url_for(directory),
                    now,
                );
                let mut retry = None;
                if let Ok(c) = result {
                    match c.renewal_window(leaf.cert_id.as_deref().unwrap()) {
                        Ok(Some(window)) => {
                            retry = window.retry_after;
                            let bounds = (window.start, window.end);
                            if meta.ari_window != Some(bounds) || meta.renew_at.is_none() {
                                use rand_core::RngCore;
                                meta.ari_window = Some(bounds);
                                meta.renew_at = Some(schedule::ari_time(
                                    bounds.0,
                                    bounds.1,
                                    rand_core::OsRng.next_u64(),
                                ));
                            }
                        }
                        Ok(None) => {
                            meta.ari_window = None;
                            meta.renew_at = None;
                        }
                        Err(AcmeError::RateLimited(seconds)) => retry = seconds,
                        Err(_) => {}
                    }
                    client = Some(c);
                } else if let Err(AcmeError::RateLimited(seconds)) = result {
                    retry = seconds;
                }
                meta.ari_next_poll = Some(now + schedule::ari_poll_delay(retry));
                let saved = meta.clone();
                self.write(version, |store| store.save_meta(&saved));
            }
            let renew_at = meta
                .renew_at
                .filter(|_| meta.cert_id.is_some() && meta.cert_id == leaf.cert_id);
            due = schedule::renewal_due(leaf.not_before, leaf.not_after, now, renew_at);
        }
        let changed = meta.provider.as_deref() != Some(provider.id());
        match &current {
            None => {}
            Some(leaf) if (due || changed) && schedule::may_replace(leaf.not_after, now, due) => {}
            Some(_) => return Check::Done,
        }
        if let Some(until) = meta.retry_after.filter(|t| *t > now) {
            return Check::Blocked(until, rate_limited());
        }
        let allowed = schedule::next_order_allowed(&meta.order_times(name), now);
        if allowed > now {
            // Held by the guard: the failure that used up the orders says why.
            return Check::Blocked(allowed, meta.last_error.unwrap_or_else(rate_limited));
        }
        let address = rt.last_address;
        self.report(
            rt.version,
            self.base_status(config, address, "requesting-certificate", rt),
        );
        let mut client = match client {
            Some(c) => c,
            None => match acme::Client::connect(
                http,
                directory,
                &account,
                self.store.account_url_for(directory),
                now,
            ) {
                Ok(c) => c,
                Err(e) => return self.acme_failed(version, now, IssueError::Acme(e)),
            },
        };
        let pause = |d: Duration| self.pause(version, d);
        let mut dns = ProviderDns {
            client: ProviderClient::new(http, &self.network.endpoints, provider, name, token, now),
            servers: self.network.name_servers.as_ref(),
            provider,
            challenge: format!("_acme-challenge.{name}"),
            timing: self.network.propagation,
            pause: &pause,
        };
        let replaces = current.as_ref().and_then(|l| l.cert_id.clone());
        let issued = acme::issue(
            &mut client,
            name,
            replaces.as_deref(),
            &mut dns,
            self.network.acme,
            &mut |d| pause(d),
            &mut || {
                self.write(version, |store| {
                    let mut meta = store.meta();
                    meta.add_order(name, now);
                    store.save_meta(&meta)
                });
            },
        );
        if let Some(kid) = client.kid() {
            if self.store.account_url_for(directory).as_deref() != Some(kid) {
                self.write(version, |store| {
                    store.save_account_directory(directory)?;
                    store.save_account_url(kid)
                });
            }
        }
        let issued = match issued {
            Ok(issued) => issued,
            Err(e) => return self.acme_failed(version, now, e),
        };
        let stored = self.write(version, |store| {
            store.save_certificate(&issued.chain, &issued.key_pem)?;
            let mut meta = store.meta();
            meta.name = Some(name.into());
            meta.provider = Some(provider.id().into());
            meta.acme = Some(config.acme.id().into());
            meta.not_before = Some(issued.leaf.not_before);
            meta.not_after = Some(issued.leaf.not_after);
            meta.issued_at = Some(now);
            meta.cert_id = issued.leaf.cert_id.clone();
            meta.ari_window = None;
            meta.renew_at = None;
            meta.ari_next_poll = None;
            meta.retry_after = None;
            meta.last_error = None;
            store.save_meta(&meta)
        });
        if !stored {
            return Check::Failed(IssueError::Interrupted);
        }
        self.refresh_served(Some(now));
        Check::Done
    }
    /// Let's Encrypt's `rateLimited` holds certificate requests (persisted, so that a restart
    /// obeys it too); other failures back off.
    fn acme_failed(&self, version: u64, now: u64, error: IssueError) -> Check {
        if let IssueError::Acme(AcmeError::RateLimited(seconds)) = &error {
            let until = now
                + seconds
                    .unwrap_or(schedule::backoff(1))
                    .min(schedule::MAX_RETRY_AFTER);
            self.write(version, |store| {
                let mut meta = store.meta();
                meta.retry_after = Some(until);
                store.save_meta(&meta)
            });
            return Check::Blocked(until, rate_limited());
        }
        Check::Failed(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::{
        http::UreqHttp,
        schedule::DAY,
        store::OrderRecord,
        testing::{FakeAcme, FakeAcmeOptions, FakeDns, FakeHttp, TempDir},
    };
    use std::{os::unix::fs::PermissionsExt, sync::atomic::AtomicBool};
    const NAME: &str = "myrelay.duckdns.org";
    const TOKEN: &str = "a1b2c3d4-0000-0000-0000-000000000000";

    struct FakeHost {
        address: Mutex<Option<Ipv4Addr>>,
        clock: Mutex<Option<u64>>,
        online: AtomicBool,
    }
    impl Host for FakeHost {
        fn address(&self) -> Option<Ipv4Addr> {
            *self.address.lock().unwrap()
        }
        fn online(&self) -> bool {
            self.online.load(Ordering::SeqCst)
        }
        fn clock(&self) -> Option<u64> {
            *self.clock.lock().unwrap()
        }
    }
    /// A DuckDNS that keeps the TXT record where the fake CA and the fake name server see it.
    struct World {
        state: TempDir,
        acme: FakeAcme,
        provider: FakeHttp,
        dns: Arc<FakeDns>,
        host: Arc<FakeHost>,
        reply: Arc<Mutex<&'static str>>,
    }
    impl World {
        fn new(options: FakeAcmeOptions) -> Self {
            let acme = FakeAcme::start(options);
            let dns = Arc::new(FakeDns::start());
            let reply = Arc::new(Mutex::new("OK"));
            let provider = {
                let (txt, dns, reply) = (acme.txt(), dns.clone(), reply.clone());
                FakeHttp::start(move |request| {
                    let query = request.path.split_once('?').map_or("", |(_, q)| q);
                    let value = |key: &str| {
                        query
                            .split('&')
                            .find_map(|p| p.strip_prefix(&format!("{key}=")))
                            .map(str::to_owned)
                    };
                    if *reply.lock().unwrap() == "429" {
                        return (429, vec![("Retry-After".into(), "600".into())], Vec::new());
                    }
                    if let Some(text) = value("txt") {
                        let text = (!text.is_empty() && value("clear").is_none()).then_some(text);
                        dns.set(&format!("_acme-challenge.{NAME}"), text.as_deref());
                        *txt.lock().unwrap() = text;
                    }
                    (200, vec![], reply.lock().unwrap().as_bytes().to_vec())
                })
            };
            Self {
                state: TempDir::new("service"),
                acme,
                provider,
                dns,
                host: Arc::new(FakeHost {
                    address: Mutex::new(Some(Ipv4Addr::new(10, 77, 0, 1))),
                    clock: Mutex::new(Some(crate::platform::wall_clock().as_secs())),
                    online: AtomicBool::new(true),
                }),
                reply,
            }
        }
        fn network(&self) -> Network {
            let server = self.dns.address();
            Network {
                http: Arc::new(UreqHttp::new(false)),
                endpoints: Endpoints {
                    duckdns: self.provider.url(),
                    dynv6: self.provider.url(),
                },
                production: self.acme.directory(),
                staging: "http://127.0.0.1:9/directory".into(),
                name_servers: Arc::new(move |_| vec![vec![server]]),
                propagation: Propagation {
                    query_timeout: Duration::from_millis(200),
                    interval: Duration::from_millis(10),
                    limit: Duration::from_secs(2),
                    settle: Duration::from_millis(1),
                    no_answer: Duration::from_millis(100),
                    fallback: Duration::from_millis(1),
                },
                acme: acme::Timing {
                    poll: Duration::from_millis(5),
                    limit: Duration::from_secs(2),
                },
                tick: Duration::from_millis(20),
                settle: Duration::from_millis(10),
                local_dns: Some(local_dns::Paths::in_dir(
                    self.state.0.join("dnsmasq-shared.d"),
                )),
            }
        }
        fn write(&self, value: Value) {
            crate::admin::auth::private_file(
                &self.state.0.join(config::FILE),
                &serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        }
        fn configure(&self) {
            self.write(
                serde_json::json!({"version":1,"provider":"duckdns","name":NAME,
                "token":TOKEN,"agreeLetsEncryptTerms":true}),
            );
        }
        fn service(&self) -> PublicNameService {
            let service = PublicNameService::new(&self.state.0, self.network());
            service.start(self.host.clone()).unwrap();
            service
        }
        fn published(&self) -> Vec<String> {
            self.provider
                .requests()
                .iter()
                .filter_map(|r| r.path.split("&ip=").nth(1).map(str::to_owned))
                .collect()
        }
        fn new_orders(&self) -> usize {
            self.acme
                .log()
                .iter()
                .filter(|l| *l == "newOrder kid")
                .count()
        }
    }
    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn tesla_published_override_follows_only_established_name_addresses() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let service = world.service();
        wait_for("ready", || service.status().state == "ready");
        for address in ["100.88.0.1", "169.254.88.1"] {
            service.published_address_override(Some(address.parse().unwrap()));
            wait_for("Tesla address", || {
                service.status().address.as_deref() == Some(address)
                    && service.status().state == "ready"
            });
            assert!(world.published().contains(&address.to_string()));
            let hosts =
                std::fs::read_to_string(world.state.0.join("dnsmasq-shared.d/hotspot-hosts"))
                    .unwrap();
            assert_eq!(hosts, format!("{address} {NAME}\n"));
        }
        service.published_address_override(None);
        wait_for("normal address", || {
            service.status().address.as_deref() == Some("10.77.0.1")
                && service.status().state == "ready"
        });
        service.stop();
    }
    #[test]
    fn privacy_import_installs_before_a_running_worker_and_preserves_all_providers() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let source = world.service();
        wait_for("source certificate", || source.status().state == "ready");
        source.stop();
        let mut backup = source.privacy_export().unwrap();
        // The test CA is HTTP; transferred accounts obey Android's HTTPS-only validation.
        backup["publicNameAccount"]["directory"] = acme::PRODUCTION.into();
        backup["publicNameAccount"]["url"] = "https://ca.example/account/1".into();
        backup["providers"]["dynv6"] =
            serde_json::json!({"name":"second.example.org","token":"second-token"});
        let destination = TempDir::new("privacy-import");
        let target = PublicNameService::new(&destination.0, world.network());
        assert_eq!(target.admin_values()["privacyConfigAvailable"], false);
        target.start(world.host.clone()).unwrap();
        let import = privacy::decode(&backup, None, world.host.clock().unwrap())
            .unwrap_or_else(|e| panic!("{e:?}"));
        target.import_privacy(import).unwrap();
        wait_for("restored certificate", || target.status().state == "ready");
        assert_eq!(world.new_orders(), 1, "import must not order a replacement");
        let exported = target.privacy_export().unwrap();
        for key in [
            "publicName",
            "publicNameCertificate",
            "publicNameAccount",
            "providers",
        ] {
            assert_eq!(exported[key], backup[key], "{key}");
        }
        let store = Store::new(&destination.0);
        let meta = store.meta();
        assert_eq!(meta.provider.as_deref(), Some("duckdns"));
        assert_eq!(meta.acme.as_deref(), Some("production"));
        let leaf = cert::leaf(
            exported["publicNameCertificate"]["chainPem"]
                .as_str()
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(meta.not_before, Some(leaf.not_before));
        assert_eq!(meta.not_after, Some(leaf.not_after));
        assert_eq!(meta.cert_id, leaf.cert_id);
        assert_eq!(
            store.account_url_for(acme::PRODUCTION).as_deref(),
            Some("https://ca.example/account/1")
        );
        assert!(store.account_url_for(acme::STAGING).is_none());
        assert!(!Value::Object(target.admin_values())
            .to_string()
            .contains("second-token"));
        for file in [
            "cert.pem",
            "key.pem",
            "meta.json",
            "providers.json",
            "account.key",
            "account.directory",
            "account.url",
        ] {
            assert_eq!(
                std::fs::metadata(store.dir().join(file))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        target.stop();
    }
    #[test]
    fn privacy_sections_without_active_config_survive_reload_and_remove() {
        let state = TempDir::new("privacy-providers-only");
        let service = PublicNameService::new(&state.0, Network::internet(None));
        let value =
            serde_json::json!({"providers":{"dynv6":{"name":"car.example.org","token":"secret"}}});
        service
            .import_privacy(
                privacy::decode(&value, None, 1_795_000_000).unwrap_or_else(|e| panic!("{e:?}")),
            )
            .unwrap();
        service.inner.reload(false);
        assert_eq!(service.privacy_export().unwrap(), value);
        assert_eq!(service.admin_values()["privacyConfigAvailable"], true);
        service.remove().unwrap();
        assert_eq!(service.privacy_export().unwrap(), serde_json::json!({}));
    }
    #[test]
    fn ari_poll_is_cached_and_the_worker_renews_at_the_saved_time() {
        let world = World::new(FakeAcmeOptions {
            ari: Some(("2027-01-01T00:00:00Z".into(), "2027-01-02T00:00:00Z".into())),
            ..Default::default()
        });
        world.configure();
        let service = world.service();
        wait_for("ready", || service.status().state == "ready");
        service.renew();
        let store = Store::new(&world.state.0);
        wait_for("ARI poll", || store.meta().ari_next_poll.is_some());
        let polls = || {
            world
                .acme
                .log()
                .iter()
                .filter(|line| line.as_str() == "renewalInfo")
                .count()
        };
        let meta = store.meta();
        let now = world.host.clock().unwrap();
        assert_eq!(meta.ari_next_poll, Some(now + 21_600));
        let first_plan = meta.renew_at;
        let first_polls = polls();
        service.renew();
        wait_for("republish", || world.published().len() >= 3);
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(store.meta().renew_at, first_plan);
        assert_eq!(
            polls(),
            first_polls,
            "Retry-After prevents another ARI request"
        );
        service.stop();
        let mut meta = store.meta();
        meta.renew_at = Some(now + 10);
        store.save_meta(&meta).unwrap();
        service.start(world.host.clone()).unwrap();
        wait_for("settled", || service.status().state == "ready");
        *world.host.clock.lock().unwrap() = Some(now + 10);
        wait_for("renewal without a manual request or 12-hour wait", || {
            world.new_orders() == 2
        });
        service.stop();
    }
    #[test]
    fn a_frozen_child_pair_survives_a_new_certificate_or_removal() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let service = world.service();
        wait_for("ready", || service.status().state == "ready");
        service.stop();
        let snapshot = service
            .certificate_snapshot(&world.state.0)
            .unwrap()
            .unwrap();
        let chain = std::fs::read(&snapshot.cert).unwrap();
        service
            .inner
            .store
            .save_certificate(b"replacement", "replacement-key")
            .unwrap();
        assert_eq!(std::fs::read(&snapshot.cert).unwrap(), chain);
        let replacement = rcgen::generate_simple_self_signed(vec![NAME.into()]).unwrap();
        service
            .inner
            .store
            .save_certificate(
                replacement.cert.pem().as_bytes(),
                &replacement.key_pair.serialize_pem(),
            )
            .unwrap();
        let renewed = service
            .certificate_snapshot(&world.state.0)
            .unwrap()
            .unwrap();
        assert_ne!(renewed.cert, snapshot.cert);
        assert_ne!(std::fs::read(&renewed.cert).unwrap(), chain);
        assert_eq!(std::fs::read(&snapshot.cert).unwrap(), chain);
        service.remove().unwrap();
        assert_eq!(std::fs::read(&snapshot.cert).unwrap(), chain);
    }

    #[test]
    fn publishes_issues_serves_renews_the_address_and_removes_everything() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let service = world.service();
        let generation = service.generation();
        wait_for("ready", || service.status().state == "ready");
        let status = service.status();
        assert_eq!(status.name.as_deref(), Some(NAME));
        assert_eq!(status.address.as_deref(), Some("10.77.0.1"));
        let served = service.served().unwrap();
        assert_eq!(status.not_after, Some(served.not_after));
        assert_ne!(service.generation(), generation);
        assert_eq!(world.published(), ["10.77.0.1"]);
        assert_eq!(world.new_orders(), 1);
        assert_eq!(world.acme.violations(), Vec::<String>::new());
        assert!(world.acme.txt().lock().unwrap().is_none(), "TXT cleared");
        assert!(world.dns.queries() > 0);
        let dir = world.state.0.join(super::super::store::DIR);
        for file in [
            "account.key",
            "account.url",
            "cert.pem",
            "key.pem",
            "meta.json",
        ] {
            let mode = std::fs::metadata(dir.join(file))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{file}");
        }
        assert_eq!(served.cert, dir.join("cert.pem"));
        // The token appears in no status and no admin value.
        let shown = serde_json::to_string(&(service.status(), service.admin_values())).unwrap();
        assert!(!shown.contains(TOKEN));
        // A new address is published at once; the valid certificate stays.
        *world.host.address.lock().unwrap() = Some(Ipv4Addr::new(172, 31, 200, 1));
        wait_for("new address", || world.published().len() == 2);
        wait_for("ready again", || {
            service.status().state == "ready"
                && service.status().address.as_deref() == Some("172.31.200.1")
        });
        // Request certificate now: with a certificate that is not due only the address is sent.
        service.renew();
        wait_for("republished", || world.published().len() == 3);
        wait_for("ready after renew", || service.status().state == "ready");
        assert_eq!(world.new_orders(), 1);
        // Remove deletes the file, the certificate, its key and the account key.
        let before = service.generation();
        service.remove().unwrap();
        assert!(!world.state.0.join(config::FILE).exists());
        assert!(!dir.exists());
        assert_eq!(service.status().state, "off");
        assert!(service.served().is_none());
        assert_ne!(service.generation(), before);
        std::thread::sleep(Duration::from_millis(100));
        assert!(!dir.exists(), "nothing comes back after Remove");
        service.stop();
    }
    #[test]
    fn disabling_keeps_the_certificate_and_deleting_the_file_removes_it() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let service = world.service();
        wait_for("ready", || service.status().state == "ready");
        let dir = world.state.0.join(super::super::store::DIR);
        world.write(
            serde_json::json!({"version":1,"enabled":false,"provider":"duckdns",
            "name":NAME,"token":TOKEN,"agreeLetsEncryptTerms":true}),
        );
        wait_for("off", || service.status().state == "off");
        assert!(service.served().is_none());
        assert!(dir.join("cert.pem").exists());
        let requests = world.provider.requests().len();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            world.provider.requests().len(),
            requests,
            "no requests while off"
        );
        world.configure();
        wait_for("ready again", || service.status().state == "ready");
        assert_eq!(world.new_orders(), 1, "the kept certificate is reused");
        std::fs::remove_file(world.state.0.join(config::FILE)).unwrap();
        wait_for("removed", || {
            !dir.exists() && service.status().state == "off"
        });
        assert!(service.served().is_none());
        service.stop();
    }
    #[test]
    fn incomplete_waiting_and_address_states() {
        let world = World::new(FakeAcmeOptions::default());
        let service = world.service();
        assert_eq!(service.status().state, "off");
        world
            .write(serde_json::json!({"version":1,"provider":"duckdns","name":NAME,"token":TOKEN}));
        wait_for("incomplete", || service.status().state == "incomplete");
        world.write(
            serde_json::json!({"version":1,"provider":"duckdns","name":NAME,"token":TOKEN,
            "agreeLetsEncryptTerms":true,"nmae":"typo"}),
        );
        wait_for("file error", || {
            service.status().file_errors.get("nmae").map(String::as_str) == Some("unknown-key")
        });
        assert_eq!(service.status().state, "incomplete");
        *world.host.clock.lock().unwrap() = None;
        world.configure();
        wait_for("waiting for the clock", || {
            service.status().state == "waiting-network"
        });
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(service.status().state, "waiting-network");
        assert!(
            world.provider.requests().is_empty(),
            "nothing sent without a clock"
        );
        *world.host.clock.lock().unwrap() = Some(crate::platform::wall_clock().as_secs());
        *world.host.address.lock().unwrap() = Some(Ipv4Addr::new(8, 8, 8, 8));
        wait_for("no address", || {
            service.status().error.as_deref() == Some("no-address")
        });
        assert!(
            world.provider.requests().is_empty(),
            "only private addresses are published"
        );
        *world.host.address.lock().unwrap() = Some(Ipv4Addr::new(192, 168, 43, 1));
        wait_for("ready", || service.status().state == "ready");
        service.stop();
    }
    #[test]
    fn failures_back_off_and_rate_limits_are_obeyed_even_on_request() {
        let world = World::new(FakeAcmeOptions::default());
        *world.reply.lock().unwrap() = "KO";
        world.configure();
        let service = world.service();
        wait_for("token rejected", || {
            service.status().error.as_deref() == Some("token-rejected")
        });
        let now = crate::platform::wall_clock().as_secs();
        let next = service.status().next_attempt.unwrap();
        assert!((now + 55..=now + 61).contains(&next), "{next} vs {now}");
        let sent = world.provider.requests().len();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(world.provider.requests().len(), sent, "backing off");
        service.renew();
        wait_for("retried at once", || world.provider.requests().len() > sent);
        // Let's Encrypt's Retry-After holds certificate requests, also against Request
        // certificate now; the rate-limited newOrder still counts for the local guard.
        *world.reply.lock().unwrap() = "OK";
        world.acme.set_rate_limited(Some(3_600));
        service.renew();
        wait_for("rate limited", || {
            service.status().error.as_deref() == Some("acme-rate-limited")
        });
        let next = service.status().next_attempt.unwrap();
        assert!(next >= now + 3_590, "{next}");
        assert_eq!(world.new_orders(), 1);
        assert_eq!(world.acme.orders(), 0);
        let meta = Store::new(&world.state.0).meta();
        assert_eq!(meta.order_times(NAME).len(), 1);
        assert!(meta.retry_after.unwrap() >= now + 3_590);
        world.acme.set_rate_limited(None);
        let published = world.published().len();
        service.renew();
        wait_for("republished", || world.published().len() == published + 1);
        // Address updates go on during the hold.
        *world.host.address.lock().unwrap() = Some(Ipv4Addr::new(10, 77, 5, 1));
        wait_for("new address published", || {
            world.published().last().map(String::as_str) == Some("10.77.5.1")
        });
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(world.new_orders(), 1, "no order before Retry-After");
        assert_eq!(service.status().error.as_deref(), Some("acme-rate-limited"));
        service.stop();
    }
    #[test]
    fn a_provider_retry_after_blocks_every_request_to_it() {
        let world = World::new(FakeAcmeOptions::default());
        *world.reply.lock().unwrap() = "429";
        world.configure();
        let service = world.service();
        wait_for("provider rate limited", || {
            service.status().error.as_deref() == Some("provider-rate-limited")
        });
        let now = crate::platform::wall_clock().as_secs();
        let next = service.status().next_attempt.unwrap();
        assert!((now + 590..=now + 601).contains(&next), "{next}");
        let sent = world.provider.requests().len();
        service.renew();
        *world.host.address.lock().unwrap() = Some(Ipv4Addr::new(10, 77, 6, 1));
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(
            world.provider.requests().len(),
            sent,
            "held across renew and new addresses"
        );
        assert_eq!(
            service.status().error.as_deref(),
            Some("provider-rate-limited")
        );
        assert_eq!(world.new_orders(), 0);
        service.stop();
    }
    #[test]
    fn unanswered_requests_without_a_network_wait_for_it() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let mut network = world.network();
        network.endpoints.duckdns = "http://127.0.0.1:9".into();
        world.host.online.store(false, Ordering::SeqCst);
        let service = PublicNameService::new(&world.state.0, network);
        service.start(world.host.clone()).unwrap();
        wait_for("waiting for the network", || {
            service.status().state == "waiting-network"
        });
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(service.status().state, "waiting-network");
        // Back online: tried at once, not after the backoff; still unreachable is an error.
        world.host.online.store(true, Ordering::SeqCst);
        wait_for("unreachable", || {
            service.status().error.as_deref() == Some("provider-unreachable")
        });
        service.stop();
    }
    #[test]
    fn a_failing_update_does_not_hide_a_name_that_still_works() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let service = world.service();
        wait_for("ready", || service.status().state == "ready");
        assert_eq!(service.status().trusted_name.as_deref(), Some(NAME));
        *world.reply.lock().unwrap() = "KO";
        service.renew();
        wait_for("rejected", || {
            service.status().error.as_deref() == Some("token-rejected")
        });
        assert_eq!(service.status().trusted_name.as_deref(), Some(NAME));
        // A new address the provider refused is not the name's address: not trusted.
        *world.host.address.lock().unwrap() = Some(Ipv4Addr::new(10, 77, 7, 1));
        wait_for("untrusted", || {
            let status = service.status();
            status.trusted_name.is_none() && status.state == "error"
        });
        service.stop();
    }
    #[test]
    fn a_staging_certificate_is_replaced_when_production_is_chosen() {
        let world = World::new(FakeAcmeOptions::default());
        world.write(
            serde_json::json!({"version":1,"provider":"duckdns","name":NAME,
            "token":TOKEN,"agreeLetsEncryptTerms":true,"acme":"staging"}),
        );
        let mut network = world.network();
        network.staging = world.acme.directory();
        let service = PublicNameService::new(&world.state.0, network);
        service.start(world.host.clone()).unwrap();
        wait_for("ready on staging", || service.status().state == "ready");
        assert_eq!(world.new_orders(), 1);
        world.configure();
        wait_for("a production order", || world.new_orders() == 2);
        wait_for("ready on production", || service.status().state == "ready");
        assert_eq!(
            Store::new(&world.state.0).meta().acme.as_deref(),
            Some("production")
        );
        service.stop();
    }
    #[test]
    fn the_local_guard_holds_a_fourth_order_within_a_day() {
        let world = World::new(FakeAcmeOptions::default());
        world.configure();
        let now = crate::platform::wall_clock().as_secs();
        let store = Store::new(&world.state.0);
        let orders = [now - 7_200, now - 3_600, now - 60];
        store
            .save_meta(&Meta {
                orders: orders
                    .iter()
                    .map(|at| OrderRecord {
                        name: NAME.into(),
                        at: *at,
                    })
                    .collect(),
                last_error: Some(ErrorRecord {
                    code: "dns-not-updated".into(),
                    detail: None,
                }),
                ..Default::default()
            })
            .unwrap();
        let service = world.service();
        wait_for("held", || service.status().state == "error");
        let status = service.status();
        assert_eq!(status.error.as_deref(), Some("dns-not-updated"));
        assert_eq!(status.next_attempt, Some(now - 7_200 + DAY));
        assert_eq!(world.new_orders(), 0);
        // The guard holds certificate requests only: the address was published.
        assert_eq!(world.published(), ["10.77.0.1"]);
        service.stop();
    }
    #[test]
    fn admin_updates_write_the_file_and_never_return_the_token() {
        let world = World::new(FakeAcmeOptions::default());
        let service = PublicNameService::new(&world.state.0, world.network());
        let values = |v: Value| v.as_object().unwrap().clone();
        service
            .update(&values(serde_json::json!({"publicNameEnabled": true})))
            .unwrap();
        assert_eq!(service.status().state, "incomplete");
        service
            .update(&values(
                serde_json::json!({"publicNameProvider":"duckdns","publicName":NAME,
                "publicNameToken":TOKEN,"publicNameAgreeTerms":true}),
            ))
            .unwrap();
        let written: Value =
            serde_json::from_slice(&std::fs::read(world.state.0.join(config::FILE)).unwrap())
                .unwrap();
        assert_eq!(written["token"], TOKEN);
        assert_eq!(written["version"], 1);
        let mode = std::fs::metadata(world.state.0.join(config::FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        service
            .update(&values(serde_json::json!({"publicNameToken": ""})))
            .unwrap();
        let shown = service.admin_values();
        assert_eq!(shown["publicNameTokenSet"], true);
        assert_eq!(shown["publicNameToken"], "");
        assert!(!Value::Object(shown).to_string().contains(TOKEN));
        let errors = service
            .update(&values(serde_json::json!({"publicNameProvider": "desec"})))
            .unwrap_err();
        assert_eq!(errors["publicNameProvider"], "error.invalid");
        assert!(service.remove().is_ok());
        assert_eq!(service.status().state, "off");
    }
    #[test]
    fn the_local_dns_answer_follows_the_hotspot_and_the_configuration() {
        let world = World::new(FakeAcmeOptions::default());
        let service = PublicNameService::new(&world.state.0, world.network());
        let dir = world.state.0.join("dnsmasq-shared.d");
        let conf = local_dns::path(&dir);
        let hosts = dir.join("hotspot-hosts");
        // Off: nothing written, also when the hotspot starts.
        service.before_hotspot(Ipv4Addr::new(10, 77, 0, 1));
        assert!(!conf.exists());
        world.configure();
        service.inner.reload(false);
        assert!(std::fs::read_to_string(&hosts)
            .unwrap()
            .contains(&format!("10.77.0.1 {NAME}\n")));
        // The next activation brings its own address.
        service.before_hotspot(Ipv4Addr::new(10, 77, 3, 1));
        assert!(std::fs::read_to_string(&hosts)
            .unwrap()
            .contains("10.77.3.1 "));
        // A separately selected browser interface updates DNS, and losing its IP removes it.
        service.head_unit_address(Some(Ipv4Addr::new(192, 0, 2, 2)));
        assert!(std::fs::read_to_string(&hosts)
            .unwrap()
            .contains("192.0.2.2 "));
        service.head_unit_address(None);
        assert!(!conf.exists());
        service.head_unit_address(Some(Ipv4Addr::new(192, 0, 2, 2)));
        // Disabled or removed: the answer goes.
        world.write(
            serde_json::json!({"version":1,"enabled":false,"provider":"duckdns",
            "name":NAME,"token":TOKEN,"agreeLetsEncryptTerms":true}),
        );
        service.inner.reload(false);
        assert!(!conf.exists());
        world.configure();
        service.inner.reload(false);
        assert!(conf.exists());
        service.remove().unwrap();
        assert!(!conf.exists());
    }
    #[test]
    fn restart_rule_waits_for_the_session_unless_the_served_certificate_expires_soon() {
        let now = 1_800_000_000;
        let served = |not_after: u64| Served {
            name: NAME.into(),
            cert: "c".into(),
            key: "k".into(),
            not_after,
            fingerprint: not_after.to_string(),
        };
        let (old, new) = (served(now + 30 * DAY), served(now + 90 * DAY));
        assert!(!should_restart(Some(&old), Some(&old), false, Some(now)));
        assert!(should_restart(None, Some(&new), false, Some(now)));
        assert!(!should_restart(None, Some(&new), true, Some(now)));
        assert!(!should_restart(Some(&old), Some(&new), true, Some(now)));
        assert!(!should_restart(Some(&old), None, true, Some(now)));
        let expiring = served(now + 2 * DAY);
        assert!(should_restart(Some(&expiring), Some(&new), true, Some(now)));
        assert!(should_restart(Some(&expiring), None, true, Some(now)));
        assert!(
            !should_restart(Some(&expiring), None, true, None),
            "no judgement without a clock"
        );
    }
}
