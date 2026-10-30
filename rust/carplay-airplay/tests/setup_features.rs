// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-30
// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay).

use carplay_airplay::{air_play_session::setup_enabled_features, *};
use BplistValue as V;
fn offers() -> Vec<Option<V>> {
    vec![
        None,
        Some(V::Array(vec![])),
        Some(V::from("iAPChannel")),
        Some(V::Array(vec!["viewAreas".into()])),
        Some(V::Array(vec!["iapchannel".into(), 1u64.into()])),
        Some(V::Array(vec!["iAPChannel".into()])),
    ]
}
#[test]
fn default_enables_iap_channel_with_or_without_an_offer() {
    let config = AirPlayConfig::linux_receiver("test".into(), "test".into());
    assert!(!config.iap_channel_only_when_offered);
    for value in offers() {
        assert_eq!(
            setup_enabled_features(&config, value.as_ref().and_then(V::as_array)),
            ["iAPChannel", "viewAreas"]
        );
    }
}
#[test]
fn only_the_exact_array_offer_enables_the_channel() {
    let mut config = AirPlayConfig::linux_receiver("test".into(), "test".into());
    config.iap_channel_only_when_offered = true;
    let offers = offers();
    for (i, value) in offers.iter().enumerate() {
        let expected = if i == offers.len() - 1 {
            vec!["iAPChannel", "viewAreas"]
        } else {
            vec!["viewAreas"]
        };
        assert_eq!(
            setup_enabled_features(&config, value.as_ref().and_then(V::as_array)),
            expected
        );
    }
}
#[test]
fn remaining_features_keep_their_negotiation_and_order() {
    let mut config = AirPlayConfig::linux_receiver("test".into(), "test".into());
    config.hevc = true;
    config.main_buffered_audio = true;
    for proposed in [
        None,
        Some(vec![V::from("mainBuffered"), V::from("videoPlayback")]),
    ] {
        let previous = setup_enabled_features(&config, proposed.as_deref());
        config.iap_channel_only_when_offered = true;
        assert_eq!(
            setup_enabled_features(&config, proposed.as_deref()),
            previous
                .into_iter()
                .filter(|f| f != "iAPChannel")
                .collect::<Vec<_>>()
        );
        config.iap_channel_only_when_offered = false;
    }
}
