// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Read-only Linux release checks, with Android's SemVer ordering and channel policy.
use crate::public_name::http::{Failure as HttpFailure, Request, Response};
use serde::Serialize;
use serde_json::Value;
use std::{
    cmp::Ordering,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const ENDPOINT: &str =
    "https://api.github.com/repos/OpenXPlay/OpenXPlay-Relay/releases?per_page=30";
pub const TIME_BUDGET: Duration = Duration::from_secs(40);
const MAX_BODY: usize = 2 * 1024 * 1024;
const CACHE_TIME: Duration = Duration::from_secs(6 * 60 * 60);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Beta,
}
impl Channel {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }
}
#[derive(Clone, Debug, Eq)]
pub struct Version {
    pub value: String,
    core: Vec<String>,
    pre: Vec<String>,
}
fn number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && (value.len() == 1 || !value.starts_with('0'))
}
fn identifiers(value: &str) -> bool {
    value
        .split('.')
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}
fn numeric(left: &str, right: &str) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}
impl Version {
    pub fn parse(value: &str) -> Option<Self> {
        let mut build = value.split('+');
        let main = build.next()?;
        if build.next().is_some_and(|part| !identifiers(part)) || build.next().is_some() {
            return None;
        }
        let (core, pre) = main
            .split_once('-')
            .map_or((main, None), |(a, b)| (a, Some(b)));
        let core: Vec<_> = core.split('.').collect();
        if core.len() != 3 || !core.iter().all(|p| number(p)) {
            return None;
        }
        let pre: Vec<_> = match pre {
            Some(pre)
                if identifiers(pre)
                    && pre
                        .split('.')
                        .all(|p| !p.bytes().all(|b| b.is_ascii_digit()) || number(p)) =>
            {
                pre.split('.').map(str::to_string).collect()
            }
            None => Vec::new(),
            _ => return None,
        };
        Some(Self {
            value: value.into(),
            core: core.into_iter().map(str::to_string).collect(),
            pre,
        })
    }
}
impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        for (a, b) in self.core.iter().zip(&other.core) {
            let order = numeric(a, b);
            if order != Ordering::Equal {
                return order;
            }
        }
        if self.pre.is_empty() || other.pre.is_empty() {
            return self.pre.is_empty().cmp(&other.pre.is_empty());
        }
        for (a, b) in self.pre.iter().zip(&other.pre) {
            let order = match (
                a.bytes().all(|b| b.is_ascii_digit()),
                b.bytes().all(|b| b.is_ascii_digit()),
            ) {
                (true, true) => numeric(a, b),
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => a.cmp(b),
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        self.pre.len().cmp(&other.pre.len())
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
#[derive(Clone, Debug)]
pub struct Release {
    pub version: Version,
    prerelease: bool,
    url: String,
    notes: String,
    published_at: String,
}
fn safe_url(url: &str) -> bool {
    let Some(path) = url.strip_prefix("https://github.com/OpenXPlay/OpenXPlay-Relay/") else {
        return false;
    };
    !path.contains(['?', '#', '%', '\\']) && path.split('/').all(|s| s != "." && s != "..")
}
pub fn parse_feed(bytes: &[u8]) -> Option<Vec<Release>> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let mut releases = Vec::new();
    for item in value.as_array()? {
        let draft = item.get("draft")?.as_bool()?;
        let tag = item.get("tag_name")?.as_str()?;
        let Some(version) = tag.strip_prefix("linux-v") else {
            continue;
        };
        if draft {
            continue;
        }
        if version.contains('+')
            || version
                .split_once('-')
                .is_some_and(|(_, pre)| pre.starts_with(['-', '.']))
        {
            continue;
        }
        let Some(version) = Version::parse(version) else {
            continue;
        };
        // Match Android's release-tag bounds while the general comparator supports large cores.
        if version.core.iter().any(|p| p.len() > 3) {
            continue;
        }
        let prerelease = item.get("prerelease")?.as_bool()?;
        crate::logs::upload::rfc3339_millis(item.get("published_at")?.as_str()?)?;
        let page = item.get("html_url")?.as_str()?;
        let url = if safe_url(page) {
            page.into()
        } else {
            format!("https://github.com/OpenXPlay/OpenXPlay-Relay/releases/tag/{tag}")
        };
        let notes = match item.get("body")? {
            Value::Null => String::new(),
            Value::String(s) => s
                .lines()
                .take_while(|line| *line != "---")
                .collect::<Vec<_>>()
                .join("\n"),
            _ => return None,
        };
        releases.push(Release {
            version,
            prerelease,
            url,
            notes,
            published_at: item.get("published_at")?.as_str()?.into(),
        });
    }
    Some(releases)
}
pub fn select(releases: &[Release], channel: Channel) -> Option<&Release> {
    releases
        .iter()
        .filter(|r| channel == Channel::Beta || !r.prerelease && r.version.pre.is_empty())
        .max_by(|a, b| a.version.cmp(&b.version))
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub latest: Option<String>,
    pub outcome: &'static str,
    pub url: Option<String>,
    pub notes: String,
    pub published_at: Option<String>,
}
fn candidate(releases: &[Release], channel: Channel, installed: &str) -> Candidate {
    let selected = select(releases, channel);
    Candidate {
        latest: selected.map(|r| r.version.value.clone()),
        outcome: match selected {
            None => "no-release",
            Some(release) if Version::parse(installed).is_none_or(|v| release.version > v) => {
                "available"
            }
            Some(_) => "up-to-date",
        },
        url: selected.map(|r| r.url.clone()),
        notes: selected.map(|r| r.notes.clone()).unwrap_or_default(),
        published_at: selected.map(|r| r.published_at.clone()),
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub current: String,
    pub channel: String,
    #[serde(flatten)]
    pub candidate: Candidate,
    pub checked_millis: u64,
    /// Both selections come from the same feed, so changing channels needs no network request.
    pub candidates: std::collections::BTreeMap<String, Candidate>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Failure {
    Offline,
    Timeout,
    RateLimited {
        #[serde(rename = "resetMillis")]
        reset_millis: Option<u64>,
    },
    Http {
        status: u16,
    },
    InvalidResponse,
}
trait Transport: Send + Sync {
    fn send(&self, request: Request, remaining: Duration) -> Result<Response, HttpFailure>;
}
struct Client;
impl Transport for Client {
    fn send(&self, request: Request, remaining: Duration) -> Result<Response, HttpFailure> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_global(Some(remaining))
            .timeout_connect(Some(Duration::from_secs(15).min(remaining)))
            .timeout_recv_response(Some(Duration::from_secs(20).min(remaining)))
            .timeout_recv_body(Some(Duration::from_secs(20).min(remaining)))
            .build()
            .into();
        let mut builder = ureq::http::Request::builder().uri(request.url);
        for (key, value) in request.headers {
            builder = builder.header(key, value);
        }
        let mut response = agent
            .run(builder.body(()).map_err(|_| HttpFailure::Refused)?)
            .map_err(transport_failure)?;
        let headers = response
            .headers()
            .iter()
            .map(|(key, value)| {
                (
                    key.as_str().to_owned(),
                    value.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        Ok(Response {
            status: response.status().as_u16(),
            headers,
            body: if response.status().as_u16() == 200 {
                response
                    .body_mut()
                    .with_config()
                    .limit(MAX_BODY as u64)
                    .read_to_vec()
                    .map_err(transport_failure)?
            } else {
                Vec::new()
            },
        })
    }
}
fn transport_failure(error: ureq::Error) -> HttpFailure {
    match error {
        ureq::Error::Timeout(_) => HttpFailure::Timeout,
        ureq::Error::BodyExceedsLimit(_) => HttpFailure::TooLarge,
        _ => HttpFailure::Unreachable,
    }
}
fn redirect_url(base: &str, location: &str) -> Option<String> {
    let target = if location.starts_with("//") {
        format!("https:{location}")
    } else if location.starts_with('/') {
        format!("https://api.github.com{location}")
    } else if location.starts_with('?') {
        format!("{}{location}", base.split('?').next()?)
    } else if location.contains(':') {
        location.to_owned()
    } else {
        format!("{}/{location}", base.split('?').next()?.rsplit_once('/')?.0)
    };
    let uri: ureq::http::Uri = target.parse().ok()?;
    (uri.scheme_str() == Some("https")
        && uri.host() == Some("api.github.com")
        && matches!(uri.port_u16(), None | Some(443))
        && !uri.authority()?.as_str().contains('@')
        && !target.contains('#'))
    .then_some(target)
}
fn fetch(http: &dyn Transport) -> Result<Vec<Release>, Failure> {
    let started = Instant::now();
    let mut url = ENDPOINT.to_owned();
    for redirects in 0..=5 {
        let remaining = TIME_BUDGET
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or(Failure::Timeout)?;
        let response = http
            .send(
                Request::get(&url)
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28")
                    .header(
                        "User-Agent",
                        format!("OpenXPlay-Linux/{}", env!("CARGO_PKG_VERSION")),
                    ),
                remaining,
            )
            .map_err(|error| match error {
                HttpFailure::Timeout => Failure::Timeout,
                HttpFailure::TooLarge => Failure::InvalidResponse,
                _ => Failure::Offline,
            })?;
        if started.elapsed() >= TIME_BUDGET {
            return Err(Failure::Timeout);
        }
        if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
            url = response
                .header("location")
                .filter(|_| redirects < 5)
                .and_then(|location| redirect_url(&url, location))
                .ok_or(Failure::Http {
                    status: response.status,
                })?;
            continue;
        }
        if matches!(response.status, 403 | 429)
            && response.header("x-ratelimit-remaining") == Some("0")
        {
            return Err(Failure::RateLimited {
                reset_millis: response
                    .header("x-ratelimit-reset")
                    .and_then(|v| v.parse::<u64>().ok())
                    .and_then(|v| v.checked_mul(1000)),
            });
        }
        if response.status != 200 {
            return Err(Failure::Http {
                status: response.status,
            });
        }
        if response.body.len() > MAX_BODY {
            return Err(Failure::InvalidResponse);
        }
        return parse_feed(&response.body).ok_or(Failure::InvalidResponse);
    }
    unreachable!()
}
struct Cached {
    at: Instant,
    checked: u64,
    releases: Vec<Release>,
}
pub struct Checker {
    http: Arc<dyn Transport>,
    cache: Mutex<Option<Cached>>,
}
impl Default for Checker {
    fn default() -> Self {
        Self::new(Arc::new(Client))
    }
}
impl Checker {
    fn new(http: Arc<dyn Transport>) -> Self {
        Self {
            http,
            cache: Mutex::new(None),
        }
    }
    pub fn check(&self, channel: Channel, fresh: bool) -> Result<Check, Failure> {
        self.check_at(channel, fresh, Instant::now(), crate::logs::now_millis())
    }
    fn check_at(
        &self,
        channel: Channel,
        fresh: bool,
        now: Instant,
        wall: u64,
    ) -> Result<Check, Failure> {
        let mut cache = self.cache.lock().unwrap();
        if fresh
            || cache
                .as_ref()
                .is_none_or(|c| now.saturating_duration_since(c.at) >= CACHE_TIME)
        {
            let releases = fetch(self.http.as_ref())?;
            *cache = Some(Cached {
                at: now,
                checked: wall,
                releases,
            });
        }
        let cached = cache.as_ref().unwrap();
        let current = env!("CARGO_PKG_VERSION");
        Ok(Check {
            current: current.into(),
            channel: channel.name().into(),
            candidate: candidate(&cached.releases, channel, current),
            checked_millis: cached.checked,
            candidates: [Channel::Stable, Channel::Beta]
                .into_iter()
                .map(|channel| {
                    (
                        channel.name().into(),
                        candidate(&cached.releases, channel, current),
                    )
                })
                .collect(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    #[test]
    fn android_semver_order_and_validation() {
        for (a, b) in [
            ("0.1.5", "0.1.5-beta.2"),
            ("0.1.5-beta.2", "0.1.5-beta.1"),
            ("0.1.6-beta.10", "0.1.6-beta.2"),
            ("0.1.10", "0.1.9"),
            ("0.1.5-rc.1", "0.1.5-beta.9"),
            ("0.1.2", "0.0.0-dev.abc1234"),
            ("12345678901234567890.0.0", "9999999999999999999.0.0"),
        ] {
            assert!(Version::parse(a).unwrap() > Version::parse(b).unwrap());
        }
        assert_eq!(Version::parse("1.2.3+build"), Version::parse("1.2.3+other"));
        for invalid in [
            "1.2",
            "01.2.3",
            "1.2.3-01",
            "1.2.3-",
            "1.2.3+",
            "1.2.3+a+b",
            "1.2.3-a..b",
        ] {
            assert!(Version::parse(invalid).is_none(), "{invalid}");
        }
    }
    fn feed() -> Vec<u8> {
        let rows = [
            ("linux-v0.1.5", false, false),
            ("linux-v0.1.5-beta.2", true, false),
            ("linux-v0.1.6-beta.1", true, false),
            ("android-v0.1.9", false, false),
            ("linux-v0.1.7", false, true),
            ("linux-v0.1.8-beta.1", false, false),
        ];
        serde_json::to_vec(&rows.into_iter().map(|(tag,pre,draft)|json!({"tag_name":tag,"draft":draft,"prerelease":pre,"published_at":"2026-10-25T12:00:00Z","html_url":"https://evil.test/","body":"Notes\n---\nprovenance"})).collect::<Vec<_>>()).unwrap()
    }
    #[test]
    fn linux_feed_channels_safe_urls_and_empty_channel() {
        let releases = parse_feed(&feed()).unwrap();
        let stable = select(&releases, Channel::Stable).unwrap();
        assert_eq!(stable.version.value, "0.1.5");
        assert!(stable.url.ends_with("/linux-v0.1.5"));
        assert_eq!(stable.notes, "Notes");
        assert_eq!(
            select(&releases, Channel::Beta).unwrap().version.value,
            "0.1.8-beta.1"
        );
        assert!(select(&[], Channel::Stable).is_none());
        assert!(parse_feed(b"{}").is_none());
    }
    struct Fake(Mutex<Vec<Request>>);
    impl Transport for Fake {
        fn send(&self, request: Request, remaining: Duration) -> Result<Response, HttpFailure> {
            assert!(remaining <= TIME_BUDGET);
            self.0.lock().unwrap().push(request);
            Ok(Response {
                status: 200,
                body: feed(),
                ..Default::default()
            })
        }
    }
    #[test]
    fn memory_cache_reselects_channel_and_expires_after_six_hours() {
        let http = Arc::new(Fake(Mutex::default()));
        let checker = Checker::new(http.clone());
        let now = Instant::now();
        assert_eq!(
            checker
                .check_at(Channel::Stable, false, now, 10)
                .unwrap()
                .candidate
                .latest
                .as_deref(),
            Some("0.1.5")
        );
        assert_eq!(
            checker
                .check_at(Channel::Beta, false, now + Duration::from_secs(1), 20)
                .unwrap()
                .checked_millis,
            10
        );
        assert_eq!(http.0.lock().unwrap().len(), 1);
        checker
            .check_at(Channel::Stable, false, now + CACHE_TIME, 30)
            .unwrap();
        let requests = http.0.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].url, ENDPOINT);
        assert!(requests[0].headers.contains(&(
            "User-Agent",
            format!("OpenXPlay-Linux/{}", env!("CARGO_PKG_VERSION"))
        )));
    }
    struct Source(Result<Response, HttpFailure>);
    impl Transport for Source {
        fn send(&self, _: Request, _: Duration) -> Result<Response, HttpFailure> {
            self.0.clone()
        }
    }
    #[test]
    fn failures_keep_timeout_status_rate_reset_and_invalid_response() {
        for (result, expected) in [
            (Err(HttpFailure::Timeout), Failure::Timeout),
            (Err(HttpFailure::Unreachable), Failure::Offline),
            (
                Ok(Response {
                    status: 503,
                    ..Default::default()
                }),
                Failure::Http { status: 503 },
            ),
            (
                Ok(Response {
                    status: 429,
                    headers: vec![
                        ("x-ratelimit-remaining".into(), "0".into()),
                        ("x-ratelimit-reset".into(), "1792713600".into()),
                    ],
                    ..Default::default()
                }),
                Failure::RateLimited {
                    reset_millis: Some(1_792_713_600_000),
                },
            ),
            (
                Ok(Response {
                    status: 403,
                    ..Default::default()
                }),
                Failure::Http { status: 403 },
            ),
            (
                Ok(Response {
                    status: 200,
                    body: b"not json".to_vec(),
                    ..Default::default()
                }),
                Failure::InvalidResponse,
            ),
            (
                Ok(Response {
                    status: 200,
                    body: vec![b' '; MAX_BODY + 1],
                    ..Default::default()
                }),
                Failure::InvalidResponse,
            ),
        ] {
            assert_eq!(
                Checker::new(Arc::new(Source(result)))
                    .check(Channel::Stable, true)
                    .unwrap_err(),
                expected
            );
        }
    }
    #[test]
    fn manual_checks_bypass_cache_and_no_release_is_distinct_from_up_to_date() {
        let http = Arc::new(Fake(Mutex::default()));
        let checker = Checker::new(http.clone());
        checker.check(Channel::Stable, true).unwrap();
        checker.check(Channel::Beta, true).unwrap();
        assert_eq!(http.0.lock().unwrap().len(), 2);
        let releases = parse_feed(&feed()).unwrap();
        for installed in ["0.1.5", "0.1.6", "0.1.5+other"] {
            assert_eq!(
                candidate(&releases, Channel::Stable, installed).outcome,
                "up-to-date"
            );
        }
        assert_eq!(
            candidate(&releases, Channel::Stable, "0.1.4").outcome,
            "available"
        );
        assert_eq!(
            candidate(&[], Channel::Stable, "0.1.4").outcome,
            "no-release"
        );
    }
    #[test]
    fn a_safe_redirect_is_followed_with_the_remaining_budget_and_request_headers() {
        struct Redirect(Mutex<Vec<(String, Duration)>>);
        impl Transport for Redirect {
            fn send(&self, request: Request, remaining: Duration) -> Result<Response, HttpFailure> {
                assert!(request.headers.iter().any(|(key, _)| *key == "User-Agent"));
                let mut seen = self.0.lock().unwrap();
                seen.push((request.url, remaining));
                Ok(if seen.len() == 1 {
                    Response {
                        status: 307,
                        headers: vec![("location".into(), "/next".into())],
                        ..Default::default()
                    }
                } else {
                    Response {
                        status: 200,
                        body: feed(),
                        ..Default::default()
                    }
                })
            }
        }
        let http = Arc::new(Redirect(Mutex::default()));
        let check = Checker::new(http.clone())
            .check(Channel::Stable, true)
            .unwrap();
        assert_eq!(
            check.candidate.published_at.as_deref(),
            Some("2026-10-25T12:00:00Z")
        );
        let seen = http.0.lock().unwrap();
        assert_eq!(seen[1].0, "https://api.github.com/next");
        assert!(seen[1].1 < seen[0].1 && seen[0].1 <= TIME_BUDGET);
    }
    #[test]
    fn redirects_allow_only_github_api_https_with_a_five_hop_limit() {
        for location in [
            "https://evil.test/",
            "http://api.github.com/",
            "https://user@api.github.com/",
            "https://api.github.com:444/",
            "https://api.github.com/#fragment",
        ] {
            assert!(redirect_url(ENDPOINT, location).is_none(), "{location}");
        }
        for location in [
            "/releases?page=2",
            "?page=2",
            "releases",
            "https://api.github.com:443/releases",
        ] {
            assert!(redirect_url(ENDPOINT, location).is_some(), "{location}");
        }
        struct Redirect(Mutex<usize>);
        impl Transport for Redirect {
            fn send(&self, _: Request, _: Duration) -> Result<Response, HttpFailure> {
                *self.0.lock().unwrap() += 1;
                Ok(Response {
                    status: 302,
                    headers: vec![("location".into(), "/next".into())],
                    ..Default::default()
                })
            }
        }
        let http = Arc::new(Redirect(Mutex::new(0)));
        assert_eq!(
            Checker::new(http.clone())
                .check(Channel::Stable, true)
                .unwrap_err(),
            Failure::Http { status: 302 }
        );
        assert_eq!(*http.0.lock().unwrap(), 6);
    }
}
