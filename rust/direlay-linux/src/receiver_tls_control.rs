// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use super::{ControlPipe, PemTls, SupervisorSettings};
use serde_json::{json, Value};
use std::{
    io,
    net::Ipv4Addr,
    sync::{Condvar, Mutex},
    time::Duration,
};

#[derive(Clone)]
pub(super) struct Settings {
    pub pem: Option<PemTls>,
    pub default_tls: Option<crate::identity::tls::DefaultTls>,
    pub sans: Vec<Ipv4Addr>,
}
struct Pending {
    id: String,
    settings: Settings,
    reply: Option<Value>,
}
struct State {
    generation: u64,
    sequence: u64,
    connected: bool,
    settings: Settings,
    pending: Option<Pending>,
}
pub(super) struct WebTlsControl {
    state: Mutex<State>,
    wake: Condvar,
}
impl WebTlsControl {
    pub fn new(settings: &SupervisorSettings) -> Self {
        Self {
            state: Mutex::new(State {
                generation: 0,
                sequence: 0,
                connected: false,
                settings: Settings {
                    pem: settings.pem_tls.clone(),
                    default_tls: settings.default_tls.clone(),
                    sans: settings.addresses.clone(),
                },
                pending: None,
            }),
            wake: Condvar::new(),
        }
    }
    pub fn settings(&self) -> Settings {
        self.state.lock().unwrap().settings.clone()
    }
    /// Called under the supervisor's control-pipe lock, before the child can emit events.
    pub fn begin_child(&self) -> (u64, Settings) {
        let mut state = self.state.lock().unwrap();
        state.generation += 1;
        state.connected = true;
        state.pending = None;
        self.wake.notify_all();
        (state.generation, state.settings.clone())
    }
    pub fn disconnect(&self, generation: u64) {
        let mut state = self.state.lock().unwrap();
        if state.generation == generation {
            state.connected = false;
            state.pending = None;
            self.wake.notify_all();
        }
    }
    pub fn observe(&self, generation: u64, event: &Value) {
        if event["type"] != "web-tls" {
            return;
        }
        let mut state = self.state.lock().unwrap();
        if state.connected && state.generation == generation {
            if let Some(pending) = state.pending.as_mut() {
                if event["id"].as_str() == Some(&pending.id) && pending.reply.is_none() {
                    pending.reply = Some(event.clone());
                    self.wake.notify_all();
                }
            }
        }
    }
    /// W2's complete replacement: immutable PEM paths and every desired DNS/IP SAN.
    /// Register before writing so even an immediate reply cannot be missed.
    pub fn send(&self, pipe: &ControlPipe, settings: Settings) -> io::Result<String> {
        let mut state = self.state.lock().unwrap();
        if !state.connected {
            return Err(io::ErrorKind::NotConnected.into());
        }
        if state.pending.is_some() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        state.sequence += 1;
        let id = format!("tls-{}-{}", state.generation, state.sequence);
        let trusted = settings
            .pem
            .as_ref()
            .map(|pem| json!({"name":pem.name,"cert":pem.cert,"key":pem.key}));
        let sans: Vec<_> = std::iter::once("openxplay-linux.local".to_owned())
            .chain(settings.sans.iter().map(ToString::to_string))
            .collect();
        let mut request = json!({"type":"web-tls","id":id,"trusted":trusted,"sans":sans});
        // W15: optional default material is separate from trusted SNI and generated SANs.
        if settings.default_tls.is_some() || state.settings.default_tls.is_some() {
            request["default"] = settings
                .default_tls
                .as_ref()
                .map(|base| json!({"cert":base.cert,"key":base.key}))
                .unwrap_or(Value::Null);
        }
        state.pending = Some(Pending {
            id: id.clone(),
            settings,
            reply: None,
        });
        if let Err(error) = pipe.send_control(&request) {
            state.pending = None;
            return Err(error);
        }
        Ok(id)
    }
    pub fn wait(&self, id: &str, timeout: Duration) -> io::Result<()> {
        let state = self.state.lock().unwrap();
        let (mut state, _) = self
            .wake
            .wait_timeout_while(state, timeout, |state| {
                state
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.id == id && p.reply.is_none())
            })
            .unwrap();
        if !state.pending.as_ref().is_some_and(|p| p.id == id) {
            return Err(io::ErrorKind::NotConnected.into());
        }
        let pending = state.pending.take().unwrap();
        let reply = pending.reply.ok_or(io::ErrorKind::TimedOut)?;
        let expected_name = json!(pending.settings.pem.as_ref().map(|pem| pem.name.as_str()));
        let default_matches = if let Some(base) = &pending.settings.default_tls {
            reply["defaultFingerprint"].as_str() == Some(&base.fingerprint)
        } else if state.settings.default_tls.is_some() {
            reply.get("defaultFingerprint") == Some(&Value::Null)
        } else {
            true
        };
        if !default_matches
            || reply["applied"] != true
            || reply.get("error").is_some()
            || reply.get("trustedName") != Some(&expected_name)
        {
            return Err(io::Error::other(
                "receiver TLS replacement was not acknowledged",
            ));
        }
        state.settings = pending.settings;
        Ok(())
    }
}
