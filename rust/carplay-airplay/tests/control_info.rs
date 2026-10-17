// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
use carplay_airplay::*;
use BplistValue as V;
fn config() -> AirPlayConfig {
    AirPlayConfig::linux_receiver("02:00:00:00:00:02".into(), "02:00:00:00:00:02".into())
}
fn get<'a>(v: &'a V, k: &str) -> &'a V {
    v.get(k).unwrap()
}
fn uint(v: &V, k: &str) -> u64 {
    get(v, k).as_u64().unwrap()
}
fn display(c: &AirPlayConfig) -> V {
    get(&AirPlayInfoPlist::build(c).unwrap(), "displays")
        .as_array()
        .unwrap()[0]
        .clone()
}
fn view(c: &AirPlayConfig) -> V {
    get(&display(c), "viewAreas").as_array().unwrap()[0].clone()
}
fn format(c: &AirPlayConfig, kind: u64, audio: &str) -> V {
    get(&AirPlayInfoPlist::build(c).unwrap(), "audioFormats")
        .as_array()
        .unwrap()
        .iter()
        .find(|v| {
            v.get("type").and_then(V::as_u64) == Some(kind)
                && v.get("audioType").and_then(V::as_str) == Some(audio)
        })
        .unwrap()
        .clone()
}
#[test]
fn linux_defaults_preserve_verified_interoperability() {
    let c = config();
    assert_eq!(c.source_version, "950.7.1");
    assert!(c.microphone);
    assert_eq!(AirPlayInfoPlist::features(&c), 0x615653aee2);
}
#[test]
fn main_display_declares_automatic_appearance() {
    let d = display(&config());
    for k in [
        "uiAppearanceMode",
        "uiAppearanceSetting",
        "mapAppearanceMode",
        "mapAppearanceSetting",
    ] {
        assert_eq!(uint(&d, k), 0);
    }
    assert_eq!(
        get(&d, "uuid").as_str(),
        Some("b7e6c5a0-1111-4000-8000-000000000001")
    );
}
#[test]
fn default_display_full_view_and_safe_area() {
    let c = config();
    let d = display(&c);
    let v = view(&c);
    let safe = get(&v, "safeArea");
    assert_eq!(
        (
            uint(&v, "widthPixels"),
            uint(&v, "heightPixels"),
            uint(&v, "originXPixels"),
            uint(&v, "originYPixels")
        ),
        (1280, 720, 0, 0)
    );
    assert_eq!(
        (uint(safe, "widthPixels"), uint(safe, "heightPixels")),
        (1280, 720)
    );
    assert_eq!(get(safe, "drawUIOutsideSafeArea"), &V::Boolean(true));
    assert_eq!(uint(&d, "primaryInputDevice"), 1);
    assert_eq!(uint(&d, "initialViewArea"), 0);
    assert!(d.get("cornerMasks").is_none());
}
#[test]
fn hevc_advertised_only_when_enabled() {
    let mut c = config();
    assert!(AirPlayInfoPlist::build(&c)
        .unwrap()
        .get("hevcInfo")
        .is_none());
    c.hevc = true;
    assert!(AirPlayInfoPlist::build(&c)
        .unwrap()
        .get("hevcInfo")
        .is_some());
}
#[test]
fn driving_side_and_display_values_advertised() {
    let mut c = config();
    c.right_hand_drive = true;
    c.manufacturer = "Example".into();
    c.model = "HeadUnit".into();
    c.main.fps = 37;
    c.main.width_physical_mm = Some(125);
    let info = AirPlayInfoPlist::build(&c).unwrap();
    assert_eq!(get(&info, "rightHandDrive"), &V::Boolean(true));
    assert_eq!(get(&info, "manufacturer").as_str(), Some("Example"));
    assert_eq!(get(&info, "model").as_str(), Some("HeadUnit"));
    let d = display(&c);
    assert_eq!(
        (
            uint(&d, "maxFPS"),
            uint(&d, "widthPhysical"),
            uint(&d, "heightPhysical")
        ),
        (35, 125, 70)
    );
}
#[test]
fn square_oem_icon_preserves_original_bytes() {
    let mut c = config();
    c.icons = vec![AirPlayIcon {
        width_pixels: 1,
        height_pixels: 1,
        data: vec![1, 2, 3, 4],
    }];
    c.oem_label = "Example".into();
    let info = AirPlayInfoPlist::build(&c).unwrap();
    assert_eq!(get(&info, "oemIconVisible"), &V::Boolean(true));
    assert_eq!(get(&info, "oemIconLabel").as_str(), Some("Example"));
    let icons = get(&info, "oemIcons").as_array().unwrap();
    assert_eq!(icons.len(), 1);
    let icon = &icons[0];
    assert_eq!(
        get(icon, "imageData").as_data(),
        Some([1, 2, 3, 4].as_slice())
    );
    assert_eq!(
        (uint(icon, "widthPixels"), uint(icon, "heightPixels")),
        (1, 1)
    );
    assert_eq!(get(icon, "prerendered"), &V::Boolean(true));
}
#[test]
fn no_icon_omits_the_car_button() {
    let c = config();
    assert!(c.icons.is_empty());
    let info = AirPlayInfoPlist::build(&c).unwrap();
    for key in ["oemIconVisible", "oemIconLabel", "oemIcons"] {
        assert!(info.get(key).is_none(), "{key}");
    }
}
#[test]
fn safe_area_insets_reach_view_area() {
    let mut c = config();
    c.main = AirPlayDisplayConfig::new(960, 540);
    c.main.safe_area = Some(AirPlayInsets {
        top: 10,
        bottom: 15,
        left: 20,
        right: 25,
    });
    c.main.safe_area_draw_outside = Some(false);
    let v = view(&c);
    let s = get(&v, "safeArea");
    assert_eq!(
        (
            uint(s, "widthPixels"),
            uint(s, "heightPixels"),
            uint(s, "originXPixels"),
            uint(s, "originYPixels")
        ),
        (915, 515, 20, 10)
    );
    assert_eq!(get(s, "drawUIOutsideSafeArea"), &V::Boolean(false));
}
#[test]
fn already_mapped_even_aligned_safe_area_is_preserved() {
    let mut c = config();
    c.main = AirPlayDisplayConfig::new(1920, 1080);
    c.main.safe_area = Some(AirPlayInsets {
        bottom: 104,
        ..Default::default()
    });
    assert_eq!(uint(get(&view(&c), "safeArea"), "heightPixels"), 976);
    c.main = AirPlayDisplayConfig::new(768, 800);
    c.main.safe_area = Some(AirPlayInsets {
        top: 75,
        bottom: 75,
        left: 34,
        right: 34,
    });
    let v = view(&c);
    let safe = get(&v, "safeArea");
    assert_eq!(
        (uint(safe, "widthPixels"), uint(safe, "heightPixels")),
        (700, 650)
    );
}
#[test]
fn microphone_inputs_only_when_enabled() {
    let mut c = config();
    c.microphone = false;
    assert!(format(&c, 100, "telephony")
        .get("audioInputFormats")
        .is_none());
    c.microphone = true;
    for audio in ["telephony", "default", "speechRecognition"] {
        assert_eq!(
            uint(&format(&c, 100, audio), "audioInputFormats"),
            0x70004154
        );
    }
    c.microphone_opus = false;
    for audio in ["telephony", "default", "speechRecognition"] {
        assert_eq!(uint(&format(&c, 100, audio), "audioInputFormats"), 0x4154);
    }
    assert_eq!(
        uint(&format(&c, 100, "telephony"), "audioOutputFormats"),
        0x70004154
    );
}
#[test]
fn output_opus_only_when_enabled() {
    let mut c = config();
    for (kind, audio) in [
        (100, "default"),
        (100, "alert"),
        (100, "telephony"),
        (100, "speechRecognition"),
        (101, "default"),
    ] {
        assert_eq!(
            uint(&format(&c, kind, audio), "audioOutputFormats") & 0x70000000,
            0x70000000
        );
    }
    let media = uint(&format(&c, 102, "media"), "audioOutputFormats");
    c.output_opus = false;
    for (kind, audio) in [
        (100, "default"),
        (100, "alert"),
        (100, "telephony"),
        (100, "speechRecognition"),
        (101, "default"),
    ] {
        let formats = uint(&format(&c, kind, audio), "audioOutputFormats");
        assert_eq!(formats & 0x70000000, 0);
        assert_ne!(formats, 0);
    }
    assert_eq!(uint(&format(&c, 102, "media"), "audioOutputFormats"), media);
}
#[test]
fn main_alt_and_high_audio_declared() {
    let formats = AirPlayInfoPlist::build(&config()).unwrap();
    let types: std::collections::BTreeSet<_> = get(&formats, "audioFormats")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| uint(v, "type"))
        .collect();
    assert_eq!(types, [100, 101, 102].into());
}
#[test]
fn without_areas_whole_screen_has_no_edge() {
    let c = config();
    assert!(view(&c).get("viewAreaStatusBarEdge").is_none());
    assert!(display(&c).get("viewAreaTransitionControl").is_none());
}
fn split_config() -> AirPlayConfig {
    let mut c = config();
    c.main = AirPlayDisplayConfig::new(2560, 1440);
    c.main.view_areas = Some(vec![
        AirPlayViewArea {
            width: 2560,
            height: 1440,
            origin_x: 0,
            origin_y: 0,
            dock_edge: Some(2),
        },
        AirPlayViewArea {
            width: 1270,
            height: 1208,
            origin_x: 0,
            origin_y: 0,
            dock_edge: Some(1),
        },
    ]);
    c.main.initial_view_area = 1;
    c
}
#[test]
fn areas_rectangle_dock_edge_and_clipped_safe_area() {
    let mut c = split_config();
    c.main.safe_area = Some(AirPlayInsets {
        top: 10,
        left: 20,
        ..Default::default()
    });
    let d = display(&c);
    let areas = get(&d, "viewAreas").as_array().unwrap();
    assert_eq!(uint(&areas[0], "viewAreaStatusBarEdge"), 2);
    assert_eq!(uint(&areas[1], "viewAreaStatusBarEdge"), 1);
    let safe = get(&areas[1], "safeArea");
    assert_eq!(
        (
            uint(safe, "widthPixels"),
            uint(safe, "heightPixels"),
            uint(safe, "originXPixels"),
            uint(safe, "originYPixels")
        ),
        (1250, 1198, 20, 10)
    );
    assert_eq!(uint(&d, "initialViewArea"), 1);
    assert_eq!(get(&d, "viewAreaTransitionControl"), &V::Boolean(true));
}
#[test]
fn view_area_command_has_animation_and_adjacent_areas() {
    let cmd = AirPlayInfoPlist::view_area_command(1, 2);
    assert_eq!(get(&cmd, "type").as_str(), Some("updateViewArea"));
    let p = get(&cmd, "params");
    assert_eq!(uint(p, "animationDurationMillis"), 300);
    assert_eq!(get(p, "adjacentViewAreas"), &V::Array(vec![0u64.into()]));
    let cmd = AirPlayInfoPlist::view_area_command(2, 4);
    assert_eq!(
        get(get(&cmd, "params"), "adjacentViewAreas"),
        &V::Array(vec![0u64.into(), 1u64.into(), 3u64.into()])
    );
}
#[test]
fn saved_mapping_outside_split_area_uses_bounds() {
    for (left, top) in [(1280, 0), (0, 1300), (1270, 0)] {
        let mut c = split_config();
        c.main.safe_area = Some(AirPlayInsets {
            left,
            top,
            ..Default::default()
        });
        let d = display(&c);
        let a = get(&d, "viewAreas").as_array().unwrap();
        assert_eq!(uint(get(&a[0], "safeArea"), "originXPixels"), left as u64);
        let safe = get(&a[1], "safeArea");
        assert_eq!(
            (
                uint(safe, "widthPixels"),
                uint(safe, "heightPixels"),
                uint(safe, "originXPixels"),
                uint(safe, "originYPixels")
            ),
            (1270, 1208, 0, 0)
        );
    }
}
#[test]
fn corner_masks_only_when_configured() {
    let mut c = config();
    for value in [Some(true), Some(false), None] {
        c.main.main_display_corner_masks = value;
        assert_eq!(
            display(&c).get("cornerMasks"),
            value.map(V::Boolean).as_ref()
        );
    }
}
#[test]
fn disabling_safe_area_omits_its_dictionary() {
    let mut c = config();
    c.main.main_display_corner_masks = Some(true);
    c.main.main_display_disable_safe_area = true;
    assert!(view(&c).get("safeArea").is_none());
}
#[test]
fn speech_removes_only_8_and_16khz() {
    let mut c = config();
    let base = c.clone();
    c.speech_without8_and16_khz = true;
    for (kind, audio) in [(100, "default"), (100, "alert"), (101, "default")] {
        let original = uint(&format(&base, kind, audio), "audioOutputFormats");
        let filtered = uint(&format(&c, kind, audio), "audioOutputFormats");
        assert_eq!(original & 0x3c, 0x3c);
        assert_eq!(filtered, original & !0x3c);
    }
    for audio in ["telephony", "media"] {
        assert_eq!(format(&base, 100, audio), format(&c, 100, audio));
    }
}
#[test]
fn buffered_offer_and_audio_disable_consistent() {
    let mut c = config();
    c.main_buffered_audio = true;
    let info = AirPlayInfoPlist::build(&c).unwrap();
    assert!(info.get("mainBufferedInfo").is_some());
    assert_eq!(
        uint(&format(&c, 103, "media"), "audioOutputFormats"),
        0x800000
    );
    c.disable_audio_output = true;
    let info = AirPlayInfoPlist::build(&c).unwrap();
    assert!(info.get("audioFormats").is_none());
    assert!(info.get("mainBufferedInfo").is_none());
    assert_eq!(
        AirPlayInfoPlist::features(&c),
        0x615653aee2 & !0x10004540a00
    );
}
#[test]
fn maximum_width_scales_physical_size() {
    let s = AirPlayDisplaySettings::resolve_physical_size_mm_with_maximum(
        768,
        800,
        1920,
        1080,
        200,
        AirPlayPhysicalSizeBasis::Width,
    )
    .unwrap();
    assert_eq!((s.width_mm, s.height_mm), (80, 83));
}
#[test]
fn maximum_height_scales_physical_size() {
    let s = AirPlayDisplaySettings::resolve_physical_size_mm_with_maximum(
        1920,
        978,
        1920,
        1080,
        200,
        AirPlayPhysicalSizeBasis::Height,
    )
    .unwrap();
    assert_eq!((s.width_mm, s.height_mm), (356, 181));
}
#[test]
fn maximum_resolution_keeps_reference() {
    let s = AirPlayDisplaySettings::resolve_physical_size_mm_with_maximum(
        1920,
        1080,
        1920,
        1080,
        250,
        AirPlayPhysicalSizeBasis::Width,
    )
    .unwrap();
    assert_eq!((s.width_mm, s.height_mm), (250, 141));
}
#[test]
fn display_sanitizers_use_stepped_ranges() {
    assert_eq!(AirPlayDisplaySettings::sanitize_fps(37), 35);
    assert_eq!(AirPlayDisplaySettings::sanitize_fps(0), 30);
    assert_eq!(AirPlayDisplaySettings::sanitize_fps(100), 60);
    assert_eq!(AirPlayDisplaySettings::sanitize_width_physical_mm(138), 150);
    assert_eq!(AirPlayDisplaySettings::sanitize_reported_physical_mm(0), 1);
    assert!(
        AirPlayDisplaySettings::resolve_physical_size_mm_with_maximum(
            0,
            1,
            1,
            1,
            200,
            AirPlayPhysicalSizeBasis::Width
        )
        .is_err()
    );
}
