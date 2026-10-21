// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! When to retry, when to order and when to renew: pure decisions over wall-clock seconds.

pub const DAY: u64 = 86_400;
/// Network or provider failures wait 1, 5 and 30 minutes, 2 and 6 hours, then every 24 hours.
const BACKOFF: [u64; 6] = [60, 300, 1_800, 7_200, 21_600, DAY];
/// At most this many new orders per name in 24 hours.
pub const ORDERS_PER_DAY: usize = 3;
/// A certificate with more than this left is never replaced unless renewal is due.
pub const KEEP_VALID: u64 = 7 * DAY;
/// The certificate is checked this often while the relay runs.
pub const CHECK_EVERY: u64 = 12 * 3600;
/// An unchanged address is published again after this long.
pub const REPUBLISH_AFTER: u64 = DAY;
/// The head-unit page switches certificates at once, even during a session, when the one it
/// serves expires within this time.
pub const URGENT: u64 = 3 * DAY;
/// The longest `Retry-After` obeyed; longer values are taken as this.
pub const MAX_RETRY_AFTER: u64 = 30 * DAY;

/// The wait after the `failures`-th consecutive failure (counting from 1).
pub fn backoff(failures: u32) -> u64 {
    let index = (failures.max(1) as usize - 1).min(BACKOFF.len() - 1);
    BACKOFF[index]
}
/// The wait after a failure: the backoff, or the server's `Retry-After` when that is longer.
pub fn retry_delay(failures: u32, retry_after: Option<u64>) -> u64 {
    backoff(failures).max(retry_after.unwrap_or(0).min(MAX_RETRY_AFTER))
}

/// The earliest time the local guard allows another new order for a name, given when its
/// earlier orders were made. Orders dated after `now` (the clock went back) count as recent.
pub fn next_order_allowed(orders: &[u64], now: u64) -> u64 {
    let mut recent: Vec<u64> = orders.iter().copied().filter(|t| t + DAY > now).collect();
    if recent.len() < ORDERS_PER_DAY {
        return now;
    }
    recent.sort_unstable();
    (recent[recent.len() - ORDERS_PER_DAY] + DAY).max(now)
}

/// Renewal is due once less than a third of the lifetime remains.
pub fn one_third_due(not_before: u64, not_after: u64, now: u64) -> bool {
    let lifetime = not_after.saturating_sub(not_before);
    now >= not_after.saturating_sub(lifetime / 3)
}
/// A time inside the ARI suggested window (RFC 9773 section 4.2), from `random`.
pub fn ari_time(start: u64, end: u64, random: u64) -> u64 {
    if end <= start {
        start
    } else {
        start + random % (end - start)
    }
}
/// Due at the chosen ARI time when the CA suggested a window, else by the one-third rule.
pub fn renewal_due(not_before: u64, not_after: u64, now: u64, renew_at: Option<u64>) -> bool {
    match renew_at {
        Some(at) => now >= at,
        None => one_third_due(not_before, not_after, now),
    }
}
/// Whether a valid certificate may be replaced by a new order now.
pub fn may_replace(not_after: u64, now: u64, due: bool) -> bool {
    due || not_after <= now + KEEP_VALID
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_sequence_then_daily_and_retry_after_wins_when_longer() {
        let waits: Vec<u64> = (1..=8).map(backoff).collect();
        assert_eq!(waits, [60, 300, 1_800, 7_200, 21_600, DAY, DAY, DAY]);
        assert_eq!(backoff(0), 60);
        assert_eq!(retry_delay(1, Some(3_600)), 3_600);
        assert_eq!(retry_delay(3, Some(30)), 1_800);
        assert_eq!(retry_delay(1, Some(400 * DAY)), MAX_RETRY_AFTER);
        assert_eq!(retry_delay(2, None), 300);
    }
    #[test]
    fn local_order_guard_allows_three_orders_per_name_per_day() {
        let now = 1_800_000_000;
        assert_eq!(next_order_allowed(&[], now), now);
        assert_eq!(next_order_allowed(&[now - 10, now - 5], now), now);
        // The third order within 24 h is the last one until the oldest of them is a day old.
        let three = [now - 7_200, now - 3_600, now - 60];
        assert_eq!(next_order_allowed(&three, now), now - 7_200 + DAY);
        assert_eq!(
            next_order_allowed(&three, now - 7_200 + DAY),
            now - 7_200 + DAY
        );
        assert_eq!(
            next_order_allowed(&three, now - 7_200 + DAY + 1),
            now - 7_200 + DAY + 1
        );
        // Older orders do not count; orders "in the future" do.
        assert_eq!(
            next_order_allowed(&[now - 2 * DAY, now - 60, now - 30], now),
            now
        );
        assert_eq!(
            next_order_allowed(&[now + 100, now + 200, now + 300], now),
            now + 100 + DAY
        );
        // Four recent orders: wait until only two remain inside the day.
        let four = [now - 4_000, now - 3_000, now - 2_000, now - 1_000];
        assert_eq!(next_order_allowed(&four, now), now - 3_000 + DAY);
    }
    #[test]
    fn renewal_by_ari_window_or_one_third_of_the_lifetime() {
        let (issued, expires) = (1_800_000_000, 1_800_000_000 + 90 * DAY);
        assert!(!one_third_due(issued, expires, issued + 59 * DAY));
        assert!(one_third_due(issued, expires, issued + 60 * DAY));
        assert!(!renewal_due(issued, expires, issued + 59 * DAY, None));
        // A suggested window decides instead, also earlier than the one-third rule.
        let (start, end) = (issued + 30 * DAY, issued + 32 * DAY);
        for random in [0, 1, 12_345, u64::MAX] {
            let at = ari_time(start, end, random);
            assert!((start..end).contains(&at));
        }
        assert_eq!(ari_time(end, start, 7), end);
        let at = ari_time(start, end, 5);
        assert!(!renewal_due(issued, expires, at - 1, Some(at)));
        assert!(renewal_due(issued, expires, at, Some(at)));
        assert!(!renewal_due(
            issued,
            expires,
            issued + 61 * DAY,
            Some(issued + 70 * DAY)
        ));
        // More than 7 days left: kept unless due.
        assert!(!may_replace(expires, expires - 8 * DAY, false));
        assert!(may_replace(expires, expires - 8 * DAY, true));
        assert!(may_replace(expires, expires - 7 * DAY, false));
    }
}
