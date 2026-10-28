// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-28.

//! Remember the enumeration on which the caller last sent StartSession.
use std::sync::Mutex;
#[derive(Default)]
pub struct WiredSessionMemory {
    started_on: Mutex<Option<String>>,
}
impl WiredSessionMemory {
    pub fn started(&self, node: impl Into<String>) {
        *self.started_on.lock().unwrap() = Some(node.into());
    }
    pub fn started_on(&self, node: &str) -> bool {
        self.started_on.lock().unwrap().as_deref() == Some(node)
    }
    pub fn clear(&self) {
        *self.started_on.lock().unwrap() = None;
    }
}
