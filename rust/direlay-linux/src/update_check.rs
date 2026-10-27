// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Read-only Linux release checks, with Android's SemVer ordering and channel policy.
use crate::public_name::http::{Http, Request};
use serde::Serialize;
use serde_json::Value;
use std::{
    cmp::Ordering,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const ENDPOINT: &str = "https://api.github.com/repos/OpenXPlay/OpenXPlay-Relay/releases";
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
pub struct Check {
    pub current: String,
    pub latest: Option<String>,
    pub channel: String,
    pub url: Option<String>,
    pub notes: String,
    pub checked_millis: u64,
}
struct Cached {
    at: Instant,
    checked: u64,
    releases: Vec<Release>,
}
pub struct Checker {
    http: Arc<dyn Http>,
    cache: Mutex<Option<Cached>>,
}
impl Default for Checker {
    fn default() -> Self {
        Self::new(Arc::new(crate::logs::http::Client::new(
            Duration::from_secs(10),
            2 * 1024 * 1024,
            &format!("OpenXPlay-Linux/{}", env!("CARGO_PKG_VERSION")),
        )))
    }
}
impl Checker {
    pub fn new(http: Arc<dyn Http>) -> Self {
        Self {
            http,
            cache: Mutex::new(None),
        }
    }
    pub fn check(&self, channel: Channel) -> Result<Check, &'static str> {
        self.check_at(channel, Instant::now(), crate::logs::now_millis())
    }
    fn check_at(&self, channel: Channel, now: Instant, wall: u64) -> Result<Check, &'static str> {
        let mut cache = self.cache.lock().unwrap();
        if cache
            .as_ref()
            .is_none_or(|c| now.saturating_duration_since(c.at) >= CACHE_TIME)
        {
            let response = self
                .http
                .send(
                    Request::get(ENDPOINT)
                        .header("Accept", "application/vnd.github+json")
                        .header("X-GitHub-Api-Version", "2022-11-28")
                        .header(
                            "User-Agent",
                            format!("OpenXPlay-Linux/{}", env!("CARGO_PKG_VERSION")),
                        ),
                )
                .map_err(|_| "offline")?;
            if response.status != 200 || response.body.len() > 2 * 1024 * 1024 {
                return Err("offline");
            }
            let releases = parse_feed(&response.body).ok_or("offline")?;
            *cache = Some(Cached {
                at: now,
                checked: wall,
                releases,
            });
        }
        let cached = cache.as_ref().unwrap();
        let selected = select(&cached.releases, channel);
        Ok(Check {
            current: env!("CARGO_PKG_VERSION").into(),
            latest: selected.map(|r| r.version.value.clone()),
            channel: channel.name().into(),
            url: selected.map(|r| r.url.clone()),
            notes: selected.map(|r| r.notes.clone()).unwrap_or_default(),
            checked_millis: cached.checked,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::http::{Failure, Response};
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
    impl Http for Fake {
        fn send(&self, request: Request) -> Result<Response, Failure> {
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
                .check_at(Channel::Stable, now, 10)
                .unwrap()
                .latest
                .as_deref(),
            Some("0.1.5")
        );
        assert_eq!(
            checker
                .check_at(Channel::Beta, now + Duration::from_secs(1), 20)
                .unwrap()
                .checked_millis,
            10
        );
        assert_eq!(http.0.lock().unwrap().len(), 1);
        checker
            .check_at(Channel::Stable, now + CACHE_TIME, 30)
            .unwrap();
        let requests = http.0.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].url, ENDPOINT);
        assert!(requests[0].headers.contains(&(
            "User-Agent",
            format!("OpenXPlay-Linux/{}", env!("CARGO_PKG_VERSION"))
        )));
    }
    #[test]
    fn network_http_and_invalid_feed_failures_are_offline() {
        struct Source(Result<Response, Failure>);
        impl Http for Source {
            fn send(&self, _: Request) -> Result<Response, Failure> {
                self.0.clone()
            }
        }
        for result in [
            Err(Failure::Timeout),
            Err(Failure::Unreachable),
            Ok(Response {
                status: 429,
                ..Default::default()
            }),
            Ok(Response {
                status: 200,
                body: b"not json".to_vec(),
                ..Default::default()
            }),
            Ok(Response {
                status: 200,
                body: vec![b' '; 2 * 1024 * 1024 + 1],
                ..Default::default()
            }),
        ] {
            assert_eq!(
                Checker::new(Arc::new(Source(result)))
                    .check(Channel::Stable)
                    .unwrap_err(),
                "offline"
            );
        }
    }
}
