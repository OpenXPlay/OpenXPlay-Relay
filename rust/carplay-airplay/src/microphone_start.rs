// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::Result;
use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
#[derive(Clone)]
pub struct MicrophoneStart {
    inner: Arc<Inner>,
}
struct Inner {
    state: Mutex<State>,
    changed: Condvar,
    action: Arc<dyn Fn() + Send + Sync>,
}
#[derive(Default)]
struct State {
    ready: bool,
    downlink: bool,
    claimed: bool,
    closed: bool,
}
pub const SILENT_START_MILLIS: u64 = 500;
impl MicrophoneStart {
    pub fn after_setup_response(action: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
                action,
            }),
        }
    }
    fn claim(&self, downlink: bool) {
        let run = {
            let mut s = self.inner.state.lock().unwrap();
            s.downlink |= downlink;
            if s.closed || s.claimed || !s.ready {
                false
            } else {
                s.claimed = true;
                true
            }
        };
        self.inner.changed.notify_all();
        if run {
            (self.inner.action)();
        }
    }
    pub fn on_setup_response_sent(&self) {
        let downlink = {
            let mut s = self.inner.state.lock().unwrap();
            s.ready = true;
            s.downlink
        };
        if downlink {
            self.claim(true);
        }
        self.inner.changed.notify_all();
    }
    pub fn close(&self) {
        self.inner.state.lock().unwrap().closed = true;
        self.inner.changed.notify_all();
    }
    pub fn on_downlink(&self) {
        self.claim(true);
    }
    pub fn schedule_silent_start(
        &self,
        delay: Duration,
        while_current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<()> {
        let gate = self.clone();
        std::thread::Builder::new()
            .name("carplay-mic-start".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                let s = gate.inner.state.lock().unwrap();
                let (s, _) = gate
                    .inner
                    .changed
                    .wait_timeout_while(s, delay, |s| !s.closed && !s.claimed)
                    .unwrap();
                if s.closed || s.claimed {
                    return;
                }
                drop(s);
                if while_current() {
                    gate.claim(false);
                }
            })?;
        Ok(())
    }
    pub fn starts_without_downlink(audio_type: &str) -> bool {
        audio_type != "telephony"
    }
}
