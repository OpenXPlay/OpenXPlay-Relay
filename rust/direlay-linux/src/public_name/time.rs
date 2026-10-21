// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-20

//! Wall-clock dates in protocol formats, and whether the wall clock can be trusted at all.

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
fn unix(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Option<u64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    u64::try_from(days * 86_400 + i64::from(hour * 3600 + minute * 60 + second)).ok()
}
fn number(text: &str) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// An RFC 3339 timestamp (`2026-10-21T05:53:00Z`, fractions and offsets allowed) in Unix seconds.
pub fn parse_rfc3339(text: &str) -> Option<u64> {
    let text = text.trim();
    let (date, time) = text.split_once(['T', 't', ' '])?;
    let mut date = date.split('-');
    let year = number(date.next()?)?;
    let month = number(date.next()?)?;
    let day = number(date.next()?)?;
    if date.next().is_some() {
        return None;
    }
    let (clock, offset) = if let Some(clock) = time.strip_suffix(['Z', 'z']) {
        (clock, 0i64)
    } else {
        let at = time.rfind(['+', '-'])?;
        let (clock, zone) = time.split_at(at);
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let (h, m) = zone[1..].split_once(':')?;
        (clock, sign * i64::from(number(h)? * 3600 + number(m)? * 60))
    };
    let clock = clock.split('.').next()?;
    let mut parts = clock.split(':');
    let hour = number(parts.next()?)?;
    let minute = number(parts.next()?)?;
    let second = number(parts.next()?)?;
    let at = unix(i64::from(year), month, day, hour, minute, second)?;
    u64::try_from(at as i64 - offset).ok()
}

/// An IMF-fixdate (`Wed, 21 Oct 2026 07:28:00 GMT`), as `Retry-After` may carry.
pub fn parse_http_date(text: &str) -> Option<u64> {
    let fields: Vec<_> = text.split_whitespace().collect();
    if fields.len() != 6 || fields[5] != "GMT" {
        return None;
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == fields[2])? as u32
        + 1;
    let mut clock = fields[4].split(':');
    unix(
        i64::from(number(fields[3])?),
        month,
        number(fields[1])?,
        number(clock.next()?)?,
        number(clock.next()?)?,
        number(clock.next()?)?,
    )
}

/// Before this date (2026-10-01) the wall clock is certainly wrong: the relay was built later.
pub const CLOCK_FLOOR: u64 = 1_790_812_800;

/// Whether the kernel considers the clock synchronised, as `timedatectl`'s `NTPSynchronized`
/// (which the clock guard reads) does: the estimated maximum error from `adjtimex` stays below
/// 16 s, the kernel's limit, only while an NTP client keeps the clock disciplined. A relay
/// without an RTC boots with the time it was switched off.
#[cfg(target_os = "linux")]
pub fn kernel_synchronized() -> bool {
    let mut timex: libc::timex = unsafe { std::mem::zeroed() };
    (unsafe { libc::adjtimex(&mut timex) }) >= 0 && timex.maxerror < 16_000_000
}
/// Development hosts keep their clock synchronised.
#[cfg(not(target_os = "linux"))]
pub fn kernel_synchronized() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_protocol_dates() {
        assert_eq!(parse_rfc3339("2026-10-21T05:53:00Z"), Some(1_792_561_980));
        assert_eq!(
            parse_rfc3339("2026-10-21T05:53:00.123456Z"),
            Some(1_792_561_980)
        );
        assert_eq!(
            parse_rfc3339("2026-10-21T07:53:00+02:00"),
            Some(1_792_561_980)
        );
        assert_eq!(parse_rfc3339("2027-01-19t05:53:00z"), Some(1_800_337_980));
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        for bad in [
            "",
            "2026-13-01T00:00:00Z",
            "2026-10-21",
            "2026-10-21T05:53Z",
            "x-10-21T05:53:00Z",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
        assert_eq!(
            parse_http_date("Wed, 21 Oct 2026 05:53:00 GMT"),
            Some(1_792_561_980)
        );
        assert_eq!(parse_http_date("Wed, 21 Okt 2026 05:53:00 GMT"), None);
        assert_eq!(parse_http_date("120"), None);
        assert_eq!(CLOCK_FLOOR, parse_rfc3339("2026-10-01T00:00:00Z").unwrap());
    }
}
