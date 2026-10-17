// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-16

//! Optional interpretation of mode notifications. Missing app states remain unknown.
use crate::BplistValue as V;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActivityChange {
    pub siri: Option<bool>,
    pub phone_call: Option<bool>,
    pub navigation: Option<bool>,
}

pub fn decode_modes(params: &V) -> Option<ActivityChange> {
    let modes = params.get("modes").unwrap_or(params);
    let states = modes.get("appStates")?.as_array()?;
    let mut change = ActivityChange::default();
    for state in states {
        let id = state.get("appStateID").and_then(V::as_u64);
        // Mode notifications report the owning entity: 1 = iPhone, 2 = accessory,
        // 0 = nobody. A missing or unknown owner must not clear another field.
        let owner = state.get("entity").and_then(V::as_u64);
        let owned = owner.filter(|v| *v <= 2).map(|v| v == 1);
        match id {
            Some(1) => {
                if let Some(mode) = state.get("speechMode").and_then(V::as_f64) {
                    if [-1.0, 0.0, 1.0, 2.0, 3.0].contains(&mode) {
                        change.siri = owned.map(|v| v && mode > 0.0);
                    }
                }
            }
            Some(2) => change.phone_call = owned,
            Some(3) => change.navigation = owned,
            _ => {}
        }
    }
    (change != ActivityChange::default()).then_some(change)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_unknown_and_ownership() {
        let modes = V::dict([(
            "appStates",
            V::Array(vec![
                V::dict([("appStateID", 3u64.into()), ("entity", 1u64.into())]),
                V::dict([("appStateID", 2u64.into()), ("entity", 2u64.into())]),
                V::dict([("appStateID", 99u64.into()), ("entity", 1u64.into())]),
            ]),
        )]);
        let bytes = crate::BplistCodec::encode(&modes).unwrap();
        assert_eq!(
            decode_modes(&crate::BplistCodec::decode(&bytes).unwrap()),
            Some(ActivityChange {
                navigation: Some(true),
                phone_call: Some(false),
                siri: None,
            })
        );
        assert_eq!(decode_modes(&V::Null), None);
        assert!(crate::BplistCodec::decode(&bytes[..bytes.len() - 1]).is_err());
    }
}
