// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// `Iap2Direction` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2Direction {
    IphoneToAccessory,
    AccessoryToIphone,
    Bidirectional,
}
/// `Iap2Confidence` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2Confidence {
    High,
    Medium,
}
/// `Iap2WireType` states; preserve wire values in its codec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iap2WireType {
    Void,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    U16List,
    Bytes,
    String,
    StringList,
    Group,
    Opaque,
}
/// `Iap2FieldSpec` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2FieldSpec {
    pub id: u16,

    pub name: String,

    pub wire_type: Iap2WireType,

    pub required: bool,

    pub repeatable: bool,

    pub children: Vec<Iap2FieldSpec>,
}
/// `Iap2Endpoint` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2Endpoint {
    pub id: u16,

    pub name: String,

    pub feature: String,

    pub direction: Iap2Direction,

    pub confidence: Iap2Confidence,

    pub fields: Vec<Iap2FieldSpec>,
}
/// State owned by `Iap2Endpoints`; construct through its factory.
pub struct Iap2Endpoints {
    _private: (),
}
impl Iap2Endpoints {
    pub fn by_id(id: u16) -> Option<Iap2Endpoint> {
        match id {
            0x1d00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x1d00,
                name: "StartIdentification".into(),
                feature: "Identification".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x1d01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 0,
                        name: "name".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 1,
                        name: "modelIdentifier".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 2,
                        name: "manufacturer".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 3,
                        name: "serialNumber".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 4,
                        name: "firmwareVersion".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 5,
                        name: "hardwareVersion".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 6,
                        name: "MessagesSentByAccessory".into(),
                        wire_type: Iap2WireType::U16List,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 7,
                        name: "MessagesReceivedFromDevice".into(),
                        wire_type: Iap2WireType::U16List,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 8,
                        name: "powerProvidingCapability".into(),
                        wire_type: Iap2WireType::U8,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 9,
                        name: "maximumCurrentDrawnFromDevice".into(),
                        wire_type: Iap2WireType::U16,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 10,
                        name: "externalAccessoryProtocols".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 12,
                        name: "currentLanguage".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 13,
                        name: "supportedLanguages".into(),
                        wire_type: Iap2WireType::StringList,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "componentID".into(),
                                wire_type: Iap2WireType::U16,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "name".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 2,
                                name: "isSupported".into(),
                                wire_type: Iap2WireType::Void,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 3,
                                name: "interfaceNumber".into(),
                                wire_type: Iap2WireType::U8,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 4,
                                name: "isAvailable".into(),
                                wire_type: Iap2WireType::Void,
                            },
                        ],
                        id: 16,
                        name: "USBHostTransport".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "componentID".into(),
                                wire_type: Iap2WireType::U16,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "name".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 2,
                                name: "isSupported".into(),
                                wire_type: Iap2WireType::Void,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 3,
                                name: "bluetoothMAC".into(),
                                wire_type: Iap2WireType::Bytes,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 4,
                                name: "transportName".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 5,
                                name: "isAvailable".into(),
                                wire_type: Iap2WireType::Void,
                            },
                        ],
                        id: 17,
                        name: "BluetoothTransport".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 20,
                        name: "VehicleInformation".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 21,
                        name: "VehicleStatus".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 22,
                        name: "LocationInformation".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "componentID".into(),
                                wire_type: Iap2WireType::U16,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "SSID".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 2,
                                name: "isSupported".into(),
                                wire_type: Iap2WireType::Void,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 3,
                                name: "transportIdentifier".into(),
                                wire_type: Iap2WireType::U16,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 4,
                                name: "isAvailable".into(),
                                wire_type: Iap2WireType::Void,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 5,
                                name: "isEnabled".into(),
                                wire_type: Iap2WireType::Void,
                            },
                        ],
                        id: 24,
                        name: "WirelessCarPlayTransport".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: true,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "Identifier".into(),
                                wire_type: Iap2WireType::U16,
                            },
                            Iap2FieldSpec {
                                required: true,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "Name".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 2,
                                name: "MaxCurrentRoadNameLength".into(),
                                wire_type: Iap2WireType::U16,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 4,
                                name: "MaxAfterManeuverRoadNameLength".into(),
                                wire_type: Iap2WireType::U16,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 6,
                                name: "MaxGuidanceManeuverStorageCapacity".into(),
                                wire_type: Iap2WireType::U16,
                            },
                        ],
                        id: 30,
                        name: "RouteGuidanceDisplayComponent".into(),
                        wire_type: Iap2WireType::Group,
                    },
                ],
                id: 0x1d01,
                name: "IdentificationInformation".into(),
                feature: "Identification".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x1d02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x1d02,
                name: "IdentificationAccepted".into(),
                feature: "Identification".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x1d03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x1d03,
                name: "IdentificationRejected".into(),
                feature: "Identification".into(),
                direction: Iap2Direction::Bidirectional,
            }),
            0x1d05 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x1d05,
                name: "CancelIdentification".into(),
                feature: "Identification".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x1d06 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x1d06,
                name: "IdentificationInformationUpdate".into(),
                feature: "Identification".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xaa00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa00,
                name: "RequestAuthenticationCertificate".into(),
                feature: "AccAuthentication".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xaa01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: true,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "certificate".into(),
                    wire_type: Iap2WireType::Bytes,
                }],
                id: 0xaa01,
                name: "AuthenticationCertificate".into(),
                feature: "AccAuthentication".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xaa02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: true,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "challenge".into(),
                    wire_type: Iap2WireType::Bytes,
                }],
                id: 0xaa02,
                name: "RequestAuthenticationChallengeResponse".into(),
                feature: "AccAuthentication".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xaa03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: true,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "signatureResponse".into(),
                    wire_type: Iap2WireType::Bytes,
                }],
                id: 0xaa03,
                name: "AuthenticationResponse".into(),
                feature: "AccAuthentication".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xaa04 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa04,
                name: "AuthenticationFailed".into(),
                feature: "AccAuthentication".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xaa05 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa05,
                name: "AuthenticationSucceeded".into(),
                feature: "AccAuthentication".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xaa06 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa06,
                name: "AuthenticationCertificateSerial".into(),
                feature: "AccAuthentication".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xaa10 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa10,
                name: "RequestDeviceCertificate".into(),
                feature: "DeviceAuthentication".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xaa11 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa11,
                name: "AuthenticationCertificate".into(),
                feature: "DeviceAuthentication".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xaa12 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa12,
                name: "RequestDeviceChallengeResponse".into(),
                feature: "DeviceAuthentication".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xaa13 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa13,
                name: "AuthenticationResponse".into(),
                feature: "DeviceAuthentication".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xaa14 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa14,
                name: "DeviceAuthenticationFailed".into(),
                feature: "DeviceAuthentication".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xaa15 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xaa15,
                name: "DeviceAuthenticationSucceeded".into(),
                feature: "DeviceAuthentication".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4300 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "wiredAvailable".into(),
                                wire_type: Iap2WireType::U8,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "usbIdentifier".into(),
                                wire_type: Iap2WireType::String,
                            },
                        ],
                        id: 0,
                        name: "wired".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "wirelessAvailable".into(),
                                wire_type: Iap2WireType::U8,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "bluetoothIdentifier".into(),
                                wire_type: Iap2WireType::String,
                            },
                        ],
                        id: 1,
                        name: "wireless".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![Iap2FieldSpec {
                            required: false,
                            repeatable: false,
                            children: vec![],
                            id: 0,
                            name: "themeAssetsAvailable".into(),
                            wire_type: Iap2WireType::U8,
                        }],
                        id: 2,
                        name: "themeAssets".into(),
                        wire_type: Iap2WireType::Group,
                    },
                ],
                id: 0x4300,
                name: "CarPlayAvailability".into(),
                feature: "CarPlayConnectionRequest".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4301 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: true,
                                children: vec![],
                                id: 0,
                                name: "wiredIP".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "reserved".into(),
                                wire_type: Iap2WireType::U32,
                            },
                        ],
                        id: 0,
                        name: "wired".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "SSID".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "passphrase".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 2,
                                name: "channel".into(),
                                wire_type: Iap2WireType::U8,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: true,
                                children: vec![],
                                id: 3,
                                name: "wirelessIP".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 4,
                                name: "securityType".into(),
                                wire_type: Iap2WireType::U8,
                            },
                        ],
                        id: 1,
                        name: "wireless".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 2,
                        name: "airPlayPort".into(),
                        wire_type: Iap2WireType::U32,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 3,
                        name: "deviceIdentifier".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 4,
                        name: "publicKey".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 5,
                        name: "sourceVersion".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 6,
                        name: "SDKVersion".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 0,
                                name: "assetID".into(),
                                wire_type: Iap2WireType::String,
                            },
                            Iap2FieldSpec {
                                required: false,
                                repeatable: false,
                                children: vec![],
                                id: 1,
                                name: "assetVersion".into(),
                                wire_type: Iap2WireType::U32,
                            },
                        ],
                        id: 7,
                        name: "clusterAsset".into(),
                        wire_type: Iap2WireType::Group,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 8,
                        name: "mutualAuth".into(),
                        wire_type: Iap2WireType::U8,
                    },
                ],
                id: 0x4301,
                name: "CarPlayStartSession".into(),
                feature: "CarPlayConnectionRequest".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4302 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4302,
                name: "AvailableDigitalCarKeys".into(),
                feature: "DigitalCarKey_Matching".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4303 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4303,
                name: "MatchedDigitalCarKeys".into(),
                feature: "DigitalCarKey_Matching".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5700 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5700,
                name: "RequestWiFiInformation".into(),
                feature: "WiFiSharing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5701 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: true,
                        repeatable: false,
                        children: vec![],
                        id: 0,
                        name: "status".into(),
                        wire_type: Iap2WireType::U8,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 1,
                        name: "securityType".into(),
                        wire_type: Iap2WireType::U8,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 2,
                        name: "SSID".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 3,
                        name: "passphrase".into(),
                        wire_type: Iap2WireType::String,
                    },
                ],
                id: 0x5701,
                name: "WiFiInformation".into(),
                feature: "WiFiSharing".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5702 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5702,
                name: "RequestAccessoryWiFiConfigurationInformation".into(),
                feature: "WiFiSharing".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5703 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 0,
                        name: "BSSID".into(),
                        wire_type: Iap2WireType::Bytes,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 1,
                        name: "SSID".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 2,
                        name: "passphrase".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 3,
                        name: "securityType".into(),
                        wire_type: Iap2WireType::U8,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 4,
                        name: "channel".into(),
                        wire_type: Iap2WireType::U8,
                    },
                ],
                id: 0x5703,
                name: "AccessoryWiFiConfigurationInformation".into(),
                feature: "WiFiSharing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xfff0 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xfff0,
                name: "GPRMCDataStatusValues".into(),
                feature: "Location".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xfffa => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xfffa,
                name: "StartLocationInformation".into(),
                feature: "Location".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xfffb => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: false,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "NMEA".into(),
                    wire_type: Iap2WireType::String,
                }],
                id: 0xfffb,
                name: "LocationInformation".into(),
                feature: "Location".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xfffc => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xfffc,
                name: "StopLocationInformation".into(),
                feature: "Location".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xae00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xae00,
                name: "StartPowerUpdates".into(),
                feature: "Power".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xae01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xae01,
                name: "PowerUpdate".into(),
                feature: "Power".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xae02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xae02,
                name: "StopPowerUpdates".into(),
                feature: "Power".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xae03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 0,
                        name: "AvailableCurrentForDevice".into(),
                        wire_type: Iap2WireType::U16,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 1,
                        name: "IsDeviceBatteryCharging".into(),
                        wire_type: Iap2WireType::U8,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 3,
                        name: "AvailableSiphoningCurrent".into(),
                        wire_type: Iap2WireType::U16,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 4,
                        name: "ReserveCurrent".into(),
                        wire_type: Iap2WireType::U16,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 5,
                        name: "MaxNonSiphoningCurrentExceeded".into(),
                        wire_type: Iap2WireType::Void,
                    },
                ],
                id: 0xae03,
                name: "PowerSourceUpdate".into(),
                feature: "Power".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xae05 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xae05,
                name: "AccessoryPowerUpdate".into(),
                feature: "Power".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xae06 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xae06,
                name: "AccessoryResetBaseCurrent".into(),
                feature: "Power".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5200 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5200,
                name: "StartRouteGuidanceUpdates".into(),
                feature: "Navigation".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5203 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5203,
                name: "StopRouteGuidanceUpdates".into(),
                feature: "Navigation".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5000 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5000,
                name: "StartNowPlayingUpdates".into(),
                feature: "Now Playing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5002 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5002,
                name: "StopNowPlayingUpdates".into(),
                feature: "Now Playing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4157 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4157,
                name: "startCommunicationsUpdatesHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4154 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4154,
                name: "startCallStateUpdatesHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4159 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4159,
                name: "stopCommunicationsUpdatesHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4156 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4156,
                name: "stopCallStateUpdatesHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4e09 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: false,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "deviceName".into(),
                    wire_type: Iap2WireType::String,
                }],
                id: 0x4e09,
                name: "DeviceInformationUpdate".into(),
                feature: "DeviceNotifications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4e0a => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: false,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "language".into(),
                    wire_type: Iap2WireType::String,
                }],
                id: 0x4e0a,
                name: "DeviceLanguageUpdate".into(),
                feature: "DeviceNotifications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4e0b => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 0,
                        name: "secondsSinceReferenceDate".into(),
                        wire_type: Iap2WireType::U64,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 1,
                        name: "timeZoneOffsetMinutes".into(),
                        wire_type: Iap2WireType::I16,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 2,
                        name: "daylightSavingsOffsetMinutes".into(),
                        wire_type: Iap2WireType::I8,
                    },
                ],
                id: 0x4e0b,
                name: "DeviceTimeUpdate".into(),
                feature: "DeviceNotifications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4e0c => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: false,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "deviceUUID".into(),
                    wire_type: Iap2WireType::String,
                }],
                id: 0x4e0c,
                name: "DeviceUuidUpdate".into(),
                feature: "DeviceNotifications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4e0d => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![Iap2FieldSpec {
                    required: true,
                    repeatable: false,
                    children: vec![],
                    id: 0,
                    name: "availability".into(),
                    wire_type: Iap2WireType::U8,
                }],
                id: 0x4e0d,
                name: "WirelessCarPlayUpdate".into(),
                feature: "DeviceNotifications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4e0e => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 0,
                        name: "bluetoothMAC".into(),
                        wire_type: Iap2WireType::String,
                    },
                    Iap2FieldSpec {
                        required: false,
                        repeatable: false,
                        children: vec![],
                        id: 1,
                        name: "usbTransportIdentifier".into(),
                        wire_type: Iap2WireType::String,
                    },
                ],
                id: 0x4e0e,
                name: "DeviceTransportIdentifierNotification".into(),
                feature: "DeviceNotifications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xea00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xea00,
                name: "externalAccessory_openEASession".into(),
                feature: "ExternalAccessory".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xea01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xea01,
                name: "externalAccessory_closeEASession".into(),
                feature: "ExternalAccessory".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xea03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xea03,
                name: "externalaccessory_statusEASessionHandler".into(),
                feature: "ExternalAccessory".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x0000 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0000,
                name: "pingHandler".into(),
                feature: "Test".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x0001 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0001,
                name: "userNotificationHandler".into(),
                feature: "Test".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x0004 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0004,
                name: "sendPingAccHandler".into(),
                feature: "Test".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x0005 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0005,
                name: "pingAccHandler".into(),
                feature: "Test".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xda00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xda00,
                name: "digitalAudio_start".into(),
                feature: "DigitalAudio".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xda01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xda01,
                name: "DigitalAudioInformationUpdate".into(),
                feature: "DigitalAudio".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xda02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xda02,
                name: "digitalAudio_stop".into(),
                feature: "DigitalAudio".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xA100 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xA100,
                name: "StartVehicleStatusUpdates".into(),
                feature: "Vehicle".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xA101 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xA101,
                name: "VehicleStatusUpdate".into(),
                feature: "Vehicle".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xA102 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xA102,
                name: "StopVehicleStatusUpdates".into(),
                feature: "Vehicle".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5001 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5001,
                name: "NowPlayingUpdate".into(),
                feature: "Now Playing".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5003 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5003,
                name: "SetNowPlayingInfo".into(),
                feature: "Now Playing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C00,
                name: "StartMediaLibraryInformation".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C01,
                name: "MediaLibraryInformation".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4C02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C02,
                name: "StopMediaLibraryInformation".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C03,
                name: "StartMediaLibraryUpdates".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C04 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C04,
                name: "MediaLibraryUpdate".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4C05 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C05,
                name: "StopMediaLibraryUpdates".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C06 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C06,
                name: "PlayMediaLibraryCurrentSelection".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C07 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C07,
                name: "PlayMediaLibraryItems".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C08 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C08,
                name: "PlayMediaLibraryCollection".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4C09 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4C09,
                name: "PlayMediaLibrarySpecial".into(),
                feature: "Media Library".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5201 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5201,
                name: "RouteGuidanceUpdate".into(),
                feature: "Navigation".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5202 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5202,
                name: "RouteGuidanceManeuverUpdate".into(),
                feature: "Navigation".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5204 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5204,
                name: "LaneGuidanceInfoUpdate".into(),
                feature: "Navigation".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4155 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4155,
                name: "CallStateUpdate".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4158 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4158,
                name: "CommunicationsUpdate".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x415A => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x415A,
                name: "initiateCallHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x415B => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x415B,
                name: "acceptCallHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x415C => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x415C,
                name: "endCallHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x415D => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x415D,
                name: "swapCallHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x415E => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x415E,
                name: "mergeCallHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x415F => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x415F,
                name: "holdStatusUpdateHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4160 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4160,
                name: "muteStatusUpdateHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4161 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4161,
                name: "sendDTMFHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4170 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4170,
                name: "startListUpdatesHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4171 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4171,
                name: "ListUpdate".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4172 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4172,
                name: "stopListUpdatesHandler".into(),
                feature: "Communications".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4E01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4E01,
                name: "BluetoothComponentInfo".into(),
                feature: "Bluetooth connection status".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4E03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4E03,
                name: "StartBluetoothConnectionUpdates".into(),
                feature: "Bluetooth connection status".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x4E04 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4E04,
                name: "BluetoothConnectionUpdate".into(),
                feature: "Bluetooth connection status".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x4E05 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x4E05,
                name: "StopBluetoothConnectionUpdates".into(),
                feature: "Bluetooth connection status".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xAD00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xAD00,
                name: "appLinks_start".into(),
                feature: "App links and launch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xAD01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xAD01,
                name: "AppLinksUpdate".into(),
                feature: "App links and launch".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xAD02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xAD02,
                name: "appLinks_stop".into(),
                feature: "App links and launch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xAD03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xAD03,
                name: "appLinks_requestAppIcons".into(),
                feature: "App links and launch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xAD04 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xAD04,
                name: "AppIconResponse".into(),
                feature: "App links and launch".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xEA02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xEA02,
                name: "requestAppLaunchHandler".into(),
                feature: "External Accessory".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x6800 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x6800,
                name: "hid_StartHIDMsgHandler".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x6801 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x6801,
                name: "hid_OutReport".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x6802 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x6802,
                name: "hid_AccessoryHIDReportHandler".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x6803 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x6803,
                name: "hid_StopHIDMsgHandler".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x6804 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x6804,
                name: "hid_GetReport".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x6805 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x6805,
                name: "hid_AccessoryHIDGetReportResponseHandler".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x6807 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x6807,
                name: "hid_ComponentUpdate".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x7E00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x7E00,
                name: "usbHostMode_StartUSBHostModeHandler".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x7E02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x7E02,
                name: "usbHostMode_StopUSBHostModeHandler".into(),
                feature: "HID and USB host".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5400 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5400,
                name: "assistiveTouch_startAssistiveTouchHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5401 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5401,
                name: "assistiveTouch_stopAssistiveTouchHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5402 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5402,
                name: "assistiveTouch_startAssistiveTouchUpdateHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5403 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5403,
                name: "NotifyStatusUpdate".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5404 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5404,
                name: "assistiveTouch_stopAssistiveTouchUpdateHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5601 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5601,
                name: "voiceOver_moveCursorHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5602 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5602,
                name: "voiceOver_selectCursorHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5603 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5603,
                name: "voiceOver_scrollPageHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5606 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5606,
                name: "voiceOver_speakTextHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5608 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5608,
                name: "voiceOver_pauseSpeakingHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5609 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5609,
                name: "voiceOver_resumeSpeakingHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x560B => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x560B,
                name: "voiceOver_startInformationHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x560C => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x560C,
                name: "VoiceOverInformation".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x560D => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x560D,
                name: "voiceOver_stopInformationHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x560E => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x560E,
                name: "voiceOver_updateInformationHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x560F => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x560F,
                name: "voiceOver_startCursorInformationHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5610 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5610,
                name: "VoiceOverCursorInformation".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x5611 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5611,
                name: "voiceOver_stopCursorInformationHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5612 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5612,
                name: "VoiceOver_startVoiceOverHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x5613 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x5613,
                name: "VoiceOver_stopVoiceOverHandler".into(),
                feature: "VoiceOver and AssistiveTouch".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x0B00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0B00,
                name: "oobBtPairing2_startPairing".into(),
                feature: "BLE and OOB pairing".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x0B01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0B01,
                name: "oobBtPairing2_accessoryInfoHandler".into(),
                feature: "BLE and OOB pairing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x0B02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0B02,
                name: "oobBtPairing2_statusHandler".into(),
                feature: "BLE and OOB pairing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x0B03 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0B03,
                name: "oobBtPairing2_stopPairing".into(),
                feature: "BLE and OOB pairing".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x10DA => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x10DA,
                name: "destinationSharing_StartDestinationInformation".into(),
                feature: "Destination sharing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x10DB => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x10DB,
                name: "DestinationInformationUpdate".into(),
                feature: "Destination sharing".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x10DC => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x10DC,
                name: "destinationSharing_StopDestinationInformation".into(),
                feature: "Destination sharing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x10DD => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x10DD,
                name: "destinationSharing_DestinationInformationStatus".into(),
                feature: "Destination sharing".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x00B0 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x00B0,
                name: "oobBtPairing_startPairing".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x00B1 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x00B1,
                name: "oobBtPairing_accessoryInfoHandler".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x00B2 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x00B2,
                name: "oobBtPairing_pairingLinkKeyInfo".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x00B3 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x00B3,
                name: "oobBtPairing_completionInfoHandler".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0x00B4 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x00B4,
                name: "oobBtPairing_stopPairing".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x0D00 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0D00,
                name: "StartRoadObjectDetection".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x0D01 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0D01,
                name: "RoadObjectDetectionUpdate".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0x0D02 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0x0D02,
                name: "StopRoadObjectDetection".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            0xB102 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xB102,
                name: "blePairing_accessoryStateUpdateHandler".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xB103 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xB103,
                name: "blePairing_accessoryPairingInfoHandler".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xB105 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xB105,
                name: "blePairing_accessoryPairingDataHandler".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::AccessoryToIphone,
            }),
            0xB107 => Some(Iap2Endpoint {
                confidence: Iap2Confidence::High,
                fields: vec![],
                id: 0xB107,
                name: "blePairing_stopBLEUpdates".into(),
                feature: "Other endpoints".into(),
                direction: Iap2Direction::IphoneToAccessory,
            }),
            _ => None,
        }
    }

    pub fn require(id: u16) -> Result<Iap2Endpoint> {
        Self::by_id(id).ok_or_else(|| invalid(format!("Unknown iAP2 endpoint 0x{id:04x}")))
    }
}

