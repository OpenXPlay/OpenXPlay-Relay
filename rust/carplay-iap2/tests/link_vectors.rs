// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.
mod common;
use carplay_iap2::*;
use common::*;
use std::time::{Duration, Instant};
fn setup(id: u8, size: u64, kind: u16) -> Vec<u8> {
    [
        vec![id, 4],
        size.to_be_bytes().to_vec(),
        kind.to_be_bytes().to_vec(),
    ]
    .concat()
}
#[test]
fn routes_and_replies_on_file_transfer_session() {
    let mut e = ready_engine();
    let d = setup(0x81, 5, 2);
    e.feed(&packet(0x40, 2, 99, 12, Some(&d)), 2).unwrap();
    assert_eq!(e.poll_event(), Some(LinkEvent::Session(12, d)));
    e.take_output();
    e.send_session(12, &[0x81, 1], 3).unwrap();
    let b = e.take_output();
    assert_eq!(b[7], 12);
    assert_eq!(b[9..11], [0x81, 1]);
}
#[test]
fn accepts_and_reassembles_artwork() {
    let mut r = Iap2FileTransferReceiver::new(1024);
    assert_eq!(
        r.accept(&setup(0x81, 5, 2)).unwrap().replies,
        vec![vec![0x81, 1]]
    );
    assert!(r.accept(&[0x81, 0x80, 1, 2, 3]).unwrap().transfer.is_none());
    let o = r.accept(&[0x81, 0x40, 4, 5]).unwrap();
    assert_eq!(o.replies, vec![vec![0x81, 5]]);
    assert_eq!(o.transfer.unwrap().bytes, [1, 2, 3, 4, 5]);
}
#[test]
fn rejects_oversized_unsupported_and_truncated_transfers() {
    let mut r = Iap2FileTransferReceiver::new(4);
    let o = r.accept(&setup(1, 5, 2)).unwrap();
    assert_eq!(o.replies, vec![vec![1, 2]]);
    assert!(o.transfer.unwrap().bytes.is_empty());
    let o = r.accept(&setup(2, 3, 7)).unwrap();
    assert_eq!(o.replies, vec![vec![2, 2]]);
    assert!(o.transfer.is_none());
    assert!(r.accept(&[4, 4]).unwrap().transfer.is_none());
    r.accept(&setup(3, 4, 2)).unwrap();
    let o = r.accept(&[3, 0x40, 1, 2]).unwrap();
    assert_eq!(o.replies, vec![vec![3, 2]]);
    assert!(o.transfer.unwrap().bytes.is_empty());
}
#[test]
fn sender_cancel_clears_only_pending_transfer() {
    let mut r = Iap2FileTransferReceiver::new(4);
    r.accept(&setup(5, 4, 2)).unwrap();
    assert_eq!(r.accept(&[5, 2]).unwrap().transfer.unwrap().id, 5);
    assert!(r.accept(&[5, 2]).unwrap().transfer.is_none());
}
#[test]
fn zero_sized_artwork_clears_current_image() {
    let o = Iap2FileTransferReceiver::new(4)
        .accept(&setup(128, 0, 2))
        .unwrap();
    assert_eq!(o.replies, vec![vec![128, 1], vec![128, 5]]);
    assert!(o.transfer.unwrap().bytes.is_empty());
}
#[test]
fn pending_artwork_is_bounded_and_oldest_evicted() {
    let mut r = Iap2FileTransferReceiver::new(4);
    for id in 0..5 {
        r.accept(&setup(id, 4, 2)).unwrap();
    }
    assert!(r.accept(&[0, 0x40, 1, 2, 3, 4]).unwrap().transfer.is_none());
    assert!(r.accept(&[4, 0x40, 1, 2, 3, 4]).unwrap().transfer.is_some());
}
#[test]
fn authenticated_drive_survives_full_day() {
    let now = Instant::now();
    let mut d = Iap2ControlDeadline::new(Duration::MAX, now);
    d.authenticated();
    for minutes in [6, 90, 25 * 60] {
        assert_eq!(
            d.remaining_millis(now + Duration::from_secs(minutes * 60)),
            30000
        );
    }
}
#[test]
fn unlimited_drive_has_bounded_handshake() {
    let now = Instant::now();
    let d = Iap2ControlDeadline::new(Duration::MAX, now);
    assert_eq!(d.remaining_millis(now + Duration::from_millis(59999)), 1);
    assert_eq!(d.remaining_millis(now + Duration::from_secs(60)), 0);
}
#[test]
fn finite_window_expires_after_authentication() {
    let now = Instant::now();
    let mut d = Iap2ControlDeadline::new(Duration::from_millis(100), now);
    d.authenticated();
    assert_eq!(d.remaining_millis(now + Duration::from_millis(100)), 0);
}
#[test]
fn periodic_updates_are_thinned_independently() {
    let mut log = Iap2RxLog::new(2, 5);
    let values = (1..=10).map(|_| log.line(0x5001)).collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![
            Some("iap2 rx=0x5001".into()),
            Some("iap2 rx=0x5001".into()),
            None,
            None,
            Some("iap2 rx=0x5001 count=5".into()),
            None,
            None,
            None,
            None,
            Some("iap2 rx=0x5001 count=10".into())
        ]
    );
    for _ in 0..10 {
        assert_eq!(log.line(0x4158).as_deref(), Some("iap2 rx=0x4158"));
    }
    assert_eq!(log.line(0x5201).as_deref(), Some("iap2 rx=0x5201"));
}
#[test]
fn marker_and_sync_retry_deadlines_match_source() {
    let mut e = Iap2LinkEngine::new(Default::default());
    e.start(false, 0).unwrap();
    assert_eq!(hex(&e.take_output()), "ff550200ee10");
    e.advance_time(999).unwrap();
    assert!(e.take_output().is_empty());
    e.advance_time(1000).unwrap();
    assert_eq!(hex(&e.take_output()), "ff550200ee10");
    e.feed(&unhex("ff550200ee10"), 1001).unwrap();
    let syn = e.take_output();
    assert_eq!(syn[4], 0x80);
    assert_eq!(checksum(&syn[..9]), 0);
    assert_eq!(checksum(&syn[9..]), 0);
    e.advance_time(1501).unwrap();
    assert_eq!(e.take_output(), syn);
}
#[test]
fn detects_wrong_marker_and_reset() {
    let mut e = Iap2LinkEngine::new(Default::default());
    e.start(false, 0).unwrap();
    e.feed(&[0; 6], 1).unwrap();
    assert_eq!(
        e.poll_event(),
        Some(LinkEvent::Dead(Some("iAP2 marker was not received".into())))
    );
    let mut e = ready_engine();
    e.feed(&packet(0x10, 2, 99, 0, None), 2).unwrap();
    assert_eq!(e.state(), LinkState::Dead);
}
#[test]
fn retransmissions_stop_at_peer_limit() {
    let mut e = ready_engine();
    e.send_control(&[1, 2], 10).unwrap();
    let first = e.take_output();
    for time in [2010, 4010, 6010] {
        e.advance_time(time).unwrap();
        assert_eq!(e.take_output(), first);
    }
    e.advance_time(8010).unwrap();
    assert_eq!(
        e.poll_event(),
        Some(LinkEvent::Dead(Some(
            "iAP2 packet 100 was not acknowledged".into()
        )))
    );
}
#[test]
fn cumulative_ack_retires_all_covered_packets() {
    let mut e = ready_engine();
    for _ in 0..3 {
        e.send_control(&[1], 10).unwrap();
    }
    e.take_output();
    e.feed(&packet(0x40, 1, 102, 0, None), 20).unwrap();
    e.take_output();
    e.advance_time(9999).unwrap();
    assert!(e.take_output().is_empty());
    assert_ne!(e.state(), LinkState::Dead);
}
#[test]
fn out_of_order_data_delivered_in_sequence_and_duplicates_acked() {
    let mut e = ready_engine();
    e.feed(&packet(0x40, 3, 99, 10, Some(&[3])), 2).unwrap();
    assert!(e.poll_event().is_none());
    e.feed(&packet(0x40, 2, 99, 10, Some(&[2])), 3).unwrap();
    assert_eq!(e.poll_event(), Some(LinkEvent::Control(vec![2])));
    assert_eq!(e.poll_event(), Some(LinkEvent::Control(vec![3])));
    e.feed(&packet(0x40, 2, 99, 10, Some(&[2])), 4).unwrap();
    assert!(e.poll_event().is_none());
}
#[test]
fn extended_ack_retransmits_missing_sequences_only() {
    let mut e = ready_engine();
    e.send_control(&[1], 3).unwrap();
    e.send_control(&[2], 3).unwrap();
    e.take_output();
    e.feed(&packet(0x20, 1, 99, 0, Some(&[100])), 4).unwrap();
    let b = e.take_output();
    assert_eq!(b[5], 100);
    assert_eq!(b.len(), 11);
}
#[test]
fn sequence_wrap_is_unsigned() {
    let mut e = ready_engine();
    for i in 0..300u16 {
        e.send_control(&[1], i as u64 + 10).unwrap();
        let b = e.take_output();
        let seq = 100u8.wrapping_add(i as u8);
        assert_eq!(b[5], seq);
        e.feed(&packet(0x40, 1, seq, 0, None), i as u64 + 11)
            .unwrap();
        e.take_output();
    }
    assert!(e.writable());
}
#[test]
fn peer_limit_is_enforced_before_sending() {
    let mut e = ready_engine();
    assert!(e.send_control(&vec![0; 4087], 2).is_err());
    assert!(e.send_control(&vec![0; 4086], 2).is_ok());
}
#[test]
fn link_bounds_output_queue_and_events() {
    let c = Iap2LinkConfig {
        maximum_pending_output_bytes: 1,
        ..Default::default()
    };
    let mut e = Iap2LinkEngine::new(c);
    e.start(false, 0).unwrap();
    assert_eq!(e.state(), LinkState::Dead);
    let c = Iap2LinkConfig {
        maximum_queued_packets: 1,
        ..Default::default()
    };
    let mut e = Iap2LinkEngine::new(c);
    e.send_control(&[1], 0).unwrap();
    e.send_control(&[2], 0).unwrap();
    assert_eq!(e.state(), LinkState::Dead);
}
#[test]
fn checksum_failure_never_delivers_payload() {
    let mut e = ready_engine();
    let mut b = packet(0x40, 2, 99, 10, Some(&[1]));
    *b.last_mut().unwrap() ^= 1;
    e.feed(&b, 2).unwrap();
    assert!(e.poll_event().is_none());
}
