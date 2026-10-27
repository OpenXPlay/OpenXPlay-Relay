// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use carplay_iap2::{
    mfi::MfiAuthenticator, DuplexStream, Iap2Session, Iap2WirelessCarPlayEndpoint,
    Iap2WirelessSecurity, Receive,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Platform interface for `SessionControl`. Implementations must support bounded shutdown.
pub trait SessionControl: Send + Sync {
    /// Return the opaque identity of the current main stream, or None when playback proof is unavailable.
    fn playback_token(&self) -> Option<PlaybackToken> {
        None
    }
    /// Confirm a frame displayed by an external receiver; reject tokens from replaced streams or sessions.
    fn report_video_frame_rendered(&self, _token: &PlaybackToken) -> bool {
        false
    }
    /// Send one media HID press and release (3 = play/pause, 4 = next, 5 = previous); false when unavailable.
    fn send_media_button(&self, _index: u8) -> Result<bool> {
        Ok(false)
    }
    fn show_view_area(&self, _index: usize) -> bool {
        false
    }
    /// Errors propagate to the lifecycle owner.
    fn send_touch(&self, contacts: &[AirPlayContact]) -> Result<bool>;
}
/// Platform interface for `MediaSinkDecorator`. Implementations must support bounded shutdown.
pub trait MediaSinkDecorator: Send + Sync {
    /// Errors propagate to the lifecycle owner.
    fn decorate(
        &self,
        sink: Arc<dyn MediaSink>,
        control: Arc<dyn SessionControl>,
    ) -> Arc<dyn MediaSink>;
}
