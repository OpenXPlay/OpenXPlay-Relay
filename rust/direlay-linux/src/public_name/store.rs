// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! The `public-name/` directory (0700) in the state directory: `account.key`, `account.url`,
//! `cert.pem` (full chain), `key.pem` and `meta.json`. Every file is written atomically with mode
//! 0600 through [`crate::admin::auth::private_file`].
use super::{
    config::{self, Config, Provider},
    jws::AccountKey,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

pub const DIR: &str = "public-name";
const META_MAX: u64 = 64 * 1024;

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSetup {
    pub name: String,
    pub token: String,
}
pub type ProviderSetups = BTreeMap<String, ProviderSetup>;

fn parse_providers(bytes: &[u8]) -> Option<ProviderSetups> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    (value.get("version")?.as_u64()? == 1).then_some(())?;
    let entries = value.get("providers")?.as_object()?;
    let mut setups = ProviderSetups::new();
    for provider in Provider::ALL {
        let Some(entry) = entries.get(provider.id()) else {
            continue;
        };
        let name = entry.get("name")?.as_str()?;
        let token = entry.get("token")?.as_str()?;
        if (!name.is_empty() && config::normalize_name(name, Some(provider)).is_none())
            || (!token.is_empty() && !config::valid_token(token, Some(provider)))
        {
            return None;
        }
        setups.insert(
            provider.id().into(),
            ProviderSetup {
                name: name.into(),
                token: token.into(),
            },
        );
    }
    Some(setups)
}

pub fn remember_provider(setups: &mut ProviderSetups, config: &Config) {
    if let Some(provider) = config.provider {
        setups.insert(
            provider.id().into(),
            ProviderSetup {
                name: config.name.clone().unwrap_or_default(),
                token: config.token.clone().unwrap_or_default(),
            },
        );
    }
}

