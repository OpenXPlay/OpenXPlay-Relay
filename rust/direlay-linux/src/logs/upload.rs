// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use super::{http::Http, jobs::Transfer};
use crate::public_name::http::{Failure, Request};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{io, path::Path};

pub const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
pub const ORIGIN: &str = "https://logs.openxplay.org";
pub const USER_AGENT: &str = "OpenXPlay-Log-Client/1.0";
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UploadError {
    Offline,
    TooLarge,
    Rejected,
    Timeout,
    Http {
        status: u16,
        retry_after_seconds: Option<u64>,
    },
}
impl UploadError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::TooLarge => "too_large",
            Self::Rejected => "rejected",
            Self::Timeout => "timeout",
            Self::Http { .. } => "http",
        }
    }
}
impl From<Failure> for UploadError {
    fn from(value: Failure) -> Self {
        match value {
            Failure::Timeout => Self::Timeout,
            Failure::Unreachable => Self::Offline,
            _ => Self::Rejected,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub log_id: String,
    pub tracking_url: String,
    pub qr_code_url: String,
    pub expires_millis: u64,
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}
impl Receipt {
    fn valid(&self) -> bool {
        hex(&self.log_id, 32)
            && self.tracking_url == format!("{ORIGIN}/r/{}", self.log_id)
            && self.qr_code_url == format!("{ORIGIN}/qr/{}.png", self.log_id)
    }
}
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
/// Persist the random installation identity before sending any request.
pub fn user_id(state: &Path) -> io::Result<String> {
    static IDENTITY: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = IDENTITY.lock().unwrap();
    let path = state.join("log-upload-user-id");
    use std::{
        fs::OpenOptions,
        io::Read,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
    {
        Ok(file) => {
            if !file.metadata()?.is_file() {
                return Err(io::Error::other("Invalid upload identity file"));
            }
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            let mut bytes = Vec::new();
            file.take(129).read_to_end(&mut bytes)?;
            if let Ok(value) = String::from_utf8(bytes) {
                if valid_user_id(&value) {
                    return Ok(value);
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let value = crate::admin::auth::random_hex(16)?;
    crate::platform::private_state_file(&path, value.as_bytes())?;
    Ok(value)
}
fn valid_user_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
pub fn upload(
    transfer: &std::sync::Arc<Transfer>,
    http: &dyn Http,
    bytes: &[u8],
    name: &str,
    user_id: &str,
    version: &str,
) -> Result<Receipt, UploadError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(UploadError::TooLarge);
    }
    if bytes.is_empty() || !valid_user_id(user_id) || version.len() > 512 {
        return Err(UploadError::Rejected);
    }
    let boundary = format!(
        "relay-{}",
        crate::admin::auth::random_hex(16).map_err(|_| UploadError::Rejected)?
    );
    let filename: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut body = Vec::with_capacity(bytes.len() + 2048);
    for (key, value) in [("user_id", user_id), ("version", version)] {
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/zip\r\n\r\n").as_bytes());
    let payload_start = body.len();
    body.extend(bytes);
    body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
    let response = http
        .send(
            Request::with_body("POST", format!("{ORIGIN}/api/uploads"), body)
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header("User-Agent", USER_AGENT),
            transfer,
            payload_start..payload_start + bytes.len(),
        )
        .map_err(|failure| {
            // Once payload bytes left the relay, a lost connection has an unconfirmed outcome.
            if transfer.status().sent > 0 && failure == Failure::Unreachable {
                UploadError::Rejected
            } else {
                failure.into()
            }
        })?;
    if response.status != 201 {
        return Err(UploadError::Http {
            status: response.status,
            retry_after_seconds: response
                .header("retry-after")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|v| v.min(86_400)),
        });
    }
    if response.body.len() > 64 * 1024 {
        return Err(UploadError::Rejected);
    }
    let value: Value = serde_json::from_slice(&response.body).map_err(|_| UploadError::Rejected)?;
    let receipt = (|| {
        if value["user_id"].as_str()? != user_id
            || value["bytes"].as_u64()? != bytes.len() as u64
            || value["sha256"].as_str()? != sha256(bytes)
        {
            return None;
        }
        let receipt = Receipt {
            log_id: value["id"].as_str()?.into(),
            tracking_url: value["tracking_url"].as_str()?.into(),
            qr_code_url: value["qr_code_url"].as_str()?.into(),
            expires_millis: rfc3339_millis(value["expires_at"].as_str()?)?,
        };
        receipt.valid().then_some(receipt)
    })();
    receipt.ok_or(UploadError::Rejected)
}
pub(crate) fn rfc3339_millis(value: &str) -> Option<u64> {
    let (date, time) = value.split_once('T')?;
    let date: Vec<_> = date.split('-').collect();
    if date.len() != 3 || date[0].len() != 4 || date[1].len() != 2 || date[2].len() != 2 {
        return None;
    }
    let year = date[0].parse::<u32>().ok()?;
    let month = date[1].parse::<usize>().ok()?;
    let day = date[2].parse::<u32>().ok()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&month) || day == 0 || day > days[month - 1] {
        return None;
    }
    let clock = if let Some(clock) = time.strip_suffix('Z') {
        clock
    } else {
        let at = time.rfind(['+', '-'])?;
        let zone = &time[at + 1..];
        let (h, m) = zone.split_once(':')?;
        if h.len() != 2
            || m.len() != 2
            || h.parse::<u32>().ok()? > 23
            || m.parse::<u32>().ok()? > 59
        {
            return None;
        }
        &time[..at]
    };
    let (clock, fraction) = clock
        .split_once('.')
        .map_or((clock, None), |(c, f)| (c, Some(f)));
    if fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let parts: Vec<_> = clock.split(':').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let seconds = crate::public_name::time::parse_rfc3339(value)?;
    let fraction = value
        .split_once('.')
        .map(|(_, v)| {
            v.bytes()
                .take_while(u8::is_ascii_digit)
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut millis = 0;
    for i in 0..3 {
        millis = millis * 10 + u64::from(fraction.get(i).copied().unwrap_or(b'0') - b'0');
    }
    seconds.checked_mul(1000)?.checked_add(millis)
}
pub fn latest(state: &Path, now: u64) -> io::Result<Option<Receipt>> {
    let Some(bytes) = super::read_private(&state.join("log-upload-receipt.json"), 64 * 1024)?
    else {
        return Ok(None);
    };
    let Ok(receipt) = serde_json::from_slice::<Receipt>(&bytes) else {
        return Ok(None);
    };
    Ok((receipt.valid() && receipt.expires_millis > now).then_some(receipt))
}
pub fn save(state: &Path, receipt: &Receipt) -> io::Result<()> {
    crate::platform::private_state_file(
        &state.join("log-upload-receipt.json"),
        &serde_json::to_vec(receipt)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_name::http::Response;
    use serde_json::json;
    use std::sync::Mutex;
    struct Fake {
        requests: Mutex<Vec<Request>>,
        response: Result<Response, Failure>,
    }
    impl Http for Fake {
        fn send(
            &self,
            request: Request,
            _: &std::sync::Arc<Transfer>,
            _: std::ops::Range<usize>,
        ) -> Result<Response, Failure> {
            self.requests.lock().unwrap().push(request);
            self.response.clone()
        }
    }
    fn upload(
        http: &dyn Http,
        bytes: &[u8],
        name: &str,
        user: &str,
        version: &str,
    ) -> Result<Receipt, UploadError> {
        super::upload(
            &Transfer::new("test".into(), "export".into(), bytes.len() as u64),
            http,
            bytes,
            name,
            user,
            version,
        )
    }
    fn response() -> Value {
        json!({"id":"a".repeat(32),"user_id":"install","bytes":3,"sha256":sha256(b"zip"),"tracking_url":format!("{ORIGIN}/r/{}","a".repeat(32)),"qr_code_url":format!("{ORIGIN}/qr/{}.png","a".repeat(32)),"expires_at":"2026-10-29T12:00:00.123456789Z"})
    }
    #[test]
    fn install_identity_and_latest_receipt_are_private_persistent_and_expire() {
        use std::os::unix::fs::PermissionsExt;
        let state = std::env::temp_dir().join(format!(
            "log-receipt-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let identity = user_id(&state).unwrap();
        assert_eq!(user_id(&state).unwrap(), identity);
        let receipt = upload(&fake(201, response()), b"zip", "a.zip", "install", "1").unwrap();
        save(&state, &receipt).unwrap();
        assert_eq!(
            latest(&state, receipt.expires_millis - 1).unwrap(),
            Some(receipt.clone())
        );
        assert_eq!(latest(&state, receipt.expires_millis).unwrap(), None);
        for name in ["log-upload-user-id", "log-upload-receipt.json"] {
            assert_eq!(
                std::fs::metadata(state.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        assert_eq!(
            std::fs::metadata(&state).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for bad in [
            "2026-02-31T12:00:00Z",
            "2026-10-29T12:00:00junkZ",
            "2026-10-29T12:00:00+99:00",
            "2026-10-29T12:00:00.Z",
        ] {
            assert!(rfc3339_millis(bad).is_none(), "{bad}");
        }
        std::fs::remove_dir_all(state).unwrap();
    }
    fn fake(status: u16, value: Value) -> Fake {
        Fake {
            requests: Mutex::default(),
            response: Ok(Response {
                status,
                body: serde_json::to_vec(&value).unwrap(),
                ..Default::default()
            }),
        }
    }
    #[test]
    fn invalid_persisted_identity_is_replaced_before_use_and_rejection_has_retry_detail() {
        let dir = std::env::temp_dir().join(format!(
            "invalid-log-id-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        for invalid in [
            b"invalid id".to_vec(),
            vec![255],
            vec![b'a'; 5000],
            Vec::new(),
        ] {
            crate::platform::private_state_file(&dir.join("log-upload-user-id"), &invalid).unwrap();
            let id = user_id(&dir).unwrap();
            assert!(valid_user_id(&id));
            assert_eq!(id.len(), 32);
            assert_eq!(user_id(&dir).unwrap(), id);
        }
        let mut f = fake(429, response());
        f.response
            .as_mut()
            .unwrap()
            .headers
            .push(("Retry-After".into(), "999999".into()));
        assert_eq!(
            upload(&f, b"zip", "a.zip", "install", "1"),
            Err(UploadError::Http {
                status: 429,
                retry_after_seconds: Some(86400)
            })
        );
        assert_eq!(f.requests.lock().unwrap().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn multipart_and_receipt_match_android_and_never_retry() {
        let f = fake(201, response());
        let receipt = upload(&f, b"zip", "logs.zip", "install", "0.2.1").unwrap();
        assert_eq!(receipt.expires_millis % 1000, 123);
        let requests = f.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, format!("{ORIGIN}/api/uploads"));
        assert_eq!(requests[0].method, "POST");
        assert!(requests[0]
            .headers
            .contains(&("User-Agent", USER_AGENT.into())));
        let body = String::from_utf8(requests[0].body.clone().unwrap()).unwrap();
        for value in [
            "name=\"user_id\"\r\n\r\ninstall",
            "name=\"version\"\r\n\r\n0.2.1",
            "filename=\"logs.zip\"",
            "application/zip\r\n\r\nzip",
        ] {
            assert!(body.contains(value));
        }
    }
    #[test]
    fn rejects_mismatches_foreign_urls_statuses_and_capped_responses() {
        for (key, value) in [
            ("user_id", json!("other")),
            ("bytes", json!(4)),
            ("sha256", json!("0".repeat(64))),
            ("tracking_url", json!("https://logs.openxplay.org.evil/r/a")),
            ("qr_code_url", json!("http://logs.openxplay.org/qr/a")),
            ("expires_at", json!("bad")),
        ] {
            let mut body = response();
            body[key] = value;
            assert_eq!(
                upload(&fake(201, body), b"zip", "a.zip", "install", "1"),
                Err(UploadError::Rejected)
            );
        }
        for status in [200, 302, 400, 429, 500, 507] {
            let f = fake(status, response());
            assert_eq!(
                upload(&f, b"zip", "a.zip", "install", "1"),
                Err(UploadError::Http {
                    status,
                    retry_after_seconds: None
                })
            );
            assert_eq!(f.requests.lock().unwrap().len(), 1);
        }
        assert_eq!(
            upload(&fake(413, response()), b"zip", "a.zip", "install", "1"),
            Err(UploadError::Http {
                status: 413,
                retry_after_seconds: None
            })
        );
        for (failure, expected) in [
            (Failure::Timeout, UploadError::Timeout),
            (Failure::Unreachable, UploadError::Offline),
            (Failure::TooLarge, UploadError::Rejected),
        ] {
            let f = Fake {
                requests: Mutex::default(),
                response: Err(failure),
            };
            assert_eq!(upload(&f, b"zip", "a.zip", "install", "1"), Err(expected));
        }
        let mut f = fake(201, response());
        f.response.as_mut().unwrap().body = vec![b' '; 65537];
        assert_eq!(
            upload(&f, b"zip", "a.zip", "install", "1"),
            Err(UploadError::Rejected)
        );
    }
}
