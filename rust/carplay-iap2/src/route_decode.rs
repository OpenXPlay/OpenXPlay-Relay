// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-16

//! Protocol observations from DiPlay's BydHudRouteState.kt and FceNavigationState.
//! No destination-name or display-unit encoding is established by those readers.
use crate::{Iap2BodyReader, Iap2Frame, Result};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteChange {
    pub state: Option<u8>,
    pub current_road: Option<String>,
    pub remaining_seconds: Option<u64>,
    pub remaining_meters: Option<u32>,
    pub maneuver_distance_meters: Option<u32>,
    /// None means absent; an empty list explicitly removes the current maneuver.
    pub current_maneuvers: Option<Vec<u16>>,
    pub app_name: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManeuverChange {
    pub index: Option<u16>,
    pub kind: Option<u8>,
    pub after_road: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteObservation {
    Route(RouteChange),
    Maneuver(ManeuverChange),
}

/// Decode atomically: any malformed used field rejects the entire observation.
pub fn decode(frame: &Iap2Frame) -> Result<Option<RouteObservation>> {
    if !matches!(frame.message_id, 0x5201 | 0x5202) {
        return Ok(None);
    }
    let b = Iap2BodyReader::of(frame)?;
    // Duplicate known fields are ambiguous, even when one would otherwise be usable.
    let known: &[u16] = if frame.message_id == 0x5201 {
        &[1, 3, 6, 7, 10, 13, 19]
    } else {
        &[1, 3, 4]
    };
    for id in known {
        if b.all(*id).len() > 1 {
            return Err(crate::invalid("Duplicate route field"));
        }
    }
    let text = |id| -> Result<Option<String>> {
        b.optional_bytes(id)?
            .map(|bytes| {
                let text = bytes
                    .strip_suffix(&[0])
                    .ok_or_else(|| crate::invalid("Unterminated route text"))?;
                if text.contains(&0) {
                    return Err(crate::invalid("Embedded NUL in route text"));
                }
                std::str::from_utf8(text)
                    .map(|s| s.trim().to_owned())
                    .map_err(|_| crate::invalid("Invalid route UTF-8"))
            })
            .transpose()
    };
    if frame.message_id == 0x5201 {
        let r = RouteChange {
            state: b.optional_u8(1)?,
            current_road: text(3)?,
            remaining_seconds: b.optional_u64(6)?,
            remaining_meters: b.optional_u32(7)?,
            maneuver_distance_meters: b.optional_u32(10)?,
            current_maneuvers: if b.has(13) {
                Some(b.u16_list(13)?)
            } else {
                None
            },
            app_name: text(19)?,
        };
        Ok((r != RouteChange::default()).then_some(RouteObservation::Route(r)))
    } else {
        let m = ManeuverChange {
            index: b.optional_u16(1)?,
            kind: b.optional_u8(3)?,
            after_road: text(4)?,
        };
        Ok((m != ManeuverChange::default()).then_some(RouteObservation::Maneuver(m)))
    }
}

/// Complete protocol state assembled from partial observations. The cache never exceeds
/// the eight maneuver records advertised in identification.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteSnapshot {
    pub active: bool,
    pub app_name: Option<String>,
    pub remaining_seconds: Option<u64>,
    pub remaining_meters: Option<u32>,
    pub maneuver: Option<ManeuverSnapshot>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManeuverSnapshot {
    pub kind: Option<u8>,
    pub road: Option<String>,
    pub distance_meters: Option<u32>,
}
#[derive(Default)]
pub struct RouteState {
    state: Option<u8>,
    app: Option<String>,
    road: Option<String>,
    seconds: Option<u64>,
    meters: Option<u32>,
    distance: Option<u32>,
    current: Option<u16>,
    maneuvers: std::collections::VecDeque<ManeuverChange>,
}
impl RouteState {
    pub fn accept(&mut self, observation: RouteObservation) {
        match observation {
            RouteObservation::Route(r) => {
                if let Some(state) = r.state {
                    self.state = Some(state);
                }
                if let Some(app) = r.app_name {
                    self.app = Some(app);
                }
                if matches!(self.state, Some(0 | 2)) || self.app.as_deref() == Some("") {
                    self.road = None;
                    self.seconds = None;
                    self.meters = None;
                    self.distance = None;
                    self.current = None;
                    self.maneuvers.clear();
                    return;
                }
                if let Some(road) = r.current_road {
                    self.road = Some(road);
                }
                if r.remaining_seconds.is_some() {
                    self.seconds = r.remaining_seconds;
                }
                if r.remaining_meters.is_some() {
                    self.meters = r.remaining_meters;
                }
                if r.maneuver_distance_meters.is_some() {
                    self.distance = r.maneuver_distance_meters;
                }
                if let Some(indices) = r.current_maneuvers {
                    let current = indices.first().copied();
                    if self.current != current && r.maneuver_distance_meters.is_none() {
                        self.distance = None;
                    }
                    self.current = current;
                }
            }
            RouteObservation::Maneuver(m) => {
                let Some(index) = m.index else {
                    return;
                };
                if let Some(cached) = self.maneuvers.iter_mut().find(|v| v.index == Some(index)) {
                    if m.kind.is_some() {
                        cached.kind = m.kind;
                    }
                    if m.after_road.is_some() {
                        cached.after_road = m.after_road;
                    }
                } else {
                    if self.maneuvers.len() == 8 {
                        self.maneuvers.pop_front();
                    }
                    self.maneuvers.push_back(m);
                }
            }
        }
    }
    pub fn snapshot(&self) -> RouteSnapshot {
        // FceNavigationState: an empty source stops guidance; a source alone can start it.
        let active = self.app.as_deref() != Some("")
            && match self.state {
                Some(s) => matches!(s, 1 | 3 | 4 | 5 | 6),
                None => self.app.is_some() || self.current.is_some(),
            };
        if !active {
            return RouteSnapshot::default();
        }
        let maneuver = self.current.map(|index| {
            let cached = self.maneuvers.iter().find(|v| v.index == Some(index));
            ManeuverSnapshot {
                kind: cached.and_then(|v| v.kind),
                road: cached
                    .and_then(|v| v.after_road.clone())
                    .filter(|s| !s.is_empty())
                    .or_else(|| self.road.clone().filter(|s| !s.is_empty())),
                distance_meters: self.distance,
            }
        });
        RouteSnapshot {
            active,
            app_name: self.app.clone().filter(|s| !s.is_empty()),
            remaining_seconds: self.seconds,
            remaining_meters: self.meters,
            maneuver,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(id: u16, fields: &[(u16, &[u8])]) -> Iap2Frame {
        let mut payload = Vec::new();
        for (id, value) in fields {
            payload.extend(((value.len() + 4) as u16).to_be_bytes());
            payload.extend(id.to_be_bytes());
            payload.extend(*value);
        }
        Iap2Frame {
            message_id: id,
            payload,
        }
    }
    #[test]
    fn state_cache_is_bounded_and_partial_maneuvers_merge() {
        let mut state = RouteState::default();
        for index in 0..100 {
            state.accept(RouteObservation::Maneuver(ManeuverChange {
                index: Some(index),
                kind: Some(2),
                after_road: None,
            }));
        }
        assert_eq!(state.maneuvers.len(), 8);
        state.accept(RouteObservation::Route(RouteChange {
            state: Some(1),
            current_maneuvers: Some(vec![99]),
            ..Default::default()
        }));
        state.accept(RouteObservation::Maneuver(ManeuverChange {
            index: Some(99),
            after_road: Some("Next St".into()),
            ..Default::default()
        }));
        let maneuver = state.snapshot().maneuver.unwrap();
        assert_eq!(maneuver.kind, Some(2));
        assert_eq!(maneuver.road.as_deref(), Some("Next St"));
        for value in 0..=7 {
            state.accept(RouteObservation::Route(RouteChange {
                state: Some(value),
                ..Default::default()
            }));
            assert_eq!(state.snapshot().active, matches!(value, 1 | 3 | 4 | 5 | 6));
        }
    }
    #[test]
    fn diplay_vectors() {
        let f = frame(
            0x5201,
            &[
                (1, &[1]),
                (3, b" Main St\0"),
                (6, &900u64.to_be_bytes()),
                (7, &12000u32.to_be_bytes()),
                (10, &250u32.to_be_bytes()),
                (13, &[0, 7, 0, 8]),
                (19, b"Maps\0"),
                (99, &[255]),
            ],
        );
        assert_eq!(
            decode(&f).unwrap(),
            Some(RouteObservation::Route(RouteChange {
                state: Some(1),
                current_road: Some("Main St".into()),
                remaining_seconds: Some(900),
                remaining_meters: Some(12000),
                maneuver_distance_meters: Some(250),
                current_maneuvers: Some(vec![7, 8]),
                app_name: Some("Maps".into()),
            }))
        );
        assert_eq!(
            decode(&frame(
                0x5202,
                &[(1, &[0, 7]), (3, &[2]), (4, b"Next St\0")]
            ))
            .unwrap(),
            Some(RouteObservation::Maneuver(ManeuverChange {
                index: Some(7),
                kind: Some(2),
                after_road: Some("Next St".into())
            }))
        );
        assert_eq!(
            decode(&frame(0x5201, &[(13, &[])])).unwrap(),
            Some(RouteObservation::Route(RouteChange {
                current_maneuvers: Some(vec![]),
                ..Default::default()
            }))
        );
    }
    #[test]
    fn malformed_unknown_and_partial() {
        assert_eq!(decode(&frame(0x5201, &[(99, &[1])])).unwrap(), None);
        assert!(decode(&frame(0x5201, &[(13, &[0])])).is_err());
        assert!(decode(&frame(0x5201, &[(6, &[0; 4])])).is_err());
        assert!(decode(&frame(0x5202, &[(3, &[1, 2])])).is_err());
        assert!(decode(&frame(0x5201, &[(1, &[1]), (1, &[0])])).is_err());
        assert!(decode(&frame(0x5201, &[(3, &[255, 0])])).is_err());
        let mut f = frame(0x5201, &[(1, &[1])]);
        f.payload.push(0);
        assert!(decode(&f).is_err());
        assert_eq!(
            decode(&frame(0x5201, &[(1, &[2])])).unwrap(),
            Some(RouteObservation::Route(RouteChange {
                state: Some(2),
                ..Default::default()
            }))
        );
    }
}
