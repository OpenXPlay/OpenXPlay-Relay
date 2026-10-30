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

/// `AirPlayInsets` protocol data.
#[derive(Clone, Debug, Default)]
pub struct AirPlayInsets {
    pub top: u32,

    pub bottom: u32,

    pub left: u32,

    pub right: u32,
}
/// `AirPlayDisplayConfig` protocol data.
#[derive(Clone, Debug)]
pub struct AirPlayDisplayConfig {
    pub width_pixels: u32,

    pub height_pixels: u32,

    pub fps: u32,
    /// Primary HID device selector.
    pub primary_input_device: u32,
    /// Optional display feature flags.
    pub features: Option<u32>,
    /// Optional view regions advertised to the phone.
    pub view_areas: Option<Vec<AirPlayViewArea>>,
    /// Initially selected view region.
    pub initial_view_area: u32,
    /// Main-display corner mask behavior.
    pub main_display_corner_masks: Option<bool>,
    /// Disable main display safe area when explicitly requested.
    pub main_display_disable_safe_area: bool,
    /// Whether the client may draw beyond safe area.
    pub safe_area_draw_outside: Option<bool>,
    /// Initial URL when configured.
    pub initial_url: Option<String>,

    pub width_physical_mm: Option<u32>,

    pub height_physical_mm: Option<u32>,

    pub safe_area: Option<AirPlayInsets>,

    pub view_area: Option<AirPlayInsets>,
}
/// `AirPlayConfig` protocol data.
#[derive(Clone, Debug)]
pub struct AirPlayConfig {
    pub device_name: String,

    pub device_id: String,

    pub bt_mac: String,

    pub source_version: String,

    pub main: AirPlayDisplayConfig,

    pub port: u16,

    pub entertainment_sample_rate: u32,

    pub hevc: bool,

    /// Enable the optional iAP2 channel only when the peer offers it.
    pub iap_channel_only_when_offered: bool,

    pub microphone: bool,

    pub microphone_opus: bool,
    /// Offer Opus for speech, alert and telephony output; only receivers that can decode Opus may keep it.
    pub output_opus: bool,

    pub manufacturer: String,

    pub model: String,
    /// Accessory icons advertised in /info.
    pub icons: Vec<AirPlayIcon>,

    pub oem_label: String,

    pub main_buffered_audio: bool,
    /// Driver orientation in /info.
    pub right_hand_drive: bool,
    /// Disable output audio only when explicitly requested.
    pub disable_audio_output: bool,
    /// Advertise speech formats without 8/16 kHz when requested.
    pub speech_without8_and16_khz: bool,
}
impl AirPlayConfig {
    pub fn linux_receiver(device_id: String, bt_mac: String) -> Self {
        Self {
            device_name: "OpenXPlay Relay".into(),
            device_id,
            bt_mac,
            source_version: "950.7.1".into(),
            main: AirPlayDisplayConfig::new(1280, 720),
            port: 7000,
            entertainment_sample_rate: 48000,
            hevc: false,
            iap_channel_only_when_offered: false,
            microphone: true,
            microphone_opus: true,
            output_opus: true,
            manufacturer: "OpenXPlay".into(),
            model: "OpenXPlay Relay".into(),
            icons: Vec::new(),
            oem_label: "OpenXPlay".into(),
            main_buffered_audio: false,
            right_hand_drive: false,
            disable_audio_output: false,
            speech_without8_and16_khz: false,
        }
    }
}

/// Main display view region.
#[derive(Clone, Debug)]
pub struct AirPlayViewArea {
    /// Width in display coordinates.
    pub width: u32,
    /// Height in display coordinates.
    pub height: u32,
    /// X origin.
    pub origin_x: u32,
    /// Y origin.
    pub origin_y: u32,
    /// Optional dock edge.
    pub dock_edge: Option<u32>,
}
/// Encoded accessory icon, passed through in /info.
#[derive(Clone, Debug)]
pub struct AirPlayIcon {
    /// Pixel width.
    pub width_pixels: u32,
    /// Pixel height.
    pub height_pixels: u32,
    /// Encoded image data.
    pub data: Vec<u8>,
}

impl AirPlayDisplayConfig {
    pub fn new(width_pixels: u32, height_pixels: u32) -> Self {
        Self {
            width_pixels,
            height_pixels,
            fps: 60,
            primary_input_device: 1,
            features: None,
            view_areas: None,
            initial_view_area: 0,
            main_display_corner_masks: None,
            main_display_disable_safe_area: false,
            safe_area_draw_outside: None,
            initial_url: None,
            width_physical_mm: None,
            height_physical_mm: None,
            safe_area: None,
            view_area: None,
        }
    }
}
impl AirPlayConfig {
    pub fn buffered_audio_output_enabled(&self) -> bool {
        self.main_buffered_audio && !self.disable_audio_output
    }
}
