// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

use openxplay_relay::admin::AdminHostAddress;

#[test]
fn addresses_identify_developer_networks_and_use_the_shared_json_contract() {
    for name in ["usb0", "rndis0", "docker0", "veth1", "tun0", "wg0"] {
        let address = AdminHostAddress::new(name.into(), "192.168.42.1".into());
        let json = serde_json::to_value(address).unwrap();
        assert_eq!(json["interface"], name);
        assert_eq!(json["ipv4"], "192.168.42.1");
        assert_eq!(json["developerOnly"], true);
    }
    for name in ["wlan0", "eth0"] {
        assert!(!AdminHostAddress::new(name.into(), "192.168.1.78".into()).developer_only);
    }
}
