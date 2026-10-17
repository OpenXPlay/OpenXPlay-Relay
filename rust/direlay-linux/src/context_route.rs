// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Fresh model translation. Stream integration is confined to `stream_update`.
use carplay_iap2::route_decode::RouteSnapshot;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteContext {
    pub active: bool,
    pub destination: Option<String>,
    pub eta_ms: Option<u64>,
    pub distance_remaining_m: Option<u32>,
    pub maneuver: Option<RouteManeuver>,
    pub app_name: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RouteManeuver {
    pub r#type: Option<u8>,
    pub road: Option<String>,
    pub distance_m: Option<u32>,
}
impl From<RouteSnapshot> for RouteContext {
    fn from(r: RouteSnapshot) -> Self {
        Self {
            active: r.active,
            destination: None,
            eta_ms: r
                .remaining_seconds
                .and_then(|s| s.checked_mul(1000))
                .filter(|ms| *ms <= 9_007_199_254_740_991),
            distance_remaining_m: r.remaining_meters,
            maneuver: r.maneuver.map(|m| RouteManeuver {
                r#type: m.kind,
                road: m.road,
                distance_m: m.distance_meters,
            }),
            app_name: r.app_name,
        }
    }
}
/// Map a known route snapshot without inventing values for unknown fields.
pub(crate) fn stream_update(route: &RouteContext) -> oxp_core::ContextUpdate {
    oxp_core::ContextUpdate::Route(Some(oxp_core::RouteContext {
        active: route.active,
        destination: route.destination.clone(),
        eta_ms: route.eta_ms.map(|n| n as f64),
        distance_remaining_m: route.distance_remaining_m.map(f64::from),
        maneuver: route.maneuver.as_ref().map(|m| oxp_core::RouteManeuver {
            r#type: m.r#type,
            road: m.road.clone(),
            distance_m: m.distance_m.map(f64::from),
        }),
        app_name: route.app_name.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapter_preserves_partial_and_inactive_snapshots() {
        for active in [false, true] {
            let mut route = RouteContext {
                active,
                ..Default::default()
            };
            let update = stream_update(&route);
            assert_eq!(
                update,
                oxp_core::ContextUpdate::from_json(
                    &serde_json::json!({"topic":"route","value":route})
                )
                .unwrap()
            );
            route.maneuver = Some(RouteManeuver {
                r#type: Some(2),
                road: None,
                distance_m: Some(10),
            });
            let update = stream_update(&route);
            assert_eq!(
                update,
                oxp_core::ContextUpdate::from_json(
                    &serde_json::json!({"topic":"route","value":route})
                )
                .unwrap()
            );
        }
    }
    #[test]
    fn model_units_nulls_and_safe_numbers() {
        let mut r = RouteSnapshot {
            active: true,
            remaining_seconds: Some(90),
            remaining_meters: Some(1000),
            app_name: Some("Maps".into()),
            maneuver: Some(carplay_iap2::route_decode::ManeuverSnapshot {
                kind: Some(2),
                road: Some("Main St".into()),
                distance_meters: Some(250),
            }),
        };
        let mapped = RouteContext::from(r.clone());
        assert_eq!(
            serde_json::to_value(&mapped).unwrap(),
            serde_json::json!({
                "active":true,"destination":null,"etaMs":90000,"distanceRemainingM":1000,
                "maneuver":{"type":2,"road":"Main St","distanceM":250},"appName":"Maps"
            })
        );
        assert_eq!(
            stream_update(&mapped),
            oxp_core::ContextUpdate::from_json(
                &serde_json::json!({"topic":"route","value":mapped})
            )
            .unwrap()
        );
        for seconds in [u64::MAX, 9_007_199_254_741] {
            r.remaining_seconds = Some(seconds);
            assert_eq!(RouteContext::from(r.clone()).eta_ms, None);
        }
        assert_eq!(
            serde_json::to_value(RouteContext::default()).unwrap(),
            serde_json::json!({
                "active":false,"destination":null,"etaMs":null,"distanceRemainingM":null,
                "maneuver":null,"appName":null
            })
        );
    }
}
