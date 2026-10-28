// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Relay-owned source selection, private configuration and capability readiness.
//! Protocol and device implementations live behind MfiAuthenticator in the GPL crate.
use crate::{settings::Settings, Result};
use carplay_iap2::mfi::{remote, MfiAuthenticator};
use std::{path::Path, sync::Arc, time::Duration};

const TOKEN_FILE: &str = "mfi-remote-token";
pub fn valid_path(path: &str) -> bool {
    path.strip_prefix("/dev/i2c-")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}
pub fn valid_server(value: &str) -> bool {
    let value = value.trim();
    !value.contains(['\0', '\r', '\n', '#'])
        && value.parse::<ureq::http::Uri>().ok().is_some_and(|uri| {
            matches!(uri.scheme_str(), Some("http" | "https"))
                && uri.host().is_some_and(|h| !h.is_empty())
                && uri.authority().is_some_and(|a| !a.as_str().contains('@'))
        })
}
pub fn valid_token(token: &str) -> bool {
    !token.contains(['\0', '\r', '\n'])
}
pub fn token(directory: &Path) -> Result<String> {
    match std::fs::read_to_string(directory.join(TOKEN_FILE)) {
        Ok(value) if valid_token(&value) => Ok(value),
        Ok(_) => Err(std::io::Error::other("Invalid saved remote MFi token")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}
pub fn save_token(directory: &Path, value: &str) -> Result<()> {
    if !valid_token(value) {
        return Err(std::io::Error::other("Invalid remote MFi token"));
    }
    crate::platform::private_state_file(&directory.join(TOKEN_FILE), value.as_bytes())
}
struct Http(crate::public_name::http::UreqHttp);
impl remote::Http for Http {
    fn send(&self, request: remote::Request) -> Result<remote::Response> {
        use crate::public_name::http::Http;
        self.0
            .send(crate::public_name::http::Request {
                method: request.method,
                url: request.url,
                headers: request.headers,
                body: request.body,
            })
            .map(|r| remote::Response {
                status: r.status,
                body: r.body,
            })
            .map_err(|_| std::io::Error::other("Remote MFi transport failed"))
    }
}
pub fn load(
    settings: &Settings,
    directory: &Path,
    identity: &Path,
) -> Result<Arc<dyn MfiAuthenticator>> {
    match settings.mfi_target.as_str() {
        "local" => crate::linux_wireless_platform::LinuxWirelessPlatform::load_local_mfi(identity),
        "remote" => {
            let http = crate::public_name::http::UreqHttp::bounded(
                false,
                Duration::from_secs(5),
                Duration::from_secs(10),
                remote::MAX_HTTP_BODY as u64,
            );
            Ok(Arc::new(remote::RemoteAuthenticator::connect(
                &settings.mfi_remote_server,
                token(directory)?,
                Arc::new(Http(http)),
            )?))
        }
        "i2c" | "usb-ch341" => hardware(settings),
        _ => Err(std::io::Error::other("Unknown MFi target")),
    }
}
#[cfg(target_os = "linux")]
fn usb_devices() -> Result<Vec<carplay_iap2::mfi::linux::UsbDevice>> {
    carplay_iap2::mfi::linux::discover(
        Path::new("/sys/bus/usb/devices"),
        Path::new("/dev/bus/usb"),
        &[(0x1a86, 0x5512)],
    )
}
#[cfg(target_os = "linux")]
fn hardware(settings: &Settings) -> Result<Arc<dyn MfiAuthenticator>> {
    use carplay_iap2::mfi::{
        ch341::Ch341,
        coprocessor::{Coprocessor, I2c},
        linux::{NativeI2c, Usbfs},
    };
    let transport: Arc<dyn I2c> = if settings.mfi_target == "i2c" {
        Arc::new(NativeI2c::open(&settings.mfi_i2c_path)?)
    } else {
        let device = usb_devices()?.into_iter().next().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No configured CH341 USB device",
            )
        })?;
        Arc::new(Ch341::new(Box::new(Usbfs::open(device)?)))
    };
    Ok(Arc::new(Coprocessor::scan(transport)?))
}
#[cfg(not(target_os = "linux"))]
fn hardware(_: &Settings) -> Result<Arc<dyn MfiAuthenticator>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "MFi hardware requires Linux",
    ))
}
/// Readiness is local capability/configuration readiness. The startup worker performs the
/// bounded chip scan or remote session reset and certificate verification before MfiReady.
/// This never claims a USB interface, sends a remote request or races a live authentication.
pub fn readiness(settings: &Settings, directory: &Path, identity: &Path) -> &'static str {
    let result = match settings.mfi_target.as_str() {
        "local" => match crate::identity::status(identity).state {
            "READY" => Ok(()),
            "MISSING" => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Missing local identity",
            )),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid local identity",
            )),
        },
        "remote" => {
            if valid_server(&settings.mfi_remote_server) {
                token(directory).map(|_| ())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid remote server",
                ))
            }
        }
        "i2c" | "usb-ch341" => hardware_readiness(settings),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Unknown MFi target",
        )),
    };
    match result {
        Ok(()) => "ready",
        Err(e) => match e.kind() {
            std::io::ErrorKind::PermissionDenied => "permission",
            std::io::ErrorKind::Unsupported => "unsupported",
            std::io::ErrorKind::NotFound => "missing",
            _ => "invalid",
        },
    }
}
#[cfg(target_os = "linux")]
fn hardware_readiness(settings: &Settings) -> Result<()> {
    use carplay_iap2::mfi::linux::{NativeI2c, Usbfs};
    if settings.mfi_target == "i2c" {
        NativeI2c::open(&settings.mfi_i2c_path).map(|_| ())
    } else {
        let device = usb_devices()?.into_iter().next().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No configured CH341 USB device",
            )
        })?;
        Usbfs::probe(&device)
    }
}
#[cfg(not(target_os = "linux"))]
fn hardware_readiness(_: &Settings) -> Result<()> {
    hardware(&Settings::default()).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_depends_on_selected_source_and_validates_configuration_without_network() {
        let dir = std::env::temp_dir().join(format!(
            "mfi-ready-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut settings = Settings::default();
        assert_eq!(readiness(&settings, &dir, &dir), "missing");
        settings.mfi_target = "remote".into();
        settings.mfi_remote_server = "https://example.org".into();
        assert_eq!(readiness(&settings, &dir, &dir), "ready");
        assert!(!valid_server("https://user:password@example.org"));
        assert!(!valid_server("https://example.org/#secret"));
        settings.mfi_remote_server = "invalid".into();
        assert_eq!(readiness(&settings, &dir, &dir), "invalid");
        assert!(valid_path("/dev/i2c-123"));
        assert!(!valid_path("/dev/i2c-1/other"));
        assert!(!valid_token("bad\0value"));
        assert!(!valid_token("bad\rvalue"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn selected_remote_source_uses_existing_bounded_http_client() {
        use crate::public_name::testing::FakeHttp;
        let server = FakeHttp::start(|request| {
            let body = if request.path == "/mfi/reset" {
                "{}"
            } else if request.path == "/mfi/certificate" {
                r#"{"protocolMajor":3,"certificate":"AQ==","certificateSha256":"4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a"}"#
            } else {
                r#"{"signature":"Ag=="}"#
            };
            (200, vec![], body.as_bytes().to_vec())
        });
        let dir = std::env::temp_dir().join(format!(
            "mfi-http-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        save_token(&dir, "integration-secret").unwrap();
        let settings = Settings {
            mfi_target: "remote".into(),
            mfi_remote_server: server.url(),
            ..Default::default()
        };
        let client = load(&settings, &dir, &dir).unwrap();
        assert_eq!(client.protocol_major().unwrap(), 3);
        assert_eq!(client.read_certificate(10).unwrap(), [1]);
        assert_eq!(client.sign_challenge(&[7]).unwrap(), [2]);
        assert_eq!(server.requests().len(), 3);
        assert!(server
            .requests()
            .iter()
            .all(|r| r.header("authorization") == Some("Bearer integration-secret")));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
