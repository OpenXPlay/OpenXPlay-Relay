// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! An ACME client (RFC 8555) for one DNS-01 order at a time, with ACME Renewal Information
//! (RFC 9773) when the directory offers it.
use super::{
    cert,
    http::{Http, Request, Response},
    jws::{b64, AccountKey},
    providers::ProviderError,
    time,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub const PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
/// Let's Encrypt staging: for testing; its certificates are not trusted.
pub const STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";
const PROBLEM: &str = "urn:ietf:params:acme:error:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcmeError {
    /// No usable response from the CA.
    Unreachable,
    /// `rateLimited` or HTTP 429, with the `Retry-After` delay in seconds.
    RateLimited(Option<u64>),
    /// The CA refused; the problem's detail, for the status line.
    Rejected(String),
    /// The saved account URL names no account at this CA.
    AccountMissing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    pub new_nonce: String,
    pub new_account: String,
    pub new_order: String,
    pub renewal_info: Option<String>,
}

/// How long to wait for the CA: `poll` when it sends no `Retry-After`, `limit` in all.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub poll: Duration,
    pub limit: Duration,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_secs(3),
            limit: Duration::from_secs(120),
        }
    }
}

struct Problem {
    kind: String,
    detail: String,
}
fn problem(response: &Response) -> Problem {
    let body = response.json().unwrap_or(Value::Null);
    let kind = body["type"]
        .as_str()
        .unwrap_or("")
        .strip_prefix(PROBLEM)
        .unwrap_or("")
        .to_string();
    let detail = body["detail"].as_str().unwrap_or("").to_string();
    Problem { kind, detail }
}
/// A CA-supplied detail made safe for the status line: printable, at most 200 characters.
fn shown(detail: &str, fallback: &str) -> String {
    let clean: String = detail
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(200)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        fallback.to_string()
    } else {
        clean.to_string()
    }
}

