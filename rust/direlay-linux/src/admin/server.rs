// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15
// Source: admin-web/dev-server.ts.
#[path = "file_routes.rs"]
mod file_routes;
#[path = "carlink_trace.rs"]
pub(crate) mod trace_files;

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
// A 16 KiB file can expand sixfold when wrapped as a JSON string.
const MAX_IMPORT_BODY: usize = crate::public_name::config::MAX_BYTES * 6 + 1024;
fn body_limit(path: &str) -> usize {
    if path.split('?').next() == Some("/api/v0/config/import") {
        64 * 1024
    } else if path.split('?').next() == Some("/api/v0/public-name/import") {
        MAX_IMPORT_BODY
    } else {
        MAX_BODY
    }
}
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
    pub files: Vec<(fs::File, u64)>,
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
            files: Vec::new(),
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
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            _ => "Internal Server Error",
        };
        write!(out, "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n", self.status, reason, self.body.len() as u64 + self.files.iter().map(|(_, size)| size).sum::<u64>())?;
        for (key, value) in &self.headers {
            write!(out, "{key}: {value}\r\n")?;
        }
        out.write_all(b"\r\n")?;
        out.write_all(&self.body)?;
        let mut buffer = [0u8; 16 * 1024];
        for (file, length) in &self.files {
            let mut input = file.take(*length);
            loop {
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buffer[..n])?;
            }
        }
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
    /// A certificate served only for its own name (trusted HTTPS); empty by default.
    pub named: NamedCertificate,
}
impl AdminServerConfig {
    pub fn new(state_dir: PathBuf, static_dir: PathBuf, addresses: Vec<Ipv4Addr>) -> Self {
        Self {
            bind: ([0, 0, 0, 0], DEFAULT_PORT).into(),
            state_dir,
            static_dir,
            addresses,
            workers: 4,
            named: NamedCertificate::default(),
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
        if request.body.len() > body_limit(&request.path) {
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
        if path == "/carlink-trace" || path.starts_with("/carlink-trace/") {
            return self.trace_route(request, path, &body);
        }
        if let Some(response) = self.file_route(request, path, &body) {
            return response;
        }
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
                    status.receiver_history = Some(self.admin.receiver_history());
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
            ("POST", "/vehicle/sample") => match body.get("enabled").and_then(Value::as_bool) {
                Some(enabled) => result(self.admin.vehicle_sample(enabled)),
                None => Response::error(422, "error.invalid"),
            },
            ("GET", "/vehicle") => Response::json(200, self.admin.vehicle()),
            ("POST", "/carlink/pairing") => {
                let Some(request) = body.get("request").and_then(Value::as_u64) else {
                    return Response::error(422, "error.invalid");
                };
                let Some(generation) = body.get("generation").and_then(Value::as_u64) else {
                    return Response::error(422, "error.invalid");
                };
                let Some(action) = string("action") else {
                    return Response::error(422, "error.invalid");
                };
                result(
                    self.admin
                        .carlink_pairing(request, generation, action, string("pin")),
                )
            }
            ("GET", "/settings") => Response::json(200, json!(self.admin.settings())),
            ("POST", "/public-name/import") => {
                use crate::public_name::service::ImportError;
                let Some(content) = string("content") else {
                    return Response::json(422, json!({"errors":{"file":"not-json"}}));
                };
                match self.admin.public_name_import(content) {
                    Ok((provider, name)) => {
                        Response::json(200, json!({"ok":true,"provider":provider,"name":name}))
                    }
                    Err(ImportError::Invalid(errors)) => {
                        Response::json(422, json!({"errors":errors}))
                    }
                    Err(ImportError::Storage) => Response::error(500, "error.request"),
                }
            }
            ("GET", "/phones") => Response::json(200, json!(self.admin.phones())),
            ("PUT", "/settings") => match self.admin.update(body) {
                AdminUpdate::AppliedNow => Response::json(200, json!({"ok":true,"applied":"now"})),
                AdminUpdate::NextStart => {
                    Response::json(200, json!({"ok":true,"applied":"next-start"}))
                }
                AdminUpdate::Errors(errors) => Response::json(422, json!({"errors":errors})),
            },
            ("PUT", "/network-roles") | ("POST", "/network-roles/preview") => {
                use crate::network_roles::SaveResult;
                let result = if path == "/network-roles/preview" {
                    self.admin.network_roles_preview(body)
                } else {
                    self.admin.network_roles_update(body)
                };
                match result {
                    Some(SaveResult::Saved(proposal)) => Response::json(200, json!(proposal)),
                    Some(SaveResult::Probing) => Response::error(503, "probing"),
                    Some(SaveResult::Invalid(error)) => {
                        Response::json(400, json!({"error": error}))
                    }
                    Some(SaveResult::Checks(checks)) => {
                        Response::json(400, json!({"checks": checks, "applied": false}))
                    }
                    Some(SaveResult::Storage) => Response::error(500, "error.request"),
                    None => Response::error(404, "error.request"),
                }
            }
            ("PUT", "/hotspot") => match (string("ssid"), string("password")) {
                _ if body.get("security").is_some_and(|s| {
                    !matches!(
                        s.as_str(),
                        Some("open" | "wpa2" | "wpa3-transition" | "wpa3")
                    )
                }) =>
                {
                    Response::error(422, "error.invalid")
                }
                _ if body
                    .get("band")
                    .is_some_and(|b| !matches!(b.as_str(), Some("auto" | "2.4" | "5"))) =>
                {
                    Response::error(422, "error.invalid")
                }
                (Some(ssid), Some(password)) => {
                    match validate_hotspot_credentials(
                        ssid,
                        if password.is_empty() {
                            "openxplay"
                        } else {
                            password
                        },
                    ) {
                        Ok(()) => result(self.admin.hotspot_update(
                            ssid,
                            password,
                            string("band"),
                            string("security"),
                        )),
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
            ("POST", "/view-area") => {
                match body.get("index").and_then(Value::as_u64).filter(|i| *i < 7) {
                    Some(index) => result(self.admin.show_view_area(index as usize)),
                    None => Response::error(422, "error.invalid"),
                }
            }
            ("POST", path) if path.starts_with("/actions/") => {
                let name = &path[9..];
                if ![
                    "start",
                    "stop",
                    "cancel-arrival",
                    "restart",
                    "regenerate-pairing-code",
                    "clear-saved-head-unit",
                    "clear-window-records",
                    "forget-wifi",
                    "clear-vehicle-history",
                    "forget-carlink",
                    "pair-iphone",
                    "cancel-pairing",
                    "public-name-renew",
                    "public-name-remove",
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
    fn trace_route(&self, request: &Request, path: &str, body: &Values) -> Response {
        use trace_files as carlink_trace;
        let Some((enabled, running, status)) = self.admin.carlink_trace() else {
            return Response::error(404, "error.request");
        };
        let dir = self.config.state_dir.join("carlink-trace");
        match (request.method.as_str(), path) {
            ("GET", "/carlink-trace") => match carlink_trace::listing(&dir) {
                Ok(files) => Response::json(
                    200,
                    json!({"enabled":enabled,"running":running,"status":status,"files":files}),
                ),
                Err(_) => Response::error(500, "error.request"),
            },
            ("GET", "/carlink-trace/tail") => {
                let mut filter = None;
                let mut limit = 5000;
                for pair in request
                    .path
                    .split_once('?')
                    .map_or("", |(_, q)| q)
                    .split('&')
                    .filter(|p| !p.is_empty())
                {
                    match pair.split_once('=') {
                        Some(("type", t))
                            if [
                                "Rtsp",
                                "Frame",
                                "Media",
                                "Link",
                                "Error",
                                "Video",
                                "Input",
                                "Socket",
                                "Negotiation",
                            ]
                            .contains(&t) =>
                        {
                            filter = Some(t)
                        }
                        Some(("limit", n)) => match n.parse::<usize>() {
                            Ok(n) if n <= 5000 => limit = n,
                            _ => return Response::error(422, "error.invalid"),
                        },
                        _ => return Response::error(422, "error.invalid"),
                    }
                }
                match carlink_trace::tail(&dir, filter, limit) {
                    Ok(lines) => Response::json(200, json!({"lines":lines})),
                    Err(_) => Response::error(500, "error.request"),
                }
            }
            ("GET", "/carlink-trace/download") => {
                let Ok(entries) = carlink_trace::listing(&dir) else {
                    return Response::error(500, "error.request");
                };
                let names = entries
                    .iter()
                    .filter(|e| e["kind"] == "trace")
                    .map(|e| e["name"].as_str().unwrap())
                    .collect::<Vec<_>>();
                self.trace_download(
                    &dir,
                    &names,
                    &format!("carlink-trace-{}.jsonl", trace_timestamp()),
                    "application/x-ndjson",
                )
            }
            ("GET", path) if path.starts_with("/carlink-trace/files/") => {
                let name = &path[21..];
                let mime = match carlink_trace::kind(name) {
                    Some("trace") => "application/x-ndjson",
                    Some("media") => "video/mp2t",
                    _ => return Response::error(404, "error.request"),
                };
                self.trace_download(&dir, &[name], name, mime)
            }
            ("POST", "/carlink-trace/clear") => {
                result(self.admin.carlink_trace_control(true, "clear"))
            }
            ("POST", "/carlink-trace/capture") => {
                match body.get("action").and_then(Value::as_str) {
                    Some(action @ ("start" | "stop")) => {
                        if !enabled || !running {
                            return Response::error(409, "error.carlinkTraceOff");
                        }
                        result(self.admin.carlink_trace_control(false, action))
                    }
                    _ => Response::error(422, "error.invalid"),
                }
            }
            _ => Response::error(404, "error.request"),
        }
    }
    fn trace_download(&self, dir: &Path, names: &[&str], filename: &str, mime: &str) -> Response {
        let mut files = Vec::new();
        for name in names {
            let Ok(mut file) = trace_files::open(dir, name) else {
                return Response::error(404, "error.request");
            };
            let Ok(length) =
                trace_files::snapshot_length(&mut file, trace_files::kind(name) == Some("trace"))
            else {
                return Response::error(500, "error.request");
            };
            files.push((file, length));
        }
        Response {
            status: 200,
            body: Vec::new(),
            files,
            headers: BTreeMap::from([
                ("Content-Type".into(), mime.into()),
                ("Cache-Control".into(), "no-store".into()),
                (
                    "Content-Disposition".into(),
                    format!("attachment; filename=\"{filename}\""),
                ),
            ]),
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
                    files: Vec::new(),
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
        let tls = Arc::new(RwLock::new(tls_config_with(
            &self.config.state_dir,
            &self.config.addresses,
            &self.config.named,
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
            named: self.config.named.clone(),
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
    if length > body_limit(first[1]) {
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
/// A certificate the admin page serves only to clients that ask for its name (SNI): the
/// trusted-HTTPS name's. It changes in place, without rebuilding the TLS configuration.
type Named = Option<(String, Arc<rustls::sign::CertifiedKey>)>;
#[derive(Clone, Debug, Default)]
pub struct NamedCertificate(Arc<RwLock<Named>>);
impl NamedCertificate {
    /// Serves the PEM `chain` and PKCS #8 `key` for `name` from now on.
    pub fn set(&self, name: &str, chain: &[u8], key: &[u8]) -> io::Result<()> {
        use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
        let certs = CertificateDer::pem_slice_iter(chain)
            .collect::<Result<Vec<_>, _>>()
            .map_err(io::Error::other)?;
        if certs.is_empty() {
            return Err(io::Error::other("empty certificate chain"));
        }
        let key = PrivateKeyDer::from_pem_slice(key).map_err(io::Error::other)?;
        let signer =
            rustls::crypto::ring::sign::any_supported_type(&key).map_err(io::Error::other)?;
        *self.0.write().unwrap() = Some((
            name.to_ascii_lowercase(),
            Arc::new(rustls::sign::CertifiedKey::new(certs, signer)),
        ));
        Ok(())
    }
    pub fn clear(&self) {
        *self.0.write().unwrap() = None;
    }
    pub fn name(&self) -> Option<String> {
        self.0
            .read()
            .unwrap()
            .as_ref()
            .map(|(name, _)| name.clone())
    }
}
/// SNI equal to the named certificate's name gets that certificate; everything else, including
/// clients that send no SNI (IP addresses), the relay's own.
#[derive(Debug)]
struct SniResolver {
    own: Arc<rustls::sign::CertifiedKey>,
    named: NamedCertificate,
}
impl rustls::server::ResolvesServerCert for SniResolver {
    fn resolve(
        &self,
        hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        if let (Some(sni), Some((name, key))) =
            (hello.server_name(), &*self.named.0.read().unwrap())
        {
            if sni.eq_ignore_ascii_case(name) {
                return Some(key.clone());
            }
        }
        Some(self.own.clone())
    }
}
/// Certificate and key are atomically stored together, never leaving a mismatched pair.
pub fn tls_config(state: &Path, addresses: &[Ipv4Addr]) -> io::Result<Arc<rustls::ServerConfig>> {
    tls_config_with(state, addresses, &NamedCertificate::default())
}
/// Oldest entries come first. Keep the current requirements and evict older extras at the cap.
fn accumulated_sans(previous: &[String], names: &[String], addresses: &[Ipv4Addr]) -> Vec<String> {
    let needed_names: Vec<_> = names.iter().take(8).cloned().collect();
    let needed_addresses: Vec<_> = addresses.iter().take(16).map(ToString::to_string).collect();
    let accumulate = |required: &[String], is_address: bool, cap: usize| {
        let mut entries = Vec::new();
        for entry in previous
            .iter()
            .filter(|s| s.parse::<IpAddr>().is_ok() == is_address)
            .chain(required)
        {
            if !entries.contains(entry) {
                entries.push(entry.clone());
            }
        }
        while entries.len() > cap {
            let index = entries.iter().position(|s| !required.contains(s)).unwrap();
            entries.remove(index);
        }
        entries
    };
    let mut sans = accumulate(&needed_names, false, 8);
    sans.extend(accumulate(&needed_addresses, true, 16));
    sans
}
/// [`tls_config`] that also serves `named` for its own name.
pub fn tls_config_with(
    state: &Path,
    addresses: &[Ipv4Addr],
    named: &NamedCertificate,
) -> io::Result<Arc<rustls::ServerConfig>> {
    let names = vec!["openxplay-linux.local".into(), "localhost".into()];
    let path = state.join("admin-tls.json");
    let saved = match fs::read(&path) {
        Ok(b) => Some(serde_json::from_slice::<SavedTls>(&b)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let sans = accumulated_sans(
        saved.as_ref().map_or(&[], |s| s.sans.as_slice()),
        &names,
        addresses,
    );
    let saved = match saved {
        Some(s) if s.sans.len() == sans.len() && sans.iter().all(|name| s.sans.contains(name)) => {
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
    let key = rustls::pki_types::PrivateKeyDer::from(rustls::pki_types::PrivatePkcs8KeyDer::from(
        saved.key,
    ));
    let own = Arc::new(rustls::sign::CertifiedKey::new(
        vec![rustls::pki_types::CertificateDer::from(saved.cert)],
        provider
            .key_provider
            .load_private_key(key)
            .map_err(io::Error::other)?,
    ));
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io::Error::other)?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(SniResolver {
            own,
            named: named.clone(),
        }));
    Ok(Arc::new(config))
}
pub struct AdminServerHandle {
    pub address: SocketAddr,
    stopped: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    tls: Arc<RwLock<Arc<rustls::ServerConfig>>>,
    state_dir: PathBuf,
    named: NamedCertificate,
}
impl AdminServerHandle {
    pub fn update_addresses(&self, addresses: &[Ipv4Addr]) -> io::Result<()> {
        *self.tls.write().unwrap() = tls_config_with(&self.state_dir, addresses, &self.named)?;
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

fn trace_timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(0);
    let mut utc = std::mem::MaybeUninit::<libc::tm>::uninit();
    if unsafe { libc::gmtime_r(&seconds, utc.as_mut_ptr()) }.is_null() {
        return "19700101-000000".into();
    }
    let utc = unsafe { utc.assume_init() };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        utc.tm_year + 1900,
        utc.tm_mon + 1,
        utc.tm_mday,
        utc.tm_hour,
        utc.tm_min,
        utc.tm_sec
    )
}

#[cfg(test)]
mod sni_tests {
    use super::*;
    use rustls::{
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, ServerName, UnixTime},
        DigitallySignedStruct, SignatureScheme,
    };
    #[test]
    fn accumulating_identity_keeps_covered_sans_and_adds_the_hotspot_address() {
        let state =
            std::env::temp_dir().join(format!("admin-sans-{}", auth::random_hex(8).unwrap()));
        let path = state.join("admin-tls.json");
        let lan = Ipv4Addr::new(192, 168, 1, 2);
        let hotspot = Ipv4Addr::new(10, 77, 0, 1);
        tls_config(&state, &[lan]).unwrap();
        let original = fs::read(&path).unwrap();
        tls_config(&state, &[]).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        tls_config(&state, &[hotspot]).unwrap();
        let updated = fs::read(&path).unwrap();
        assert_ne!(updated, original);
        let saved: SavedTls = serde_json::from_slice(&updated).unwrap();
        for san in [
            "localhost",
            "openxplay-linux.local",
            "192.168.1.2",
            "10.77.0.1",
        ] {
            assert!(saved.sans.contains(&san.into()));
        }
        tls_config(&state, &[hotspot, lan]).unwrap();
        assert_eq!(fs::read(&path).unwrap(), updated);
        fs::remove_dir_all(state).unwrap();
    }
    #[test]
    fn accumulating_sans_evict_the_oldest_extras_at_separate_caps() {
        let names = vec!["openxplay-linux.local".into(), "localhost".into()];
        let mut previous = names.clone();
        previous.extend((0..8).map(|i| format!("old-{i}.local")));
        previous.extend((1..=16).map(|i| format!("192.168.1.{i}")));
        let addresses = [Ipv4Addr::new(192, 168, 1, 1), Ipv4Addr::new(10, 77, 0, 1)];
        let sans = accumulated_sans(&previous, &names, &addresses);
        assert_eq!(
            sans.iter().filter(|s| s.parse::<IpAddr>().is_err()).count(),
            8
        );
        assert_eq!(
            sans.iter().filter(|s| s.parse::<IpAddr>().is_ok()).count(),
            16
        );
        for name in names {
            assert!(sans.contains(&name));
        }
        for address in addresses {
            assert!(sans.contains(&address.to_string()));
        }
        assert!(!sans.contains(&"old-0.local".into()));
        assert!(!sans.contains(&"old-1.local".into()));
        assert!(sans.contains(&"old-2.local".into()));
        assert!(!sans.contains(&"192.168.1.2".into()));
        assert!(sans.contains(&"192.168.1.3".into()));
    }
    /// Accepts any certificate and remembers the leaf it was shown.
    #[derive(Debug, Default)]
    struct Recorder(Mutex<Option<Vec<u8>>>);
    impl ServerCertVerifier for Recorder {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            *self.0.lock().unwrap() = Some(end_entity.to_vec());
            Ok(ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
    /// The leaf the server presents to a client asking for `name`.
    fn presented(server: &Arc<rustls::ServerConfig>, name: ServerName<'static>) -> Vec<u8> {
        let recorder = Arc::new(Recorder::default());
        let client = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(recorder.clone())
        .with_no_client_auth();
        let mut client = rustls::ClientConnection::new(Arc::new(client), name).unwrap();
        let mut server = rustls::ServerConnection::new(server.clone()).unwrap();
        for _ in 0..10 {
            let mut buffer = Vec::new();
            client.write_tls(&mut buffer).unwrap();
            server.read_tls(&mut buffer.as_slice()).unwrap();
            server.process_new_packets().unwrap();
            let mut buffer = Vec::new();
            server.write_tls(&mut buffer).unwrap();
            client.read_tls(&mut buffer.as_slice()).unwrap();
            client.process_new_packets().unwrap();
            if !client.is_handshaking() {
                break;
            }
        }
        let leaf = recorder.0.lock().unwrap().clone();
        leaf.unwrap()
    }
    #[test]
    fn the_named_certificate_answers_only_its_own_name() {
        let state = std::env::temp_dir().join(format!(
            "admin-sni-{}",
            crate::admin::auth::random_hex(6).unwrap()
        ));
        let named = NamedCertificate::default();
        let config = tls_config_with(&state, &[Ipv4Addr::new(10, 77, 0, 1)], &named).unwrap();
        let saved: SavedTls =
            serde_json::from_slice(&fs::read(state.join("admin-tls.json")).unwrap()).unwrap();
        let dns = |n: &str| ServerName::try_from(n.to_string()).unwrap();
        let ip = ServerName::IpAddress(rustls::pki_types::IpAddr::from(std::net::IpAddr::from([
            10, 77, 0, 1,
        ])));
        assert_eq!(presented(&config, dns("myrelay.duckdns.org")), saved.cert);
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let acme = rcgen::CertificateParams::new(vec!["myrelay.duckdns.org".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        named
            .set(
                "MyRelay.duckdns.org",
                acme.pem().as_bytes(),
                key.serialize_pem().as_bytes(),
            )
            .unwrap();
        assert_eq!(named.name().as_deref(), Some("myrelay.duckdns.org"));
        // The same configuration picks the new certificate up without a rebuild.
        assert_eq!(
            presented(&config, dns("myrelay.duckdns.org")),
            acme.der().to_vec()
        );
        assert_eq!(
            presented(&config, dns("MYRELAY.duckdns.org")),
            acme.der().to_vec()
        );
        assert_eq!(presented(&config, dns("openxplay-linux.local")), saved.cert);
        assert_eq!(presented(&config, ip.clone()), saved.cert);
        assert!(named.set("x", b"", b"").is_err());
        named.clear();
        assert_eq!(presented(&config, dns("myrelay.duckdns.org")), saved.cert);
        fs::remove_dir_all(state).unwrap();
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    struct Bounded {
        bytes: Vec<u8>,
        largest: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.largest = self.largest.max(buf.len());
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn trace_bundle_content_length_matches_complete_json_lines() {
        let dir =
            std::env::temp_dir().join(format!("trace-bundle-{}", auth::random_hex(8).unwrap()));
        fs::create_dir_all(&dir).unwrap();
        let mut files = Vec::new();
        for (name, body) in [
            (
                "trace-00000001-0-00000001.jsonl",
                "{\"type\":\"Link\"}\n{\"type\":",
            ),
            ("trace-00000002-0-00000001.jsonl", "{\"type\":\"Error\"}\n"),
        ] {
            fs::write(dir.join(name), body).unwrap();
            let mut file = trace_files::open(&dir, name).unwrap();
            let size = trace_files::snapshot_length(&mut file, true).unwrap();
            files.push((file, size));
        }
        let response = Response {
            status: 200,
            headers: BTreeMap::new(),
            body: vec![],
            files,
        };
        let mut out = Vec::new();
        response.write(&mut out).unwrap();
        let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let body = &out[split..];
        assert!(String::from_utf8_lossy(&out[..split])
            .contains(&format!("Content-Length: {}\r\n", body.len())));
        assert_eq!(body, b"{\"type\":\"Link\"}\n{\"type\":\"Error\"}\n");
        for line in std::str::from_utf8(body).unwrap().lines() {
            assert!(serde_json::from_str::<Value>(line).unwrap().is_object());
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn file_response_writes_headers_and_bounded_chunks_and_ignores_growth() {
        let path =
            std::env::temp_dir().join(format!("trace-stream-{}", auth::random_hex(8).unwrap()));
        fs::write(&path, vec![0x47; 100_000]).unwrap();
        let file = fs::File::open(&path).unwrap();
        let response = Response {
            status: 200,
            headers: BTreeMap::from([
                ("Content-Type".into(), "video/mp2t".into()),
                ("Cache-Control".into(), "no-store".into()),
            ]),
            body: vec![],
            files: vec![(file, 90_000)],
        };
        let mut out = Bounded {
            bytes: vec![],
            largest: 0,
        };
        response.write(&mut out).unwrap();
        assert!(out.largest <= 16 * 1024);
        let split = out.bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(out.bytes.len() - split, 90_000);
        assert!(String::from_utf8_lossy(&out.bytes[..split]).contains("Content-Length: 90000"));
        assert!(out.bytes[split..].iter().all(|b| *b == 0x47));
        fs::remove_file(path).unwrap();
    }
}
