// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

//! Child stdout contract: driver capabilities are observations, not hardware acceptance.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoder {
    codec: String,
    max_width: Option<u32>,
    max_height: Option<u32>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Capability {
    pub enabled: bool,
    pub available: bool,
    decode: Vec<String>,
    encode: Vec<Encoder>,
    scaling: bool,
    description: String,
}
impl Capability {
    pub fn converts(&self, codec: &str, size: Option<crate::display::Size>) -> bool {
        self.enabled
            && self.available
            && self.decode.iter().any(|v| v == "H264")
            && self.encode.iter().any(|e| {
                e.codec == codec
                    && size.is_none_or(|s| {
                        e.max_width.is_none_or(|w| s.width <= w)
                            && e.max_height.is_none_or(|h| s.height <= h)
                    })
            })
    }
    pub fn geometry(&self, size: Option<crate::display::Size>) -> bool {
        // Geometry must also work when a head unit selects the H.264 fallback.
        self.scaling && self.converts("H264", size)
    }
}
#[derive(Default, Serialize)]
pub struct Capabilities {
    pub web: Capability,
    pub clink: Capability,
}
impl Capabilities {
    pub fn observe(&mut self, event: &Value) -> bool {
        if event["type"] != "transcode-capabilities" {
            return false;
        }
        if serde_json::to_vec(event).map_or(true, |v| v.len() > 16384) {
            return false;
        }
        let Ok(value) = serde_json::from_value::<Capability>(event.clone()) else {
            return false;
        };
        if value.decode.len() > 16 || value.encode.len() > 64 || value.description.len() > 4096 {
            return false;
        }
        match event["output"].as_str() {
            Some("web") => self.web = value,
            Some("clink") => self.clink = value,
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_compatible_enabled_pairs_unlock_conversion_and_geometry() {
        let mut caps = Capabilities::default();
        assert!(!caps.clink.geometry(None));
        let mut event = serde_json::json!({"type":"transcode-capabilities","output":"clink","enabled":true,"available":true,"decode":["H264"],"encode":[{"codec":"H265","maxWidth":1280,"maxHeight":720}],"scaling":true,"description":"fake encoder"});
        assert!(caps.observe(&event));
        assert!(caps.clink.converts(
            "H265",
            Some(crate::display::Size {
                width: 1280,
                height: 720
            })
        ));
        assert!(!caps.clink.converts(
            "H265",
            Some(crate::display::Size {
                width: 1920,
                height: 1080
            })
        ));
        assert!(!caps.clink.geometry(None));
        event["encode"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"codec":"H264","maxWidth":1280,"maxHeight":720}));
        caps.observe(&event);
        assert!(caps.clink.geometry(None));
        event["enabled"] = false.into();
        caps.observe(&event);
        assert!(!caps.clink.geometry(None));
        event["enabled"] = true.into();
        event["decode"] = serde_json::json!(["H265"]);
        caps.observe(&event);
        assert!(!caps.clink.converts("H265", None));
        event["output"] = "other".into();
        assert!(!caps.observe(&event));
    }
}
