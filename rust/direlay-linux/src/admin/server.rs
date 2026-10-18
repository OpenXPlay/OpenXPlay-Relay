// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Source: admin-web/dev-server.ts.
use super::{
    auth::{self, LoginError, PasswordError},
    types::*,
    AdminAuth,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, RwLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
pub const MAX_BODY: usize = 16_384;
pub const MAX_HEADERS: usize = 8_192;
pub const DEFAULT_PORT: u16 = 6975;
const COOKIE: &str = "oxp_admin";
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub peer: IpAddr,
}
impl Request {
    fn token(&self) -> &str {
        self.headers
            .get("cookie")
            .and_then(|s| {
                s.split(';').find_map(|p| {
                    let (k, v) = p.trim().split_once('=')?;
                    (k == COOKIE).then_some(v)
                })
            })
            .unwrap_or("")
    }
}
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
impl Response {
    fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            headers: BTreeMap::from([
                ("Content-Type".into(), "application/json".into()),
                ("Cache-Control".into(), "no-store".into()),
            ]),
            body: serde_json::to_vec(&value).unwrap(),
        }
    }
    fn error(status: u16, key: &str) -> Self {
        Self::json(status, json!({"error": key}))
    }
    fn cookie(mut self, token: &str, age: u64) -> Self {
        self.headers.insert(
            "Set-Cookie".into(),
            format!("{COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={age}"),
        );
        self
    }
    fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            413 => "Content Too Large",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            _ => "Internal Server Error",
        };
        write!(out, "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n", self.status, reason, self.body.len())?;
        for (key, value) in &self.headers {
            write!(out, "{key}: {value}\r\n")?;
        }
        out.write_all(b"\r\n")?;
        out.write_all(&self.body)?;
        out.flush()
    }
}
#[derive(Clone)]
pub struct AdminServerConfig {
    pub bind: SocketAddr,
    pub state_dir: PathBuf,
    pub static_dir: PathBuf,
    pub addresses: Vec<Ipv4Addr>,
    pub workers: usize,
}
impl AdminServerConfig {
    pub fn new(state_dir: PathBuf, static_dir: PathBuf, addresses: Vec<Ipv4Addr>) -> Self {
        Self {
            bind: ([0, 0, 0, 0], DEFAULT_PORT).into(),
            state_dir,
            static_dir,
            addresses,
            workers: 4,
        }
    }
}
pub struct AdminServer {
    admin: Arc<dyn RelayAdmin>,
    auth: Mutex<AdminAuth>,
    config: AdminServerConfig,
    pin_attempt: Mutex<Option<u64>>,
}
impl AdminServer {
    pub fn new(admin: Arc<dyn RelayAdmin>, auth: AdminAuth, config: AdminServerConfig) -> Self {
        Self {
            admin,
            auth: Mutex::new(auth),
            config,
            pin_attempt: Mutex::new(None),
        }
    }
    /// Answers `request` at `now`, in `auth::monotonic_seconds`.
    pub fn route(&self, request: &Request, now: u64) -> Response {
        if !auth::private_address(request.peer) {
            return Response::error(403, "error.request");
        }
        if request.body.len() > MAX_BODY {
            return Response::error(413, "error.request");
        }
        let path = request.path.split('?').next().unwrap_or("");
        if !path.starts_with("/api/") {
            return self.static_file(&request.method, path);
        }
        let Some(path) = path.strip_prefix("/api/v0").or_else(|| {
            (path == "/api/carlink/pin"
                || path == "/api/carlink/connect"
                || path == "/api/actions/forget-carlink")
                .then(|| &path[4..])
        }) else {
            return Response::error(404, "error.request");
        };
        let method = request.method.as_str();
        let ready = self.auth.lock().unwrap().ready(request.token(), now);
        if path != "/session" && path != "/login" && !ready {
            return Response::error(401, "password");
        }
        let body = if method != "GET" {
            if request.headers.get("x-openxplay-admin").map(String::as_str) != Some("1")
                || !request.headers.get("content-type").is_some_and(|s| {
                    s.split(';')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .eq_ignore_ascii_case("application/json")
                })
            {
                return Response::error(403, "error.request");
            }
            match serde_json::from_slice::<Value>(if request.body.is_empty() {
                b"{}"
            } else {
                &request.body
            }) {
                Ok(Value::Object(v)) => v,
                _ => return Response::error(400, "error.request"),
            }
        } else {
            Values::new()
        };
        let string = |key: &str| body.get(key).and_then(Value::as_str);
        match (method, path) {
            ("GET", "/session") => Response::json(
                200,
                json!({"state": if ready {"ready"} else {"login"}, "defaultPassword": self.auth.lock().unwrap().default_password()}),
            ),
            ("POST", "/login") => match self.auth.lock().unwrap().login(
                request.peer,
                string("password").unwrap_or(""),
                now,
            ) {
                Ok(token) => {
                    Response::json(200, json!({"ok":true})).cookie(&token, auth::SESSION_SECONDS)
                }
                Err(LoginError::Password) => Response::error(401, "password"),
                Err(LoginError::RateLimit(seconds)) => {
                    let mut r = Response::error(429, "error.rateLimit");
                    r.headers.insert("Retry-After".into(), seconds.to_string());
                    r
                }
                Err(LoginError::Storage(_)) => Response::error(500, "error.request"),
            },
            ("POST", "/password") => match self
                .auth
                .lock()
                .unwrap()
                .change_password(string("current").unwrap_or(""), string("new").unwrap_or(""))
            {
                Ok(()) => Response::json(200, json!({"ok":true})),
                Err(PasswordError::Current) => Response::error(403, "password"),
                Err(PasswordError::Format) => Response::error(422, "error.passwordFormat"),
                Err(PasswordError::Storage(_)) => Response::error(500, "error.request"),
            },
            ("POST", "/logout") => match self.auth.lock().unwrap().logout(request.token()) {
                Ok(()) => Response::json(200, json!({"ok":true})).cookie("", 0),
                Err(_) => Response::error(500, "error.request"),
            },
            ("GET", "/status") => {
                let mut status = self.admin.status();
                if status.wifi_provisioning == Some(true) {
                    status.wifi_client = Some(self.admin.wifi_status());
                }
                // Diagnostics are large and change every second; only the developer section asks.
                if request
                    .path
                    .split_once('?')
                    .is_some_and(|(_, query)| query.split('&').any(|pair| pair == "diagnostics=1"))
                {
                    status.receivers = Some(self.admin.receivers());
                }
                Response::json(200, json!(status))
            }
            ("POST", path @ ("/carlink/pin" | "/carlink/connect")) => {
                let mut last = self.pin_attempt.lock().unwrap();
                if last.is_some_and(|last| now < last.saturating_add(2)) {
                    let mut response = Response::error(429, "error.rateLimit");
                    response.headers.insert("Retry-After".into(), "2".into());
                    return response;
                }
                *last = Some(now);
                if path == "/carlink/connect" && !body.contains_key("pin") {
                    return result(self.admin.carlink_connect(None));
                }
                match string("pin") {
                    Some(pin)
                        if (4..=8).contains(&pin.len())
                            && pin.bytes().all(|b| b.is_ascii_digit()) =>
                    {
                        result(if path == "/carlink/connect" {
                            self.admin.carlink_connect(Some(pin))
                        } else {
                            self.admin.carlink_pin(pin)
                        })
                    }
                    _ => Response::error(422, "error.carlinkPin"),
                }
            }
            ("GET", "/settings") => Response::json(200, json!(self.admin.settings())),
            ("GET", "/phones") => Response::json(200, json!(self.admin.phones())),
            ("PUT", "/settings") => match self.admin.update(body) {
                AdminUpdate::AppliedNow => Response::json(200, json!({"ok":true,"applied":"now"})),
                AdminUpdate::NextStart => {
                    Response::json(200, json!({"ok":true,"applied":"next-start"}))
                }
                AdminUpdate::Errors(errors) => Response::json(422, json!({"errors":errors})),
            },
            ("PUT", "/hotspot") => match (string("ssid"), string("password")) {
                (Some(ssid), Some(password)) => {
                    match validate_hotspot_credentials(ssid, password) {
                        Ok(()) => result(self.admin.hotspot_update(ssid, password)),
                        Err(key) => Response::error(422, key),
                    }
                }
                _ => Response::error(422, "error.invalid"),
            },
            ("GET", "/wifi/networks") => match self.admin.wifi_networks() {
                Ok(v) => Response::json(200, json!(v)),
                Err(e) => Response::error(409, &e),
            },
            ("PUT", "/wifi") => match (string("ssid"), string("password")) {
                (Some(ssid), password)
                    if !ssid.is_empty()
                        && ssid.len() <= 32
                        && !ssid.contains(['\r', '\n', '\0'])
                        && (!body.contains_key("password") || password.is_some())
                        && password.is_none_or(|p| !p.contains('\0')) =>
                {
                    result(self.admin.wifi_join(ssid, password))
                }
                _ => Response::error(422, "error.invalid"),
            },
            ("POST", path) if path.starts_with("/actions/") => {
                let name = &path[9..];
                if ![
                    "start",
                    "stop",
                    "restart",
                    "regenerate-pairing-code",
                    "forget-wifi",
                    "forget-carlink",
                    "pair-iphone",
                    "cancel-pairing",
                ]
                .contains(&name)
                {
                    Response::error(404, "error.request")
                } else {
                    result(self.admin.action(name))
                }
            }
            _ => Response::error(404, "error.request"),
        }
    }
    fn static_file(&self, method: &str, path: &str) -> Response {
        if method != "GET" {
            return Response::error(404, "error.request");
        }
        let Some(decoded) = decode_path(path) else {
            return Response::error(403, "error.request");
        };
        let relative = decoded.strip_prefix('/').unwrap_or(&decoded);
        if Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || decoded.contains('\\')
            || decoded.contains('\0')
        {
            return Response::error(403, "error.request");
        }
        let file = self.config.static_dir.join(if relative.is_empty() {
            "index.html"
        } else {
            relative
        });
        let checked = (|| {
            let root = self.config.static_dir.canonicalize()?;
            let file = file.canonicalize()?;
            if !file.starts_with(root) || !file.is_file() {
                return Err(io::Error::other("Invalid asset path"));
            }
            Ok(file)
        })();
        let Ok(file) = checked else {
            return Response::error(404, "error.request");
        };
        let data = (|| {
            let mut b = Vec::new();
            fs::File::open(&file)?
                .take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut b)?;
            if b.len() > 4 * 1024 * 1024 {
                return Err(io::Error::other("Asset too large"));
            }
            Ok::<_, io::Error>(b)
        })();
        match data {
            Ok(body) => {
                let mime = match file.extension().and_then(|s| s.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("js") => "text/javascript; charset=utf-8",
                    Some("css") => "text/css; charset=utf-8",
                    Some("svg") => "image/svg+xml",
                    Some("png") => "image/png",
                    Some("ico") => "image/x-icon",
                    Some("json") => "application/json",
                    Some("woff2") => "font/woff2",
                    _ => "application/octet-stream",
                };
                Response {
                    status: 200,
                    body,
                    headers: BTreeMap::from([
                        ("Content-Type".into(), mime.into()),
                        ("Cache-Control".into(), "no-cache".into()),
                    ]),
                }
            }
            Err(_) => Response::error(404, "error.request"),
        }
    }
    pub fn start(self: Arc<Self>) -> io::Result<AdminServerHandle> {
        let tls = Arc::new(RwLock::new(tls_config(
            &self.config.state_dir,
            &self.config.addresses,
        )?));
        let listener = TcpListener::bind(self.config.bind)?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel::<std::net::TcpStream>(16);
        let rx = Arc::new(Mutex::new(rx));
        let mut threads = Vec::new();
        for _ in 0..self.config.workers.clamp(1, 8) {
            let (rx, server, tls) = (rx.clone(), self.clone(), tls.clone());
            threads.push(
                thread::Builder::new()
                    .name("admin-http".into())
                    .stack_size(256 * 1024)
                    .spawn(move || loop {
                        let socket = match rx.lock().unwrap().recv() {
                            Ok(s) => s,
                            Err(_) => break,
                        };
                        // BSD sockets inherit the listener's non-blocking flag; Linux sockets do not.
                        let _ = socket.set_nonblocking(false);
                        let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                        let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                        let Ok(peer) = socket.peer_addr() else {
                            continue;
                        };
                        let Ok(connection) =
                            rustls::ServerConnection::new(tls.read().unwrap().clone())
                        else {
                            continue;
                        };
                        let mut stream = rustls::StreamOwned::new(connection, socket);
                        let response = match read_request(&mut stream, peer.ip()) {
                            Ok(r) => server.route(&r, auth::monotonic_seconds()),
                            Err(status) => Response::error(status, "error.request"),
                        };
                        let sent = response.write(&mut stream).is_ok();
                        stream.conn.send_close_notify();
                        let flushed = stream.flush().is_ok();
                        if sent && flushed {
                            server.admin.response_sent();
                        } else {
                            server.admin.response_failed();
                        }
                    })?,
            );
        }
        let stop = stopped.clone();
        threads.push(
            thread::Builder::new()
                .name("admin-accept".into())
                .stack_size(256 * 1024)
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((socket, _)) => {
                                let _ = tx.try_send(socket);
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(20))
                            }
                            Err(_) => break,
                        }
                    }
                })?,
        );
        Ok(AdminServerHandle {
            address,
            stopped,
            threads,
            tls,
            state_dir: self.config.state_dir.clone(),
        })
    }
}
fn result(r: AdminResult) -> Response {
    match r {
        Ok(()) => Response::json(200, json!({"ok":true})),
        Err(e) => Response::error(409, &e),
    }
}
fn decode_path(path: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut input = path.bytes();
    while let Some(b) = input.next() {
        if b == b'%' {
            let h = (input.next()? as char).to_digit(16)?;
            let l = (input.next()? as char).to_digit(16)?;
            bytes.push((h * 16 + l) as u8);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}
/// One request per TLS connection; ambiguous framing and transfer encodings are rejected.
pub fn read_request(input: &mut impl Read, peer: IpAddr) -> Result<Request, u16> {
    let mut header = Vec::new();
    let mut b = [0];
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= MAX_HEADERS {
            return Err(413);
        }
        input.read_exact(&mut b).map_err(|_| 400u16)?;
        header.push(b[0]);
    }
    let text = std::str::from_utf8(&header).map_err(|_| 400u16)?;
    let mut lines = text.split("\r\n");
    let first: Vec<_> = lines.next().ok_or(400u16)?.split(' ').collect();
    if first.len() != 3 || first[2] != "HTTP/1.1" || !first[1].starts_with('/') {
        return Err(400);
    }
    let mut headers = BTreeMap::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (k, v) = line.split_once(':').ok_or(400u16)?;
        if k.is_empty()
            || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || v.bytes().any(|b| b < 32 && b != b'\t')
        {
            return Err(400);
        }
        if headers
            .insert(k.to_ascii_lowercase(), v.trim().to_owned())
            .is_some()
        {
            return Err(400);
        }
    }
    if !headers.contains_key("host") || headers.contains_key("transfer-encoding") {
        return Err(400);
    }
    let length = headers
        .get("content-length")
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|_| 400u16)?
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err(413);
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body).map_err(|_| 400u16)?;
    Ok(Request {
        method: first[0].into(),
        path: first[1].into(),
        headers,
        body,
        peer,
    })
}
#[derive(Serialize, Deserialize)]
struct SavedTls {
    sans: Vec<String>,
    cert: Vec<u8>,
    key: Vec<u8>,
}
/// Certificate and key are atomically stored together, never leaving a mismatched pair.
pub fn tls_config(state: &Path, addresses: &[Ipv4Addr]) -> io::Result<Arc<rustls::ServerConfig>> {
    let mut sans = vec!["openxplay-linux.local".into(), "localhost".into()];
    sans.extend(addresses.iter().map(ToString::to_string));
    sans.sort();
    sans.dedup();
    let path = state.join("admin-tls.json");
    let saved = match fs::read(&path) {
        Ok(b) => Some(serde_json::from_slice::<SavedTls>(&b)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let saved = match saved {
        Some(s) if s.sans == sans => {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            s
        }
        _ => {
            let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
                .map_err(io::Error::other)?;
            let cert = rcgen::CertificateParams::new(sans.clone())
                .map_err(io::Error::other)?
                .self_signed(&key)
                .map_err(io::Error::other)?;
            let saved = SavedTls {
                sans,
                cert: cert.der().to_vec(),
                key: key.serialize_der(),
            };
            auth::private_file(&path, &serde_json::to_vec(&saved)?)?;
            saved
        }
    };
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(saved.cert)],
            rustls::pki_types::PrivatePkcs8KeyDer::from(saved.key).into(),
        )
        .map_err(io::Error::other)?;
    Ok(Arc::new(config))
}
pub struct AdminServerHandle {
    pub address: SocketAddr,
    stopped: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    tls: Arc<RwLock<Arc<rustls::ServerConfig>>>,
    state_dir: PathBuf,
}
impl AdminServerHandle {
    pub fn update_addresses(&self, addresses: &[Ipv4Addr]) -> io::Result<()> {
        *self.tls.write().unwrap() = tls_config(&self.state_dir, addresses)?;
        Ok(())
    }
    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..).rev() {
            let _ = thread.join();
        }
    }
}
impl Drop for AdminServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