/// Names and token-present flags only; both supported providers are always listed.
pub fn provider_values(setups: &ProviderSetups) -> serde_json::Value {
    Provider::ALL
        .into_iter()
        .map(|p| {
            let setup = setups.get(p.id());
            (
                p.id().into(),
                serde_json::json!({
                    "name": setup.map(|s| s.name.as_str()).unwrap_or_default(),
                    "tokenSet": setup.is_some_and(|s| !s.token.is_empty()),
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRecord {
    pub name: String,
    pub at: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorRecord {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
/// What the relay remembers about its name and certificate. Times are wall-clock Unix seconds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Meta {
    /// The certificate's name, provider and CA environment when it was issued.
    pub name: Option<String>,
    pub provider: Option<String>,
    pub acme: Option<String>,
    pub not_before: Option<u64>,
    pub not_after: Option<u64>,
    pub issued_at: Option<u64>,
    /// The ARI identifier of the certificate, and the renewal time chosen in its window.
    pub cert_id: Option<String>,
    pub ari_window: Option<(u64, u64)>,
    pub renew_at: Option<u64>,
    /// The address last published, and when.
    pub address: Option<String>,
    pub published_at: Option<u64>,
    /// New orders, for the local guard.
    pub orders: Vec<OrderRecord>,
    /// The CA's `Retry-After`, as the time before which no order is sent.
    pub retry_after: Option<u64>,
    pub last_error: Option<ErrorRecord>,
}
impl Meta {
    pub fn order_times(&self, name: &str) -> Vec<u64> {
        self.orders
            .iter()
            .filter(|o| o.name == name)
            .map(|o| o.at)
            .collect()
    }
    /// Records an order and forgets those older than two days.
    pub fn add_order(&mut self, name: &str, at: u64) {
        self.orders.retain(|o| o.at + 2 * super::schedule::DAY > at);
        self.orders.push(OrderRecord {
            name: name.into(),
            at,
        });
    }
}

#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}
impl Store {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            dir: state_dir.join(DIR),
        }
    }
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn cert_path(&self) -> PathBuf {
        self.dir.join("cert.pem")
    }
    pub fn key_path(&self) -> PathBuf {
        self.dir.join("key.pem")
    }
    fn write(&self, file: &str, bytes: &[u8]) -> io::Result<()> {
        crate::admin::auth::private_file(&self.dir.join(file), bytes)
    }
    fn read(&self, file: &str, limit: u64) -> Option<Vec<u8>> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(self.dir.join(file))
            .ok()?
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        (bytes.len() as u64 <= limit).then_some(bytes)
    }
    /// Older installs seed the provider store without rewriting the active version-1 file.
    pub fn providers(&self, active: Option<&Config>) -> io::Result<ProviderSetups> {
        if let Some(setups) = self
            .read("providers.json", config::MAX_BYTES as u64)
            .and_then(|bytes| parse_providers(&bytes))
        {
            return Ok(setups);
        }
        let mut setups = ProviderSetups::new();
        if let Some(active) = active {
            remember_provider(&mut setups, active);
            self.save_providers(&setups)?;
        }
        Ok(setups)
    }
    pub fn save_providers(&self, setups: &ProviderSetups) -> io::Result<()> {
        self.write(
            "providers.json",
            &serde_json::to_vec(&serde_json::json!({
                "version": 1, "providers": setups,
            }))?,
        )
    }
    /// The account key, created on first use.
    pub fn account_key(&self) -> io::Result<AccountKey> {
        if let Some(key) = self
            .read("account.key", 16 * 1024)
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|pem| AccountKey::from_pem(&pem))
        {
            return Ok(key);
        }
        let key = AccountKey::generate()?;
        self.write("account.key", key.pem().as_bytes())?;
        // A new key has no account yet.
        let _ = std::fs::remove_file(self.dir.join("account.url"));
        Ok(key)
    }
    pub fn account_url(&self) -> Option<String> {
        let url = String::from_utf8(self.read("account.url", 4096)?).ok()?;
        let url = url.trim();
        (!url.is_empty()).then(|| url.to_string())
    }
    pub fn save_account_url(&self, url: &str) -> io::Result<()> {
        self.write("account.url", format!("{url}\n").as_bytes())
    }
    pub fn meta(&self) -> Meta {
        self.read("meta.json", META_MAX)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save_meta(&self, meta: &Meta) -> io::Result<()> {
        self.write("meta.json", &serde_json::to_vec_pretty(meta)?)
    }
    /// The chain and its key, as stored.
    pub fn certificate(&self) -> Option<(Vec<u8>, String)> {
        let chain = self.read("cert.pem", 64 * 1024)?;
        let key = String::from_utf8(self.read("key.pem", 16 * 1024)?).ok()?;
        Some((chain, key))
    }
    /// Stores a new chain and key. The key goes first: a crash in between leaves a pair that
    /// fails the key check and is ordered again, never a chain served with a stale key.
    pub fn save_certificate(&self, chain: &[u8], key_pem: &str) -> io::Result<()> {
        let _ = std::fs::remove_file(self.cert_path());
        self.write("key.pem", key_pem.as_bytes())?;
        self.write("cert.pem", chain)
    }
    pub fn exists(&self) -> bool {
        self.dir.exists()
    }
    /// Deletes the certificate, its key, the account key and everything else kept here.
    pub fn remove_all(&self) -> io::Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::testing::TempDir;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn providers_seed_read_validate_and_write_the_android_format() {
        let state = TempDir::new("providers");
        let store = Store::new(&state.0);
        assert!(store.providers(None).unwrap().is_empty());
        assert!(!store.exists());
        let bytes = br#"{"version":1,"provider":"duckdns","name":"a.duckdns.org","token":"a1b2c3d4-0000-0000-0000-000000000000"}"#;
        let active = config::parse(bytes);
        let path = state.0.join(config::FILE);
        std::fs::write(&path, bytes).unwrap();
        let seed = store.providers(Some(&active)).unwrap();
        assert_eq!(seed["duckdns"].name, "a.duckdns.org");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let providers = store.dir().join("providers.json");
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&providers).unwrap()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"version":1,"providers":{"duckdns":{"name":active.name,"token":active.token}}})
        );
        assert_eq!(
            std::fs::metadata(&providers).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(store.dir()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for broken in [
            b"not json".to_vec(),
            serde_json::to_vec(&serde_json::json!({"version":2,"providers":{}})).unwrap(),
            serde_json::to_vec(&serde_json::json!({"version":1})).unwrap(),
            vec![b' '; config::MAX_BYTES + 1],
        ] {
            std::fs::write(&providers, broken).unwrap();
            assert!(store.providers(Some(&active)).unwrap() == seed);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        for (provider, entry) in [
            (
                "duckdns",
                serde_json::json!({"name":"a.dynv6.net","token":""}),
            ),
            (
                "duckdns",
                serde_json::json!({"name":"","token":"not-a-uuid"}),
            ),
            (
                "dynv6",
                serde_json::json!({"name":"bad_name.net","token":""}),
            ),
            ("dynv6", serde_json::json!({"name":"","token":"has space"})),
            ("dynv6", serde_json::json!({"name":null,"token":""})),
            ("dynv6", serde_json::json!({"name":""})),
        ] {
            let value = serde_json::json!({"version":1,"providers":{provider:entry}});
            std::fs::write(&providers, serde_json::to_vec(&value).unwrap()).unwrap();
            assert!(store.providers(Some(&active)).unwrap() == seed);
        }
        let mut both = seed;
        both.insert(
            "dynv6".into(),
            ProviderSetup {
                name: "car.example.net".into(),
                token: "t~1".into(),
            },
        );
        store.save_providers(&both).unwrap();
        assert!(store.providers(None).unwrap() == both);
        let values = provider_values(&both);
        assert_eq!(
            values["dynv6"],
            serde_json::json!({"name":"car.example.net","tokenSet":true})
        );
        assert!(!values.to_string().contains("t~1"));
        both.insert("duckdns".into(), ProviderSetup::default());
        store.save_providers(&both).unwrap();
        assert!(store.providers(None).unwrap() == both);
    }
    #[test]
    fn files_are_private_and_removal_deletes_them_all() {
        let state = TempDir::new("store");
        let store = Store::new(&state.0);
        assert!(!store.exists() && store.certificate().is_none() && store.account_url().is_none());
        assert_eq!(store.meta(), Meta::default());
        let key = store.account_key().unwrap();
        assert_eq!(store.account_key().unwrap().thumbprint(), key.thumbprint());
        store.save_account_url("https://ca/acct/1").unwrap();
        assert_eq!(store.account_url().as_deref(), Some("https://ca/acct/1"));
        store.save_certificate(b"chain", "key").unwrap();
        assert_eq!(store.certificate(), Some((b"chain".to_vec(), "key".into())));
        let mut meta = Meta::default();
        meta.add_order("a.duckdns.org", 10);
        meta.add_order("b.duckdns.org", 20 + 2 * crate::public_name::schedule::DAY);
        meta.add_order("a.duckdns.org", 30 + 2 * crate::public_name::schedule::DAY);
        assert_eq!(
            meta.order_times("a.duckdns.org"),
            [30 + 2 * crate::public_name::schedule::DAY]
        );
        store.save_meta(&meta).unwrap();
        assert_eq!(store.meta(), meta);
        let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(store.dir().to_path_buf()), 0o700);
        for file in [
            "account.key",
            "account.url",
            "cert.pem",
            "key.pem",
            "meta.json",
        ] {
            assert_eq!(mode(store.dir().join(file)), 0o600, "{file}");
        }
        // A new account key forgets the old account.
        std::fs::write(store.dir().join("account.key"), "broken").unwrap();
        store.account_key().unwrap();
        assert!(store.account_url().is_none());
        store.remove_all().unwrap();
        assert!(!store.exists());
        store.remove_all().unwrap();
    }
}
