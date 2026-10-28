// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! In-process fakes for the tests: a plain-HTTP server and an authoritative DNS server on
//! loopback. Nothing here reaches the network.
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

/// A request as the fake server saw it; header names lower-cased.
#[derive(Clone, Debug, Default)]
pub struct SeenRequest {
    pub method: String,
    /// Path and query, exactly as sent.
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl SeenRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    /// The URL this request was sent to, as the client spelled it.
    pub fn url(&self) -> String {
        format!("http://{}{}", self.header("host").unwrap_or(""), self.path)
    }
}

pub type Reply = (u16, Vec<(String, String)>, Vec<u8>);
type Handler = dyn Fn(&SeenRequest) -> Reply + Send + Sync;

/// One response per connection (`Connection: close`), every request recorded.
pub struct FakeHttp {
    address: SocketAddr,
    seen: Arc<Mutex<Vec<SeenRequest>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl FakeHttp {
    pub fn start(handler: impl Fn(&SeenRequest) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let handler: Arc<Handler> = Arc::new(handler);
        let worker = {
            let (seen, stop) = (seen.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((socket, _)) => {
                            let (seen, handler) = (seen.clone(), handler.clone());
                            std::thread::spawn(move || serve(socket, &seen, &*handler));
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(2)),
                    }
                }
            })
        };
        Self {
            address,
            seen,
            stop,
            worker: Some(worker),
        }
    }
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }
    pub fn requests(&self) -> Vec<SeenRequest> {
        self.seen.lock().unwrap().clone()
    }
}
impl Drop for FakeHttp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn serve(socket: std::net::TcpStream, seen: &Mutex<Vec<SeenRequest>>, handler: &Handler) {
    let _ = socket.set_nonblocking(false);
    let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(socket.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let mut request = SeenRequest {
        method: parts.next().unwrap_or("").into(),
        path: parts.next().unwrap_or("").into(),
        ..Default::default()
    };
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 {
            return;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            request
                .headers
                .push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let length = request
        .header("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    request.body = vec![0; length];
    if reader.read_exact(&mut request.body).is_err() {
        return;
    }
    seen.lock().unwrap().push(request.clone());
    let (status, headers, body) = handler(&request);
    let mut out = format!("HTTP/1.1 {status} Fake\r\nConnection: close\r\n");
    if !headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-length"))
    {
        out.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    for (name, value) in headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    let mut socket = socket;
    let _ = socket.write_all(out.as_bytes());
    if request.method != "HEAD" {
        let _ = socket.write_all(&body);
    }
    let _ = socket.flush();
}

/// An authoritative name server for TXT records only.
pub struct FakeDns {
    address: SocketAddr,
    records: Arc<Mutex<BTreeMap<String, Option<String>>>>,
    queries: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl FakeDns {
    pub fn start() -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        let address = socket.local_addr().unwrap();
        let records = Arc::new(Mutex::new(BTreeMap::<String, Option<String>>::new()));
        let queries = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (records, queries, stop) = (records.clone(), queries.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut buffer = [0u8; 512];
                while !stop.load(Ordering::Acquire) {
                    let Ok((n, peer)) = socket.recv_from(&mut buffer) else {
                        continue;
                    };
                    queries.fetch_add(1, Ordering::SeqCst);
                    if let Some(reply) = answer(&buffer[..n], &records.lock().unwrap()) {
                        let _ = socket.send_to(&reply, peer);
                    }
                }
            })
        };
        Self {
            address,
            records,
            queries,
            stop,
            worker: Some(worker),
        }
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    /// `None` answers NXDOMAIN.
    pub fn set(&self, name: &str, value: Option<&str>) {
        self.records
            .lock()
            .unwrap()
            .insert(name.to_ascii_lowercase(), value.map(str::to_owned));
    }
    pub fn queries(&self) -> usize {
        self.queries.load(Ordering::SeqCst)
    }
}
impl Drop for FakeDns {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn answer(query: &[u8], records: &BTreeMap<String, Option<String>>) -> Option<Vec<u8>> {
    if query.len() < 17 {
        return None;
    }
    let mut labels = Vec::new();
    let mut at = 12;
    loop {
        let len = *query.get(at)? as usize;
        at += 1;
        if len == 0 {
            break;
        }
        labels.push(String::from_utf8_lossy(query.get(at..at + len)?).to_ascii_lowercase());
        at += len;
    }
    let question_end = at + 4;
    let name = labels.join(".");
    let mut reply = query[..2].to_vec();
    let found = records.get(&name);
    let rcode = if matches!(found, Some(Some(_))) { 0 } else { 3 };
    reply.extend_from_slice(&(0x8400u16 | rcode).to_be_bytes());
    reply.extend_from_slice(&[0, 1, 0, u8::from(rcode == 0), 0, 0, 0, 0]);
    reply.extend_from_slice(query.get(12..question_end)?);
    if let Some(Some(value)) = found {
        reply.extend_from_slice(&[0xc0, 12, 0, 16, 0, 1, 0, 0, 0, 60]);
        reply.extend_from_slice(&((value.len() + 1) as u16).to_be_bytes());
        reply.push(value.len() as u8);
        reply.extend_from_slice(value.as_bytes());
    }
    Some(reply)
}

/// A temporary state directory, removed on drop.
pub struct TempDir(pub std::path::PathBuf);
impl TempDir {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "public-name-{name}-{}",
            crate::admin::auth::random_hex(6).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What the fake CA does differently from a plain successful order.
#[derive(Clone, Debug, Default)]
pub struct FakeAcmeOptions {
    /// The authorization stays `pending` for this many polls after the challenge response.
    pub pending_polls: u32,
    /// The order stays `processing` for this many polls after finalize.
    pub processing_polls: u32,
    /// The next this many `newOrder` requests are refused with `badNonce`.
    pub bad_nonces: u32,
    /// `newOrder` answers 429 `rateLimited` with this `Retry-After`.
    pub rate_limited: Option<u64>,
    /// The TXT record never matches, so the challenge fails.
    pub wrong_txt: bool,
    /// ARI: the directory offers `renewalInfo` with this suggested window (RFC 3339).
    pub ari: Option<(String, String)>,
}
struct FakeOrder {
    account: usize,
    name: String,
    token: String,
    authorization: &'static str,
    challenge_error: Option<String>,
    pending_left: u32,
    status: &'static str,
    processing_left: u32,
    chain: Option<String>,
    cert_id: Option<String>,
}
struct AcmeState {
    options: FakeAcmeOptions,
    nonces: std::collections::HashSet<String>,
    counter: u64,
    accounts: Vec<serde_json::Value>,
    orders: Vec<FakeOrder>,
    log: Vec<String>,
    violations: Vec<String>,
    replaced: Vec<String>,
    txt: Arc<Mutex<Option<String>>>,
    ca_key: rcgen::KeyPair,
    ca: rcgen::Certificate,
}
/// An in-process ACME CA over plain HTTP that checks what a real one would: ES256 JWS with raw
/// r‖s signatures, single-use nonces, `url`, `jwk` only for `newAccount` and `kid` otherwise,
/// POST-as-GET, and the DNS-01 TXT digest against [`FakeAcme::txt`].
pub struct FakeAcme {
    server: FakeHttp,
    state: Arc<Mutex<AcmeState>>,
}
impl FakeAcme {
    pub fn start(options: FakeAcmeOptions) -> Self {
        let ca_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Fake ACME CA");
        let ca = params.self_signed(&ca_key).unwrap();
        let state = Arc::new(Mutex::new(AcmeState {
            options,
            nonces: Default::default(),
            counter: 0,
            accounts: Vec::new(),
            orders: Vec::new(),
            log: Vec::new(),
            violations: Vec::new(),
            replaced: Vec::new(),
            txt: Arc::default(),
            ca_key,
            ca,
        }));
        let handler = state.clone();
        let server = FakeHttp::start(move |request| handler.lock().unwrap().handle(request));
        Self { server, state }
    }
    pub fn base(&self) -> String {
        self.server.url()
    }
    pub fn directory(&self) -> String {
        format!("{}/directory", self.base())
    }
    /// The `_acme-challenge` record the CA looks up.
    pub fn txt(&self) -> Arc<Mutex<Option<String>>> {
        self.state.lock().unwrap().txt.clone()
    }
    pub fn token(&self, order: usize) -> String {
        self.state.lock().unwrap().orders[order].token.clone()
    }
    pub fn cert_id(&self, order: usize) -> String {
        self.state.lock().unwrap().orders[order]
            .cert_id
            .clone()
            .unwrap()
    }
    pub fn orders(&self) -> usize {
        self.state.lock().unwrap().orders.len()
    }
    pub fn log(&self) -> Vec<String> {
        self.state.lock().unwrap().log.clone()
    }
    pub fn violations(&self) -> Vec<String> {
        self.state.lock().unwrap().violations.clone()
    }
    pub fn replaced(&self) -> Vec<String> {
        self.state.lock().unwrap().replaced.clone()
    }
    pub fn set_rate_limited(&self, seconds: Option<u64>) {
        self.state.lock().unwrap().options.rate_limited = seconds;
    }
}
struct RawKey(Vec<u8>);
impl rcgen::PublicKeyData for RawKey {
    fn der_bytes(&self) -> &[u8] {
        &self.0
    }
    fn algorithm(&self) -> &rcgen::SignatureAlgorithm {
        &rcgen::PKCS_ECDSA_P256_SHA256
    }
}
/// RFC 7638 thumbprint, written out here independently of the client's.
fn fake_thumbprint(jwk: &serde_json::Value) -> String {
    use sha2::Digest;
    let canonical = format!(
        r#"{{"crv":"{}","kty":"{}","x":"{}","y":"{}"}}"#,
        jwk["crv"].as_str().unwrap_or(""),
        jwk["kty"].as_str().unwrap_or(""),
        jwk["x"].as_str().unwrap_or(""),
        jwk["y"].as_str().unwrap_or("")
    );
    super::jws::b64(&sha2::Sha256::digest(canonical.as_bytes()))
}
impl AcmeState {
    fn nonce(&mut self) -> String {
        self.counter += 1;
        let nonce = format!("nonce-{}", self.counter);
        self.nonces.insert(nonce.clone());
        nonce
    }
    fn reply(
        &mut self,
        status: u16,
        mut headers: Vec<(String, String)>,
        body: serde_json::Value,
    ) -> Reply {
        headers.push(("Replay-Nonce".into(), self.nonce()));
        headers.push(("Content-Type".into(), "application/json".into()));
        (status, headers, body.to_string().into_bytes())
    }
    fn problem(&mut self, status: u16, kind: &str, detail: &str) -> Reply {
        self.log.push(format!("problem {kind}"));
        let mut reply = self.reply(
            status,
            Vec::new(),
            serde_json::json!({"type": format!("urn:ietf:params:acme:error:{kind}"), "detail": detail}),
        );
        reply.1.retain(|(k, _)| k != "Content-Type");
        reply
            .1
            .push(("Content-Type".into(), "application/problem+json".into()));
        reply
    }
    fn violation(&mut self, what: String) -> Reply {
        self.violations.push(what.clone());
        self.problem(400, "malformed", &what)
    }
    fn order_json(&self, base: &str, id: usize) -> serde_json::Value {
        let order = &self.orders[id];
        let mut json = serde_json::json!({
            "status": order.status,
            "identifiers": [{"type": "dns", "value": order.name}],
            "authorizations": [format!("{base}/authz/{id}")],
            "finalize": format!("{base}/finalize/{id}"),
        });
        if order.chain.is_some() && order.status == "valid" {
            json["certificate"] = format!("{base}/cert/{id}").into();
        }
        json
    }
    /// Checks the JWS and returns the account, the payload (`None` for POST-as-GET) and the
    /// header, or the error reply.
    fn verify(
        &mut self,
        request: &SeenRequest,
    ) -> Result<(Option<usize>, Option<serde_json::Value>, serde_json::Value), Reply> {
        use super::jws::unb64;
        use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
        if request.header("content-type") != Some("application/jose+json") {
            return Err(self.violation("content type".into()));
        }
        let Ok(body) = serde_json::from_slice::<serde_json::Value>(&request.body) else {
            return Err(self.violation("body is not JSON".into()));
        };
        let field = |k: &str| body[k].as_str().map(str::to_owned);
        let (Some(protected64), Some(payload64), Some(signature64)) =
            (field("protected"), field("payload"), field("signature"))
        else {
            return Err(self.violation("not a flattened JWS".into()));
        };
        let Some(protected) =
            unb64(&protected64).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        else {
            return Err(self.violation("protected header".into()));
        };
        if protected["alg"] != "ES256" {
            return Err(self.violation("alg".into()));
        }
        if protected["url"].as_str() != Some(request.url().as_str()) {
            return Err(self.violation(format!("url {} for {}", protected["url"], request.url())));
        }
        let nonce = protected["nonce"].as_str().unwrap_or("").to_string();
        if !self.nonces.remove(&nonce) {
            return Err(self.problem(400, "badNonce", "JWS has an invalid anti-replay nonce"));
        }
        if request.path == "/order" && self.options.bad_nonces > 0 {
            self.options.bad_nonces -= 1;
            return Err(self.problem(400, "badNonce", "JWS has an invalid anti-replay nonce"));
        }
        let new_account = request.path == "/account";
        let (account, jwk) = match (protected.get("jwk"), protected["kid"].as_str()) {
            (Some(jwk), None) if new_account => (None, jwk.clone()),
            (None, Some(kid)) if !new_account => {
                let index = kid
                    .rsplit('/')
                    .next()
                    .and_then(|i| i.parse::<usize>().ok())
                    .filter(|i| *i < self.accounts.len() && kid.starts_with("http://"));
                match index {
                    Some(i) => (Some(i), self.accounts[i].clone()),
                    None => {
                        return Err(self.problem(400, "accountDoesNotExist", "no such account"))
                    }
                }
            }
            _ => return Err(self.violation(format!("jwk/kid use on {}", request.path))),
        };
        let mut point = vec![4u8];
        for coordinate in ["x", "y"] {
            match jwk[coordinate]
                .as_str()
                .and_then(unb64)
                .filter(|c| c.len() == 32)
            {
                Some(bytes) => point.extend(bytes),
                None => return Err(self.violation("jwk coordinates".into())),
            }
        }
        let verified = unb64(&signature64)
            .filter(|s| s.len() == 64)
            .and_then(|s| Signature::from_slice(&s).ok())
            .zip(VerifyingKey::from_sec1_bytes(&point).ok())
            .is_some_and(|(signature, key)| {
                key.verify(format!("{protected64}.{payload64}").as_bytes(), &signature)
                    .is_ok()
            });
        if !verified {
            return Err(self.violation("signature".into()));
        }
        let payload = if payload64.is_empty() {
            None
        } else {
            match unb64(&payload64).and_then(|b| serde_json::from_slice(&b).ok()) {
                Some(p) => Some(p),
                None => return Err(self.violation("payload".into())),
            }
        };
        let account = match account {
            Some(i) => Some(i),
            None => self.accounts.iter().position(|a| *a == jwk).or_else(|| {
                if payload
                    .as_ref()
                    .is_some_and(|p| *p == serde_json::json!({"termsOfServiceAgreed": true}))
                {
                    self.accounts.push(jwk.clone());
                }
                None
            }),
        };
        let mut header = protected;
        header["jwk"] = jwk;
        Ok((account, payload, header))
    }
    fn handle(&mut self, request: &SeenRequest) -> Reply {
        let base = format!("http://{}", request.header("host").unwrap_or(""));
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/directory") => {
                let mut directory = serde_json::json!({
                    "newNonce": format!("{base}/nonce"),
                    "newAccount": format!("{base}/account"),
                    "newOrder": format!("{base}/order"),
                    "meta": {"termsOfService": "https://example.invalid/terms"},
                });
                if self.options.ari.is_some() {
                    directory["renewalInfo"] = format!("{base}/ari/").into();
                }
                return (200, vec![], directory.to_string().into_bytes());
            }
            ("HEAD" | "GET", "/nonce") => {
                let nonce = self.nonce();
                return (200, vec![("Replay-Nonce".into(), nonce)], Vec::new());
            }
            ("GET", path) if path.starts_with("/ari/") => {
                self.log.push("renewalInfo".into());
                let id = &path[5..];
                let known = self.orders.iter().any(|o| o.cert_id.as_deref() == Some(id));
                return match (&self.options.ari, known) {
                    (Some((start, end)), true) => (
                        200,
                        vec![("Retry-After".into(), "21600".into())],
                        serde_json::json!({"suggestedWindow": {"start": start, "end": end}})
                            .to_string()
                            .into_bytes(),
                    ),
                    _ => (404, vec![], Vec::new()),
                };
            }
            ("POST", _) => {}
            _ => return (404, vec![], Vec::new()),
        }
        let (account, payload, header) = match self.verify(request) {
            Ok(verified) => verified,
            Err(reply) => return reply,
        };
        let path = request.path.clone();
        let (route, id) = match path[1..].split_once('/') {
            Some((route, id)) => (route.to_string(), id.parse::<usize>().ok()),
            None => (path[1..].to_string(), None),
        };
        if let Some(id) = id {
            if id >= self.orders.len() || Some(self.orders[id].account) != account {
                return self.problem(404, "malformed", "no such resource");
            }
        }
        let post_as_get = |payload: &Option<serde_json::Value>| payload.is_none();
        match (route.as_str(), id) {
            ("account", None) => {
                let payload = payload.unwrap_or_default();
                if payload != serde_json::json!({"termsOfServiceAgreed": true}) {
                    return self.violation(format!("newAccount payload {payload}"));
                }
                self.log.push("newAccount jwk terms".into());
                let index = self
                    .accounts
                    .iter()
                    .position(|a| *a == header["jwk"])
                    .unwrap();
                let status = if account.is_some() { 200 } else { 201 };
                let location = format!("{base}/acct/{index}");
                self.reply(
                    status,
                    vec![("Location".into(), location)],
                    serde_json::json!({"status": "valid"}),
                )
            }
            ("order", None) => {
                let account = account.unwrap();
                self.log.push("newOrder kid".into());
                if let Some(seconds) = self.options.rate_limited {
                    let mut reply =
                        self.problem(429, "rateLimited", "too many certificates already issued");
                    reply.1.push(("Retry-After".into(), seconds.to_string()));
                    return reply;
                }
                let payload = payload.unwrap_or_default();
                let identifiers = payload["identifiers"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let [identifier] = identifiers.as_slice() else {
                    return self.violation("identifiers".into());
                };
                if identifier["type"] != "dns" {
                    return self.violation("identifier type".into());
                }
                if let Some(replaces) = payload["replaces"].as_str() {
                    if self.options.ari.is_none() {
                        return self.violation("replaces without ARI".into());
                    }
                    self.replaced.push(replaces.into());
                }
                self.counter += 1;
                self.orders.push(FakeOrder {
                    account,
                    name: identifier["value"].as_str().unwrap_or("").into(),
                    token: format!("token-{}", self.counter),
                    authorization: "pending",
                    challenge_error: None,
                    pending_left: self.options.pending_polls,
                    status: "pending",
                    processing_left: self.options.processing_polls,
                    chain: None,
                    cert_id: None,
                });
                let id = self.orders.len() - 1;
                let body = self.order_json(&base, id);
                self.reply(
                    201,
                    vec![("Location".into(), format!("{base}/order/{id}"))],
                    body,
                )
            }
            ("authz", Some(id)) if post_as_get(&payload) => {
                self.log.push("authz kid".into());
                let order = &mut self.orders[id];
                if order.authorization == "processing" {
                    if order.pending_left > 0 {
                        order.pending_left -= 1;
                    } else if order.challenge_error.is_some() {
                        order.authorization = "invalid";
                        order.status = "invalid";
                    } else {
                        order.authorization = "valid";
                        order.status = "ready";
                    }
                }
                let shown = if order.authorization == "processing" {
                    "pending"
                } else {
                    order.authorization
                };
                let mut challenge = serde_json::json!({
                    "type": "dns-01", "url": format!("{base}/chall/{id}"), "token": order.token,
                    "status": if shown == "pending" { "pending" } else { shown },
                });
                if let (Some(error), "invalid") = (&order.challenge_error, shown) {
                    challenge["error"] = serde_json::json!({
                        "type": "urn:ietf:params:acme:error:unauthorized", "detail": error});
                }
                let body = serde_json::json!({
                    "status": shown,
                    "identifier": {"type": "dns", "value": order.name},
                    "challenges": [
                        {"type": "http-01", "url": format!("{base}/http/{id}"), "token": "other", "status": "pending"},
                        challenge,
                    ],
                });
                self.reply(200, vec![], body)
            }
            ("chall", Some(id)) => {
                if payload != Some(serde_json::json!({})) {
                    return self.violation("challenge response payload".into());
                }
                self.log.push("challenge kid".into());
                let expected = super::jws::b64(&<sha2::Sha256 as sha2::Digest>::digest(
                    format!(
                        "{}.{}",
                        self.orders[id].token,
                        fake_thumbprint(&header["jwk"])
                    )
                    .as_bytes(),
                ));
                let found = self.txt.lock().unwrap().clone();
                let order = &mut self.orders[id];
                if self.options.wrong_txt || found.as_deref() != Some(expected.as_str()) {
                    order.challenge_error = Some(format!(
                        "Incorrect TXT record {:?} found at _acme-challenge.{}",
                        found.unwrap_or_default(),
                        order.name
                    ));
                }
                order.authorization = "processing";
                self.reply(
                    200,
                    vec![],
                    serde_json::json!({"type": "dns-01", "status": "processing"}),
                )
            }
            ("order", Some(id)) if post_as_get(&payload) => {
                self.log.push("order kid".into());
                let order = &mut self.orders[id];
                if order.status == "processing" {
                    if order.processing_left > 0 {
                        order.processing_left -= 1;
                    } else {
                        order.status = "valid";
                    }
                }
                let body = self.order_json(&base, id);
                self.reply(200, vec![], body)
            }
            ("finalize", Some(id)) => {
                self.log.push("finalize kid".into());
                if self.orders[id].status != "ready" {
                    return self.problem(403, "orderNotReady", "order not ready");
                }
                let csr = payload
                    .as_ref()
                    .and_then(|p| p["csr"].as_str())
                    .and_then(super::jws::unb64)
                    .and_then(|der| super::cert::csr_request(&der));
                let Some((names, key)) = csr else {
                    return self.problem(400, "badCSR", "unreadable CSR");
                };
                if names != [self.orders[id].name.clone()] {
                    return self.problem(400, "badCSR", "CSR names differ from the order");
                }
                let (chain, cert_id) = self.issue(&self.orders[id].name.clone(), key);
                let order = &mut self.orders[id];
                order.chain = Some(chain);
                order.cert_id = Some(cert_id);
                order.status = if order.processing_left > 0 {
                    "processing"
                } else {
                    "valid"
                };
                let body = self.order_json(&base, id);
                self.reply(200, vec![], body)
            }
            ("cert", Some(id)) if post_as_get(&payload) => {
                if request.header("accept") != Some("application/pem-certificate-chain") {
                    return self.violation("certificate Accept".into());
                }
                self.log.push("certificate kid".into());
                let chain = self.orders[id].chain.clone().unwrap_or_default();
                let nonce = self.nonce();
                (
                    200,
                    vec![
                        ("Replay-Nonce".into(), nonce),
                        (
                            "Content-Type".into(),
                            "application/pem-certificate-chain".into(),
                        ),
                    ],
                    chain.into_bytes(),
                )
            }
            _ => self.violation(format!("unexpected {} {}", request.method, request.path)),
        }
    }
    /// A 90-day leaf for `name` and `key`, its PEM chain and its ARI certificate identifier.
    fn issue(&mut self, name: &str, key: Vec<u8>) -> (String, String) {
        use der::{oid::AssociatedOid, Decode};
        use x509_cert::ext::pkix::SubjectKeyIdentifier;
        let now = crate::platform::wall_clock().as_secs();
        let epoch = rcgen::date_time_ymd(1970, 1, 1);
        let mut params = rcgen::CertificateParams::new(vec![name.to_string()]).unwrap();
        params.distinguished_name = rcgen::DistinguishedName::new();
        params.not_before = epoch + Duration::from_secs(now - 3600);
        params.not_after = epoch + Duration::from_secs(now - 3600 + 90 * 86_400);
        self.counter += 1;
        let serial = vec![0x80, 0, 0, self.counter as u8];
        params.serial_number = Some(rcgen::SerialNumber::from_slice(&serial));
        params.use_authority_key_identifier_extension = true;
        let leaf = params
            .signed_by(&RawKey(key), &self.ca, &self.ca_key)
            .unwrap();
        let ca = x509_cert::Certificate::from_der(self.ca.der()).unwrap();
        let ski = ca
            .tbs_certificate
            .extensions
            .iter()
            .flatten()
            .find(|e| e.extn_id == SubjectKeyIdentifier::OID)
            .map(|e| SubjectKeyIdentifier::from_der(e.extn_value.as_bytes()).unwrap())
            .unwrap();
        // The serial's DER content: a leading zero keeps it positive.
        let mut content = vec![0];
        content.extend(&serial);
        let cert_id = format!(
            "{}.{}",
            super::jws::b64(ski.0.as_bytes()),
            super::jws::b64(&content)
        );
        (format!("{}{}", leaf.pem(), self.ca.pem()), cert_id)
    }
}
