// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

use crate::{
    supervisor::{BleCarlinkSettings, CarlinkSettings, SupervisorSettings},
    HotspotAddressPolicy, Options,
};
use std::net::Ipv4Addr;

/// Pure assembly: callers supply the current host observations.
pub fn supervisor_settings(
    o: &Options,
    mut addresses: Vec<Ipv4Addr>,
    occupied: &[String],
    pairing_id: String,
    ble_adapter: Option<String>,
    station_5ghz: bool,
) -> SupervisorSettings {
    addresses.push(Ipv4Addr::new(10, 78, 0, 1));
    if let Ok(a) = HotspotAddressPolicy::select_subnet(occupied) {
        addresses.push(a);
    }
    let carlink = if o.settings.output != "web" {
        Some(CarlinkSettings {
            name: o.settings.carlink_name.clone(),
            trace: o.settings.carlink_trace == "on",
            model: o.settings.carlink_model.clone(),
            // Linux has no H.265 encoder; both outputs share the iPhone stream.
            hevc: if o.settings.carlink_hevc == "auto"
                && o.settings.output == "carlink"
                && crate::display::DisplayState::load(&o.state_dir.join("display.json")).video_codec
                    != "h264"
            {
                "auto"
            } else {
                "off"
            }
            .into(),
            auto_reconnect: o.settings.carlink_auto_reconnect,
            ble: if o.settings.carlink_link == "ble" {
                Some(BleCarlinkSettings {
                    adapter: ble_adapter,
                    interface: o.carlink_interface().into(),
                    address: o.settings.carlink_address.clone(),
                    id: pairing_id,
                    band: station_5ghz as u8,
                    phone_mac: (o.settings.experimental_active
                        && o.settings.experimental_send_bluetooth_address)
                        .then(|| {
                            crate::carlink_policy::bluetooth_address(
                                &o.settings.experimental_bluetooth_address,
                            )
                        })
                        .flatten(),
                })
            } else {
                None
            },
            head_unit: o
                .settings
                .carlink_head_unit
                .parse()
                .unwrap_or(std::net::Ipv4Addr::UNSPECIFIED),
            local: o
                .settings
                .carlink_local
                .parse()
                .unwrap_or(std::net::Ipv4Addr::UNSPECIFIED),
            pin: o.settings.carlink_pin.clone(),
            ports: o.settings.carlink_ports.clone(),
        })
    } else {
        None
    };
    SupervisorSettings {
        binary: o.oxp_relay.clone(),
        socket: o.stream_socket.clone(),
        web_port: if o.settings.output == "carlink" {
            0
        } else {
            o.settings.receiver_port
        },
        plugins: o.settings.plugins.then(|| crate::plugins::OutputPlugins {
            root: o.plugins_root.clone(),
            enabled: o.settings.plugins_enabled.clone(),
        }),
        web_root: o.web_root.clone(),
        state_dir: o.state_dir.clone(),
        addresses,
        pairing: o.settings.require_pairing_code,
        carlink,
        pem_tls: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_flags_use_root_state_and_only_an_enabled_valid_bluetooth_override() {
        let root = std::env::temp_dir().join(format!(
            "supervisor-runtime-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let mut o =
            Options::parse(&["--state-dir".into(), root.to_string_lossy().into_owned()]).unwrap();
        o.settings.output = "both".into();
        for (active, send, address, expected) in [
            (true, true, "AA:BB:CC:DD:EE:FF", true),
            (false, true, "aa:bb:cc:dd:ee:ff", false),
            (true, false, "aa:bb:cc:dd:ee:ff", false),
            (true, true, "01:00:00:00:00:01", false),
        ] {
            o.settings.experimental_active = active;
            o.settings.experimental_send_bluetooth_address = send;
            o.settings.experimental_bluetooth_address = address.into();
            let args = supervisor_settings(&o, vec![], &[], String::new(), None, false).arguments();
            assert!(args.iter().any(|a| a == "--host-ui"));
            assert!(args
                .windows(2)
                .any(|a| a[0] == "--clink-state-dir" && a[1] == root.as_os_str()));
            assert!(args
                .windows(2)
                .any(|a| a[0] == "--clink-scan" && a[1] == "continuous"));
            assert_eq!(args.iter().any(|a| a == "--clink-phone-mac"), expected);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn hevc_argument_matrix_uses_current_picture_settings() {
        let root = std::env::temp_dir().join(format!(
            "supervisor-hevc-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let mut options =
            Options::parse(&["--state-dir".into(), root.to_string_lossy().into_owned()]).unwrap();
        for hevc in ["auto", "off"] {
            for output in ["web", "carlink", "both"] {
                for codec in ["auto", "h264", "h265"] {
                    options.settings.carlink_hevc = hevc.into();
                    options.settings.output = output.into();
                    std::fs::write(
                        root.join("display.json"),
                        serde_json::to_vec(&serde_json::json!({"videoCodec":codec})).unwrap(),
                    )
                    .unwrap();
                    let settings =
                        supervisor_settings(&options, vec![], &[], String::new(), None, false);
                    let args = settings.arguments();
                    if output == "web" {
                        assert!(!args.iter().any(|a| a == "--clink-hevc"));
                    } else {
                        let expected = if hevc == "auto" && output == "carlink" && codec != "h264" {
                            "auto"
                        } else {
                            "off"
                        };
                        assert!(
                            args.windows(2)
                                .any(|a| a[0] == "--clink-hevc" && a[1] == expected),
                            "{hevc}/{output}/{codec}: {args:?}"
                        );
                    }
                }
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn settings_and_arguments_use_observed_ble_adapter_interface_and_station_band() {
        let root = std::env::temp_dir().join(format!(
            "supervisor-settings-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let mut options =
            Options::parse(&["--state-dir".into(), root.to_string_lossy().into_owned()]).unwrap();
        options.settings.output = "both".into();
        options.settings.carlink_interface = "wlan1".into();
        options.settings.carlink_address = Some("AA:BB:CC:DD:EE:FF".into());
        options.settings.require_pairing_code = true;
        let settings = supervisor_settings(
            &options,
            vec!["192.168.1.2".parse().unwrap()],
            &[],
            "relay-id".into(),
            Some("hci1".into()),
            true,
        );
        let args = settings
            .arguments()
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for pair in [
            ["--clink-adapter", "hci1"],
            ["--clink-interface", "wlan1"],
            ["--clink-band", "1"],
            ["--clink-id", "relay-id"],
            ["--clink-address", "AA:BB:CC:DD:EE:FF"],
        ] {
            assert!(args.windows(2).any(|w| w == pair));
        }
        assert!(args.iter().any(|a| a == "--pairing"));
        assert_eq!(settings.web_port, 8443);
        assert!(settings.addresses.contains(&"10.78.0.1".parse().unwrap()));
        options.settings.carlink_interface.clear();
        let settings = supervisor_settings(
            &options,
            vec![],
            &[],
            "id".into(),
            Some("hci0".into()),
            false,
        );
        let ble = settings.carlink.unwrap().ble.unwrap();
        assert_eq!(
            (ble.interface.as_str(), ble.band, ble.adapter.as_deref()),
            ("wlan0", 0, Some("hci0"))
        );
        options.settings.carlink_link = "static".into();
        options.settings.output = "carlink".into();
        let settings = supervisor_settings(&options, vec![], &[], String::new(), None, false);
        assert_eq!(settings.web_port, 0);
        assert!(settings.carlink.unwrap().ble.is_none());
        options.settings.output = "web".into();
        assert!(
            supervisor_settings(&options, vec![], &[], String::new(), None, false)
                .carlink
                .is_none()
        );
    }
}
