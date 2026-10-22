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
            model: o.settings.carlink_model.clone(),
            hevc: o.settings.carlink_hevc.clone(),
            auto_reconnect: o.settings.carlink_auto_reconnect,
            ble: if o.settings.carlink_link == "ble" {
                Some(BleCarlinkSettings {
                    adapter: ble_adapter,
                    interface: o.carlink_interface().into(),
                    address: o.settings.carlink_address.clone(),
                    id: pairing_id,
                    band: station_5ghz as u8,
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
