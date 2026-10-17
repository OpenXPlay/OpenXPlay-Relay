// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// State owned by `Iap2Messages`; construct through its factory.
pub struct Iap2Messages {
    _private: (),
}
impl Iap2Messages {
    pub fn reader(frame: &Iap2Frame) -> Result<Iap2BodyReader> {
        Iap2BodyReader::of(frame)
    }
}
/// State owned by `Iap2AuthenticationMessages`; construct through its factory.
pub struct Iap2AuthenticationMessages {
    _private: (),
}
impl Iap2AuthenticationMessages {
    pub fn accessory_certificate(certificate: &[u8]) -> Result<Iap2Frame> {
        let mut b = Iap2BodyBuilder::of(&Iap2Endpoints::require(0xaa01)?);
        b.bytes(0, certificate)?;
        b.build()
    }

    pub fn response(signature: &[u8]) -> Result<Iap2Frame> {
        let mut b = Iap2BodyBuilder::of(&Iap2Endpoints::require(0xaa03)?);
        b.bytes(0, signature)?;
        b.build()
    }

    pub fn challenge(frame: &Iap2Frame) -> Result<Vec<u8>> {
        if frame.message_id != 0xaa02 {
            return Err(invalid(format!(
                "Expected RequestAuthenticationChallengeResponse, got 0x{:x}",
                frame.message_id
            )));
        }
        Iap2BodyReader::of(frame)?.bytes(0)
    }
}
/// State owned by `Iap2WirelessMessages`; construct through its factory.
pub struct Iap2WirelessMessages {
    _private: (),
}
impl Iap2WirelessMessages {
    pub fn accessory_wifi_configuration(
        endpoint: &Iap2WirelessCarPlayEndpoint,
    ) -> Result<Iap2Frame> {
        endpoint.validate()?;
        let mut b = Iap2BodyBuilder::of(&Iap2Endpoints::require(0x5703)?);
        b.optional_bytes(
            0,
            endpoint.access_point_bssid.as_ref().map(|v| v.as_slice()),
        )?;
        b.string(1, &endpoint.ssid)?
            .string(2, &endpoint.passphrase)?
            .u8(3, endpoint.security as u8)?
            .u8(4, endpoint.channel)?;
        b.build()
    }

    pub fn wifi_information(frame: &Iap2Frame) -> Result<Iap2WifiInformation> {
        if frame.message_id != 0x5701 {
            return Err(invalid(format!(
                "Expected WiFiInformation, got 0x{:x}",
                frame.message_id
            )));
        }
        let b = Iap2BodyReader::of(frame)?;
        let status = b.u8(0)?;
        Ok(Iap2WifiInformation {
            status,
            security_type: if status == 0 { b.optional_u8(1)? } else { None },
            ssid: if status == 0 {
                b.optional_string(2)?
            } else {
                None
            },
            passphrase: if status == 0 {
                b.optional_string(3)?
            } else {
                None
            },
        })
    }

    pub fn wireless_car_play_availability(frame: &Iap2Frame) -> Result<bool> {
        if frame.message_id != 0x4e0d {
            return Err(invalid(format!(
                "Expected WirelessCarPlayUpdate, got 0x{:x}",
                frame.message_id
            )));
        }
        match Iap2BodyReader::of(frame)?.u8(0)? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(invalid(format!(
                "WirelessCarPlayUpdate availability must be 0 or 1, got {v}"
            ))),
        }
    }
}
/// `Iap2WifiInformation` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2WifiInformation {
    pub status: u8,
    pub security_type: Option<u8>,
    pub ssid: Option<String>,
    pub passphrase: Option<String>,
}
/// State owned by `Iap2CarPlayMessages`; construct through its factory.
pub struct Iap2CarPlayMessages {
    _private: (),
}
impl Iap2CarPlayMessages {
    pub fn start_session(endpoint: &Iap2WirelessCarPlayEndpoint) -> Result<Iap2Frame> {
        endpoint.validate()?;
        let mut b = Iap2BodyBuilder::of(&Iap2Endpoints::require(0x4301)?);
        let mut w = Iap2BodyBuilder::opaque(0);
        w.string(0, &endpoint.ssid)?
            .string(1, &endpoint.passphrase)?
            .u8(2, endpoint.channel)?;
        for address in &endpoint.ip_addresses {
            w.string(3, address)?;
        }
        w.u8(4, endpoint.security as u8)?;
        b.group(1, w)?
            .u32(2, endpoint.air_play_port as u32)?
            .string(3, &endpoint.device_identifier)?
            .string(4, &endpoint.public_key)?
            .string(5, &endpoint.source_version)?;
        b.build()
    }

