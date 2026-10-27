// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! The ACME client's ureq/rustls and built-in Mozilla roots, with per-operation budgets.
use crate::public_name::http::{Failure, Http, Request, Response};
use std::time::Duration;

pub(crate) struct Client {
    agent: ureq::Agent,
    limit: u64,
}
impl Client {
    pub fn new(timeout: Duration, limit: u64, user_agent: &str) -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .proxy(None)
                .https_only(true)
                .http_status_as_error(false)
                .max_redirects(0)
                .max_redirects_will_error(false)
                .user_agent(user_agent)
                .timeout_global(Some(timeout))
                .timeout_connect(Some(timeout.min(Duration::from_secs(30))))
                .build()
                .into(),
            limit,
        }
    }
}
fn failure(error: ureq::Error) -> Failure {
    match error {
        ureq::Error::Timeout(_) => Failure::Timeout,
        ureq::Error::BodyExceedsLimit(_) => Failure::TooLarge,
        ureq::Error::RequireHttpsOnly(_) => Failure::Refused,
        _ => Failure::Unreachable,
    }
}
impl Http for Client {
    fn send(&self, request: Request) -> Result<Response, Failure> {
        let mut builder = ureq::http::Request::builder()
            .method(request.method)
            .uri(request.url);
        for (key, value) in request.headers {
            builder = builder.header(key, value);
        }
        // POST is sent once. No redirects, retry middleware or application retries.
        let request = builder
            .body(request.body.unwrap_or_default())
            .map_err(|_| Failure::Refused)?;
        let mut response = self.agent.run(request).map_err(failure)?;
        Ok(Response {
            status: response.status().as_u16(),
            headers: Vec::new(),
            body: response
                .body_mut()
                .with_config()
                .limit(self.limit)
                .read_to_vec()
                .map_err(failure)?,
        })
    }
}
