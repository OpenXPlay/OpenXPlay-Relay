// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! HTTPS requests to the DNS providers and Let's Encrypt, behind a seam the tests replace.
use std::time::Duration;

/// Larger response bodies fail the request.
pub const MAX_BODY: u64 = 64 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(15);

pub struct Request {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Vec<u8>>,
}
impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: "GET",
            url: url.into(),
            headers: Vec::new(),
            body: None,
        }
    }
    pub fn with_body(method: &'static str, url: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Some(body),
        }
    }
    pub fn header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u16,
    /// Names lower-cased.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    pub fn success(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).trim().to_string()
    }
    /// `Retry-After` in seconds from `now`, as delta seconds or an HTTP date.
    pub fn retry_after(&self, now: u64) -> Option<u64> {
        let value = self.header("retry-after")?.trim();
        value
            .parse::<u64>()
            .ok()
            .or_else(|| super::time::parse_http_date(value).map(|at| at.saturating_sub(now)))
    }
}

/// Why a request got no response. Deliberately without the URL or any other request detail:
/// provider URLs carry the token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Unreachable,
    Timeout,
    TooLarge,
    Refused,
}

pub trait Http: Send + Sync {
    fn send(&self, request: Request) -> Result<Response, Failure>;
}

/// ureq over rustls (ring) with certificate verification against the Mozilla roots built in
/// (webpki-roots): the Debian package does not depend on `ca-certificates`, and a frozen device
/// image's store would age, while these roots update with the relay.
pub struct UreqHttp {
    agent: ureq::Agent,
    maximum_body: u64,
}
impl UreqHttp {
    /// Production clients use HTTPS only; tests talk plain HTTP to local fakes.
    pub fn new(https_only: bool) -> Self {
        Self::bounded(https_only, TIMEOUT, TIMEOUT, MAX_BODY)
    }
    /// Shared HTTP implementation with caller-specific timeouts and response limits.
    pub fn bounded(https_only: bool, connect: Duration, read: Duration, maximum_body: u64) -> Self {
        let agent = ureq::Agent::config_builder()
            // Straight to the provider or the CA: no proxy taken from the environment.
            .proxy(None)
            .https_only(https_only)
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(user_agent())
            .timeout_resolve(Some(connect))
            .timeout_connect(Some(connect))
            .timeout_send_request(Some(read))
            .timeout_send_body(Some(read))
            .timeout_recv_response(Some(read))
            .timeout_recv_body(Some(read))
            .build()
            .into();
        Self {
            agent,
            maximum_body,
        }
    }
}
pub fn user_agent() -> String {
    format!("OpenXPlay-Relay/{} (Linux)", env!("CARGO_PKG_VERSION"))
}
fn failure(error: &ureq::Error) -> Failure {
    match error {
        ureq::Error::Timeout(_) => Failure::Timeout,
        ureq::Error::BodyExceedsLimit(_) => Failure::TooLarge,
        ureq::Error::RequireHttpsOnly(_) => Failure::Refused,
        _ => Failure::Unreachable,
    }
}
impl Http for UreqHttp {
    fn send(&self, request: Request) -> Result<Response, Failure> {
        let mut builder = ureq::http::Request::builder()
            .method(request.method)
            .uri(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(*name, value);
        }
        let result = match request.body {
            Some(body) => builder
                .body(body)
                .map_err(|_| Failure::Refused)
                .and_then(|r| self.agent.run(r).map_err(|e| failure(&e))),
            None => builder
                .body(())
                .map_err(|_| Failure::Refused)
                .and_then(|r| self.agent.run(r).map_err(|e| failure(&e))),
        };
        let mut response = result?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_ascii_lowercase(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        let body = if request.method == "HEAD" {
            Vec::new()
        } else {
            response
                .body_mut()
                .with_config()
                .limit(self.maximum_body)
                .read_to_vec()
                .map_err(|e| failure(&e))?
        };
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

/// RFC 3986 percent-encoding of everything but the unreserved characters, for query values and
/// path segments.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::testing::FakeHttp;
    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(encode("a b&c=d/é~._-"), "a%20b%26c%3Dd%2F%C3%A9~._-");
    }
    #[test]
    fn ureq_client_sends_headers_caps_bodies_and_refuses_plain_http_in_production() {
        let server = FakeHttp::start(|request| {
            if request.path == "/big" {
                (200, vec![], vec![b'x'; MAX_BODY as usize + 1])
            } else {
                (
                    201,
                    vec![("Retry-After".into(), "7".into())],
                    request.body.clone(),
                )
            }
        });
        let client = UreqHttp::new(false);
        let response = client
            .send(
                Request::with_body("PUT", format!("{}/echo", server.url()), b"{}".to_vec())
                    .header("Authorization", "Token secret"),
            )
            .unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.body, b"{}");
        assert_eq!(response.retry_after(0), Some(7));
        let seen = server.requests();
        assert_eq!(seen[0].method, "PUT");
        assert_eq!(seen[0].header("authorization"), Some("Token secret"));
        assert_eq!(
            seen[0].header("user-agent").map(str::to_owned),
            Some(user_agent())
        );
        assert_eq!(
            client
                .send(Request::get(format!("{}/big", server.url())))
                .unwrap_err(),
            Failure::TooLarge
        );
        assert_eq!(
            UreqHttp::new(true)
                .send(Request::get(format!("{}/echo", server.url())))
                .unwrap_err(),
            Failure::Refused
        );
        // A closed port is unreachable, not an error that could carry the URL.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(
            client
                .send(Request::get(format!("http://127.0.0.1:{port}/?token=x")))
                .unwrap_err(),
            Failure::Unreachable
        );
    }
    #[test]
    fn retry_after_accepts_an_http_date() {
        let response = Response {
            headers: vec![("retry-after".into(), "Wed, 21 Oct 2026 07:28:00 GMT".into())],
            ..Default::default()
        };
        assert_eq!(response.retry_after(1792567680 - 60), Some(60));
    }
}