    pub fn availability(frame: &Iap2Frame) -> Result<bool> {
        if frame.message_id != 0x4300 {
            return Err(invalid(format!(
                "Expected CarPlayAvailability, got 0x{:x}",
                frame.message_id
            )));
        }
        let b = Iap2BodyReader::of(frame)?;
        for (id, label) in [(0, "wired"), (1, "wireless"), (2, "themeAssets")] {
            if let Some(g) = b.optional_group(id)? {
                if let Some(v) = g.optional_u8(0)? {
                    if v > 1 {
                        return Err(invalid(format!(
                            "{label} availability must be 0 or 1, got {v}"
                        )));
                    }
                }
            }
        }
        Ok(b.optional_group(1)?
            .map(|g| g.optional_u8(0))
            .transpose()?
            .flatten()
            == Some(1))
    }
}
/// State owned by `Iap2ControlMessages`; construct through its factory.
#[derive(Default)]
pub struct Iap2ControlMessages {
    _private: (),
}
impl Iap2ControlMessages {
    pub fn power_source_update(&self, source: u8) -> Result<Iap2Frame> {
        let mut b = Iap2BodyBuilder::of(&Iap2Endpoints::require(0xae03)?);
        b.u16(0, source as u16)?;
        b.build()
    }

    // Field ids follow DiPlay Iap2Messages.kt.
    pub fn subscriptions() -> Result<Vec<Iap2Frame>> {
        Self::subscriptions_with_route_guidance(false)
    }

    pub fn subscriptions_with_route_guidance(route_guidance: bool) -> Result<Vec<Iap2Frame>> {
        let mut frames = Vec::new();
        let mut b = Iap2BodyBuilder::opaque(0x5000);
        for (id, fields) in [(0, &[1, 4, 6, 12, 26][..]), (1, &[0, 1, 7][..])] {
            let mut g = Iap2BodyBuilder::opaque(0);
            for f in fields {
                g.void(*f)?;
            }
            b.group(id, g)?;
        }
        frames.push(b.build()?);
        if route_guidance {
            let mut b = Iap2BodyBuilder::opaque(0x5200);
            b.u16(0, 42)?.void(1)?.void(2)?;
            frames.push(b.build()?);
        }
        for (id, fields) in [
            (0xae00, &[4, 5, 6][..]),
            (0x4157, &[0, 4, 5][..]),
            (0x4154, &[0, 1, 2, 3, 4, 11][..]),
        ] {
            let mut b = Iap2BodyBuilder::opaque(id);
            for f in fields {
                b.void(*f)?;
            }
            frames.push(b.build()?);
        }
        Ok(frames)
    }
}

