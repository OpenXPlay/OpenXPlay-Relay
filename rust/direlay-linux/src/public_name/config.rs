// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! `public-name.json`: the trusted-HTTPS configuration, in the format the Android relay imports.
//! It lives outside `settings.json`, which rejects unknown keys and must survive a rollback.
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

pub const FILE: &str = "public-name.json";
/// Larger files are rejected unread.
pub const MAX_BYTES: usize = 16 * 1024;
/// The key under which errors of the whole file (`too-large`, `not-json`) are reported.
pub const FILE_KEY: &str = "file";
/// The file keys the admin page writes.
const ADMIN_KEYS: [&str; 5] = [
    "enabled",
    "provider",
    "name",
    "token",
    "agreeLetsEncryptTerms",
];
const KEYS: [&str; 7] = [
    "version",
    "enabled",
    "provider",
    "name",
    "token",
    "agreeLetsEncryptTerms",
    "acme",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    DuckDns,
    Dynv6,
}
impl Provider {
    pub const ALL: [Provider; 2] = [Provider::DuckDns, Provider::Dynv6];
    pub fn id(self) -> &'static str {
        match self {
            Provider::DuckDns => "duckdns",
            Provider::Dynv6 => "dynv6",
        }
    }
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AcmeEnvironment {
    #[default]
    Production,
    /// Let's Encrypt staging: for testing, its certificates are not trusted.
    Staging,
}
impl AcmeEnvironment {
    pub fn id(self) -> &'static str {
        match self {
            AcmeEnvironment::Production => "production",
            AcmeEnvironment::Staging => "staging",
        }
    }
    pub fn parse(id: &str) -> Option<Self> {
        [AcmeEnvironment::Production, AcmeEnvironment::Staging]
            .into_iter()
            .find(|e| e.id() == id)
    }
}

/// A configuration as read: the fields that passed their rules, plus an error code per key
/// (`missing`, `invalid`, `unknown-key`, `bad-version`, `too-large`, `not-json`).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub provider: Option<Provider>,
    pub name: Option<String>,
    pub token: Option<String>,
    pub agree_terms: bool,
    pub acme: AcmeEnvironment,
    /// Whether the file names `acme` itself; a rewrite keeps the key only then.
    pub acme_set: bool,
    pub errors: BTreeMap<String, String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            // An imported or hand-written file means "use it".
            enabled: true,
            provider: None,
            name: None,
            token: None,
            agree_terms: false,
            acme: AcmeEnvironment::Production,
            acme_set: false,
            errors: BTreeMap::new(),
        }
    }
}
impl Config {
    /// Every field present and valid, and the terms agreed: requests may start.
    pub fn complete(&self) -> bool {
        self.errors.is_empty()
            && self.provider.is_some()
            && self.name.is_some()
            && self.token.is_some()
            && self.agree_terms
    }
    /// Errors the admin page cannot fix by filling in its fields (unknown keys, the version, the
    /// `acme` value, a file that is not JSON or too large): the file must be fixed by hand.
    pub fn needs_hand_fix(&self) -> bool {
        self.errors.iter().any(|(key, code)| match code.as_str() {
            "missing" => false,
            "invalid" => !ADMIN_KEYS.contains(&key.as_str()),
            _ => true,
        })
    }
    /// The file text, keys in the documented order.
    pub fn to_json(&self) -> Vec<u8> {
        let mut lines = vec![
            "  \"version\": 1".to_string(),
            format!("  \"enabled\": {}", self.enabled),
        ];
        let string = |s: &str| Value::String(s.into()).to_string();
        if let Some(provider) = self.provider {
            lines.push(format!("  \"provider\": {}", string(provider.id())));
        }
        if let Some(name) = &self.name {
            lines.push(format!("  \"name\": {}", string(name)));
        }
        if let Some(token) = &self.token {
            lines.push(format!("  \"token\": {}", string(token)));
        }
        lines.push(format!("  \"agreeLetsEncryptTerms\": {}", self.agree_terms));
        if self.acme_set {
            lines.push(format!("  \"acme\": {}", string(self.acme.id())));
        }
        format!("{{\n{}\n}}\n", lines.join(",\n")).into_bytes()
    }
}

