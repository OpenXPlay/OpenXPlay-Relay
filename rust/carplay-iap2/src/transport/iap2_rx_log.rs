// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

/// State owned by `Iap2RxLog`; construct through its factory.
pub struct Iap2RxLog {
    counts: std::collections::HashMap<u16, u64>,
    first: u64,
    every: u64,
}
impl Iap2RxLog {
    pub fn line(&mut self, message_id: u16) -> Option<String> {
        if message_id != 0x5001 && message_id != 0x5201 {
            return Some(format!("iap2 rx=0x{message_id:04x}"));
        }
        let count = self.counts.entry(message_id).or_default();
        *count += 1;
        if *count <= self.first {
            Some(format!("iap2 rx=0x{message_id:04x}"))
        } else if (*count).is_multiple_of(self.every) {
            Some(format!("iap2 rx=0x{message_id:04x} count={count}"))
        } else {
            None
        }
    }
}

impl Default for Iap2RxLog {
    fn default() -> Self {
        Self::new(5, 100)
    }
}

impl Iap2RxLog {
    pub fn new(first: u64, every: u64) -> Self {
        assert!(every > 0);
        Self {
            counts: Default::default(),
            first,
            every,
        }
    }
}
