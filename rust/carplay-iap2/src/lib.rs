// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

/// Fallible protocol/platform operation; malformed data and I/O errors propagate.
pub type Result<T> = std::io::Result<T>;
/// body protocol module.
pub mod body;
/// catalog protocol module.
pub mod catalog;
/// message protocol module.
pub mod message;
/// mfi protocol module.
pub mod mfi;
/// session protocol module.
pub mod session;
/// trace protocol module.
pub mod trace;
/// transport protocol module.
pub mod transport;
/// wire protocol module.
pub mod wire;

pub use body::iap2_body_builder::Iap2BodyBuilder;
pub use body::iap2_body_reader::Iap2BodyReader;
pub use catalog::iap2_endpoint::Iap2Confidence;
pub use catalog::iap2_endpoint::Iap2Direction;
pub use catalog::iap2_endpoint::Iap2Endpoint;
pub use catalog::iap2_endpoint::Iap2Endpoints;
pub use catalog::iap2_endpoint::Iap2FieldSpec;
pub use catalog::iap2_endpoint::Iap2WireType;
pub use message::iap2_messages::Iap2AuthenticationMessages;
pub use message::iap2_messages::Iap2CarPlayMessages;
pub use message::iap2_messages::Iap2ControlMessages;
pub use message::iap2_messages::Iap2Messages;
pub use message::iap2_messages::Iap2WifiInformation;
pub use message::iap2_messages::Iap2WirelessMessages;
pub use mfi::iap2_mfi_authentication_client::Iap2MfiAuthenticationClient;
pub use mfi::local_mfi_authentication_client::LocalMfiAuthenticationClient;
pub use mfi::mfi_authentication_client::BaaCertificatePair;
pub use mfi::mfi_authentication_client::MfiAuthenticator;
pub use mfi::mfi_authentication_client::MfiCertificateType;
pub use session::iap2_session::Iap2Session;
pub use trace::iap2_frame_formatter::Iap2FrameFormatter;
pub use trace::iap2_frame_formatter::Iap2TraceDirection;
pub use transport::blocking_duplex_byte_stream::DuplexStream;
pub use transport::blocking_duplex_byte_stream::Receive;
pub use transport::iap2_control_deadline::Iap2ControlDeadline;
pub use transport::iap2_csm_channel::Iap2CsmChannel;
pub use transport::iap2_file_transfer_receiver::FileTransferOutcome;
pub use transport::iap2_file_transfer_receiver::Iap2ArtworkTransfer;
pub use transport::iap2_file_transfer_receiver::Iap2FileTransferReceiver;
pub use transport::iap2_identification_client::Iap2IdentificationClient;
pub use transport::iap2_identification_client::Iap2IdentificationConfig;
pub use transport::iap2_identification_client::Iap2WirelessIdentification;
pub use transport::iap2_identification_client::Iap2WirelessLinkRole;
pub use transport::iap2_link_channel::Iap2LinkChannel;
pub use transport::iap2_link_engine::Iap2LinkConfig;
pub use transport::iap2_link_engine::Iap2LinkEngine;
pub use transport::iap2_link_engine::LinkEvent;
pub use transport::iap2_link_engine::LinkState;
pub use transport::iap2_link_engine::SessionDescriptor;
pub use transport::iap2_link_engine::SynchronizationPayload;
pub use transport::iap2_rx_log::Iap2RxLog;
pub use transport::iap2_wireless_control_client::Iap2StartSessionSent;
pub use transport::iap2_wireless_control_client::Iap2WirelessCarPlayEndpoint;
pub use transport::iap2_wireless_control_client::Iap2WirelessControlClient;
pub use transport::iap2_wireless_control_client::Iap2WirelessControlResult;
pub use transport::iap2_wireless_control_client::Iap2WirelessControlStage;
pub use transport::iap2_wireless_control_client::Iap2WirelessControlTerminal;
pub use transport::iap2_wireless_control_client::Iap2WirelessSecurity;
pub use transport::iap2_wireless_control_client::WirelessControlEvents;
pub use transport::iap2_wireless_control_client::WirelessEndpointSource;
pub use wire::iap2_csm_framer::Iap2CsmFramer;
pub use wire::iap2_wire::Iap2Frame;
pub use wire::iap2_wire::Iap2Parameter;
pub use wire::iap2_wire::Iap2ParameterList;
pub use wire::iap2_wire_codec::Iap2WireCodec;

pub(crate) fn invalid(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

pub use message::iap2_messages::{
    Iap2AvailabilityState, Iap2CarPlayAvailability, Iap2ClusterAsset, Iap2StartSessionOptions,
};
pub use transport::iap2_identification_client::{
    Iap2IdentificationRejection, Iap2RejectedMessageList,
};

pub mod context_decode;

pub mod route_decode;