/// One conversation with the CA: the nonce chain and the account.
pub struct Client<'a> {
    http: &'a dyn Http,
    pub directory: Directory,
    key: &'a AccountKey,
    kid: Option<String>,
    nonce: Option<String>,
    /// Wall-clock seconds, for `Retry-After` dates and the certificate check.
    now: u64,
}
impl<'a> Client<'a> {
    /// Reads the directory at `url`. A saved `kid` from another CA is dropped.
    pub fn connect(
        http: &'a dyn Http,
        url: &str,
        key: &'a AccountKey,
        kid: Option<String>,
        now: u64,
    ) -> Result<Self, AcmeError> {
        let response = http
            .send(Request::get(url))
            .map_err(|_| AcmeError::Unreachable)?;
        if response.status == 429 {
            return Err(AcmeError::RateLimited(response.retry_after(now)));
        }
        let body = response
            .json()
            .filter(|_| response.success())
            .ok_or(AcmeError::Unreachable)?;
        let field = |key: &str| body[key].as_str().map(str::to_owned);
        let directory = Directory {
            new_nonce: field("newNonce").ok_or(AcmeError::Unreachable)?,
            new_account: field("newAccount").ok_or(AcmeError::Unreachable)?,
            new_order: field("newOrder").ok_or(AcmeError::Unreachable)?,
            renewal_info: field("renewalInfo"),
        };
        let kid = kid.filter(|kid| origin(kid) == origin(&directory.new_account));
        Ok(Self {
            http,
            directory,
            key,
            kid,
            nonce: None,
            now,
        })
    }
    pub fn kid(&self) -> Option<&str> {
        self.kid.as_deref()
    }
    fn nonce(&mut self) -> Result<String, AcmeError> {
        if let Some(nonce) = self.nonce.take() {
            return Ok(nonce);
        }
        let request = Request {
            method: "HEAD",
            url: self.directory.new_nonce.clone(),
            headers: Vec::new(),
            body: None,
        };
        let response = self
            .http
            .send(request)
            .map_err(|_| AcmeError::Unreachable)?;
        response
            .header("replay-nonce")
            .map(str::to_owned)
            .ok_or(AcmeError::Unreachable)
    }
    fn error(&self, response: &Response, problem: Problem) -> AcmeError {
        if response.status == 429 || problem.kind == "rateLimited" {
            AcmeError::RateLimited(response.retry_after(self.now))
        } else if problem.kind == "accountDoesNotExist" {
            AcmeError::AccountMissing
        } else if response.status >= 500 && problem.kind.is_empty() {
            AcmeError::Unreachable
        } else {
            let fallback = if problem.kind.is_empty() {
                format!("HTTP {}", response.status)
            } else {
                problem.kind.clone()
            };
            AcmeError::Rejected(shown(&problem.detail, &fallback))
        }
    }
    /// A signed POST; `payload` `None` is POST-as-GET. A `badNonce` is retried once with the
    /// fresh nonce that came with it.
    fn post(
        &mut self,
        url: &str,
        payload: Option<&Value>,
        accept: Option<&str>,
    ) -> Result<Response, AcmeError> {
        for attempt in 0..2 {
            let nonce = self.nonce()?;
            let mut protected = json!({"alg": "ES256", "nonce": nonce, "url": url});
            match (&self.kid, url == self.directory.new_account) {
                (Some(kid), false) => protected["kid"] = kid.as_str().into(),
                _ => protected["jwk"] = self.key.jwk(),
            }
            let body = self.key.sign(&protected, payload);
            let mut request = Request::with_body("POST", url, body.to_string().into_bytes())
                .header("Content-Type", "application/jose+json");
            if let Some(accept) = accept {
                request = request.header("Accept", accept);
            }
            let response = self
                .http
                .send(request)
                .map_err(|_| AcmeError::Unreachable)?;
            if let Some(nonce) = response.header("replay-nonce") {
                self.nonce = Some(nonce.to_owned());
            }
            if response.success() {
                return Ok(response);
            }
            let problem = problem(&response);
            if problem.kind == "badNonce" && attempt == 0 {
                continue;
            }
            return Err(self.error(&response, problem));
        }
        unreachable!("the loop returns on its second attempt")
    }
    /// POST-as-GET of a JSON resource, with the CA's `Retry-After`.
    fn fetch(&mut self, url: &str) -> Result<(Value, Option<u64>), AcmeError> {
        let response = self.post(url, None, None)?;
        let retry = response.retry_after(self.now);
        Ok((response.json().ok_or(AcmeError::Unreachable)?, retry))
    }
    /// The account URL, registering the key first if needed: terms agreed, no contact.
    pub fn account(&mut self) -> Result<String, AcmeError> {
        if let Some(kid) = &self.kid {
            return Ok(kid.clone());
        }
        let url = self.directory.new_account.clone();
        let response = self.post(&url, Some(&json!({"termsOfServiceAgreed": true})), None)?;
        let kid = response
            .header("location")
            .map(str::to_owned)
            .ok_or(AcmeError::Unreachable)?;
        self.kid = Some(kid.clone());
        Ok(kid)
    }
    /// A new order for `name`, replacing the certificate `replaces` when the CA supports ARI.
    fn new_order(
        &mut self,
        name: &str,
        replaces: Option<&str>,
    ) -> Result<(String, Value), AcmeError> {
        let mut payload = json!({"identifiers": [{"type": "dns", "value": name}]});
        if let Some(replaces) = replaces.filter(|_| self.directory.renewal_info.is_some()) {
            payload["replaces"] = replaces.into();
        }
        let url = self.directory.new_order.clone();
        let response = self.post(&url, Some(&payload), None)?;
        let location = response
            .header("location")
            .map(str::to_owned)
            .ok_or(AcmeError::Unreachable)?;
        Ok((location, response.json().ok_or(AcmeError::Unreachable)?))
    }
    /// The suggested renewal window of the certificate `cert_id`, as Unix seconds, when the
    /// directory offers renewal information.
    pub fn renewal_window(&self, cert_id: &str) -> Result<Option<(u64, u64)>, AcmeError> {
        let Some(base) = &self.directory.renewal_info else {
            return Ok(None);
        };
        let url = format!("{}/{cert_id}", base.trim_end_matches('/'));
        let response = self
            .http
            .send(Request::get(url))
            .map_err(|_| AcmeError::Unreachable)?;
        if !response.success() {
            return Err(self.error(&response, problem(&response)));
        }
        let body = response.json().ok_or(AcmeError::Unreachable)?;
        let window = &body["suggestedWindow"];
        let at = |key: &str| window[key].as_str().and_then(time::parse_rfc3339);
        match (at("start"), at("end")) {
            (Some(start), Some(end)) if start <= end => Ok(Some((start, end))),
            _ => Err(AcmeError::Unreachable),
        }
    }
}
fn origin(url: &str) -> &str {
    let rest = url.find("://").map_or(0, |i| i + 3);
    let end = url[rest..].find('/').map_or(url.len(), |i| rest + i);
    &url[..end]
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueError {
    Acme(AcmeError),
    Provider(ProviderError),
    DnsNotUpdated,
    /// A stop, removal or configuration change ended the attempt.
    Interrupted,
}
impl From<AcmeError> for IssueError {
    fn from(error: AcmeError) -> Self {
        Self::Acme(error)
    }
}

/// The DNS side of a DNS-01 challenge: the provider's `_acme-challenge` record and the wait until
/// its name servers show it.
pub trait Dns01 {
    fn set(&mut self, value: &str) -> Result<(), IssueError>;
    fn propagate(&mut self, value: &str) -> Result<(), IssueError>;
    fn clear(&mut self);
}

pub struct Issued {
    /// The PEM chain, leaf first.
    pub chain: Vec<u8>,
    /// The certificate's own key, PKCS #8 PEM.
    pub key_pem: String,
    pub leaf: cert::Leaf,
}

/// Polls `url` until `done` says it is finished (`Some(result)`), honouring `Retry-After`.
fn poll<T>(
    client: &mut Client,
    url: &str,
    timing: Timing,
    sleep: &mut dyn FnMut(Duration) -> bool,
    done: impl Fn(&Value) -> Option<Result<T, IssueError>>,
) -> Result<T, IssueError> {
    let deadline = Instant::now() + timing.limit;
    loop {
        let (body, retry) = client.fetch(url)?;
        if let Some(result) = done(&body) {
            return result;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(AcmeError::Rejected("the CA did not finish in time".into()).into());
        }
        let wait = retry.map_or(timing.poll, Duration::from_secs).min(left);
        if !sleep(wait) {
            return Err(IssueError::Interrupted);
        }
    }
}
fn detail(body: &Value, fallback: &str) -> String {
    let error = if body["error"].is_object() {
        &body["error"]
    } else {
        body["challenges"]
            .as_array()
            .and_then(|c| c.iter().find(|c| c["error"].is_object()))
            .map_or(&Value::Null, |c| &c["error"])
    };
    shown(error["detail"].as_str().unwrap_or(""), fallback)
}

/// One complete order for `name`: account, order, DNS-01 for every pending authorization,
/// finalize with a new key, download and check. The TXT record is always cleared afterwards.
/// `on_order` runs before every `newOrder` sent: the local order guard counts them all, also
/// those the CA answers with `rateLimited`.
pub fn issue(
    client: &mut Client,
    name: &str,
    replaces: Option<&str>,
    dns: &mut dyn Dns01,
    timing: Timing,
    sleep: &mut dyn FnMut(Duration) -> bool,
    on_order: &mut dyn FnMut(),
) -> Result<Issued, IssueError> {
    client.account()?;
    on_order();
    let mut ordered = client.new_order(name, replaces);
    if ordered == Err(AcmeError::AccountMissing) {
        client.kid = None;
        client.account()?;
        on_order();
        ordered = client.new_order(name, replaces);
    }
    if replaces.is_some()
        && matches!(&ordered, Err(AcmeError::Rejected(_)))
        && client.directory.renewal_info.is_some()
    {
        // A certificate that was already replaced, or that this CA does not know, cannot be
        // named again; order without `replaces`.
        on_order();
        ordered = client.new_order(name, None);
    }
    let (order_url, order) = ordered?;
    let authorizations: Vec<String> = order["authorizations"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    for authorization in &authorizations {
        let (body, _) = client.fetch(authorization)?;
        match body["status"].as_str() {
            Some("valid") => continue,
            Some("pending") => {}
            _ => return Err(AcmeError::Rejected(detail(&body, "authorization unusable")).into()),
        }
        let challenge = body["challenges"]
            .as_array()
            .and_then(|c| c.iter().find(|c| c["type"] == "dns-01"))
            .ok_or_else(|| AcmeError::Rejected("no dns-01 challenge offered".into()))?;
        let (Some(url), Some(token)) = (challenge["url"].as_str(), challenge["token"].as_str())
        else {
            return Err(AcmeError::Rejected("malformed dns-01 challenge".into()).into());
        };
        let value = super::jws::txt_value(&client.key.key_authorization(token));
        let url = url.to_string();
        let result = (|| {
            dns.set(&value)?;
            dns.propagate(&value)?;
            client.post(&url, Some(&json!({})), None)?;
            poll(client, authorization, timing, sleep, |body| {
                match body["status"].as_str() {
                    Some("valid") => Some(Ok(())),
                    Some("pending" | "processing") => None,
                    _ => Some(Err(
                        AcmeError::Rejected(detail(body, "challenge failed")).into()
                    )),
                }
            })
        })();
        dns.clear();
        result?;
    }
    let finalize = order["finalize"]
        .as_str()
        .ok_or(AcmeError::Unreachable)?
        .to_string();
    poll(client, &order_url, timing, sleep, |body| {
        match body["status"].as_str() {
            Some("ready" | "processing" | "valid") => Some(Ok(())),
            Some("pending") => None,
            _ => Some(Err(AcmeError::Rejected(detail(body, "order failed")).into())),
        }
    })?;
    let (key, csr) = cert::new_key_and_csr(name)
        .map_err(|_| AcmeError::Rejected("could not create the certificate key".into()))?;
    let response = client.post(&finalize, Some(&json!({"csr": b64(&csr)})), None)?;
    let finalized = response.json().unwrap_or(Value::Null);
    let certificate = match finalized["certificate"].as_str() {
        Some(url) if finalized["status"] == "valid" => url.to_string(),
        _ => poll(client, &order_url, timing, sleep, |body| {
            match body["status"].as_str() {
                Some("valid") => Some(
                    body["certificate"]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| AcmeError::Unreachable.into()),
                ),
                Some("processing" | "ready") => None,
                _ => Some(Err(AcmeError::Rejected(detail(body, "order failed")).into())),
            }
        })?,
    };
    let chain = client
        .post(
            &certificate,
            None,
            Some("application/pem-certificate-chain"),
        )?
        .body;
    let key_pem = key.serialize_pem();
    let leaf = cert::check(&chain, &key_pem, name, Some(client.now)).map_err(|problem| {
        AcmeError::Rejected(format!("the issued certificate is unusable ({problem:?})"))
    })?;
    Ok(Issued {
        chain,
        key_pem,
        leaf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::{
        http::UreqHttp,
        testing::{FakeAcme, FakeAcmeOptions},
    };
    use std::sync::{Arc, Mutex};
    const NAME: &str = "myrelay.duckdns.org";
    /// The fake ACME server's TXT record, set and cleared like a provider would.
    struct SharedTxt(Arc<Mutex<Option<String>>>, Vec<String>);
    impl Dns01 for SharedTxt {
        fn set(&mut self, value: &str) -> Result<(), IssueError> {
            *self.0.lock().unwrap() = Some(value.into());
            self.1.push(format!("set {value}"));
            Ok(())
        }
        fn propagate(&mut self, _: &str) -> Result<(), IssueError> {
            self.1.push("propagate".into());
            Ok(())
        }
        fn clear(&mut self) {
            *self.0.lock().unwrap() = None;
            self.1.push("clear".into());
        }
    }
    fn timing() -> Timing {
        Timing {
            poll: Duration::from_millis(5),
            limit: Duration::from_secs(2),
        }
    }
    fn now() -> u64 {
        crate::platform::wall_clock().as_secs()
    }
    fn order(
        acme: &FakeAcme,
        key: &AccountKey,
        kid: Option<String>,
        replaces: Option<&str>,
    ) -> (Result<Issued, IssueError>, Option<String>, SharedTxt, u32) {
        let http = UreqHttp::new(false);
        let mut dns = SharedTxt(acme.txt(), Vec::new());
        let mut orders = 0;
        let result = Client::connect(&http, &acme.directory(), key, kid, now())
            .map_err(IssueError::from)
            .and_then(|mut client| {
                let issued = issue(
                    &mut client,
                    NAME,
                    replaces,
                    &mut dns,
                    timing(),
                    &mut |d| {
                        std::thread::sleep(d);
                        true
                    },
                    &mut || orders += 1,
                );
                issued.map(|i| (i, client.kid().map(str::to_owned)))
            });
        match result {
            Ok((issued, kid)) => (Ok(issued), kid, dns, orders),
            Err(e) => (Err(e), None, dns, orders),
        }
    }
    #[test]
    fn full_order_with_signed_requests_nonces_and_the_txt_digest() {
        let acme = FakeAcme::start(FakeAcmeOptions {
            pending_polls: 2,
            processing_polls: 1,
            ..Default::default()
        });
        let key = AccountKey::generate().unwrap();
        let (issued, kid, dns, orders) = order(&acme, &key, None, None);
        let issued = issued.unwrap();
        assert_eq!(orders, 1);
        assert_eq!(issued.leaf.names, [NAME]);
        assert!(issued.leaf.not_after > now() + 80 * 86_400);
        assert!(cert::check(&issued.chain, &issued.key_pem, NAME, Some(now())).is_ok());
        assert_eq!(cert::chain(&issued.chain).unwrap().len(), 2);
        let expected = super::super::jws::txt_value(&key.key_authorization(&acme.token(0)));
        assert_eq!(
            dns.1,
            [
                format!("set {expected}"),
                "propagate".into(),
                "clear".into()
            ]
        );
        assert!(acme.txt().lock().unwrap().is_none());
        assert_eq!(acme.violations(), Vec::<String>::new());
        let log = acme.log();
        assert_eq!(log[0], "newAccount jwk terms");
        assert!(log[1..].iter().all(|line| line.contains(" kid")), "{log:?}");
        // The account is reused by its URL; a second order needs no newAccount.
        let (second, _, _, _) = order(&acme, &key, kid.clone(), None);
        assert!(second.is_ok());
        assert_eq!(
            acme.log()
                .iter()
                .filter(|l| l.starts_with("newAccount"))
                .count(),
            1
        );
        assert_eq!(acme.violations(), Vec::<String>::new());
        // An account URL the CA no longer knows is replaced.
        let (third, new_kid, _, _) =
            order(&acme, &key, Some(format!("{}/acct/99", acme.base())), None);
        assert!(third.is_ok());
        assert_eq!(new_kid, kid);
    }
    #[test]
    fn bad_nonce_is_retried_once_with_the_fresh_nonce() {
        let acme = FakeAcme::start(FakeAcmeOptions {
            bad_nonces: 1,
            ..Default::default()
        });
        let key = AccountKey::generate().unwrap();
        assert!(order(&acme, &key, None, None).0.is_ok());
        assert_eq!(
            acme.log().iter().filter(|l| l.contains("badNonce")).count(),
            1
        );
        let acme = FakeAcme::start(FakeAcmeOptions {
            bad_nonces: 2,
            ..Default::default()
        });
        match order(&acme, &key, None, None).0 {
            Err(IssueError::Acme(AcmeError::Rejected(detail))) => assert!(detail.contains("nonce")),
            other => panic!("{:?}", other.err()),
        }
    }
    #[test]
    fn rate_limits_and_rejections_map_to_their_errors() {
        let acme = FakeAcme::start(FakeAcmeOptions {
            rate_limited: Some(3600),
            ..Default::default()
        });
        let key = AccountKey::generate().unwrap();
        let (result, _, dns, orders) = order(&acme, &key, None, None);
        assert_eq!(
            result.err(),
            Some(IssueError::Acme(AcmeError::RateLimited(Some(3600))))
        );
        assert_eq!(orders, 1, "a rate-limited newOrder counts too");
        assert!(dns.1.is_empty());
        // A wrong TXT value fails the challenge with the CA's detail, and the record is cleared.
        let acme = FakeAcme::start(FakeAcmeOptions {
            wrong_txt: true,
            ..Default::default()
        });
        let (result, _, dns, orders) = order(&acme, &key, None, None);
        match result {
            Err(IssueError::Acme(AcmeError::Rejected(detail))) => {
                assert!(detail.contains("Incorrect TXT record"), "{detail}")
            }
            other => panic!("{:?}", other.err()),
        }
        assert_eq!(orders, 1);
        assert_eq!(dns.1.last().map(String::as_str), Some("clear"));
        let http = UreqHttp::new(false);
        assert_eq!(
            Client::connect(&http, "http://127.0.0.1:9/directory", &key, None, 0).err(),
            Some(AcmeError::Unreachable)
        );
    }
    #[test]
    fn renewal_information_and_replaces() {
        let acme = FakeAcme::start(FakeAcmeOptions {
            ari: Some(("2026-12-20T00:00:00Z".into(), "2026-12-22T00:00:00Z".into())),
            ..Default::default()
        });
        let key = AccountKey::generate().unwrap();
        let issued = order(&acme, &key, None, None).0.unwrap();
        let cert_id = issued.leaf.cert_id.clone().unwrap();
        assert_eq!(cert_id, acme.cert_id(0));
        let http = UreqHttp::new(false);
        let client = Client::connect(&http, &acme.directory(), &key, None, now()).unwrap();
        assert_eq!(
            client.renewal_window(&cert_id).unwrap(),
            Some((1_797_724_800, 1_797_897_600))
        );
        assert!(order(&acme, &key, None, Some(&cert_id)).0.is_ok());
        assert_eq!(acme.replaced(), std::slice::from_ref(&cert_id));
        // Without renewal information the directory offers no window and orders never name one.
        let plain = FakeAcme::start(FakeAcmeOptions::default());
        let client = Client::connect(&http, &plain.directory(), &key, None, now()).unwrap();
        assert_eq!(client.renewal_window(&cert_id).unwrap(), None);
        assert!(order(&plain, &key, None, Some(&cert_id)).0.is_ok());
        assert!(plain.replaced().is_empty());
    }
    #[test]
    fn problem_details_are_bounded_and_printable() {
        assert_eq!(shown(&format!("a\nb{}", "x".repeat(300)), "f").len(), 200);
        assert_eq!(shown("  ", "fallback"), "fallback");
        assert_eq!(
            origin("https://acme.example/acme/acct/1"),
            "https://acme.example"
        );
        assert_eq!(origin("http://127.0.0.1:5/x"), "http://127.0.0.1:5");
    }
}
