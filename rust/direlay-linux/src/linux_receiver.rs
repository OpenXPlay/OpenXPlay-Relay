// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-28

use crate::Result;
use crate::*;
use carplay_airplay::*;
use carplay_iap2::mfi::MfiAuthenticator;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};

pub(crate) type Watchdog =
    Arc<Mutex<Option<carplay_airplay::first_tcp_watchdog::FirstTcpWatchdog>>>;
pub(crate) type BridgeWiring = (Arc<DiPlayInput>, Arc<dyn MediaSink>, Arc<JsonEvents>);
pub(crate) struct ReceiverSetup {
    pub address: BindAddress,
    pub config: AirPlayConfig,
    pub identity: Arc<AirPlayIdentity>,
    pub pairings: Arc<dyn PairingStore>,
    pub mfi: Arc<dyn MfiAuthenticator>,
    pub sink: Arc<dyn MediaSink>,
    pub events: Arc<JsonEvents>,
    pub callbacks: NetworkCallbacks,
    pub closed: Arc<AtomicBool>,
    pub input: Option<BridgeWiring>,
    pub watchdog: Watchdog,
    pub current: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub tunnel: Option<Arc<dyn IapTunnelListener>>,
}

pub(crate) fn build_receiver(setup: ReceiverSetup) -> Result<Arc<LinuxAirPlayServer>> {
    let transport: Arc<dyn StreamTransport> = Arc::new(LinuxStreamTransport::new(setup.address));
    let media = Arc::new(CarPlayMediaEngine::with_screen_session_policy(
        setup.sink.clone(),
        transport.clone(),
        true,
        crate::carplay_policy::screen_session_policy(
            &setup
                .input
                .as_ref()
                .map(|(input, _, _)| input.connection_settings())
                .unwrap_or_default(),
        ),
    ));
    media.set_iap_tunnel_handler(setup.tunnel.clone())?;
    let listener = Arc::new(SessionListener {
        input: setup.input.as_ref().map(|(i, _, _)| i.clone()),
        callbacks: setup.callbacks,
        closed: setup.closed,
        events: setup.events,
        active: Mutex::new(Vec::new()),
        current: setup.current,
        watchdog: setup.watchdog.clone(),
    });
    let mut server = LinuxAirPlayServer::new(
        setup.address,
        setup.config,
        setup.identity,
        setup.pairings,
        setup.mfi,
        media,
        listener,
        transport,
    );
    if let Some((input, sink, events)) = setup.input {
        server = server.with_media_factory(Arc::new(move |control, transport| {
            let bridge = Arc::new(
                StreamBridgeSink::new(input.clone(), sink.clone(), control, events.clone())
                    .with_arrival_clock(Arc::new(|| oxp_core::monotonic_nanos() as u64)),
            );
            let engine = Arc::new(CarPlayMediaEngine::with_screen_session_policy(
                bridge,
                transport,
                true,
                crate::carplay_policy::screen_session_policy(&input.connection_settings()),
            ));
            let _ = engine.set_iap_tunnel_handler(setup.tunnel.clone());
            engine
        }));
    }
    let server = Arc::new(server);
    *setup.watchdog.lock().unwrap() = Some(
        carplay_airplay::first_tcp_watchdog::FirstTcpWatchdog::new(server.listener_identity())
            .with_session_timeout(Duration::from_secs(45)),
    );
    Ok(server)
}

pub(crate) struct SessionListener {
    pub(crate) current: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub(crate) input: Option<Arc<DiPlayInput>>,
    pub(crate) callbacks: NetworkCallbacks,
    pub(crate) closed: Arc<AtomicBool>,
    pub(crate) events: Arc<JsonEvents>,
    pub(crate) active: Mutex<Vec<Weak<AirPlaySession>>>,
    pub(crate) watchdog: Arc<Mutex<Option<carplay_airplay::first_tcp_watchdog::FirstTcpWatchdog>>>,
}
impl SessionListener {
    fn current(&self) -> bool {
        !self.closed.load(Ordering::Acquire) && self.current.as_ref().is_none_or(|f| f())
    }
}
impl AirPlaySessionListener for SessionListener {
    fn on_tcp_accepted(&self, event: AirPlayTcpAccepted) {
        if !self.current() {
            return;
        }
        if let Some(w) = self.watchdog.lock().unwrap().as_mut() {
            w.accepted(&event, std::time::Instant::now());
        }
    }
    fn on_host_ui_requested(&self, _: &AirPlaySession) {
        if self.current() {
            if let Some(input) = &self.input {
                input.host_ui_requested();
            }
        }
    }
    fn on_startup_milestone(&self, _: &AirPlaySession, name: &str) {
        if self.current() {
            self.events.startup_milestone(name);
        }
    }
    fn on_video_frame_rendered(&self, _: &AirPlaySession) {
        if self.current() {
            self.events.startup_milestone("first_video_rendered");
        }
    }
    fn on_session_active(&self, s: Arc<AirPlaySession>) {
        if self.current() {
            if let Some(w) = self.watchdog.lock().unwrap().as_mut() {
                if !w.session_established() {
                    return;
                }
            }
            self.active.lock().unwrap().push(Arc::downgrade(&s));
            if let Some(input) = &self.input {
                input.session_context(true);
            }
            self.events
                .emit("airplay-session-active", &serde_json::json!({}));
            (self.callbacks.on_air_play)()
        }
    }
    fn on_session_ended(&self, s: Arc<AirPlaySession>) {
        if self.current.as_ref().is_some_and(|f| !f()) {
            return;
        }
        let mut active = self.active.lock().unwrap();
        let before = active.len();
        active.retain(|w| w.upgrade().is_some_and(|x| !Arc::ptr_eq(&x, &s)));
        if before > 0 && active.is_empty() {
            if let Some(input) = &self.input {
                input.session_context(false);
            }
        }
        if before > 0 && active.is_empty() && self.current() {
            (self.callbacks.on_failure)(std::io::Error::other("AirPlay session ended"));
        }
    }
    fn on_transport_error(&self, s: &str) {
        if self.current() {
            (self.callbacks.on_failure)(std::io::Error::other(s));
        }
    }
    fn on_command(&self, _: &AirPlaySession, kind: &str, params: &BplistValue) {
        if self.current.as_ref().is_some_and(|f| !f()) {
            return;
        }
        if let Some(input) = &self.input {
            input.airplay_context(kind, params);
        }
        self.events
            .log(&format!("airplay command type={kind} params={params:?}"))
    }
    fn on_debug_log(&self, s: &str) {
        if self.current.as_ref().is_none_or(|f| f()) {
            self.events.log(s);
        }
    }
}