/// Lower-cases `name` and checks it: LDH labels, at most 253 characters, no trailing dot, and for
/// DuckDNS exactly `<label>.duckdns.org`. A dynv6 name is the zone apex its token manages
/// (normally `<label>.dynv6.net`; own zones too).
pub fn normalize_name(name: &str, provider: Option<Provider>) -> Option<String> {
    if !name.is_ascii() {
        return None;
    }
    let name = name.to_ascii_lowercase();
    let labels: Vec<_> = name.split('.').collect();
    let ldh = |label: &&str| {
        (1..=63).contains(&label.len())
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    };
    if name.len() > 253 || labels.len() < 2 || !labels.iter().all(ldh) {
        return None;
    }
    if provider == Some(Provider::DuckDns) && (labels.len() != 3 || !name.ends_with(".duckdns.org"))
    {
        return None;
    }
    Some(name)
}

/// 1–256 printable ASCII characters without spaces; a DuckDNS token is a UUID.
pub fn valid_token(token: &str, provider: Option<Provider>) -> bool {
    (1..=256).contains(&token.len())
        && token.bytes().all(|b| (0x21..=0x7e).contains(&b))
        && (provider != Some(Provider::DuckDns) || uuid(token))
}
fn uuid(token: &str) -> bool {
    let groups: Vec<_> = token.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Parses the file text. Never fails: problems become per-key errors.
pub fn parse(bytes: &[u8]) -> Config {
    let mut config = Config::default();
    let errors = &mut config.errors;
    if bytes.len() > MAX_BYTES {
        errors.insert(FILE_KEY.into(), "too-large".into());
        return config;
    }
    let Ok(Value::Object(object)) = serde_json::from_slice::<Value>(bytes) else {
        errors.insert(FILE_KEY.into(), "not-json".into());
        return config;
    };
    for key in object.keys().filter(|k| !KEYS.contains(&k.as_str())) {
        errors.insert(key.clone(), "unknown-key".into());
    }
    match object.get("version") {
        None => {
            errors.insert("version".into(), "missing".into());
        }
        Some(v) if v.as_u64() == Some(1) => {}
        Some(_) => {
            errors.insert("version".into(), "bad-version".into());
        }
    }
    let mut invalid = |key: &str| {
        errors.insert(key.into(), "invalid".into());
    };
    match object.get("enabled") {
        None => {}
        Some(Value::Bool(b)) => config.enabled = *b,
        Some(_) => invalid("enabled"),
    }
    match object.get("agreeLetsEncryptTerms") {
        None => {}
        Some(Value::Bool(b)) => config.agree_terms = *b,
        Some(_) => invalid("agreeLetsEncryptTerms"),
    }
    match object.get("acme") {
        None => {}
        Some(v) => match v.as_str().and_then(AcmeEnvironment::parse) {
            Some(acme) => {
                config.acme = acme;
                config.acme_set = true;
            }
            None => invalid("acme"),
        },
    }
    match object.get("provider") {
        None => {}
        Some(v) => match v.as_str().and_then(Provider::parse) {
            Some(p) => config.provider = Some(p),
            None => invalid("provider"),
        },
    }
    match object.get("name") {
        None => {}
        Some(v) => match v.as_str().and_then(|s| normalize_name(s, config.provider)) {
            Some(name) => config.name = Some(name),
            None => invalid("name"),
        },
    }
    match object.get("token") {
        None => {}
        Some(v) => match v.as_str().filter(|t| valid_token(t, config.provider)) {
            Some(token) => config.token = Some(token.into()),
            None => invalid("token"),
        },
    }
    for key in ["provider", "name", "token"] {
        if !object.contains_key(key) {
            config.errors.insert(key.into(), "missing".into());
        }
    }
    config
}

/// What identifies one version of the file on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    modified: (i64, i64),
    length: u64,
    inode: u64,
}

