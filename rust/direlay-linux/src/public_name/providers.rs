// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! The dynamic DNS providers, DuckDNS and dynv6: the name's A record and the `_acme-challenge`
//! TXT record.
use super::{
    config::Provider,
    http::{encode, Failure, Http, Request, Response},
};
use serde_json::json;
use std::net::Ipv4Addr;

/// Where each provider's API lives; tests point both at a local fake.
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub duckdns: String,
    pub dynv6: String,
}
impl Endpoints {
    pub fn production() -> Self {
        Self {
            duckdns: "https://www.duckdns.org".into(),
            dynv6: "https://dynv6.com".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderError {
    TokenRejected,
    NameNotFound,
    /// With the provider's `Retry-After`, in seconds.
    RateLimited(Option<u64>),
    Unreachable,
}
impl ProviderError {
    pub fn code(self) -> &'static str {
        match self {
            ProviderError::TokenRejected => "token-rejected",
            ProviderError::NameNotFound => "name-not-found",
            ProviderError::RateLimited(_) => "provider-rate-limited",
            ProviderError::Unreachable => "provider-unreachable",
        }
    }
}

/// The provider's authoritative name servers, for the TXT propagation check.
pub fn name_servers(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::DuckDns => &[
            "ns1.duckdns.org",
            "ns2.duckdns.org",
            "ns3.duckdns.org",
            "ns4.duckdns.org",
            "ns5.duckdns.org",
            "ns6.duckdns.org",
            "ns7.duckdns.org",
            "ns8.duckdns.org",
            "ns9.duckdns.org",
        ],
        Provider::Dynv6 => &["ns1.dynv6.com", "ns2.dynv6.com", "ns3.dynv6.com"],
    }
}

/// Maps a provider response: any 2xx passes; 401/403, 404 and 429 have their own errors.
fn check(result: Result<Response, Failure>, now: u64) -> Result<Response, ProviderError> {
    let response = result.map_err(|_| ProviderError::Unreachable)?;
    match response.status {
        200..=299 => Ok(response),
        401 | 403 => Err(ProviderError::TokenRejected),
        404 => Err(ProviderError::NameNotFound),
        429 => Err(ProviderError::RateLimited(response.retry_after(now))),
        _ => Err(ProviderError::Unreachable),
    }
}

pub struct ProviderClient<'a> {
    http: &'a dyn Http,
    endpoints: &'a Endpoints,
    provider: Provider,
    name: &'a str,
    token: &'a str,
    /// Wall-clock seconds, for `Retry-After` dates.
    now: u64,
    /// The dynv6 zone, and the TXT record this client created in it.
    zone: Option<u64>,
    record: Option<u64>,
}
impl<'a> ProviderClient<'a> {
    pub fn new(
        http: &'a dyn Http,
        endpoints: &'a Endpoints,
        provider: Provider,
        name: &'a str,
        token: &'a str,
        now: u64,
    ) -> Self {
        Self {
            http,
            endpoints,
            provider,
            name,
            token,
            now,
            zone: None,
            record: None,
        }
    }
    fn send(&self, request: Request) -> Result<Response, ProviderError> {
        check(self.http.send(request), self.now)
    }
    /// One DuckDNS update call; `query` adds `&ip=…` or `&txt=…`. The body is `OK` or `KO`.
    fn duckdns(&self, query: &str) -> Result<(), ProviderError> {
        let label = self.name.strip_suffix(".duckdns.org").unwrap_or(self.name);
        let url = format!(
            "{}/update?domains={}&token={}{query}",
            self.endpoints.duckdns,
            encode(label),
            encode(self.token)
        );
        match self.send(Request::get(url))?.text().as_str() {
            "OK" => Ok(()),
            "KO" => Err(ProviderError::TokenRejected),
            _ => Err(ProviderError::Unreachable),
        }
    }
    fn dynv6_auth(&self) -> String {
        format!("Bearer {}", self.token)
    }
    fn dynv6_zone(&mut self) -> Result<u64, ProviderError> {
        if let Some(zone) = self.zone {
            return Ok(zone);
        }
        let url = format!(
            "{}/api/v2/zones/by-name/{}",
            self.endpoints.dynv6,
            encode(self.name)
        );
        let zone = self
            .send(Request::get(url).header("Authorization", self.dynv6_auth()))?
            .json()
            .and_then(|v| v["id"].as_u64())
            .ok_or(ProviderError::NameNotFound)?;
        self.zone = Some(zone);
        Ok(zone)
    }
    fn dynv6_records(&self, zone: u64) -> String {
        format!("{}/api/v2/zones/{zone}/records", self.endpoints.dynv6)
    }
    /// Deletes every `_acme-challenge` TXT record in the zone.
    fn dynv6_delete_challenges(&mut self, zone: u64) -> Result<(), ProviderError> {
        let records = self
            .send(
                Request::get(self.dynv6_records(zone)).header("Authorization", self.dynv6_auth()),
            )?
            .json()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        for record in records {
            if record["type"] == "TXT" && record["name"] == "_acme-challenge" {
                if let Some(id) = record["id"].as_u64() {
                    self.dynv6_delete(zone, id)?;
                }
            }
        }
        Ok(())
    }
    fn dynv6_delete(&self, zone: u64, id: u64) -> Result<(), ProviderError> {
        let request = Request {
            method: "DELETE",
            url: format!("{}/{id}", self.dynv6_records(zone)),
            headers: vec![("Authorization", self.dynv6_auth())],
            body: None,
        };
        match self.send(request) {
            // Already gone.
            Err(ProviderError::NameNotFound) => Ok(()),
            result => result.map(|_| ()),
        }
    }