/// Protocol message, field, or framing constant.
pub const START_IDENTIFICATION: u16 = 0x1d00;
/// Protocol message, field, or framing constant.
pub const IDENTIFICATION_INFORMATION: u16 = 0x1d01;
/// Protocol message, field, or framing constant.
pub const IDENTIFICATION_ACCEPTED: u16 = 0x1d02;
/// Protocol message, field, or framing constant.
pub const IDENTIFICATION_REJECTED: u16 = 0x1d03;
/// Protocol message, field, or framing constant.
pub const CANCEL_IDENTIFICATION: u16 = 0x1d05;
/// Protocol message, field, or framing constant.
pub const IDENTIFICATION_INFORMATION_UPDATE: u16 = 0x1d06;
/// Protocol message, field, or framing constant.
pub const REQUEST_AUTHENTICATION_CERTIFICATE: u16 = 0xaa00;
/// Protocol message, field, or framing constant.
pub const AUTHENTICATION_CERTIFICATE: u16 = 0xaa01;
/// Protocol message, field, or framing constant.
pub const REQUEST_AUTHENTICATION_CHALLENGE_RESPONSE: u16 = 0xaa02;
/// Protocol message, field, or framing constant.
pub const AUTHENTICATION_RESPONSE: u16 = 0xaa03;
/// Protocol message, field, or framing constant.
pub const AUTHENTICATION_FAILED: u16 = 0xaa04;
/// Protocol message, field, or framing constant.
pub const AUTHENTICATION_SUCCEEDED: u16 = 0xaa05;
/// Protocol message, field, or framing constant.
pub const AUTHENTICATION_CERTIFICATE_SERIAL: u16 = 0xaa06;
/// Protocol message, field, or framing constant.
pub const REQUEST_DEVICE_CERTIFICATE: u16 = 0xaa10;
/// Protocol message, field, or framing constant.
pub const DEVICE_AUTHENTICATION_CERTIFICATE: u16 = 0xaa11;
/// Protocol message, field, or framing constant.
pub const REQUEST_DEVICE_CHALLENGE_RESPONSE: u16 = 0xaa12;
/// Protocol message, field, or framing constant.
pub const DEVICE_AUTHENTICATION_RESPONSE: u16 = 0xaa13;
/// Protocol message, field, or framing constant.
pub const DEVICE_AUTHENTICATION_FAILED: u16 = 0xaa14;
/// Protocol message, field, or framing constant.
pub const DEVICE_AUTHENTICATION_SUCCEEDED: u16 = 0xaa15;
/// Protocol message, field, or framing constant.
pub const CARPLAY_AVAILABILITY: u16 = 0x4300;
/// Protocol message, field, or framing constant.
pub const CARPLAY_START_SESSION: u16 = 0x4301;
/// Protocol message, field, or framing constant.
pub const AVAILABLE_DIGITAL_CAR_KEYS: u16 = 0x4302;
/// Protocol message, field, or framing constant.
pub const MATCHED_DIGITAL_CAR_KEYS: u16 = 0x4303;
/// Protocol message, field, or framing constant.
pub const REQUEST_WIFI_INFORMATION: u16 = 0x5700;
/// Protocol message, field, or framing constant.
pub const WIFI_INFORMATION: u16 = 0x5701;
/// Protocol message, field, or framing constant.
pub const REQUEST_ACCESSORY_WIFI_CONFIGURATION: u16 = 0x5702;
/// Protocol message, field, or framing constant.
pub const ACCESSORY_WIFI_CONFIGURATION: u16 = 0x5703;
/// Protocol message, field, or framing constant.
pub const GPRMC_DATA_STATUS_VALUES: u16 = 0xfff0;
/// Protocol message, field, or framing constant.
pub const START_LOCATION_INFORMATION: u16 = 0xfffa;
/// Protocol message, field, or framing constant.
pub const LOCATION_INFORMATION: u16 = 0xfffb;
/// Protocol message, field, or framing constant.
pub const STOP_LOCATION_INFORMATION: u16 = 0xfffc;
/// Protocol message, field, or framing constant.
pub const START_POWER_UPDATES: u16 = 0xae00;
/// Protocol message, field, or framing constant.
pub const POWER_UPDATE: u16 = 0xae01;
/// Protocol message, field, or framing constant.
pub const STOP_POWER_UPDATES: u16 = 0xae02;
/// Protocol message, field, or framing constant.
pub const POWER_SOURCE_UPDATE: u16 = 0xae03;
/// Protocol message, field, or framing constant.
pub const ACCESSORY_POWER_UPDATE: u16 = 0xae05;
/// Protocol message, field, or framing constant.
pub const START_ROUTE_GUIDANCE_UPDATES: u16 = 0x5200;
/// Protocol message, field, or framing constant.
pub const STOP_ROUTE_GUIDANCE_UPDATES: u16 = 0x5203;
/// Protocol message, field, or framing constant.
pub const START_NOW_PLAYING_UPDATES: u16 = 0x5000;
/// Protocol message, field, or framing constant.
pub const STOP_NOW_PLAYING_UPDATES: u16 = 0x5002;
/// Protocol message, field, or framing constant.
pub const START_COMMUNICATIONS_UPDATES: u16 = 0x4157;
/// Protocol message, field, or framing constant.
pub const START_CALL_STATE_UPDATES: u16 = 0x4154;
/// Protocol message, field, or framing constant.
pub const STOP_COMMUNICATIONS_UPDATES: u16 = 0x4159;
/// Protocol message, field, or framing constant.
pub const STOP_CALL_STATE_UPDATES: u16 = 0x4156;
/// Protocol message, field, or framing constant.
pub const DEVICE_INFORMATION_UPDATE: u16 = 0x4e09;
/// Protocol message, field, or framing constant.
pub const DEVICE_LANGUAGE_UPDATE: u16 = 0x4e0a;
/// Protocol message, field, or framing constant.
pub const DEVICE_TIME_UPDATE: u16 = 0x4e0b;
/// Protocol message, field, or framing constant.
pub const DEVICE_UUID_UPDATE: u16 = 0x4e0c;
/// Protocol message, field, or framing constant.
pub const WIRELESS_CARPLAY_UPDATE: u16 = 0x4e0d;
/// Protocol message, field, or framing constant.
pub const DEVICE_TRANSPORT_IDENTIFIER: u16 = 0x4e0e;
/// Protocol message, field, or framing constant.
pub const EXTERNAL_ACCESSORY_OPEN_SESSION: u16 = 0xea00;
/// Protocol message, field, or framing constant.
pub const EXTERNAL_ACCESSORY_CLOSE_SESSION: u16 = 0xea01;
/// Protocol message, field, or framing constant.
pub const EXTERNAL_ACCESSORY_STATUS_SESSION: u16 = 0xea03;
/// Protocol message, field, or framing constant.
pub const PING: u16 = 0x0000;
/// Protocol message, field, or framing constant.
pub const USER_NOTIFICATION: u16 = 0x0001;
/// Protocol message, field, or framing constant.
pub const SEND_PING_ACC: u16 = 0x0004;
/// Protocol message, field, or framing constant.
pub const PING_ACC: u16 = 0x0005;
/// Protocol message, field, or framing constant.
pub const DIGITAL_AUDIO_START: u16 = 0xda00;
/// Protocol message, field, or framing constant.
pub const DIGITAL_AUDIO_STOP: u16 = 0xda02;