/// The file as read, `None` when it does not exist.
pub fn load(path: &Path) -> std::io::Result<Option<(Config, Stamp)>> {
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let metadata = file.metadata()?;
    // A hand-written file may be readable by others; the token stays private from now on.
    if metadata.permissions().mode() & 0o077 != 0 {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(Some((parse(&bytes), stamp(&metadata))))
}
pub fn stamp_of(path: &Path) -> Option<Stamp> {
    std::fs::metadata(path).ok().map(|m| stamp(&m))
}
fn stamp(m: &std::fs::Metadata) -> Stamp {
    Stamp {
        modified: (m.mtime(), m.mtime_nsec()),
        length: m.len(),
        inode: m.ino(),
    }
}

/// The admin page's view of the file: never the token itself.
pub fn admin_values(config: Option<&Config>) -> Map<String, Value> {
    let mut values = Map::new();
    values.insert(
        "publicNameEnabled".into(),
        config.is_some_and(|c| c.enabled).into(),
    );
    values.insert(
        "publicNameProvider".into(),
        config
            .and_then(|c| c.provider)
            .map_or("", Provider::id)
            .into(),
    );
    values.insert(
        "publicName".into(),
        config
            .and_then(|c| c.name.clone())
            .unwrap_or_default()
            .into(),
    );
    values.insert("publicNameToken".into(), "".into());
    values.insert(
        "publicNameTokenSet".into(),
        config.is_some_and(|c| c.token.is_some()).into(),
    );
    values.insert(
        "publicNameAgreeTerms".into(),
        config.is_some_and(|c| c.agree_terms).into(),
    );
    values
}

/// Applies the admin page's partial update to `current` (`None`: no file yet, so off). Each field
/// saves on its own, so a partly filled configuration is accepted; an empty token keeps the saved
/// one. Errors are keyed by admin field.
pub fn apply_admin(
    current: Option<&Config>,
    values: &Map<String, Value>,
) -> Result<Config, BTreeMap<String, String>> {
    let mut next = match current {
        Some(c) if c.needs_hand_fix() => {
            return Err(values
                .keys()
                .map(|k| (k.clone(), "error.publicNameFile".to_string()))
                .collect())
        }
        Some(c) => c.clone(),
        None => Config {
            enabled: false,
            ..Config::default()
        },
    };
    next.errors.clear();
    let mut errors = BTreeMap::new();
    let invalid = |errors: &mut BTreeMap<String, String>, key: &str| {
        errors.insert(key.to_string(), "error.invalid".to_string());
    };
    // The provider first: the name and token rules depend on it.
    if let Some(v) = values.get("publicNameProvider") {
        match v.as_str() {
            Some("") => next.provider = None,
            Some(id) if Provider::parse(id).is_some() => next.provider = Provider::parse(id),
            _ => invalid(&mut errors, "publicNameProvider"),
        }
    }
    for (key, value) in values {
        match key.as_str() {
            "publicNameProvider" => {}
            "publicNameEnabled" => match value.as_bool() {
                Some(b) => next.enabled = b,
                None => invalid(&mut errors, key),
            },
            "publicNameAgreeTerms" => match value.as_bool() {
                Some(b) => next.agree_terms = b,
                None => invalid(&mut errors, key),
            },
            "publicName" => match value.as_str() {
                Some("") => next.name = None,
                Some(name) => match normalize_name(name.trim(), next.provider) {
                    Some(name) => next.name = Some(name),
                    None => invalid(&mut errors, key),
                },
                None => invalid(&mut errors, key),
            },
            "publicNameToken" => match value.as_str() {
                Some("") => {}
                Some(token) if valid_token(token, next.provider) => next.token = Some(token.into()),
                _ => invalid(&mut errors, key),
            },
            _ => {
                errors.insert(key.clone(), "error.setting".into());
            }
        }
    }
    if errors.is_empty() {
        Ok(next)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const UUID: &str = "a1b2c3d4-0000-0000-0000-000000000000";
    fn file(value: Value) -> Config {
        parse(&serde_json::to_vec(&value).unwrap())
    }
    #[test]
    fn valid_files_for_each_provider_fold_case_and_default_to_enabled_production() {
        for (provider, name, token) in [
            ("duckdns", "MyRelay.DuckDNS.org", UUID),
            ("dynv6", "myrelay.dynv6.net", "Zs1x_Token-.~"),
            ("dynv6", "Car-1.example.net", "abc"),
        ] {
            let c = file(
                serde_json::json!({"version":1,"provider":provider,"name":name,
                "token":token,"agreeLetsEncryptTerms":true}),
            );
            assert!(c.errors.is_empty(), "{provider}: {:?}", c.errors);
            assert!(c.complete() && c.enabled);
            assert_eq!(c.provider.unwrap().id(), provider);
            assert_eq!(c.name.as_deref(), Some(name.to_ascii_lowercase().as_str()));
            assert_eq!(c.acme, AcmeEnvironment::Production);
            assert!(!c.acme_set);
        }
        let c = file(
            serde_json::json!({"version":1,"enabled":false,"provider":"dynv6",
            "name":"a.dynv6.net","token":"t","agreeLetsEncryptTerms":false,"acme":"staging"}),
        );
        assert!(c.errors.is_empty());
        assert!(!c.enabled && !c.agree_terms && !c.complete());
        assert_eq!(c.acme, AcmeEnvironment::Staging);
    }
    #[test]
    fn each_error_code_names_its_key() {
        let c = file(serde_json::json!({}));
        assert_eq!(c.errors["version"], "missing");
        for key in ["provider", "name", "token"] {
            assert_eq!(c.errors[key], "missing");
        }
        assert!(!c.needs_hand_fix() && !c.complete());
        let c = file(
            serde_json::json!({"version":2,"provider":"cloudflare","name":"bad_name.org",
            "token":"has space","enabled":"yes","agreeLetsEncryptTerms":1,"acme":"test","nmae":"x"}),
        );
        assert_eq!(c.errors["version"], "bad-version");
        for key in [
            "provider",
            "name",
            "token",
            "enabled",
            "agreeLetsEncryptTerms",
            "acme",
        ] {
            assert_eq!(c.errors[key], "invalid", "{key}");
        }
        assert_eq!(c.errors["nmae"], "unknown-key");
        assert!(c.needs_hand_fix());
        assert_eq!(parse(b"{not json").errors[FILE_KEY], "not-json");
        assert_eq!(parse(b"[1]").errors[FILE_KEY], "not-json");
        let big = format!("{{\"version\":1,\"name\":\"{}\"}}", "a".repeat(MAX_BYTES));
        assert_eq!(parse(big.as_bytes()).errors[FILE_KEY], "too-large");
        assert_eq!(
            file(serde_json::json!({"version":"1"})).errors["version"],
            "bad-version"
        );
        // deSEC is no longer a provider: such a file reads as an invalid provider the page can fix.
        let c = file(
            serde_json::json!({"version":1,"provider":"desec","name":"a.dedyn.io",
            "token":"t","agreeLetsEncryptTerms":true}),
        );
        assert_eq!(c.errors["provider"], "invalid");
        assert!(!c.complete() && !c.needs_hand_fix());
    }
    #[test]
    fn name_and_token_rules() {
        let duck = Some(Provider::DuckDns);
        assert_eq!(
            normalize_name("A.DuckDNS.ORG", duck).as_deref(),
            Some("a.duckdns.org")
        );
        for bad in [
            "duckdns.org",
            "a.b.duckdns.org",
            "a.dynv6.net",
            "-a.duckdns.org",
            "a-.duckdns.org",
        ] {
            assert!(normalize_name(bad, duck).is_none(), "{bad}");
        }
        for bad in [
            "example",
            "a..b",
            "a.b.",
            "ä.example",
            "a_b.example",
            &format!("{}.example", "a".repeat(64)),
            &format!("{}.io", ["abcdefghi"; 26].join(".")),
        ] {
            assert!(
                normalize_name(bad, Some(Provider::Dynv6)).is_none(),
                "{bad}"
            );
        }
        assert!(normalize_name(&format!("{}.example", "a".repeat(63)), None).is_some());
        assert!(valid_token(UUID, duck));
        assert!(!valid_token("a1b2c3d4", duck));
        assert!(!valid_token(&UUID.replace('a', "g"), duck));
        assert!(valid_token(&"x".repeat(256), None));
        assert!(!valid_token(&"x".repeat(257), None));
        assert!(!valid_token("", None) && !valid_token("a b", None) && !valid_token("é", None));
    }
    #[test]
    fn admin_updates_accept_partial_files_keep_the_token_and_never_return_it() {
        let values = |v: Value| v.as_object().unwrap().clone();
        let c = apply_admin(None, &values(serde_json::json!({"publicNameEnabled":true}))).unwrap();
        assert!(c.enabled && c.provider.is_none());
        let text = c.to_json();
        let reread = parse(&text);
        assert_eq!(reread.errors["provider"], "missing");
        assert!(reread.enabled && !reread.needs_hand_fix());
        let c = apply_admin(
            Some(&reread),
            &values(serde_json::json!({"publicNameProvider":"duckdns",
                "publicName":"My.duckdns.org","publicNameToken":UUID,"publicNameAgreeTerms":true})),
        )
        .unwrap();
        let reread = parse(&c.to_json());
        assert!(reread.complete(), "{:?}", reread.errors);
        let kept = apply_admin(
            Some(&reread),
            &values(serde_json::json!({"publicNameToken":""})),
        )
        .unwrap();
        assert_eq!(kept.token.as_deref(), Some(UUID));
        let shown = admin_values(Some(&kept));
        assert_eq!(shown["publicNameToken"], "");
        assert_eq!(shown["publicNameTokenSet"], true);
        assert!(!Value::Object(shown).to_string().contains(UUID));
        let errors = apply_admin(
            Some(&reread),
            &values(
                serde_json::json!({"publicName":"x.dynv6.net","publicNameToken":"nope",
                "publicNameProvider":"other","publicNameOther":1,"publicNameEnabled":"on"}),
            ),
        )
        .unwrap_err();
        for key in [
            "publicName",
            "publicNameToken",
            "publicNameProvider",
            "publicNameEnabled",
        ] {
            assert_eq!(errors[key], "error.invalid", "{key}");
        }
        assert_eq!(errors["publicNameOther"], "error.setting");
        // A provider change keeps the other fields even where they no longer fit: the page saves
        // each field on its own, and the stale name reads as invalid until it is replaced.
        let dynv6 = apply_admin(
            Some(&reread),
            &values(serde_json::json!({"publicNameProvider":"dynv6","publicName":"x.dynv6.net"})),
        )
        .unwrap();
        let switched = apply_admin(
            Some(&parse(&dynv6.to_json())),
            &values(serde_json::json!({"publicNameProvider":"duckdns"})),
        )
        .unwrap();
        let stale = parse(&switched.to_json());
        assert_eq!(stale.errors["name"], "invalid");
        assert!(!stale.needs_hand_fix() && !stale.complete());
        let fixed = apply_admin(
            Some(&stale),
            &values(serde_json::json!({"publicName":"y.duckdns.org"})),
        )
        .unwrap();
        assert!(parse(&fixed.to_json()).complete());
        // A file that needs fixing is never overwritten by the page.
        let broken = parse(br#"{"version":1,"nmae":"x"}"#);
        assert_eq!(
            apply_admin(
                Some(&broken),
                &values(serde_json::json!({"publicNameEnabled":false}))
            )
            .unwrap_err()["publicNameEnabled"],
            "error.publicNameFile"
        );
    }
    #[test]
    fn load_reports_absence_and_tightens_permissions() {
        let dir = std::env::temp_dir().join(format!(
            "public-name-config-{}",
            crate::admin::auth::random_hex(6).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        assert!(load(&path).unwrap().is_none());
        std::fs::write(&path, br#"{"version":1}"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let (config, stamp) = load(&path).unwrap().unwrap();
        assert_eq!(config.errors["name"], "missing");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(stamp_of(&path), Some(stamp));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