    /// Points the name's A record at `address`.
    pub fn set_address(&mut self, address: Ipv4Addr) -> Result<(), ProviderError> {
        let ip = address.to_string();
        match self.provider {
            Provider::DuckDns => self.duckdns(&format!("&ip={}", encode(&ip))),
            Provider::Dynv6 => {
                let url = format!(
                    "{}/api/update?zone={}&token={}&ipv4={}",
                    self.endpoints.dynv6,
                    encode(self.name),
                    encode(self.token),
                    encode(&ip)
                );
                self.send(Request::get(url)).map(|_| ())
            }
        }
    }
    /// Sets the one `_acme-challenge` TXT record to `value`.
    pub fn set_txt(&mut self, value: &str) -> Result<(), ProviderError> {
        match self.provider {
            Provider::DuckDns => self.duckdns(&format!("&txt={}", encode(value))),
            Provider::Dynv6 => {
                let zone = self.dynv6_zone()?;
                self.dynv6_delete_challenges(zone)?;
                let body = json!({"type":"TXT","name":"_acme-challenge","data":value});
                let created = self.send(
                    Request::with_body(
                        "POST",
                        self.dynv6_records(zone),
                        body.to_string().into_bytes(),
                    )
                    .header("Authorization", self.dynv6_auth())
                    .header("Content-Type", "application/json"),
                )?;
                self.record = created.json().and_then(|v| v["id"].as_u64());
                Ok(())
            }
        }
    }
    /// Removes the challenge record again.
    pub fn clear_txt(&mut self) -> Result<(), ProviderError> {
        match self.provider {
            Provider::DuckDns => self.duckdns("&txt=&clear=true"),
            Provider::Dynv6 => {
                let zone = self.dynv6_zone()?;
                match self.record.take() {
                    Some(id) => self.dynv6_delete(zone, id),
                    None => self.dynv6_delete_challenges(zone),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::{http::UreqHttp, testing::FakeHttp};
    use std::sync::{Arc, Mutex};
    const TOKEN: &str = "a1b2c3d4-0000-0000-0000-00000000000f";
    fn endpoints(server: &FakeHttp) -> Endpoints {
        Endpoints {
            duckdns: server.url(),
            dynv6: server.url(),
        }
    }
    #[test]
    fn duckdns_requests_and_responses() {
        let body = Arc::new(Mutex::new("OK"));
        let reply = body.clone();
        let server =
            FakeHttp::start(move |_| (200, vec![], reply.lock().unwrap().as_bytes().to_vec()));
        let http = UreqHttp::new(false);
        let endpoints = endpoints(&server);
        let mut client = ProviderClient::new(
            &http,
            &endpoints,
            Provider::DuckDns,
            "my-relay.duckdns.org",
            TOKEN,
            0,
        );
        client.set_address("10.77.0.1".parse().unwrap()).unwrap();
        client.set_txt("a+b/c=").unwrap();
        client.clear_txt().unwrap();
        let paths: Vec<_> = server.requests().iter().map(|r| r.path.clone()).collect();
        assert_eq!(
            paths,
            [
                format!("/update?domains=my-relay&token={TOKEN}&ip=10.77.0.1"),
                format!("/update?domains=my-relay&token={TOKEN}&txt=a%2Bb%2Fc%3D"),
                format!("/update?domains=my-relay&token={TOKEN}&txt=&clear=true"),
            ]
        );
        assert!(server.requests().iter().all(|r| r.method == "GET"
            && r.header("authorization").is_none()
            && r.header("user-agent") == Some(crate::public_name::http::user_agent().as_str())));
        *body.lock().unwrap() = "KO";
        assert_eq!(
            client.set_address("10.77.0.1".parse().unwrap()),
            Err(ProviderError::TokenRejected)
        );
        *body.lock().unwrap() = "<html>";
        assert_eq!(client.set_txt("x"), Err(ProviderError::Unreachable));
    }
    #[test]
    fn dynv6_requests_and_responses() {
        let server = FakeHttp::start(|r| match (r.method.as_str(), r.path.as_str()) {
            ("GET", "/api/v2/zones/by-name/car.dynv6.net") => {
                (200, vec![], br#"{"id":42}"#.to_vec())
            }
            ("GET", "/api/v2/zones/42/records") => (
                200,
                vec![],
                br#"[{"id":7,"type":"TXT","name":"_acme-challenge","data":"old"},
                     {"id":8,"type":"A","name":"","data":"10.0.0.1"},
                     {"id":9,"type":"TXT","name":"other","data":"keep"}]"#
                    .to_vec(),
            ),
            ("POST", _) => (200, vec![], br#"{"id":11}"#.to_vec()),
            // dynv6 answers deletes of records it no longer has with 404.
            ("DELETE", "/api/v2/zones/42/records/7") => (404, vec![], vec![]),
            ("GET", _) => (200, vec![], b"addresses updated".to_vec()),
            _ => (204, vec![], vec![]),
        });
        let http = UreqHttp::new(false);
        let endpoints = endpoints(&server);
        let mut client = ProviderClient::new(
            &http,
            &endpoints,
            Provider::Dynv6,
            "car.dynv6.net",
            "tok&en",
            0,
        );
        client.set_address("172.31.200.1".parse().unwrap()).unwrap();
        client.set_txt("value").unwrap();
        client.clear_txt().unwrap();
        let seen: Vec<_> = server
            .requests()
            .iter()
            .map(|r| format!("{} {}", r.method, r.path))
            .collect();
        assert_eq!(
            seen,
            [
                "GET /api/update?zone=car.dynv6.net&token=tok%26en&ipv4=172.31.200.1",
                "GET /api/v2/zones/by-name/car.dynv6.net",
                "GET /api/v2/zones/42/records",
                "DELETE /api/v2/zones/42/records/7",
                "POST /api/v2/zones/42/records",
                "DELETE /api/v2/zones/42/records/11",
            ]
        );
        let requests = server.requests();
        assert!(requests[0].header("authorization").is_none());
        assert!(requests[1..]
            .iter()
            .all(|r| r.header("authorization") == Some("Bearer tok&en")));
        assert_eq!(requests[4].header("content-type"), Some("application/json"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&requests[4].body).unwrap(),
            json!({"type":"TXT","name":"_acme-challenge","data":"value"})
        );
        // A zone without an id is unknown.
        let empty = FakeHttp::start(|_| (200, vec![], b"{}".to_vec()));
        let endpoints = super::tests::endpoints(&empty);
        let mut client =
            ProviderClient::new(&http, &endpoints, Provider::Dynv6, "car.dynv6.net", "t", 0);
        assert_eq!(client.set_txt("v"), Err(ProviderError::NameNotFound));
    }
    #[test]
    fn status_codes_map_to_errors() {
        for (status, error) in [
            (401, ProviderError::TokenRejected),
            (403, ProviderError::TokenRejected),
            (404, ProviderError::NameNotFound),
            (429, ProviderError::RateLimited(Some(30))),
            (500, ProviderError::Unreachable),
            (302, ProviderError::Unreachable),
        ] {
            let server = FakeHttp::start(move |_| {
                (status, vec![("Retry-After".into(), "30".into())], vec![])
            });
            let http = UreqHttp::new(false);
            let endpoints = endpoints(&server);
            for provider in Provider::ALL {
                let name = if provider == Provider::DuckDns {
                    "a.duckdns.org"
                } else {
                    "a.dynv6.net"
                };
                let mut client = ProviderClient::new(&http, &endpoints, provider, name, TOKEN, 0);
                assert_eq!(
                    client.set_address(Ipv4Addr::new(10, 0, 0, 1)),
                    Err(error),
                    "{status}"
                );
                assert_eq!(client.set_txt("v"), Err(error), "{status}");
            }
        }
        let http = UreqHttp::new(false);
        let closed = Endpoints {
            duckdns: "http://127.0.0.1:9".into(),
            ..Endpoints::production()
        };
        let mut client =
            ProviderClient::new(&http, &closed, Provider::DuckDns, "a.duckdns.org", TOKEN, 0);
        assert_eq!(client.set_txt("v"), Err(ProviderError::Unreachable));
        assert_eq!(
            ProviderError::RateLimited(None).code(),
            "provider-rate-limited"
        );
        assert_eq!(name_servers(Provider::Dynv6).len(), 3);
        assert_eq!(name_servers(Provider::DuckDns)[8], "ns9.duckdns.org");
    }
}
