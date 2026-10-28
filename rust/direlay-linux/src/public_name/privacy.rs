// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Android privacy sections. Values here contain secrets and deliberately do not implement Debug.
use super::{
    cert,
    config::{self, Config, Provider},
    jws::AccountKey,
    store::{ProviderSetups, Store},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Certificate {
    pub chain: String,
    pub key: String,
    pub name: String,
    pub leaf: cert::Leaf,
}
pub struct Account {
    pub key: String,
    pub directory: String,
    pub url: Option<String>,
}
#[derive(Default)]
pub struct Import {
    pub config: Option<Config>,
    pub certificate: Option<Certificate>,
    pub account: Option<Account>,
    pub providers: Option<ProviderSetups>,
}
impl Import {
    pub fn sections(&self) -> Vec<String> {
        [
            ("publicName", self.config.is_some()),
            ("publicNameCertificate", self.certificate.is_some()),
            ("publicNameAccount", self.account.is_some()),
            ("providers", self.providers.is_some()),
        ]
        .into_iter()
        .filter(|(_, present)| *present)
        .map(|(key, _)| key.into())
        .collect()
    }
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}
fn https(value: &str) -> bool {
    value.parse::<ureq::http::Uri>().ok().is_some_and(|uri| {
        uri.scheme_str() == Some("https")
            && uri.host().is_some_and(|h| !h.is_empty())
            && uri.authority().is_some_and(|a| !a.as_str().contains('@'))
    })
}
/// PEM decoders must not silently skip text or additional key blocks, as Android rejects those.
fn strict_pem(value: &str, label: &str, single: bool) -> bool {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut rest = value.trim();
    let mut count = 0;
    while let Some(after) = rest.strip_prefix(&begin) {
        let Some((body, tail)) = after.split_once(&end) else {
            return false;
        };
        if body.trim().is_empty()
            || !body
                .trim()
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+/=\r\n".contains(&b))
        {
            return false;
        }
        count += 1;
        rest = tail.trim();
    }
    rest.is_empty() && count > 0 && (!single || count == 1)
}
fn certificate(value: &Value, config: Option<&Config>, now: u64) -> Option<Certificate> {
    let name = text(value, "name")?;
    if Some(name) != config?.name.as_deref() {
        return None;
    }
    let chain = text(value, "chainPem")?;
    let key = text(value, "keyPem")?;
    if !strict_pem(chain, "CERTIFICATE", false) || !strict_pem(key, "PRIVATE KEY", true) {
        return None;
    }
    // Android accepts EC public-name keys only. Parse every certificate, not just the leaf.
    use der::Decode;
    for der in cert::chain(chain.as_bytes())? {
        x509_cert::Certificate::from_der(&der).ok()?;
    }
    let pair = rcgen::KeyPair::from_pem(key).ok()?;
    if ![
        &rcgen::PKCS_ECDSA_P256_SHA256,
        &rcgen::PKCS_ECDSA_P384_SHA384,
    ]
    .contains(&pair.algorithm())
    {
        return None;
    }
    let leaf = cert::check(chain.as_bytes(), key, name, Some(now)).ok()?;
    Some(Certificate {
        chain: chain.into(),
        key: key.into(),
        name: name.into(),
        leaf,
    })
}
fn account(value: &Value) -> Option<Account> {
    let key = text(value, "keyPem")?;
    if !strict_pem(key, "PRIVATE KEY", true) {
        return None;
    }
    AccountKey::from_pem(key)?;
    let directory = text(value, "directory")?;
    let url = match value.get("url") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str()?),
    };
    if !https(directory) || url.is_some_and(|u| !https(u)) {
        return None;
    }
    Some(Account {
        key: key.into(),
        directory: directory.into(),
        url: url.map(str::to_owned),
    })
}
fn providers(value: &Value) -> Option<ProviderSetups> {
    let mut setups = ProviderSetups::new();
    for (id, value) in value.as_object()? {
        let provider = Provider::parse(id)?;
        let name = text(value, "name")?;
        let token = text(value, "token")?;
        if (!name.is_empty() && config::normalize_name(name, Some(provider)).is_none())
            || (!token.is_empty() && !config::valid_token(token, Some(provider)))
        {
            return None;
        }
        setups.insert(
            id.clone(),
            super::store::ProviderSetup {
                name: name.into(),
                token: token.into(),
            },
        );
    }
    Some(setups)
}
/// Optional sections replace their stores; Android-only identity material is handled by W15.
pub fn decode(
    value: &Value,
    current: Option<&Config>,
    now: u64,
) -> Result<Import, BTreeMap<String, String>> {
    let mut import = Import::default();
    let mut errors = BTreeMap::new();
    if let Some(value) = value.get("publicName") {
        let config = config::parse(&serde_json::to_vec(value).unwrap());
        if config.errors.is_empty() {
            import.config = Some(config);
        } else {
            errors.extend(
                config
                    .errors
                    .into_iter()
                    .map(|(key, code)| (format!("publicName.{key}"), code)),
            );
        }
    }
    if let Some(value) = value.get("publicNameCertificate") {
        import.certificate = certificate(value, import.config.as_ref().or(current), now);
        if import.certificate.is_none() {
            errors.insert("publicNameCertificate".into(), "invalid".into());
        }
    }
    if let Some(value) = value.get("publicNameAccount") {
        import.account = account(value);
        if import.account.is_none() {
            errors.insert("publicNameAccount".into(), "invalid".into());
        }
    }
    if let Some(value) = value.get("providers") {
        import.providers = providers(value);
        if import.providers.is_none() {
            errors.insert("providers".into(), "invalid".into());
        }
    }
    if errors.is_empty() {
        Ok(import)
    } else {
        Err(errors)
    }
}
pub fn export(store: &Store, config: Option<&Config>) -> std::io::Result<Value> {
    let mut value = json!({});
    if let Some(config) = config {
        value["publicName"] = serde_json::from_slice(&config.to_json())?;
    }
    let setups = store.providers(config)?;
    if !setups.is_empty() {
        value["providers"] = serde_json::to_value(setups)?;
    }
    if let Some((chain, key)) = store.certificate() {
        if let (Ok(chain), Some(name)) = (String::from_utf8(chain), store.meta().name) {
            value["publicNameCertificate"] = json!({"chainPem":chain,"keyPem":key,"name":name});
        }
    }
    if let Some((key, directory, url)) = store.account(config.map(|c| c.acme)) {
        value["publicNameAccount"] = json!({"keyPem":key,"directory":directory,"url":url});
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::{cert::tests::LEAF, jws::tests::PAD_KEY};
    const NOW: u64 = 1_795_000_000;
    fn config() -> Config {
        config::parse(br#"{"version":1,"provider":"duckdns","name":"myrelay.duckdns.org","token":"a1b2c3d4-0000-0000-0000-000000000000"}"#)
    }
    fn material() -> Value {
        json!({"publicNameCertificate":{"chainPem":LEAF,"keyPem":PAD_KEY,"name":"myrelay.duckdns.org"},
            "publicNameAccount":{"keyPem":PAD_KEY,"directory":super::super::acme::PRODUCTION,"url":null},
            "providers":{"duckdns":{"name":"myrelay.duckdns.org","token":"a1b2c3d4-0000-0000-0000-000000000000"},"dynv6":{"name":"car.example.org","token":"dyn-token"}}})
    }
    #[test]
    fn android_sections_validate_against_the_current_name_and_exact_material() {
        let decoded = decode(&material(), Some(&config()), NOW).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(decoded.providers.unwrap().len(), 2);
        assert_eq!(
            decoded.certificate.unwrap().leaf.cert_id.as_deref(),
            Some("qDKbCiBmhNp3hvez8EcGgwJawYY.AIdlQyE")
        );
        assert!(decode(&material(), None, NOW).is_err());
        for now in [1_792_561_979, 1_800_337_980] {
            assert!(decode(&material(), Some(&config()), now).is_err());
        }
        let other = AccountKey::generate().unwrap();
        for (section, field, invalid) in [
            ("publicNameCertificate", "name", "other.duckdns.org".into()),
            ("publicNameCertificate", "chainPem", format!("junk{LEAF}")),
            (
                "publicNameCertificate",
                "chainPem",
                format!("{LEAF}-----BEGIN CERTIFICATE-----\nYWJj\n-----END CERTIFICATE-----\n"),
            ),
            (
                "publicNameCertificate",
                "keyPem",
                format!("{PAD_KEY}{PAD_KEY}"),
            ),
            ("publicNameCertificate", "keyPem", other.pem().into()),
            (
                "publicNameAccount",
                "directory",
                "http://ca.example/directory".into(),
            ),
            (
                "publicNameAccount",
                "url",
                "https://user@ca.example/account".into(),
            ),
            ("publicNameAccount", "keyPem", "broken".into()),
        ] {
            let mut value = material();
            value[section][field] = invalid.into();
            assert_eq!(
                decode(&value, Some(&config()), NOW).err().unwrap()[section],
                "invalid"
            );
        }
        for entries in [
            json!({"unknown":{"name":"","token":""}}),
            json!({"duckdns":{"name":"car.dynv6.net","token":""}}),
            json!({"duckdns":{"name":"","token":"bad"}}),
            json!({"dynv6":{"name":null,"token":""}}),
        ] {
            assert_eq!(
                decode(&json!({"providers":entries}), None, NOW)
                    .err()
                    .unwrap()["providers"],
                "invalid"
            );
        }
    }
    #[test]
    fn certificate_can_use_the_enclosed_config_and_identity_sections_are_left_for_w15() {
        let mut value = material();
        value["publicName"] = serde_json::from_slice(&config().to_json()).unwrap();
        value["importedCertificate"] = json!({"anything":"ignored"});
        value["mfiRemoteToken"] = json!("ignored-token");
        let decoded = decode(&value, None, NOW).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(decoded.sections().len(), 4);
    }
}
