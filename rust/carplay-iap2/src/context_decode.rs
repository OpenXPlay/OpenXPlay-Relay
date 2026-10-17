// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Created by riko chen on 2026-10-16

//! Optional state decoding; protocol values only, independent of Stream.
//! Field ids follow DiPlay: CarPlayPlaybackStatus.kt, BydClusterSong.kt and BydCarPlayCall.kt.
use crate::{Iap2BodyReader, Iap2Frame, Result};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowPlayingChange {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u32>,
    pub elapsed_ms: Option<u32>,
    pub status: Option<u8>,
    /// None means absent; Some(None) explicitly clears the source.
    pub source_app: Option<Option<String>>,
}

pub fn now_playing(frame: &Iap2Frame) -> Result<Option<NowPlayingChange>> {
    if frame.message_id != 0x5001 {
        return Ok(None);
    }
    let body = Iap2BodyReader::of(frame)?;
    let mut change = NowPlayingChange::default();
    if let Some(media) = body.optional_group(0)? {
        change.title = media.optional_string(1)?;
        change.duration_ms = media.optional_u32(4)?;
        change.album = media.optional_string(6)?;
        change.artist = media.optional_string(12)?;
    }
    if let Some(playback) = body.optional_group(1)? {
        change.status = playback.optional_u8(0)?;
        change.elapsed_ms = playback.optional_u32(1)?;
        change.source_app = playback.optional_string(7)?.map(|s| {
            let s = s.trim();
            (!s.is_empty()).then(|| s.to_owned())
        });
    }
    Ok((change != NowPlayingChange::default()).then_some(change))
}

/// Caller identity is deliberately not decoded.
pub fn call_status(frame: &Iap2Frame) -> Result<Option<u8>> {
    if frame.message_id != 0x4155 {
        return Ok(None);
    }
    Ok(Iap2BodyReader::of(frame)?
        .optional_u8(2)?
        .filter(|s| *s <= 6))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(id: u16, bytes: &[u8]) -> Vec<u8> {
        let mut out = ((bytes.len() + 4) as u16).to_be_bytes().to_vec();
        out.extend(id.to_be_bytes());
        out.extend(bytes);
        out
    }
    #[test]
    fn context_catalog_contains_only_supported_fields() {
        for id in [0x5001, 0x4155] {
            assert!(crate::Iap2Endpoints::by_id(id).unwrap().fields.is_empty());
        }
        let call = crate::Iap2Endpoints::context_by_id(0x4155).unwrap();
        assert_eq!(call.fields.len(), 1);
        assert_eq!(call.fields[0].id, 2);
        assert_eq!(call.fields[0].wire_type, crate::Iap2WireType::U8);
        let endpoint = crate::Iap2Endpoints::context_by_id(0x5001).unwrap();
        assert_eq!(
            endpoint.fields.iter().map(|f| f.id).collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(
            endpoint.fields[0]
                .children
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            [1, 4, 6, 12]
        );
        assert_eq!(
            endpoint.fields[1]
                .children
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            [0, 1, 7]
        );
    }
    #[test]
    fn source_app_absent_trim_empty_and_malformed() {
        for (bytes, expected) in [
            (b" Music \0".as_slice(), Some("Music")),
            (b" \0".as_slice(), None),
        ] {
            let frame = Iap2Frame {
                message_id: 0x5001,
                payload: field(1, &field(7, bytes)),
            };
            assert_eq!(
                now_playing(&frame).unwrap().unwrap().source_app,
                Some(expected.map(str::to_owned))
            );
        }
        let mut frame = Iap2Frame {
            message_id: 0x5001,
            payload: field(1, &field(0, &[1])),
        };
        assert_eq!(now_playing(&frame).unwrap().unwrap().source_app, None);
        frame.payload = field(1, &field(7, b"unterminated"));
        assert!(now_playing(&frame).is_err());
        frame.payload = field(1, &[0, 9, 0, 7, b'x']);
        assert!(now_playing(&frame).is_err());
    }
    #[test]
    fn playback_status_is_preserved_for_relay_mapping() {
        for status in [0, 1, 2, 3, 4, 255] {
            let frame = Iap2Frame {
                message_id: 0x5001,
                payload: field(1, &field(0, &[status])),
            };
            assert_eq!(now_playing(&frame).unwrap().unwrap().status, Some(status));
        }
    }
    #[test]
    fn catalog_vectors_partial_unknown_and_truncated() {
        let mut media = field(1, b"Track\0");
        media.extend(field(4, &120_000u32.to_be_bytes()));
        media.extend(field(6, b"Album\0"));
        media.extend(field(12, b"Artist\0"));
        media.extend(field(99, &[8]));
        let mut playback = field(0, &[1]);
        playback.extend(field(1, &5000u32.to_be_bytes()));
        playback.extend(field(99, &[8]));
        let mut payload = field(0, &media);
        payload.extend(field(1, &playback));
        let mut frame = Iap2Frame {
            message_id: 0x5001,
            payload,
        };
        let n = now_playing(&frame).unwrap().unwrap();
        assert_eq!(n.title.as_deref(), Some("Track"));
        assert_eq!(n.artist.as_deref(), Some("Artist"));
        assert_eq!(n.album.as_deref(), Some("Album"));
        assert_eq!(n.duration_ms, Some(120_000));
        assert_eq!(n.elapsed_ms, Some(5000));
        assert_eq!(n.status, Some(1));
        frame.payload = field(1, &field(1, &6000u32.to_be_bytes()));
        assert_eq!(
            now_playing(&frame).unwrap().unwrap(),
            NowPlayingChange {
                elapsed_ms: Some(6000),
                ..Default::default()
            }
        );
        frame.payload = field(0, &[0, 8, 0, 1, b'x']);
        assert!(now_playing(&frame).is_err());
        frame.payload = field(99, &[1]);
        assert_eq!(now_playing(&frame).unwrap(), None);
        for value in 0..=7 {
            frame = Iap2Frame {
                message_id: 0x4155,
                payload: field(2, &[value]),
            };
            assert_eq!(call_status(&frame).unwrap(), (value <= 6).then_some(value));
        }
        frame.payload = field(0, b"identity\0");
        assert_eq!(call_status(&frame).unwrap(), None);
    }
}