/// Optional fields of the reference StartSession body. No optional capability is advertised implicitly.
#[derive(Clone, Debug, Default)]
pub struct Iap2StartSessionOptions {
    pub wired_ipv6_addresses: Vec<String>,
    pub wired_reserved: Option<u32>,
    pub sdk_version: Option<String>,
    pub cluster_asset: Option<Iap2ClusterAsset>,
    pub mutual_auth: Option<bool>,
}
#[derive(Clone, Debug)]
pub struct Iap2ClusterAsset {
    pub asset_id: String,
    pub asset_version: u32,
}
#[derive(Clone, Debug)]
pub struct Iap2AvailabilityState {
    pub available: Option<bool>,
    pub identifier: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Iap2CarPlayAvailability {
    pub wired: Option<Iap2AvailabilityState>,
    pub wireless: Option<Iap2AvailabilityState>,
    pub theme_assets: Option<Iap2AvailabilityState>,
}
impl Iap2CarPlayMessages {
    pub fn start_session_with_options(
        endpoint: &Iap2WirelessCarPlayEndpoint,
        options: &Iap2StartSessionOptions,
    ) -> Result<Iap2Frame> {
        let base = Self::start_session(endpoint)?;
        let mut parameters = Vec::new();
        if !options.wired_ipv6_addresses.is_empty() || options.wired_reserved.is_some() {
            let mut wired = Iap2BodyBuilder::opaque(0);
            for address in &options.wired_ipv6_addresses {
                wired.string(0, address)?;
            }
            wired.optional_u32(1, options.wired_reserved)?;
            parameters.push(Iap2Parameter {
                id: 0,
                payload: wired.build()?.payload,
            });
        }
        parameters.extend(base.body()?.parameters);
        if let Some(sdk) = &options.sdk_version {
            parameters.push(Iap2Parameter {
                id: 6,
                payload: Iap2WireCodec::string(sdk)?,
            });
        }
        if let Some(asset) = &options.cluster_asset {
            if asset.asset_id.trim().is_empty() {
                return Err(invalid("cluster asset ID must not be blank"));
            }
            let mut group = Iap2BodyBuilder::opaque(0);
            group
                .string(0, &asset.asset_id)?
                .u32(1, asset.asset_version)?;
            parameters.push(Iap2Parameter {
                id: 7,
                payload: group.build()?.payload,
            });
        }
        if let Some(mutual) = options.mutual_auth {
            parameters.push(Iap2Parameter {
                id: 8,
                payload: vec![u8::from(mutual)],
            });
        }
        Ok(Iap2Frame {
            message_id: 0x4301,
            payload: Iap2ParameterList { parameters }.encode()?,
        })
    }
    pub fn availability_details(frame: &Iap2Frame) -> Result<Iap2CarPlayAvailability> {
        Self::availability(frame)?;
        let body = Iap2BodyReader::of(frame)?;
        let group = |id: u16| -> Result<Option<Iap2AvailabilityState>> {
            body.optional_group(id)?
                .map(|b| {
                    Ok(Iap2AvailabilityState {
                        available: b.optional_u8(0)?.map(|v| v == 1),
                        identifier: if id != 2 { b.optional_string(1)? } else { None },
                    })
                })
                .transpose()
        };
        Ok(Iap2CarPlayAvailability {
            wired: group(0)?,
            wireless: group(1)?,
            theme_assets: group(2)?,
        })
    }
}
impl Iap2WifiInformation {
    pub fn successful(&self) -> bool {
        self.status == 0
    }
}
impl Iap2AuthenticationMessages {
    pub fn accessory_certificate_body(body: &[u8]) -> Result<Iap2Frame> {
        Iap2ParameterList::parse(body)?;
        Iap2CsmFramer::encode_frame(0xaa01, body)?;
        Ok(Iap2Frame {
            message_id: 0xaa01,
            payload: body.to_vec(),
        })
    }
    pub fn baa_certificate_package(leaf: &[u8], intermediate: &[u8]) -> Result<Iap2Frame> {
        let mut b = Iap2BodyBuilder::opaque(0xaa01);
        b.bytes(0, leaf)?.u8(1, 1)?.bytes(2, intermediate)?;
        b.build()
    }
}

impl Iap2ControlMessages {
    pub fn power_source_update_with_options(
        available_current_milli_amps: u16,
        charging: Option<bool>,
        available_siphoning_current_milli_amps: Option<u16>,
        reserve_current_milli_amps: Option<u16>,
        max_non_siphoning_current_exceeded: bool,
    ) -> Result<Iap2Frame> {
        let mut b = Iap2BodyBuilder::of(&Iap2Endpoints::require(0xae03)?);
        b.u16(0, available_current_milli_amps)?
            .optional_u8(1, charging.map(u8::from))?
            .optional_u16(3, available_siphoning_current_milli_amps)?
            .optional_u16(4, reserve_current_milli_amps)?;
        if max_non_siphoning_current_exceeded {
            b.void(5)?;
        }
        b.build()
    }
}
