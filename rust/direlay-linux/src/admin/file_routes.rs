// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! These routes run only after the common session and write-header checks.
use super::{AdminServer, Request, Response};
use crate::admin::Values;
use serde_json::{json, Value};
use std::io;

impl AdminServer {
    pub(super) fn file_route(
        &self,
        request: &Request,
        path: &str,
        body: &Values,
    ) -> Option<Response> {
        let response = match (request.method.as_str(), path) {
            ("POST", "/logs/export") => {
                let range = body
                    .get("range")
                    .and_then(Value::as_str)
                    .filter(|v| matches!(*v, "10m" | "30m" | "24h"));
                match range {
                    Some(range) => match self.admin.logs_export(range) {
                        Ok(export) => Response::json(200, json!(export)),
                        Err(error) => Response::error(500, &error),
                    },
                    None => Response::error(422, "error.invalid"),
                }
            }
            ("GET", path) if path.starts_with("/logs/export/") => {
                match self.admin.logs_download(&path[13..]) {
                    Ok((file, bytes, name)) => {
                        let mut response = Response::json(200, Value::Null);
                        response
                            .headers
                            .insert("Content-Type".into(), "application/zip".into());
                        response.headers.insert(
                            "Content-Disposition".into(),
                            format!("attachment; filename=\"{}\"", name),
                        );
                        response.body.clear();
                        response.files.push((file, bytes));
                        response
                    }
                    Err(e) => io_error(e),
                }
            }
            ("POST", "/logs/upload") => match body.get("id").and_then(Value::as_str) {
                Some(id) => match self.admin.logs_upload(id) {
                    Ok(receipt) => Response::json(200, json!(receipt)),
                    Err(error) => {
                        Response::error(if error == "too_large" { 413 } else { 502 }, &error)
                    }
                },
                None => Response::error(422, "error.invalid"),
            },
            ("GET", "/logs/receipt") => match self.admin.logs_receipt() {
                Ok(Some(receipt)) => Response::json(200, json!(receipt)),
                Ok(None) => Response::error(404, "error.request"),
                Err(e) => io_error(e),
            },
            ("GET", path @ ("/config/user" | "/config/privacy")) => {
                let privacy = path == "/config/privacy";
                match self.admin.config_export(privacy) {
                    Ok(value) => {
                        let mut response = Response::json(200, value);
                        response.headers.insert(
                            "Content-Disposition".into(),
                            format!(
                                "attachment; filename=\"openxplay-relay-{}-{}.json\"",
                                if privacy { "privacy" } else { "user" },
                                crate::platform::wall_clock().as_millis()
                            ),
                        );
                        response
                    }
                    Err(errors) => Response::json(422, json!({"errors":errors})),
                }
            }
            ("POST", "/config/import") => {
                let dry_run = query(request, "dryRun") == Some("1");
                match self.admin.config_import(&request.body, dry_run) {
                    Ok(report) => Response::json(200, json!(report)),
                    Err(errors) => Response::json(422, json!({"errors":errors})),
                }
            }
            ("GET", "/update") => {
                match Some(query(request, "channel").unwrap_or("stable"))
                    .filter(|v| matches!(*v, "stable" | "beta"))
                {
                    Some(channel) => match self.admin.update_check(channel) {
                        Ok(check) => Response::json(200, json!(check)),
                        Err(_) => Response::error(503, "offline"),
                    },
                    None => Response::error(422, "error.invalid"),
                }
            }
            _ => return None,
        };
        Some(response)
    }
}
fn query<'a>(request: &'a Request, key: &str) -> Option<&'a str> {
    request
        .path
        .split_once('?')?
        .1
        .split('&')
        .filter_map(|p| p.split_once('='))
        .find_map(|(k, v)| (k == key).then_some(v))
}
fn io_error(error: io::Error) -> Response {
    Response::error(
        if error.kind() == io::ErrorKind::NotFound {
            404
        } else {
            500
        },
        "error.request",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{auth::AuthState, *};
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };
    #[derive(Default)]
    struct Memory(Mutex<AuthState>);
    impl AuthStore for Memory {
        fn load(&self) -> io::Result<AuthState> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, value: &AuthState) -> io::Result<()> {
            *self.0.lock().unwrap() = value.clone();
            Ok(())
        }
    }
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
    }
    impl RelayAdmin for Fake {
        fn logs_download(&self, id: &str) -> io::Result<(std::fs::File, u64, String)> {
            if id != "test" {
                return Err(io::ErrorKind::NotFound.into());
            }
            let path = std::env::temp_dir().join(format!(
                "admin-log-download-{}",
                crate::admin::auth::random_hex(8)?
            ));
            std::fs::write(&path, b"zip")?;
            let file = std::fs::File::open(&path)?;
            std::fs::remove_file(path)?;
            Ok((file, 3, "openxplay-linux-logs-1.zip".into()))
        }
        fn logs_upload(&self, id: &str) -> Result<Value, String> {
            Err(id.into())
        }
        fn logs_export(&self, _: &str) -> Result<Value, String> {
            self.calls.lock().unwrap().push("export".into());
            Ok(json!({"id":"test","bytes":3,"sha256":"hash","createdMillis":1}))
        }
        fn config_export(&self, privacy: bool) -> Result<Value, BTreeMap<String, String>> {
            Ok(
                json!({"format":if privacy {"openxplay-privacy-config"} else {"openxplay-user-config"},"version":1}),
            )
        }
        fn config_import(&self, _: &[u8], dry: bool) -> Result<Value, BTreeMap<String, String>> {
            self.calls.lock().unwrap().push(format!("import:{dry}"));
            Ok(json!({"applied":["output"],"ignored":["androidOnly"],"restartNeeded":true}))
        }
        fn status(&self) -> AdminStatus {
            panic!("unused")
        }
        fn settings(&self) -> AdminSettings {
            panic!("unused")
        }
        fn update(&self, _: Values) -> AdminUpdate {
            panic!("unused")
        }
        fn action(&self, _: &str) -> AdminResult {
            panic!("unused")
        }
        fn phones(&self) -> Vec<AdminPhone> {
            vec![]
        }
        fn wifi_status(&self) -> WifiClient {
            panic!("unused")
        }
        fn wifi_networks(&self) -> Result<Vec<WifiNetwork>, String> {
            panic!("unused")
        }
        fn wifi_join(&self, _: &str, _: Option<&str>) -> AdminResult {
            panic!("unused")
        }
    }
    fn request(method: &str, path: &str, body: Value) -> Request {
        Request {
            method: method.into(),
            path: format!("/api/v0{path}"),
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("x-openxplay-admin".into(), "1".into()),
            ]),
            body: if method == "GET" {
                vec![]
            } else {
                serde_json::to_vec(&body).unwrap()
            },
            peer: "192.168.1.2".parse().unwrap(),
        }
    }
    fn server() -> (AdminServer, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        let auth = AdminAuth::for_boot(Box::new(Memory::default()), "test-boot".into()).unwrap();
        (
            AdminServer::new(
                fake.clone(),
                auth,
                AdminServerConfig::new("/tmp/unused".into(), "/tmp/unused".into(), vec![]),
            ),
            fake,
        )
    }
    fn login(server: &AdminServer) -> String {
        let response = server.route(
            &request("POST", "/login", json!({"password":"openxplay"})),
            1,
        );
        assert_eq!(response.status, 200);
        response.headers["Set-Cookie"]
            .split(';')
            .next()
            .unwrap()
            .into()
    }
    #[test]
    fn sessions_and_write_headers_guard_every_new_route() {
        let (server, fake) = server();
        let cookie = login(&server);
        for (method, path, body) in [
            ("POST", "/logs/export", json!({"range":"10m"})),
            ("GET", "/logs/export/test", json!({})),
            ("POST", "/logs/upload", json!({"id":"test"})),
            ("GET", "/logs/receipt", json!({})),
            ("GET", "/config/user", json!({})),
            ("GET", "/config/privacy", json!({})),
            (
                "POST",
                "/config/import",
                json!({"format":"openxplay-user-config"}),
            ),
            ("GET", "/update?channel=stable", json!({})),
        ] {
            let mut request = request(method, path, body);
            assert_eq!(server.route(&request, 2).status, 401, "{path}");
            request.headers.insert("cookie".into(), cookie.clone());
            if method == "POST" {
                request.headers.remove("x-openxplay-admin");
                assert_eq!(server.route(&request, 2).status, 403);
            }
        }
        assert!(fake.calls.lock().unwrap().is_empty());
    }
    #[test]
    fn shapes_download_headers_dry_run_caps_receipt_and_offline_error() {
        let (server, fake) = server();
        let cookie = login(&server);
        let route = |method, path, body| {
            let mut r = request(method, path, body);
            r.headers.insert("cookie".into(), cookie.clone());
            server.route(&r, 2)
        };
        let response = route("POST", "/logs/export", json!({"range":"24h"}));
        assert_eq!(response.status, 200);
        assert_eq!(
            serde_json::from_slice::<Value>(&response.body).unwrap(),
            json!({"id":"test","bytes":3,"sha256":"hash","createdMillis":1})
        );
        assert_eq!(
            route("POST", "/logs/export", json!({"range":"1h"})).status,
            422
        );
        let download = route("GET", "/logs/export/test", json!({}));
        assert_eq!(download.status, 200);
        assert_eq!(download.headers["Content-Type"], "application/zip");
        assert_eq!(
            download.headers["Content-Disposition"],
            "attachment; filename=\"openxplay-linux-logs-1.zip\""
        );
        assert!(download.body.is_empty());
        assert_eq!(download.files[0].1, 3);
        let mut wire = Vec::new();
        download.write(&mut wire).unwrap();
        assert!(wire.ends_with(b"\r\n\r\nzip"));
        for code in ["offline", "too_large", "rejected", "timeout"] {
            let response = route("POST", "/logs/upload", json!({"id":code}));
            assert_eq!(response.status, if code == "too_large" { 413 } else { 502 });
            assert_eq!(
                serde_json::from_slice::<Value>(&response.body).unwrap(),
                json!({"error":code})
            );
        }
        for path in ["/config/user", "/config/privacy"] {
            let response = route("GET", path, json!({}));
            assert_eq!(response.status, 200);
            assert!(response.headers["Content-Disposition"].starts_with("attachment;"));
        }
        let response = route("POST", "/config/import?dryRun=1", json!({}));
        assert_eq!(
            serde_json::from_slice::<Value>(&response.body).unwrap(),
            json!({"applied":["output"],"ignored":["androidOnly"],"restartNeeded":true})
        );
        assert!(fake.calls.lock().unwrap().contains(&"import:true".into()));
        assert_eq!(route("GET", "/logs/receipt", json!({})).status, 404);
        let response = route("GET", "/update?channel=beta", json!({}));
        assert_eq!(
            serde_json::from_slice::<Value>(&response.body).unwrap(),
            json!({"error":"offline"})
        );
        assert_eq!(
            route("GET", "/update?channel=nightly", json!({})).status,
            422
        );
        assert_eq!(
            route(
                "POST",
                "/config/import",
                json!({"padding":"x".repeat(65536)})
            )
            .status,
            413
        );
    }
}
