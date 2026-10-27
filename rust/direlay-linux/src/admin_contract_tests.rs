// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

fn contract_fixture(name: &str, value: &serde_json::Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../admin-web/tests/fixtures/linux")
        .join(format!("{name}.json"));
    let expected = format!("{}\n", serde_json::to_string_pretty(value).unwrap());
    if std::env::var("UPDATE_ADMIN_FIXTURES").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
        // The sidecar's header, not this file's own (REUSE must not read it).
        // REUSE-IgnoreStart
        std::fs::write(
            path.with_extension("json.license"),
            "SPDX-FileCopyrightText: 2026 OpenXPlay.org\nSPDX-License-Identifier: AGPL-3.0-only\n",
        )
        .unwrap();
        // REUSE-IgnoreEnd
    } else {
        let actual = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(actual, expected, "{} drifted. Review the diff after running UPDATE_ADMIN_FIXTURES=1 cargo test -p openxplay-relay --lib --offline admin_contract", path.display());
    }
}

#[test]
fn admin_contract_settings() {
    let (admin, dir) = fake();
    let mut settings = admin.settings();
    // Host adapter addresses are environment-specific, never part of the schema contract.
    settings
        .values
        .insert("experimentalBluetoothAddress".into(), "".into());
    contract_fixture("settings", &serde_json::to_value(settings).unwrap());
    {
        let mut s = admin.settings.lock().unwrap();
        s.output = "both".into();
        s.car_brand = "xpeng".into();
        s.car_model = "g6".into();
        s.car_year = "g6-2023".into();
    }
    let mut settings = admin.settings();
    settings
        .values
        .insert("experimentalBluetoothAddress".into(), "".into());
    contract_fixture("settings-carlink", &serde_json::to_value(settings).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}

fn admin_contract_status(state: &str) {
    use crate::display::Size;
    let (admin, dir) = fake();
    let now = 1_793_000_000_000;
    let mut settings = admin.settings.lock().unwrap().clone();
    let mut status = admin.status();
    // Pin host observations and clocks; every serialized field comes from the production response.
    status.host_addresses = vec![AdminHostAddress::new("wlan0".into(), "10.42.0.1".into())];
    status.host_profile.as_mut().unwrap().profile_path = "/etc/openxplay/host.json".into();
    status.host_profile.as_mut().unwrap().wifi_present = true;
    status.host_profile.as_mut().unwrap().bluetooth_adapter = None;
    status.host_capabilities = None;
    status.bluetooth_guard = None;
    status.network.planned = None;
    status.wireless = None;
    status.hotspot = Some(AdminHotspot {
        ssid: "OpenXPlay-1A2B".into(),
        password: "openxplay123".into(),
        has_password: true,
        ip: Some("10.42.0.1".into()),
        active: true,
    });
    status.wifi_client = Some(WifiClient::default());
    status.receiver_urls = vec!["https://10.42.0.1:8443/".into()];
    status.admin_urls = vec!["https://10.42.0.1:6975/".into()];
    status.iphone = AdminIphone {
        name: Some("iPhone".into()),
        reason: "Selected".into(),
    };
    status.running = false;
    status.auto_start_paused = true;
    let mut facts = crate::path_facts::RuntimePathFacts::default();
    facts.web.phase = "LISTENING".into();
    facts.web.port = 8443;
    facts.web.since_millis = Some(now - 10_000);
    if state != "idle" {
        facts.pages = 1;
        facts.screen = Some(Size {
            width: 1920,
            height: 720,
        });
        facts.saved_screen = facts.screen;
    } else {
        status.iphone = AdminIphone {
            name: None,
            reason: "NoIphone".into(),
        };
    }
    match state {
        "pairing" => {
            status.pairing_remaining = Some(600);
            status.pairing_deadline = Some(now / 1000 + 600);
            status.iphone.reason = "NoIphone".into();
            status.iphone.name = None;
        }
        "carlink-scanning" | "carlink-pin" | "live-carlink" => {
            settings.output = "carlink".into();
            facts.web.phase = "OFF".into();
            facts.pages = 0;
            let event = match state {
                "carlink-scanning" => serde_json::json!({"type":"link","state":"scanning"}),
                "carlink-pin" => serde_json::json!({"type":"pin-request"}),
                _ => serde_json::json!({"type":"state","state":"playing","headUnit":{"name":"G6"}}),
            };
            facts.carlink_event(&event, now - 1_000);
            let mut clink = AdminCarlink::default();
            clink.apply_event(&event, 100);
            status.carlink = Some(clink.at(100, now / 1000));
            if state == "live-carlink" {
                facts.carlink_event(&serde_json::json!({"type":"media-stats","liveFrames":10,"width":1920,"height":720,"codec":"H265"}), now);
            }
        }
        "iphone-failure" => {
            status.running = true;
            facts.observe(
                "iap2-stage",
                &serde_json::json!({"stage":"ConnectingBluetooth"}),
                now - 3_000,
            );
            facts.failure("rfcomm timeout", true, now - 1_000);
        }
        "car-first-wait" => {
            facts.saved_screen = None;
            facts.screen = None;
            facts.pages = 0;
            facts.iphone.car_wait_until_millis = Some(now + 20_000);
        }
        "shared-wifi" => {
            settings.network_mode = "shared-wifi".into();
            status.network.mode = settings.network_mode.clone();
            status.network.wifi = Some("Car Wi-Fi".into());
            status.wifi_client = Some(WifiClient {
                ssid: Some("Car Wi-Fi".into()),
                ip: Some("192.168.1.2".into()),
                ..Default::default()
            });
            status.hotspot = None;
        }
        _ => {}
    }
    if state.starts_with("live-") {
        status.running = true;
        facts.iphone.step = "VIDEO".into();
        facts.iphone.session_since_millis = Some(now - 5_000);
        facts.picture_on_car = true;
        facts.chips = vec!["1920×720".into(), "60 fps".into(), "H.265".into()];
    }
    status.output = settings.output.clone();
    crate::path_input::update(&facts, &settings, &mut status, now, true, true);
    let p = &status.path_input;
    assert_eq!(p["platform"], "linux");
    assert_eq!(p["relay"]["keepAlive"], "NOT_NEEDED");
    assert_eq!(p["iphone"]["usb"], "UNKNOWN");
    assert_eq!(p["live"]["pictureOnCar"], state.starts_with("live-"));
    match state {
        "idle" | "pairing" => assert_eq!(p["iphone"]["pick"], "NONE"),
        "carlink-scanning" => assert_eq!(p["car"]["carlink"]["phase"], "SCANNING"),
        "carlink-pin" => assert_eq!(p["car"]["carlink"]["pinPending"], true),
        "iphone-failure" => assert_eq!(p["iphone"]["failure"]["retrying"], true),
        "car-first-wait" => assert_eq!(p["iphone"]["started"], true),
        "shared-wifi" => assert_eq!(p["setup"]["network"], "SHARED_WIFI"),
        _ => assert_eq!(p["iphone"]["pick"], "PICKED"),
    }
    contract_fixture(
        &format!("status-{state}"),
        &serde_json::to_value(status).unwrap(),
    );
    std::fs::remove_dir_all(dir).unwrap();
}
macro_rules! contract_states {
    ($($name:ident => $state:literal),* $(,)?) => { $(#[test] fn $name() { admin_contract_status($state); })* };
}
contract_states! {
    admin_contract_idle => "idle", admin_contract_ready => "ready", admin_contract_pairing => "pairing",
    admin_contract_scanning => "carlink-scanning", admin_contract_pin => "carlink-pin",
    admin_contract_failure => "iphone-failure", admin_contract_wait => "car-first-wait",
    admin_contract_live_browser => "live-browser", admin_contract_live_carlink => "live-carlink",
    admin_contract_shared_wifi => "shared-wifi",
}

#[test]
fn admin_contract_trial_restarts_once_and_never_saves() {
    let (admin, dir) = fake();
    admin.settings.lock().unwrap().output = "carlink".into();
    let before = std::fs::read(dir.join("settings.json")).unwrap();
    let changes = serde_json::json!({"carlinkPictureTry":{"width":1440,"height":900}});
    assert!(matches!(
        admin.update(changes.as_object().unwrap().clone()),
        AdminUpdate::AppliedNow
    ));
    let since = admin.input.display.lock().unwrap().settings_since;
    assert!(since.is_some());
    assert!(matches!(
        admin.update(changes.as_object().unwrap().clone()),
        AdminUpdate::AppliedNow
    ));
    assert_eq!(admin.input.display.lock().unwrap().settings_since, since);
    assert_eq!(std::fs::read(dir.join("settings.json")).unwrap(), before);
    assert_eq!(
        admin.settings().values["carlinkPictureTry"],
        changes["carlinkPictureTry"]
    );
    admin.input.clear_picture_trial();
    assert!(admin.settings().values["carlinkPictureTry"].is_null());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn admin_contract_timeline_export_uses_runtime_milestones_and_carlink_summary() {
    let (admin, dir) = fake();
    {
        let mut facts = admin.events.facts.lock().unwrap();
        facts.observe(
            "airplay-milestone",
            &serde_json::json!({"name":"first_video_received"}),
            1234,
        );
        facts.carlink_summary = Some("Carlink summary: codec=H265 liveFrames=10".into());
    }
    let timeline = admin.recent_connection_timeline_for_log_export();
    assert!(timeline.contains("1234 phone first_video_received"));
    assert!(timeline.contains("Carlink summary: codec=H265 liveFrames=10"));
    std::fs::remove_dir_all(dir).unwrap();
}