impl Iap2Endpoints {
    /// Extended context catalog is opt-in: ordinary tracing keeps its original opaque fields.
    /// Field ids follow DiPlay: CarPlayPlaybackStatus.kt, BydClusterSong.kt and BydCarPlayCall.kt.
    pub fn context_by_id(id: u16) -> Option<Iap2Endpoint> {
        let mut endpoint = Self::by_id(id)?;
        endpoint.fields = match id {
            0x5001 => vec![
                Iap2FieldSpec {
                    id: 0,
                    name: "mediaItemAttributes".into(),
                    wire_type: Iap2WireType::Group,
                    required: false,
                    repeatable: false,
                    children: vec![
                        Iap2FieldSpec {
                            id: 1,
                            name: "title".into(),
                            wire_type: Iap2WireType::String,
                            required: false,
                            repeatable: false,
                            children: vec![],
                        },
                        Iap2FieldSpec {
                            id: 4,
                            name: "durationMs".into(),
                            wire_type: Iap2WireType::U32,
                            required: false,
                            repeatable: false,
                            children: vec![],
                        },
                        Iap2FieldSpec {
                            id: 6,
                            name: "album".into(),
                            wire_type: Iap2WireType::String,
                            required: false,
                            repeatable: false,
                            children: vec![],
                        },
                        Iap2FieldSpec {
                            id: 12,
                            name: "artist".into(),
                            wire_type: Iap2WireType::String,
                            required: false,
                            repeatable: false,
                            children: vec![],
                        },
                    ],
                },
                Iap2FieldSpec {
                    id: 1,
                    name: "playbackAttributes".into(),
                    wire_type: Iap2WireType::Group,
                    required: false,
                    repeatable: false,
                    children: vec![
                        Iap2FieldSpec {
                            id: 0,
                            name: "status".into(),
                            wire_type: Iap2WireType::U8,
                            required: false,
                            repeatable: false,
                            children: vec![],
                        },
                        Iap2FieldSpec {
                            id: 1,
                            name: "elapsedMs".into(),
                            wire_type: Iap2WireType::U32,
                            required: false,
                            repeatable: false,
                            children: vec![],
                        },
                        Iap2FieldSpec {
                            id: 7,
                            name: "sourceApp".into(),
                            wire_type: Iap2WireType::String,
                            required: false,
                            repeatable: false,
                            children: vec![],
                        },
                    ],
                },
            ],
            0x4155 => vec![Iap2FieldSpec {
                id: 2,
                name: "status".into(),
                wire_type: Iap2WireType::U8,
                required: false,
                repeatable: false,
                children: vec![],
            }],
            _ => return None,
        };
        Some(endpoint)
    }
}
