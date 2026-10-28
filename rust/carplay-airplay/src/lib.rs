// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
#![allow(unused_imports, unused_variables)]

/// Fallible protocol/platform operation; malformed data and I/O errors propagate.
pub type Result<T> = std::io::Result<T>;
/// air play config protocol module.
pub mod air_play_config;
/// air play control diagnostics protocol module.
pub mod air_play_control_diagnostics;
/// air play crypto protocol module.
pub mod air_play_crypto;
/// air play hid protocol module.
pub mod air_play_hid;
/// air play iap tunnel stream protocol module.
pub mod air_play_iap_tunnel_stream;
/// air play identity protocol module.
pub mod air_play_identity;
/// air play info plist protocol module.
pub mod air_play_info_plist;
/// air play peer diagnostics protocol module.
pub mod air_play_peer_diagnostics;
/// air play session protocol module.
pub mod air_play_session;
/// air play tcp accepted protocol module.
pub mod air_play_tcp_accepted;
/// audio stream protocol module.
pub mod audio_stream;
/// bplist codec protocol module.
pub mod bplist_codec;
/// buffered audio stream protocol module.
pub mod buffered_audio_stream;
/// bytes protocol module.
pub mod bytes;
/// Host-neutral wireless network recovery policy.
pub mod car_play_controller;
/// car play media engine protocol module.
pub mod car_play_media_engine;
/// control cipher protocol module.
pub mod control_cipher;
/// feedback position monitor protocol module.
pub mod feedback_position_monitor;
/// Host-polled first-TCP and optional session startup deadlines.
pub mod first_tcp_watchdog;
/// iap tunnel protocol module.
pub mod iap_tunnel;
/// media sink decorator protocol module.
pub mod media_sink_decorator;
/// mfi sap auth setup protocol module.
pub mod mfi_sap_auth_setup;
/// microphone packetizer protocol module.
pub mod microphone_packetizer;
/// microphone start protocol module.
pub mod microphone_start;
/// ntp clock protocol module.
pub mod ntp_clock;
/// pair setup protocol module.
pub mod pair_setup;
/// pair verify protocol module.
pub mod pair_verify;
/// pairing store protocol module.
pub mod pairing_store;
/// rtp clock probe protocol module.
pub mod rtp_clock_probe;
/// rtsp message protocol module.
pub mod rtsp_message;
/// Captured screen ownership and delivery policy.
pub mod screen_session_policy;
/// screen stream protocol module.
pub mod screen_stream;
/// srp6a protocol module.
pub mod srp6a;
/// stream receive stats protocol module.
pub mod stream_receive_stats;
/// tlv8 codec protocol module.
pub mod tlv8_codec;

pub use air_play_config::AirPlayConfig;
pub use air_play_config::AirPlayDisplayConfig;
pub use air_play_config::AirPlayInsets;
pub use air_play_control_diagnostics::AirPlayControlDiagnostics;
pub use air_play_crypto::AirPlayCrypto;
pub use air_play_crypto::Ed25519KeyPair;
pub use air_play_crypto::X25519KeyPair;
pub use air_play_hid::AirPlayContact;
pub use air_play_hid::AirPlayHid;
pub use air_play_hid::AirPlayKnobState;
pub use air_play_iap_tunnel_stream::AirPlayIapTunnelStream;
pub use air_play_identity::AirPlayIdentity;
pub use air_play_info_plist::AirPlayInfoPlist;
pub use air_play_peer_diagnostics::AirPlayPeerDiagnostics;
pub use air_play_session::AirPlayMediaHandler;
pub use air_play_session::AirPlaySession;
pub use air_play_session::AirPlaySessionListener;
pub use air_play_session::PlaybackToken;
pub use air_play_tcp_accepted::AirPlayListenerIdentity;
pub use air_play_tcp_accepted::AirPlayTcpAccepted;
pub use audio_stream::AudioCodecKind;
pub use audio_stream::AudioFormat;
pub use audio_stream::AudioListener;
pub use audio_stream::AudioStream;
pub use audio_stream::AudioStreamCodec;
pub use bplist_codec::BplistCodec;
pub use buffered_audio_stream::BufferedAudioStream;
pub use car_play_media_engine::AudioStreamId;
pub use car_play_media_engine::CarPlayMediaEngine;
pub use car_play_media_engine::InputEvents;
pub use car_play_media_engine::MediaSink;
pub use carplay_iap2::mfi::MfiAuthenticator;
pub use control_cipher::ControlCipher;
pub use control_cipher::Decrypted;
pub use feedback_position_monitor::FeedbackPositionMonitor;
pub use iap_tunnel::IapTunnel;
pub use iap_tunnel::IapTunnelListener;
pub use media_sink_decorator::MediaSinkDecorator;
pub use media_sink_decorator::SessionControl;
pub use mfi_sap_auth_setup::MfiSapAuthSetup;
pub use microphone_packetizer::MicrophoneConfig;
pub use microphone_packetizer::MicrophoneCounters;
pub use microphone_packetizer::MicrophonePacketizer;
pub use microphone_start::MicrophoneStart;
pub use ntp_clock::NtpClock;
pub use pair_setup::PairSetup;
pub use pair_verify::PairVerify;
pub use pairing_store::PairingStore;
pub use rtp_clock_probe::RtpClockProbe;
pub use rtsp_message::ParsedRequest;
pub use rtsp_message::RtspMessage;
pub use rtsp_message::RtspRequest;
pub use rtsp_message::RtspResponse;
pub use screen_session_policy::{ScreenSessionPolicy, ScreenSessionPolicyProvider};
pub use screen_stream::DatagramTransport;
pub use screen_stream::ScreenCodec;
pub use screen_stream::ScreenListener;
pub use screen_stream::ScreenStream;
pub use screen_stream::StreamListener;
pub use screen_stream::StreamTransport;
pub use screen_stream::VideoCodec;
pub use srp6a::Srp6a;
pub use srp6a::Srp6aChallenge;
pub use srp6a::{Srp6aResult, Srp6aSession};
pub use stream_receive_stats::StreamReceiveStats;
pub use tlv8_codec::Tlv8Codec;
pub use tlv8_codec::Tlv8Item;
/// Physical display values advertised by /info.
pub mod air_play_display_settings;
pub use air_play_config::{AirPlayIcon, AirPlayViewArea};
pub use air_play_display_settings::{
    AirPlayDisplaySettings, AirPlayPhysicalSizeBasis, AirPlayPhysicalSizeMm,
};
pub use bplist_codec::BplistValue;

pub use air_play_session::AirPlayDeviceInfo;

pub mod activity;
