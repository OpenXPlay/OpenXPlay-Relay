// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Admin authentication contract tests.
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    net::IpAddr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
pub const ITERATIONS: u32 = 120_000;
/// Session lifetime on the monotonic clock; a reboot ends a session sooner (see [`BootSessions`]).
pub const SESSION_SECONDS: u64 = 30 * 24 * 3600;
#[derive(Clone, Serialize, Deserialize)]
pub struct PasswordHash {
    pub salt: [u8; 16],
    pub hash: [u8; 32],
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AuthState {
    pub password: Option<PasswordHash>,
    /// Earlier releases saved wall-clock expiries under `sessions`. Loading ignores that key, so
    /// those sessions end and the password stays.
    #[serde(default, rename = "bootSessions")]
    pub sessions: BootSessions,
}
/// Session expiries in seconds on one boot's monotonic clock, keyed by token hash. The relay may
/// run without an RTC or network time, so its wall clock can be hours off and can step: a
/// wall-clock expiry could end every session at once or keep one for years. The monotonic clock
/// never steps, and the boot id tells whether a saved expiry is on the current clock. A relay
/// restart keeps the sessions; a reboot ends them, because how long the relay was off is unknown.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct BootSessions {
    /// The boot whose monotonic clock the expiries use.
    pub boot: String,
    pub expiries: BTreeMap<String, u64>,
}
pub trait AuthStore: Send {
    fn load(&self) -> io::Result<AuthState>;
    fn save(&self, state: &AuthState) -> io::Result<()>;
}
pub struct FileAuthStore(pub PathBuf);
impl AuthStore for FileAuthStore {
    fn load(&self) -> io::Result<AuthState> {
        match fs::read(&self.0) {
            Ok(b) => {
                fs::set_permissions(&self.0, fs::Permissions::from_mode(0o600))?;
                Ok(serde_json::from_slice(&b)?)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(AuthState::default()),
            Err(e) => Err(e),
        }
    }
    fn save(&self, state: &AuthState) -> io::Result<()> {
        private_file(&self.0, &serde_json::to_vec(state)?)
    }
}
/// Atomic replacement with private mode, including when replacing a permissive old file.
pub fn private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing state directory"))?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let tmp = path.with_extension(format!("tmp-{}", random_hex(8)?));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
fn random<const N: usize>() -> io::Result<[u8; N]> {
    let mut b = [0; N];
    fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b)
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
pub fn random_hex(bytes: usize) -> io::Result<String> {
    let mut b = vec![0; bytes];
    fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(hex(&b))
}
pub fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    assert!(iterations > 0);
    let base = Hmac::<Sha256>::new_from_slice(password).expect("HMAC accepts any key length");
    let mut first = base.clone();
    first.update(salt);
    first.update(&1u32.to_be_bytes());
    let mut u: [u8; 32] = first.finalize().into_bytes().into();
    let mut result = u;
    for _ in 1..iterations {
        let mut mac = base.clone();
        mac.update(&u);
        u = mac.finalize().into_bytes().into();
        for (out, byte) in result.iter_mut().zip(u) {
            *out ^= byte;
        }
    }
    result
}
fn equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |v, (a, b)| v | (a ^ b)) == 0
}
/// Seconds on the system's monotonic clock, which counts from boot, never steps and is shared by
/// every process of one boot. Admin expiries, deadlines and rate limits compare against it, never
/// against the wall clock.
pub fn monotonic_seconds() -> u64 {
    crate::boot_timing::monotonic_millis() / 1000
}
pub enum LoginError {
    Password,
    RateLimit(u64),
    Storage(io::Error),
}
pub enum PasswordError {
    Current,
    Format,
    Storage(io::Error),
}
pub struct AdminAuth {
    store: Box<dyn AuthStore>,
    state: AuthState,
    failures: BTreeMap<IpAddr, (u8, u64)>,
}
impl AdminAuth {
    /// Every `now` this takes is in [`monotonic_seconds`].
    pub fn new(store: Box<dyn AuthStore>) -> io::Result<Self> {
        Self::for_boot(store, crate::boot_timing::boot_id())
    }
    /// Loads the state, keeping only sessions saved during `boot`. An unknown (empty) boot keeps
    /// none.
    pub fn for_boot(store: Box<dyn AuthStore>, boot: String) -> io::Result<Self> {
        let mut state = store.load()?;
        if boot.is_empty() || state.sessions.boot != boot {
            state.sessions = BootSessions {
                boot,
                expiries: BTreeMap::new(),
            };
        }
        Ok(Self {
            store,
            state,
            failures: BTreeMap::new(),
        })
    }
    pub fn default_password(&self) -> bool {
        self.state.password.is_none()
    }
    fn verify(&self, password: &str) -> bool {
        match &self.state.password {
            None => equal(password.as_bytes(), b"openxplay"),
            Some(p) => equal(&pbkdf2(password.as_bytes(), &p.salt, ITERATIONS), &p.hash),
        }
    }
    pub fn ready(&self, token: &str, now: u64) -> bool {
        self.state
            .sessions
            .expiries
            .get(&hex(&Sha256::digest(token.as_bytes())))
            .is_some_and(|expiry| *expiry > now)
    }
    pub fn login(
        &mut self,
        address: IpAddr,
        password: &str,
        now: u64,
    ) -> Result<String, LoginError> {
        self.failures.retain(|_, (_, until)| *until > now);
        let failure = self
            .failures
            .entry(address)
            .or_insert((0, now.saturating_add(60)));
        if failure.0 >= 5 {
            return Err(LoginError::RateLimit(failure.1 - now));
        }
        if !self.verify(password) {
            self.failures.get_mut(&address).unwrap().0 += 1;
            return Err(LoginError::Password);
        }
        self.failures.remove(&address);
        let token = random_hex(32).map_err(LoginError::Storage)?;
        let mut next = self.state.clone();
        next.sessions.expiries.retain(|_, expiry| *expiry > now);
        next.sessions.expiries.insert(
            hex(&Sha256::digest(token.as_bytes())),
            now.saturating_add(SESSION_SECONDS),
        );
        self.store.save(&next).map_err(LoginError::Storage)?;
        self.state = next;
        Ok(token)
    }
    pub fn logout(&mut self, token: &str) -> io::Result<()> {
        let mut next = self.state.clone();
        next.sessions
            .expiries
            .remove(&hex(&Sha256::digest(token.as_bytes())));
        self.store.save(&next)?;
        self.state = next;
        Ok(())
    }
    pub fn change_password(&mut self, current: &str, new: &str) -> Result<(), PasswordError> {
        if !self.verify(current) {
            return Err(PasswordError::Current);
        }
        if new.chars().count() < 8 {
            return Err(PasswordError::Format);
        }
        let mut next = self.state.clone();
        next.password = if new == "openxplay" {
            None
        } else {
            let salt = random().map_err(PasswordError::Storage)?;
            Some(PasswordHash {
                salt,
                hash: pbkdf2(new.as_bytes(), &salt, ITERATIONS),
            })
        };
        self.store.save(&next).map_err(PasswordError::Storage)?;
        self.state = next;
        Ok(())
    }
    pub fn reset(&mut self) -> io::Result<()> {
        let next = AuthState {
            password: None,
            sessions: BootSessions {
                boot: self.state.sessions.boot.clone(),
                expiries: BTreeMap::new(),
            },
        };
        self.store.save(&next)?;
        self.state = next;
        self.failures.clear();
        Ok(())
    }
}
/// IPv4-mapped IPv6 peers are checked as IPv4. Never trust forwarded headers.
pub fn private_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(a) => a.is_private() || a.is_loopback() || a.is_link_local(),
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => private_address(IpAddr::V4(v4)),
            None => {
                a.is_loopback()
                    || (a.segments()[0] & 0xfe00 == 0xfc00)
                    || (a.segments()[0] & 0xffc0 == 0xfe80)
            }
        },
    }
}
