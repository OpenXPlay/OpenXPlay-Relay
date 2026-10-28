// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
/// blocking duplex byte stream protocol module.
pub mod blocking_duplex_byte_stream;
/// iap2 control deadline protocol module.
pub mod iap2_control_deadline;
/// iap2 csm channel protocol module.
pub mod iap2_csm_channel;
/// iap2 file transfer receiver protocol module.
pub mod iap2_file_transfer_receiver;
/// iap2 identification client protocol module.
pub mod iap2_identification_client;
/// iap2 link channel protocol module.
pub mod iap2_link_channel;
/// iap2 link engine protocol module.
pub mod iap2_link_engine;
/// iap2 rx log protocol module.
pub mod iap2_rx_log;
pub mod iap2_usb_mux_host;
pub mod iap2_wired_control_client;
/// iap2 wireless control client protocol module.
pub mod iap2_wireless_control_client;
pub mod iphone_car_play_configuration;
pub mod iphone_usb_host;
#[cfg(any(target_os = "linux", test))]
pub mod linux_usb_host;
pub mod lockdown_car_kit_client;
pub mod lockdown_pair_record;
pub mod lockdown_pairing_client;
pub mod lockdown_plist_channel;
pub mod lockdown_tls_engine_factory;
pub mod ncm_function_discovery;
pub mod ncm_usb_bridge;
pub mod ntb16_codec;
pub mod tls_duplex_channel;
pub mod usb_configuration_selection;
pub mod usb_mux_frame_buffer;
