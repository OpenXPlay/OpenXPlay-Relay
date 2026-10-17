// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Fresh relay mapping of protocol observations to complete Stream snapshots.
use crate::DiPlayInput;
use carplay_iap2::{context_decode::NowPlayingChange, Iap2Frame};
use oxp_core::{
    ActivityContext, AudioSourceContext, CallContext, CallState, CarPlayState, ContextTopic,
    ContextUpdate, NowPlayingContext, SessionContext,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub(crate) struct Producer {
    route_state: carplay_iap2::route_decode::RouteState,
    route: crate::context_route::RouteContext,
    peer: bool,
    latest: BTreeMap<ContextTopic, ContextUpdate>,
    dirty: bool,
    sent: Option<Instant>,
    position_at: Instant,
}
impl Default for Producer {
    fn default() -> Self {
        Self {
            route_state: Default::default(),
            route: Default::default(),
            peer: false,
            latest: ContextUpdate::idle()
                .into_iter()
                .map(|u| (u.topic(), u))
                .collect(),
            dirty: true,
            sent: None,
            position_at: Instant::now(),
        }
    }
}
impl Producer {
    fn publish(&mut self, update: ContextUpdate) {
        let topic = update.topic();
        if self.latest.get(&topic) != Some(&update) {
            self.latest.insert(topic, update);
            self.dirty = true;
        }
    }
    fn advance(n: &mut NowPlayingContext, elapsed: Duration) {
        if n.playing == Some(true) {
            if let (Some(position), Some(rate)) = (n.elapsed_ms, n.rate) {
                let value = (position as f64 + elapsed.as_secs_f64() * 1000.0 * rate)
                    .clamp(0.0, 9_007_199_254_740_991.0) as u64;
                n.elapsed_ms = Some(n.duration_ms.map_or(value, |d| value.min(d)));
            }
        }
    }
    fn now_playing(&mut self, mut change: NowPlayingChange, now: Instant) {
        if let Some(app_name) = change.source_app.take() {
            self.publish(ContextUpdate::AudioSource(Some(AudioSourceContext {
                app_name,
            })));
        }
        if change == NowPlayingChange::default() {
            return;
        }
        let mut n = match self.latest.get(&ContextTopic::NowPlaying) {
            Some(ContextUpdate::NowPlaying(Some(n))) => n.clone(),
            _ => NowPlayingContext {
                title: None,
                artist: None,
                album: None,
                duration_ms: None,
                elapsed_ms: None,
                playing: None,
                rate: None,
                artwork: None,
            },
        };
        Self::advance(&mut n, now.saturating_duration_since(self.position_at));
        self.position_at = now;
        if let Some(v) = change.title {
            n.title = Some(v);
        }
        if let Some(v) = change.artist {
            n.artist = Some(v);
        }
        if let Some(v) = change.album {
            n.album = Some(v);
        }
        if let Some(v) = change.duration_ms {
            n.duration_ms = Some(v.into());
        }
        if let Some(v) = change.elapsed_ms {
            n.elapsed_ms = Some(v.into());
        }
        if let Some(v) = change.status {
            n.playing = Some(matches!(v, 1 | 3 | 4));
        }
        n.rate = Some(if n.playing == Some(true) { 1.0 } else { 0.0 });
        self.publish(ContextUpdate::NowPlaying(Some(n)));
    }
    fn activity(&mut self, change: carplay_airplay::activity::ActivityChange) {
        let mut a = match self.latest.get(&ContextTopic::Activity) {
            Some(ContextUpdate::Activity(Some(a))) => a.clone(),
            _ => ActivityContext {
                siri: None,
                phone_call: None,
                navigation: None,
            },
        };
        if let Some(v) = change.siri {
            a.siri = Some(v);
        }
        if let Some(v) = change.phone_call {
            a.phone_call = Some(v);
        }
        if let Some(v) = change.navigation {
            a.navigation = Some(v);
        }
        self.publish(ContextUpdate::Activity(Some(a)));
    }
    fn take(&mut self, now: Instant) -> Vec<ContextUpdate> {
        if !self.peer
            || !self.dirty
            || self
                .sent
                .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(250))
        {
            return Vec::new();
        }
        self.dirty = false;
        self.sent = Some(now);
        self.latest
            .values()
            .map(|u| {
                let mut update = u.clone();
                if let ContextUpdate::NowPlaying(Some(n)) = &mut update {
                    Self::advance(n, now.saturating_duration_since(self.position_at));
                }
                update
            })
            .collect()
    }
}
fn call_state(status: u8) -> Option<CallState> {
    Some(match status {
        0 => CallState::Idle,
        1 | 3 => CallState::Dialing,
        2 => CallState::Ringing,
        4 => CallState::Active,
        5 => CallState::Held,
        6 => CallState::Ending,
        _ => return None,
    })
}
impl DiPlayInput {
    /// The closure (including decoding and allocations) is never entered when disabled.
    fn with_context(&self, action: impl FnOnce(&mut Producer)) {
        if let Some(producer) = self.context.lock().unwrap().as_mut() {
            action(producer);
        }
    }
    pub fn plugins_enabled(&self) -> bool {
        self.context.lock().unwrap().is_some()
    }
    pub fn set_plugins_enabled(&self, enabled: bool) {
        let connected = self.state.lock().unwrap().current.is_some();
        let mut context = self.context.lock().unwrap();
        if enabled && context.is_none() {
            let mut producer = Producer::default();
            if connected {
                producer.publish(ContextUpdate::Session(Some(SessionContext {
                    carplay: Some(CarPlayState::Connected),
                })));
            }
            *context = Some(producer);
        } else if !enabled {
            *context = None;
        }
    }
    pub fn iap2_context(&self, frame: &Iap2Frame) {
        self.with_context(|p| match frame.message_id {
            0x5001 => {
                if let Ok(Some(change)) = carplay_iap2::context_decode::now_playing(frame) {
                    p.now_playing(change, Instant::now());
                }
            }
            0x4155 => {
                if let Ok(Some(state)) = carplay_iap2::context_decode::call_status(frame) {
                    p.publish(ContextUpdate::Call(Some(CallContext {
                        state: call_state(state),
                    })));
                }
            }
            0x5201 | 0x5202 => {
                if let Ok(Some(change)) = carplay_iap2::route_decode::decode(frame) {
                    p.route_state.accept(change);
                    let route = p.route_state.snapshot().into();
                    p.route = route;
                    p.publish(crate::context_route::stream_update(&p.route));
                }
            }
            _ => {}
        });
    }
    pub fn airplay_context(&self, kind: &str, params: &carplay_airplay::BplistValue) {
        self.with_context(|p| {
            if matches!(kind, "modesChanged" | "changeModes") {
                if let Some(change) = carplay_airplay::activity::decode_modes(params) {
                    p.activity(change);
                }
            }
        });
    }
    /// Changes that do not come from iAP2 or AirPlay decoding (the test input's context script)
    /// enter the same producer.
    pub(crate) fn now_playing_context(&self, change: NowPlayingChange) {
        self.with_context(|p| p.now_playing(change, Instant::now()));
    }
    pub(crate) fn call_context(&self, state: Option<CallState>) {
        self.with_context(|p| p.publish(ContextUpdate::Call(Some(CallContext { state }))));
    }
    pub(crate) fn activity_context(&self, change: carplay_airplay::activity::ActivityChange) {
        self.with_context(|p| p.activity(change));
    }
    pub fn session_context(&self, connected: bool) {
        self.with_context(|p| {
            if !connected {
                p.route_state = Default::default();
                p.route = Default::default();
                p.dirty = true;
                for update in ContextUpdate::idle() {
                    p.publish(update);
                }
                p.position_at = Instant::now();
            } else {
                p.publish(ContextUpdate::Session(Some(SessionContext {
                    carplay: Some(CarPlayState::Connected),
                })));
            }
        });
    }
    pub(crate) fn context_peer(&self, hello: &serde_json::Value) {
        self.with_context(|p| {
            p.peer = oxp_wire::supports_context(hello);
            p.dirty = true;
            p.sent = None;
        });
    }
    pub(crate) fn context_disconnected(&self) {
        self.with_context(|p| p.peer = false);
    }
    pub(crate) fn take_context(&self) -> Vec<oxp_wire::Message> {
        // Clone the bounded slots, then release the producer lock before encoding or I/O.
        let updates = self
            .context
            .lock()
            .unwrap()
            .as_mut()
            .map(|p| p.take(Instant::now()))
            .unwrap_or_default();
        updates
            .iter()
            .filter_map(|u| oxp_wire::Message::context(u).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn input(enabled: bool) -> DiPlayInput {
        DiPlayInput::with_plugins(Arc::new(crate::JsonEvents::new(Arc::new(|_| {}))), enabled)
    }
    fn route_frame(id: u16, fields: &[(u16, &[u8])]) -> Iap2Frame {
        let mut payload = Vec::new();
        for (id, bytes) in fields {
            payload.extend(((bytes.len() + 4) as u16).to_be_bytes());
            payload.extend(id.to_be_bytes());
            payload.extend(*bytes);
        }
        Iap2Frame {
            message_id: id,
            payload,
        }
    }
    #[test]
    fn route_gating_latest_partial_mapping_and_session_clear() {
        for enabled in [false, true] {
            let input = input(enabled);
            input.iap2_context(&route_frame(
                0x5202,
                &[(1, &[0, 7]), (3, &[2]), (4, b"Next St\0")],
            ));
            input.iap2_context(&route_frame(
                0x5201,
                &[
                    (1, &[1]),
                    (3, b"Main St\0"),
                    (19, b"Maps\0"),
                    (6, &90u64.to_be_bytes()),
                    (7, &1000u32.to_be_bytes()),
                    (10, &250u32.to_be_bytes()),
                    (13, &[0, 7]),
                ],
            ));
            if !enabled {
                assert!(input.context.lock().unwrap().is_none());
                continue;
            }
            input.iap2_context(&route_frame(0x5201, &[(10, &200u32.to_be_bytes())]));
            let snapshot = input
                .context
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .route
                .clone();
            assert_eq!(snapshot.eta_ms, Some(90000));
            assert_eq!(snapshot.distance_remaining_m, Some(1000));
            assert!(snapshot.active);
            assert!(snapshot.destination.is_none());
            let m = snapshot.maneuver.as_ref().unwrap();
            assert_eq!(m.r#type, Some(2));
            assert_eq!(m.road.as_deref(), Some("Next St"));
            assert_eq!(m.distance_m, Some(200));
            assert_eq!(
                input.context.lock().unwrap().as_ref().unwrap().latest[&ContextTopic::Route],
                crate::context_route::stream_update(&snapshot)
            );
            for id in [0x5201, 0x5202] {
                input.iap2_context(&Iap2Frame {
                    message_id: id,
                    payload: vec![0],
                });
            }
            input.iap2_context(&route_frame(0x5201, &[(10, &[1])]));
            assert_eq!(
                input.context.lock().unwrap().as_ref().unwrap().route,
                snapshot
            );
            input.session_context(false);
            assert_eq!(
                input.context.lock().unwrap().as_ref().unwrap().route,
                Default::default()
            );
            assert_eq!(
                input.context.lock().unwrap().as_ref().unwrap().latest[&ContextTopic::Route],
                ContextUpdate::Route(None)
            );
            // The new session cannot reuse an indexed maneuver from the old one.
            input.iap2_context(&route_frame(0x5201, &[(1, &[1]), (13, &[0, 7])]));
            let p = input.context.lock().unwrap();
            let r = &p.as_ref().unwrap().route;
            assert!(r.eta_ms.is_none() && r.app_name.is_none());
            assert!(r.maneuver.as_ref().unwrap().r#type.is_none());
        }
    }
    #[test]
    fn route_end_empty_list_app_only_and_unknown_index() {
        let input = input(true);
        assert_eq!(
            input.context.lock().unwrap().as_ref().unwrap().latest[&ContextTopic::Route],
            ContextUpdate::Route(None)
        );
        // A known inactive route is a snapshot, even when all optional values are unknown.
        input.iap2_context(&route_frame(0x5201, &[(1, &[0])]));
        assert_eq!(
            input.context.lock().unwrap().as_ref().unwrap().latest[&ContextTopic::Route],
            crate::context_route::stream_update(&Default::default())
        );
        input.session_context(false);
        input.iap2_context(&route_frame(0x5201, &[(19, b"Maps\0")]));
        assert!(input.context.lock().unwrap().as_ref().unwrap().route.active);
        input.iap2_context(&route_frame(
            0x5201,
            &[(1, &[1]), (13, &[0, 8]), (3, b"Main St\0")],
        ));
        assert_eq!(
            input
                .context
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .route
                .maneuver
                .as_ref()
                .unwrap()
                .road
                .as_deref(),
            Some("Main St")
        );
        input.iap2_context(&route_frame(0x5201, &[(13, &[])]));
        assert!(input
            .context
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .route
            .maneuver
            .is_none());
        for state in [0, 2] {
            input.iap2_context(&route_frame(0x5201, &[(1, &[state])]));
            assert_eq!(
                input.context.lock().unwrap().as_ref().unwrap().route,
                Default::default()
            );
        }
        input.iap2_context(&route_frame(0x5201, &[(1, &[1]), (19, b"\0")]));
        assert!(!input.context.lock().unwrap().as_ref().unwrap().route.active);
    }
    #[test]
    fn audio_source_gating_partial_clear_and_wire_delivery() {
        for enabled in [false, true] {
            let input = input(enabled);
            input.context_peer(
                &serde_json::json!({"role":"consumer","client":oxp_wire::context_client("fake")}),
            );
            let source = route_frame(1, &[(7, b" Music \0")]);
            input.iap2_context(&route_frame(0x5001, &[(1, &source.payload)]));
            if !enabled {
                assert!(input.take_context().is_empty());
                continue;
            }
            let updates: Vec<_> = input
                .take_context()
                .into_iter()
                .map(|m| match m {
                    oxp_wire::Message::Json(v) => oxp_wire::decode_context(&v).unwrap(),
                    _ => panic!(),
                })
                .collect();
            assert_eq!(updates.len(), ContextUpdate::idle().len());
            assert!(
                updates.contains(&ContextUpdate::AudioSource(Some(AudioSourceContext {
                    app_name: Some("Music".into())
                })))
            );
            assert!(updates.contains(&ContextUpdate::NowPlaying(None)));
            let unrelated = route_frame(1, &[(0, &[1])]);
            input.iap2_context(&route_frame(0x5001, &[(1, &unrelated.payload)]));
            let before = input.context.lock().unwrap().as_ref().unwrap().latest
                [&ContextTopic::AudioSource]
                .clone();
            assert_eq!(
                before,
                ContextUpdate::AudioSource(Some(AudioSourceContext {
                    app_name: Some("Music".into())
                }))
            );
            let empty = route_frame(1, &[(7, b" \0")]);
            input.iap2_context(&route_frame(0x5001, &[(1, &empty.payload)]));
            assert_eq!(
                input.context.lock().unwrap().as_ref().unwrap().latest[&ContextTopic::AudioSource],
                ContextUpdate::AudioSource(Some(AudioSourceContext { app_name: None }))
            );
            input.session_context(false);
            assert_eq!(
                input.context.lock().unwrap().as_ref().unwrap().latest[&ContextTopic::AudioSource],
                ContextUpdate::AudioSource(None)
            );
        }
    }
    #[test]
    fn subscriptions_add_route_only_in_plugin_mode() {
        let expected = [
            "4040002e5000001800000004000100040004000400060004000c0004001a00100001000400000004000100040007",
            "40400012ae00000400040004000500040006",
            "404000124157000400000004000400040005",
            "4040001e415400040000000400010004000200040003000400040004000b",
        ];
        for enabled in [false, true] {
            let input = input(enabled);
            assert_eq!(input.plugins_enabled(), enabled);
            let actual: Vec<String> =
                carplay_iap2::Iap2ControlMessages::subscriptions_with_route_guidance(
                    input.plugins_enabled(),
                )
                .unwrap()
                .iter()
                .map(|f| {
                    f.encoded_frame()
                        .unwrap()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect()
                })
                .collect();
            let mut expected = expected.to_vec();
            if enabled {
                expected.insert(1, "40400014520000060000002a0004000100040002");
            }
            assert_eq!(actual, expected);
        }
    }
    #[test]
    fn playback_status_controls_rate_and_extrapolation() {
        for status in [0, 1, 2, 3, 4, 255] {
            let mut p = Producer::default();
            let t = Instant::now();
            p.now_playing(
                NowPlayingChange {
                    elapsed_ms: Some(1000),
                    status: Some(status),
                    ..Default::default()
                },
                t,
            );
            p.now_playing(
                NowPlayingChange {
                    title: Some("Track".into()),
                    ..Default::default()
                },
                t + Duration::from_secs(2),
            );
            let ContextUpdate::NowPlaying(Some(n)) = &p.latest[&ContextTopic::NowPlaying] else {
                panic!()
            };
            let playing = matches!(status, 1 | 3 | 4);
            assert_eq!(n.playing, Some(playing));
            assert_eq!(n.rate, Some(if playing { 1.0 } else { 0.0 }));
            assert_eq!(n.elapsed_ms, Some(if playing { 3000 } else { 1000 }));
        }
    }
    #[test]
    fn disabled_paths_never_decode_or_publish() {
        let input = input(false);
        input.with_context(|_| panic!("decoder must not run"));
        input.iap2_context(&Iap2Frame {
            message_id: 0x5001,
            payload: vec![0],
        });
        input.iap2_context(&Iap2Frame {
            message_id: 0x4155,
            payload: vec![0],
        });
        input.airplay_context("modesChanged", &carplay_airplay::BplistValue::Null);
        input.session_context(true);
        input.context_peer(
            &serde_json::json!({"role":"consumer", "client":oxp_wire::context_client("fake")}),
        );
        assert!(!input
            .control(
                &oxp_wire::Message::hello("consumer", "fake", &oxp_wire::context_client("fake"))
                    .unwrap()
            )
            .unwrap());
        assert!(!input.plugins_enabled());
        assert!(input.take_context().is_empty());
    }
    #[test]
    fn malformed_and_unknown_updates_leave_snapshot_unchanged() {
        let input = input(true);
        input.session_context(true);
        input.iap2_context(&Iap2Frame {
            message_id: 0x4155,
            payload: vec![0, 5, 0, 2, 4],
        });
        let before = input
            .context
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .latest
            .clone();
        for message_id in [0x5001, 0x4155, 0x4158, 0xffff] {
            input.iap2_context(&Iap2Frame {
                message_id,
                payload: vec![0],
            });
        }
        input.airplay_context("modesChanged", &carplay_airplay::BplistValue::Null);
        assert_eq!(
            input.context.lock().unwrap().as_ref().unwrap().latest,
            before
        );
        assert_eq!(
            before[&ContextTopic::Call],
            ContextUpdate::Call(Some(CallContext {
                state: Some(CallState::Active)
            }))
        );
        input.airplay_context(
            "modesChanged",
            &carplay_airplay::BplistValue::dict([(
                "appStates",
                carplay_airplay::BplistValue::Array(vec![carplay_airplay::BplistValue::dict([
                    ("appStateID", 3u64.into()),
                    ("entity", 1u64.into()),
                ])]),
            )]),
        );
        let guard = input.context.lock().unwrap();
        let p = guard.as_ref().unwrap();
        assert_eq!(
            p.latest[&ContextTopic::Activity],
            ContextUpdate::Activity(Some(ActivityContext {
                siri: Some(false),
                phone_call: Some(false),
                navigation: Some(true)
            }))
        );
    }
    #[test]
    fn partial_snapshots_advance_before_status_changes() {
        let mut p = Producer::default();
        let t = Instant::now();
        p.now_playing(
            NowPlayingChange {
                title: Some("A".into()),
                elapsed_ms: Some(1000),
                status: Some(1),
                ..Default::default()
            },
            t,
        );
        p.now_playing(
            NowPlayingChange {
                artist: Some("B".into()),
                ..Default::default()
            },
            t + Duration::from_secs(2),
        );
        p.now_playing(
            NowPlayingChange {
                status: Some(2),
                ..Default::default()
            },
            t + Duration::from_secs(3),
        );
        let ContextUpdate::NowPlaying(Some(n)) = &p.latest[&ContextTopic::NowPlaying] else {
            panic!()
        };
        assert_eq!(n.title.as_deref(), Some("A"));
        assert_eq!(n.artist.as_deref(), Some("B"));
        assert_eq!(n.elapsed_ms, Some(4000));
        assert_eq!(n.rate, Some(0.0));
        assert_eq!(n.playing, Some(false));
        assert!(n.artwork.is_none());
        for (s, expected) in [
            (0, CallState::Idle),
            (1, CallState::Dialing),
            (2, CallState::Ringing),
            (3, CallState::Dialing),
            (4, CallState::Active),
            (5, CallState::Held),
            (6, CallState::Ending),
        ] {
            assert_eq!(call_state(s), Some(expected));
        }
        assert_eq!(call_state(255), None);
    }
    #[test]
    fn producer_to_fake_consumer_uses_negotiated_wire_frames() {
        for enabled in [false, true] {
            let input = input(true);
            let client = if enabled {
                oxp_wire::context_client("fake")
            } else {
                "legacy".into()
            };
            let hello = oxp_wire::Message::hello("consumer", "fake", &client).unwrap();
            input.control(&hello).unwrap();
            input.iap2_context(&Iap2Frame {
                message_id: 0x5001,
                payload: vec![0, 13, 0, 0, 0, 9, 0, 1, b'S', b'o', b'n', b'g', 0],
            });
            let mut bytes = Vec::new();
            for message in input.take_context() {
                oxp_wire::write(&mut bytes, &message).unwrap();
            }
            let mut reader = std::io::Cursor::new(bytes);
            let mut title = None;
            while let Some(oxp_wire::Message::Json(v)) = oxp_wire::read(&mut reader).unwrap() {
                if let ContextUpdate::NowPlaying(Some(n)) = oxp_wire::decode_context(&v).unwrap() {
                    title = n.title;
                }
            }
            assert_eq!(title.as_deref(), enabled.then_some("Song"));
            if !enabled {
                assert!(reader.into_inner().is_empty());
            }
        }
    }
    #[test]
    fn disconnect_resets_all_topics_and_old_peer_cannot_receive() {
        let input = input(true);
        input.session_context(true);
        input.context_peer(&serde_json::json!({"role":"consumer","client":"legacy"}));
        assert!(input.take_context().is_empty());
        input.context_peer(
            &serde_json::json!({"role":"consumer","client":oxp_wire::context_client("fake")}),
        );
        assert_eq!(input.take_context().len(), ContextUpdate::idle().len());
        input.session_context(false);
        input.context_disconnected();
        assert!(input.take_context().is_empty());
        input.context_peer(
            &serde_json::json!({"role":"consumer","client":oxp_wire::context_client("fake")}),
        );
        let updates: Vec<_> = input
            .take_context()
            .into_iter()
            .map(|m| match m {
                oxp_wire::Message::Json(v) => oxp_wire::decode_context(&v).unwrap(),
                _ => panic!(),
            })
            .collect();
        assert_eq!(updates, ContextUpdate::idle());
    }
}
