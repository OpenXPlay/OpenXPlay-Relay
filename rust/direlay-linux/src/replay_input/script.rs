// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Scripted CarPlay context for test sessions. Steps enter the context producer the real iAP2 and
//! AirPlay decoders feed, so they reach Stream only when the plugins setting is on.
//!
//! A script is a JSON array of steps (played once) or `{"repeat":true,"steps":[...]}`. A step
//! waits `afterMs` after the previous one, then applies any of `nowPlaying` (`title`, `artist`,
//! `album`, `durationMs`, `elapsedMs`, `playing`), `call` (`idle`, `ringing`, `dialing`,
//! `active`, `held`, `ending` or `unknown`) and `activity` (`siri`, `phoneCall`, `navigation`).
use crate::Result;
use crate::{DiPlayInput, JsonEvents};
use carplay_airplay::activity::ActivityChange;
use carplay_iap2::context_decode::NowPlayingChange;
use oxp_core::CallState;
use std::time::Duration;

fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NowPlayingStep {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration_ms: Option<u32>,
    elapsed_ms: Option<u32>,
    playing: Option<bool>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ActivityStep {
    siri: Option<bool>,
    phone_call: Option<bool>,
    navigation: Option<bool>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct StepJson {
    #[serde(default)]
    after_ms: u64,
    now_playing: Option<NowPlayingStep>,
    call: Option<String>,
    activity: Option<ActivityStep>,
}
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum ScriptJson {
    Steps(Vec<StepJson>),
    Script {
        #[serde(default)]
        repeat: bool,
        steps: Vec<StepJson>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContextStep {
    pub after: Duration,
    pub now_playing: Option<NowPlayingChange>,
    /// `Some(None)` publishes a call whose state is unknown.
    pub call: Option<Option<CallState>>,
    pub activity: Option<ActivityChange>,
}
impl ContextStep {
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(n) = &self.now_playing {
            let mut s = "nowPlaying".to_string();
            if let Some(t) = &n.title {
                s.push_str(&format!(" title={t:?}"));
            }
            if let Some(status) = n.status {
                s.push_str(if status == 1 { " playing" } else { " paused" });
            }
            parts.push(s);
        }
        if let Some(call) = &self.call {
            parts.push(format!(
                "call {}",
                call.map_or("unknown".into(), |c| format!("{c:?}").to_lowercase())
            ));
        }
        if let Some(a) = &self.activity {
            parts.push(format!(
                "activity siri={:?} phoneCall={:?} navigation={:?}",
                a.siri, a.phone_call, a.navigation
            ));
        }
        if parts.is_empty() {
            "pause".into()
        } else {
            parts.join(", ")
        }
    }
    fn apply(&self, input: &DiPlayInput) {
        if let Some(n) = &self.now_playing {
            input.now_playing_context(n.clone());
        }
        if let Some(state) = self.call {
            input.call_context(state);
        }
        if let Some(a) = &self.activity {
            input.activity_context(a.clone());
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ContextScript {
    pub name: String,
    pub steps: Vec<ContextStep>,
    pub repeat: bool,
}
impl ContextScript {
    /// `demo` or a script file.
    pub fn load(spec: &str) -> Result<Self> {
        if spec == "demo" {
            return Ok(Self::demo());
        }
        let text = std::fs::read_to_string(spec).map_err(|e| invalid(format!("{spec}: {e}")))?;
        Self::parse(spec, &text)
    }
    pub fn parse(name: &str, text: &str) -> Result<Self> {
        let script: ScriptJson = serde_json::from_str(text)
            .map_err(|e| invalid(format!("{name}: invalid context script: {e}")))?;
        let (repeat, steps) = match script {
            ScriptJson::Steps(steps) => (false, steps),
            ScriptJson::Script { repeat, steps } => (repeat, steps),
        };
        let steps = steps
            .into_iter()
            .map(|s| -> Result<ContextStep> {
                Ok(ContextStep {
                    after: Duration::from_millis(s.after_ms),
                    now_playing: s.now_playing.map(|n| NowPlayingChange {
                        title: n.title,
                        artist: n.artist,
                        album: n.album,
                        duration_ms: n.duration_ms,
                        elapsed_ms: n.elapsed_ms,
                        // Playback status values: 1 playing, 2 paused.
                        status: n.playing.map(|p| if p { 1 } else { 2 }),
                        source_app: None,
                    }),
                    call: s
                        .call
                        .map(|c| match c.as_str() {
                            "unknown" => Ok(None),
                            _ => serde_json::from_value::<CallState>(serde_json::json!(c))
                                .map(Some)
                                .map_err(|_| invalid(format!("{name}: unknown call state {c}"))),
                        })
                        .transpose()?,
                    activity: s.activity.map(|a| ActivityChange {
                        siri: a.siri,
                        phone_call: a.phone_call,
                        navigation: a.navigation,
                    }),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if steps.is_empty() {
            return Err(invalid(format!("{name}: context script has no steps")));
        }
        if repeat && steps.iter().all(|s| s.after.is_zero()) {
            return Err(invalid(format!(
                "{name}: a repeating context script needs a delay"
            )));
        }
        Ok(Self {
            name: name.into(),
            steps,
            repeat,
        })
    }
    /// A repeating minute of media, navigation and a phone call.
    pub fn demo() -> Self {
        Self::parse(
            "demo",
            r#"{"repeat":true,"steps":[
              {"nowPlaying":{"title":"Replay Demo Track","artist":"OpenXPlay","album":"Test Input",
                "durationMs":215000,"elapsedMs":0,"playing":true},
               "activity":{"siri":false,"phoneCall":false,"navigation":true}},
              {"afterMs":15000,"call":"ringing","activity":{"phoneCall":true}},
              {"afterMs":4000,"call":"active"},
              {"afterMs":10000,"call":"ending"},
              {"afterMs":1000,"call":"idle","activity":{"phoneCall":false}},
              {"afterMs":10000,"nowPlaying":{"playing":false}},
              {"afterMs":5000,"nowPlaying":{"title":"Second Demo Track","durationMs":184000,
                "elapsedMs":0,"playing":true},"activity":{"navigation":false}},
              {"afterMs":15000}
            ]}"#,
        )
        .expect("built-in demo script")
    }
    /// Plays the script until `wait` reports a stop; `wait` sleeps up to the given time and
    /// returns true once the session ended.
    pub fn run(&self, input: &DiPlayInput, events: &JsonEvents, wait: &dyn Fn(Duration) -> bool) {
        loop {
            for (n, step) in self.steps.iter().enumerate() {
                if wait(step.after) {
                    return;
                }
                step.apply(input);
                events.log(&format!(
                    "Replay context step {} of {}: {}",
                    n + 1,
                    self.name,
                    step.describe()
                ));
            }
            if !self.repeat {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn scripts_parse_strictly_and_drive_the_context_producer() {
        let script = ContextScript::parse(
            "s.json",
            r#"[{"call":"ringing"},{"afterMs":5,"nowPlaying":{"title":"T","playing":true},
                "activity":{"navigation":true}},{"call":"unknown"}]"#,
        )
        .unwrap();
        assert!(!script.repeat);
        assert_eq!(script.steps[0].call, Some(Some(CallState::Ringing)));
        assert_eq!(script.steps[2].call, Some(None));
        assert_eq!(
            script.steps[1].now_playing.as_ref().unwrap().status,
            Some(1)
        );
        for bad in [
            "[]",
            r#"[{"call":"busy"}]"#,
            r#"[{"nowPlaying":{"titel":"x"}}]"#,
            r#"{"repeat":true,"steps":[{"call":"idle"}]}"#,
            "{",
        ] {
            assert!(ContextScript::parse("bad", bad).is_err(), "{bad}");
        }
        assert!(ContextScript::demo().repeat);

        let lines = Arc::new(Mutex::new(Vec::new()));
        let copy = lines.clone();
        let events = JsonEvents::new(Arc::new(move |l| copy.lock().unwrap().push(l)));
        let input = DiPlayInput::with_plugins(Arc::new(JsonEvents::new(Arc::new(|_| {}))), true);
        input
            .control(
                &oxp_wire::Message::hello("consumer", "fake", &oxp_wire::context_client("fake"))
                    .unwrap(),
            )
            .unwrap();
        let waits = Mutex::new(Vec::new());
        script.run(&input, &events, &|d| {
            waits.lock().unwrap().push(d.as_millis());
            false
        });
        assert_eq!(*waits.lock().unwrap(), [0, 5, 0]);
        let updates = input
            .take_context()
            .into_iter()
            .map(|m| match m {
                oxp_wire::Message::Json(v) => oxp_wire::decode_context(&v).unwrap(),
                _ => panic!(),
            })
            .collect::<Vec<_>>();
        assert!(updates.contains(&oxp_core::ContextUpdate::Call(Some(
            oxp_core::CallContext { state: None }
        ))));
        assert!(updates.iter().any(|u| matches!(u,
            oxp_core::ContextUpdate::NowPlaying(Some(n)) if n.title.as_deref() == Some("T") && n.playing == Some(true))));
        assert!(updates.iter().any(|u| matches!(u,
            oxp_core::ContextUpdate::Activity(Some(a)) if a.navigation == Some(true))));
        assert_eq!(lines.lock().unwrap().len(), 3);
        // A stop during the first wait applies nothing.
        let quiet = DiPlayInput::with_plugins(Arc::new(JsonEvents::new(Arc::new(|_| {}))), true);
        script.run(&quiet, &events, &|_| true);
        assert_eq!(lines.lock().unwrap().len(), 3);
    }
}
