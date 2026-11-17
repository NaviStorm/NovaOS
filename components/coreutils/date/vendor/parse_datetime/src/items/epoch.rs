// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Parse a timestamp item.
//!
//! From the GNU docs:
//!
//! > If you precede a number with ‘@’, it represents an internal timestamp as
//! > a count of seconds.  The number can contain an internal decimal point
//! > (either ‘.’ or ‘,’); any excess precision not supported by the internal
//! > representation is truncated toward minus infinity.  Such a number cannot
//! > be combined with any other date item, as it specifies a complete
//! > timestamp.
//! >
//! >    On most hosts, these counts ignore the presence of leap seconds.  For
//! > example, on most hosts ‘@1483228799’ represents 2016-12-31 23:59:59 UTC,
//! > ‘@1483228800’ represents 2017-01-01 00:00:00 UTC, and there is no way to
//! > represent the intervening leap second 2016-12-31 23:59:60 UTC.

use winnow::{
    ModalResult, Parser,
    ascii::digit1,
    combinator::{opt, preceded},
    token::one_of,
};

use super::primitive::{dec_uint, plus_or_minus, s};

/// Represents a timestamp with nanosecond accuracy.
///
/// # Invariants
///
/// - `nanosecond` is always in the range of `0..1_000_000_000`.
/// - Negative timestamps are represented by a negative `second` value and a
///   positive `nanosecond` value.
#[derive(Debug, PartialEq, Clone, Copy)]
pub(super) struct Timestamp {
    second: i64,
    nanosecond: u32,
}

impl TryFrom<Timestamp> for jiff::Timestamp {
    type Error = &'static str;

    fn try_from(ts: Timestamp) -> Result<Self, Self::Error> {
        jiff::Timestamp::new(
            ts.second,
            i32::try_from(ts.nanosecond).map_err(|_| "nanosecond in timestamp exceeds i32::MAX")?,
        )
        .map_err(|_| "timestamp value is out of valid range")
    }
}

/// Parse a timestamp in the form of `@1234567890` or `@-1234567890.12345` or
/// `@1234567890,12345`.
pub(super) fn parse(input: &mut &str) -> ModalResult<Timestamp> {
    (s("@"), opt(plus_or_minus), s(sec_and_nsec).with_taken())
        .verify_map(|(_, sign, ((sec, mut nsec), raw))| {
            let mut sec = i64::try_from(sec).ok()?;
            if sign == Some('-')
                && raw
                    .trim()
                    .split_once(['.', ','])
                    .and_then(|(_, f)| f.get(9..))
                    .is_some_and(|rest| rest.bytes().any(|c| c.is_ascii_digit() && c != b'0'))
            {
                nsec += 1;
                if nsec == 1_000_000_000 {
                    sec = sec.checked_add(1)?;
                    nsec = 0
                }
            }
            let (second, nanosecond) = match (sign, nsec) {
                (Some('-'), 0) => (-sec, 0),
                // Truncate towards minus infinity.
                (Some('-'), _) => ((-sec).checked_sub(1)?, 1_000_000_000 - nsec),
                _ => (sec, nsec),
            };
            Some(Timestamp { second, nanosecond })
        })
        .parse_next(input)
}

/// Parse a second value in the form of `1234567890` or `1234567890.12345` or
/// `1234567890,12345`.
///
/// The first part represents whole seconds. The optional second part represents
/// fractional seconds, parsed as a nanosecond value from up to 9 digits
/// (padded with zeros on the right if fewer digits are present). If the second
/// part is omitted, it defaults to 0 nanoseconds.
pub(super) fn sec_and_nsec(input: &mut &str) -> ModalResult<(u64, u32)> {
    (dec_uint, opt(preceded(one_of(['.', ',']), digit1)))
        .verify_map(|(sec, opt_nsec_str)| match opt_nsec_str {
            Some(nsec_str) if nsec_str.len() >= 9 => Some((sec, nsec_str[..9].parse().ok()?)),
            Some(nsec_str) => {
                let multiplier = 10_u32.pow(9 - nsec_str.len() as u32);
                Some((sec, nsec_str.parse::<u32>().ok()?.checked_mul(multiplier)?))
            }
            None => Some((sec, 0)),
        })
        .parse_next(input)
}
