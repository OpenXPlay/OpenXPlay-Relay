// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;
use BplistValue as V;
pub struct AirPlayInfoPlist;
fn d<const N: usize>(items: [(&str, V); N]) -> V {
    V::dict(items)
}
fn push(value: &mut V, key: &str, v: V) {
    if let V::Dictionary(entries) = value {
        entries.push((key.into(), v));
    }
}
fn resource(id: u64) -> V {
    d([
        ("resourceID", id.into()),
        ("transferType", 1u64.into()),
        ("transferPriority", 100u64.into()),
        ("takeConstraint", 100u64.into()),
        ("borrowConstraint", 100u64.into()),
        ("unborrowConstraint", 100u64.into()),
    ])
}
impl AirPlayInfoPlist {
    pub fn features(config: &AirPlayConfig) -> u64 {
        if config.disable_audio_output {
            CARPLAY_FEATURES_NO_AUDIO
        } else {
            CARPLAY_FEATURES
        }
    }
    pub fn build(config: &AirPlayConfig) -> Result<V> {
        let modes = d([
            ("resources", V::Array(vec![resource(1), resource(2)])),
            (
                "appStates",
                V::Array(vec![
                    d([("appStateID", 2u64.into()), ("state", false.into())]),
                    d([("appStateID", 1u64.into()), ("speechMode", V::Real(-1.0))]),
                    d([("appStateID", 3u64.into()), ("state", false.into())]),
                ]),
            ),
        ]);
        let mut info = d([
            ("sourceVersion", config.source_version.clone().into()),
            ("features", Self::features(config).into()),
            ("statusFlags", 4u64.into()),
            ("model", config.model.clone().into()),
            ("manufacturer", config.manufacturer.clone().into()),
            ("deviceID", config.device_id.clone().into()),
            ("bluetoothIDs", V::Array(vec![config.bt_mac.clone().into()])),
            ("name", config.device_name.clone().into()),
            ("rightHandDrive", config.right_hand_drive.into()),
            ("keepAliveLowPower", false.into()),
            ("keepAliveSendStatsAsBody", false.into()),
            ("modes", modes),
        ]);
        if !config.disable_audio_output {
            push(&mut info, "audioLatencies", audio_latencies());
            push(&mut info, "audioFormats", audio_formats(config));
        }
        push(
            &mut info,
            "extendedFeatures",
            V::Array(vec!["vocoderInfo".into(), "enhancedRequestCarUI".into()]),
        );
        push(
            &mut info,
            "displays",
            V::Array(vec![display_entry(&config.main)]),
        );
        push(
            &mut info,
            "hidDevices",
            V::Array(vec![
                AirPlayHid::touch_hid_device(
                    config.main.width_pixels,
                    config.main.height_pixels,
                    MAIN_UUID,
                ),
                AirPlayHid::knob_hid_device(MAIN_UUID),
                AirPlayHid::media_hid_device(MAIN_UUID),
                AirPlayHid::telephony_hid_device(MAIN_UUID),
            ]),
        );
        if !config.icons.is_empty() {
            push(&mut info, "oemIconVisible", true.into());
            push(&mut info, "oemIconLabel", config.oem_label.clone().into());
            push(
                &mut info,
                "oemIcons",
                V::Array(
                    config
                        .icons
                        .iter()
                        .map(|i| {
                            d([
                                ("imageData", V::Data(i.data.clone())),
                                ("widthPixels", i.width_pixels.into()),
                                ("heightPixels", i.height_pixels.into()),
                                ("prerendered", true.into()),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        if config.hevc {
            push(&mut info, "hevcInfo", V::Dictionary(Vec::new()));
        }
        if config.buffered_audio_output_enabled() {
            push(&mut info, "mainBufferedInfo", V::Dictionary(Vec::new()));
        }
        Ok(info)
    }
    pub fn view_area_command(index: u32, area_count: u32) -> V {
        d([
            ("type", "updateViewArea".into()),
            (
                "params",
                d([
                    ("uuid", MAIN_UUID.into()),
                    ("viewAreaIndex", index.into()),
                    ("animationDurationMillis", VIEW_AREA_ANIMATION_MILLIS.into()),
                    (
                        "adjacentViewAreas",
                        V::Array(
                            (0..area_count)
                                .filter(|i| *i != index)
                                .map(V::from)
                                .collect(),
                        ),
                    ),
                ]),
            ),
        ])
    }
}
fn audio_latencies() -> V {
    V::Array(
        [
            (100, None),
            (100, Some("default")),
            (100, Some("media")),
            (100, Some("telephony")),
            (100, Some("speechRecognition")),
            (100, Some("alert")),
            (101, None),
            (101, Some("default")),
            (102, Some("default")),
        ]
        .into_iter()
        .map(|(kind, audio)| {
            let mut entry = d([
                ("type", V::from(kind as u64)),
                ("inputLatencyMicros", 0u64.into()),
                ("outputLatencyMicros", 0u64.into()),
            ]);
            if let Some(audio) = audio {
                push(&mut entry, "audioType", audio.into());
            }
            entry
        })
        .collect(),
    )
}
fn audio_formats(c: &AirPlayConfig) -> V {
    let is48 = c.entertainment_sample_rate == 48000;
    let pcm = 0x3fc | if is48 { 0xc000u64 } else { 0xc00 };
    let speech = if c.speech_without8_and16_khz {
        pcm & !0x3c
    } else {
        pcm
    };
    let mono = 0x154 | if is48 { 0x4000u64 } else { 0x400 };
    let opus = 0x70000000u64;
    let aac = if is48 { 0x800000u64 } else { 0x400000 };
    let input = c.microphone.then_some(mono);
    let wireless = c
        .microphone
        .then_some(mono | if c.microphone_opus { opus } else { 0 });
    let output_opus = if c.output_opus { opus } else { 0 };
    let mut formats = vec![
        (100, "compatibility", pcm, input),
        (101, "compatibility", pcm, None),
        (100, "default", speech | output_opus, wireless),
        (100, "alert", speech | output_opus, None),
        (100, "media", pcm, None),
        (100, "telephony", mono | output_opus, wireless),
        (100, "speechRecognition", mono | output_opus, wireless),
        (101, "default", speech | output_opus, None),
        (102, "media", aac, None),
    ];
    if c.main_buffered_audio {
        formats.push((103, "media", aac, None));
    }
    V::Array(
        formats
            .into_iter()
            .map(|(kind, audio, output, input)| {
                let mut e = d([
                    ("type", V::from(kind as u64)),
                    ("audioType", audio.into()),
                    ("audioOutputFormats", output.into()),
                ]);
                if let Some(input) = input {
                    push(&mut e, "audioInputFormats", input.into());
                }
                e
            })
            .collect(),
    )
}
fn display_entry(c: &AirPlayDisplayConfig) -> V {
    let w =
        AirPlayDisplaySettings::sanitize_reported_physical_mm(c.width_physical_mm.unwrap_or(200));
    let h = AirPlayDisplaySettings::sanitize_reported_physical_mm(c.height_physical_mm.unwrap_or(
        (w as f64 * c.height_pixels as f64 / c.width_pixels.max(1) as f64).round() as u32,
    ));
    let mut entry = d([
        ("uuid", MAIN_UUID.into()),
        ("type", 110u64.into()),
        ("maxFPS", AirPlayDisplaySettings::sanitize_fps(c.fps).into()),
        ("widthPixels", c.width_pixels.into()),
        ("heightPixels", c.height_pixels.into()),
        ("widthPhysical", w.into()),
        ("heightPhysical", h.into()),
        ("features", c.features.unwrap_or(10).into()),
        ("primaryInputDevice", c.primary_input_device.into()),
        ("uiAppearanceMode", 0u64.into()),
        ("uiAppearanceSetting", 0u64.into()),
        ("mapAppearanceMode", 0u64.into()),
        ("mapAppearanceSetting", 0u64.into()),
    ]);
    let areas = c.view_areas.as_ref().filter(|a| !a.is_empty());
    let views = match areas {
        Some(a) => a.iter().map(|a| area_dict(c, Some(a))).collect(),
        None => vec![area_dict(c, None)],
    };
    push(&mut entry, "viewAreas", V::Array(views));
    push(
        &mut entry,
        "initialViewArea",
        areas
            .map_or(0, |a| c.initial_view_area.min(a.len() as u32 - 1))
            .into(),
    );
    if let Some(masks) = c.main_display_corner_masks {
        push(&mut entry, "cornerMasks", masks.into());
    }
    if areas.is_some_and(|a| a.len() > 1) {
        push(&mut entry, "viewAreaTransitionControl", true.into());
    }
    if let Some(url) = &c.initial_url {
        push(&mut entry, "initialURL", url.clone().into());
    }
    entry
}
fn rect(w: u32, h: u32, i: &AirPlayInsets) -> V {
    d([
        (
            "widthPixels",
            w.saturating_sub(i.left).saturating_sub(i.right).into(),
        ),
        (
            "heightPixels",
            h.saturating_sub(i.top).saturating_sub(i.bottom).into(),
        ),
        ("originXPixels", i.left.into()),
        ("originYPixels", i.top.into()),
    ])
}
fn area_dict(c: &AirPlayDisplayConfig, area: Option<&AirPlayViewArea>) -> V {
    let w = c.width_pixels;
    let h = c.height_pixels;
    let view = area
        .map(|a| AirPlayInsets {
            top: a.origin_y,
            bottom: h.saturating_sub(a.origin_y).saturating_sub(a.height),
            left: a.origin_x,
            right: w.saturating_sub(a.origin_x).saturating_sub(a.width),
        })
        .unwrap_or_else(|| c.view_area.clone().unwrap_or_default());
    let mut result = rect(w, h, &view);
    if let Some(edge) = area.and_then(|a| a.dock_edge) {
        push(&mut result, "viewAreaStatusBarEdge", edge.into());
    }
    if !c.main_display_disable_safe_area {
        let safe = c.safe_area.clone().unwrap_or_default();
        let clipped = if area.is_some() {
            AirPlayInsets {
                top: safe.top.max(view.top),
                bottom: safe.bottom.max(view.bottom),
                left: safe.left.max(view.left),
                right: safe.right.max(view.right),
            }
        } else {
            safe
        };
        let safe = if area.is_some()
            && (clipped.left.saturating_add(clipped.right) >= w
                || clipped.top.saturating_add(clipped.bottom) >= h)
        {
            view
        } else {
            clipped
        };
        let mut safe = rect(w, h, &safe);
        push(
            &mut safe,
            "drawUIOutsideSafeArea",
            c.safe_area_draw_outside.unwrap_or(true).into(),
        );
        push(&mut result, "safeArea", safe);
    }
    result
}
/// Protocol message, field, or framing constant.
pub const MAIN_UUID: &str = "b7e6c5a0-1111-4000-8000-000000000001";
/// Protocol message, field, or framing constant.
pub const ALT_UUID: &str = "b7e6c5a0-2222-4000-8000-000000000002";
/// Protocol message, field, or framing constant.
pub const STREAM_TYPE_MAIN_SCREEN: u64 = 110;
/// Protocol message, field, or framing constant.
pub const STREAM_TYPE_ALT_SCREEN: u64 = 111;
/// Protocol message, field, or framing constant.
pub const DISPLAY_FEATURE_KNOBS: u64 = 0x02;
/// Protocol message, field, or framing constant.
pub const DISPLAY_FEATURE_HIGH_FIDELITY_TOUCH: u64 = 0x08;
/// Protocol message, field, or framing constant.
pub const CARPLAY_FEATURES: u64 = 0x615653aee2;
/// Protocol message, field, or framing constant.
pub const CARPLAY_AUDIO_FEATURES: u64 = 0x10004540a00;
/// Protocol message, field, or framing constant.
pub const RESOURCE_SCREEN: u64 = 1;
/// Protocol message, field, or framing constant.
pub const RESOURCE_AUDIO: u64 = 2;
/// Protocol message, field, or framing constant.
pub const TRANSFER_TAKE: u64 = 1;
/// Protocol message, field, or framing constant.
pub const PRIORITY_NICE_TO_HAVE: u64 = 100;
/// Protocol message, field, or framing constant.
pub const CONSTRAINT_ANYTIME: u64 = 100;
/// Protocol message, field, or framing constant.
pub const DOCK_EDGE_BOTTOM: u64 = 1;
/// Protocol message, field, or framing constant.
pub const DOCK_EDGE_DRIVER_SIDE: u64 = 2;
/// Protocol message, field, or framing constant.
pub const VIEW_AREA_ANIMATION_MILLIS: u64 = 300;

pub const CARPLAY_FEATURES_NO_AUDIO: u64 = CARPLAY_FEATURES & !CARPLAY_AUDIO_FEATURES;
