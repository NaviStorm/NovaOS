// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Parse an offset item.
//!
//! From the GNU docs:
//!
//! > A “time zone item” specifies an international time zone, indicated by a
//! > small set of letters, e.g., ‘UTC’ or ‘Z’ for Coordinated Universal Time.
//! > Any included periods are ignored.  By following a non-daylight-saving
//! > time zone by the string ‘DST’ in a separate word (that is, separated by
//! > some white space), the corresponding daylight saving time zone may be
//! > specified.  Alternatively, a non-daylight-saving time zone can be
//! > followed by a time zone correction, to add the two values.  This is
//! > normally done only for ‘UTC’; for example, ‘UTC+05:30’ is equivalent to
//! > ‘+05:30’.
//! >
//! >    Time zone items other than ‘UTC’ and ‘Z’ are obsolescent and are not
//! > recommended, because they are ambiguous; for example, ‘EST’ has a
//! > different meaning in Australia than in the United States, and ‘A’ has
//! > different meaning as a military time zone than as an obsolete RFC 822
//! > time zone.  Instead, it's better to use unambiguous numeric time zone
//! > corrections like ‘-0500’, as described in the previous section.
//! >
//! >    If neither a time zone item nor a time zone correction is supplied,
//! > timestamps are interpreted using the rules of the default time zone.

use core::fmt::Display;

use winnow::{
    ModalResult, Parser,
    combinator::{alt, peek},
    error::{ContextError, ErrMode},
    stream::{AsChar, Stream},
    token::take_while,
};

use super::{
    primitive::{colon, ctx_err, dec_uint, dec_uint_str, plus_or_minus, s},
    relative,
};

/// Represents a time zone offset from UTC.
///
/// This struct is used to represent a time zone offset in hours and minutes,
/// with a boolean indicating whether the offset is negative (i.e., west of
/// UTC).
#[derive(PartialEq, Debug, Clone, Default)]
pub(super) struct Offset {
    negative: bool,
    hours: u8,
    minutes: u8,
}

impl Offset {
    /// Merge two timezone offsets.
    ///
    /// Note: when parsing an offset from a string (e.g., "+08:00"), the hours
    /// and minutes are validated to ensure they fall within valid bounds. In
    /// contrast, merging two offsets does not perform such validation. This
    /// behavior is intentional to match GNU date.
    fn merge(self, offset: Offset) -> Offset {
        fn combine(a: u16, neg_a: bool, b: u16, neg_b: bool) -> (u16, bool) {
            if neg_a == neg_b {
                (a + b, neg_a)
            } else if a > b {
                (a - b, neg_a)
            } else {
                (b - a, neg_b)
            }
        }
        let (total_minutes, negative) = combine(
            (self.hours as u16) * 60 + (self.minutes as u16),
            self.negative,
            (offset.hours as u16) * 60 + (offset.minutes as u16),
            offset.negative,
        );
        let hours = (total_minutes / 60) as u8;
        let minutes = (total_minutes % 60) as u8;

        Offset {
            negative,
            hours,
            minutes,
        }
    }

    /// Normalize the offset so that the hour field is within the accepted range.
    ///
    /// - If the hour field is less than 24, or exactly 24 with a zero minute,
    ///   the offset is already normalized, and the function returns the offset
    ///   itself along with a zero hour adjustment.
    /// - Otherwise, the hour field is reduced to 23 while preserving the minute
    ///   field, and the function returns the normalized offset along with the
    ///   hour adjustment needed to reach the original offset.
    pub(super) fn normalize(self) -> (Offset, i8) {
        if self.hours < 24 || (self.hours == 24 && self.minutes == 0) {
            return (self, 0);
        }

        let hour_adjustment = (self.hours as i8 - 23) * if self.negative { 1 } else { -1 };
        (
            Offset {
                negative: self.negative,
                hours: 23,
                minutes: self.minutes,
            },
            hour_adjustment,
        )
    }

    pub(super) fn total_seconds(&self) -> i32 {
        let secs = (self.hours as i32) * 3600 + (self.minutes as i32) * 60;
        if self.negative { -secs } else { secs }
    }
}

impl TryFrom<(bool, u8, u8)> for Offset {
    type Error = &'static str;

    fn try_from((negative, hours, minutes): (bool, u8, u8)) -> Result<Self, Self::Error> {
        if hours > 24 {
            return Err("timezone hour must be between 0 and 24");
        }
        if u16::from(hours) * 60 + u16::from(minutes) > 1440 {
            return Err("timezone minute must be between 0 and 60");
        }

        Ok(Offset {
            negative,
            hours,
            minutes,
        })
    }
}

impl TryFrom<&Offset> for jiff::tz::TimeZone {
    type Error = &'static str;

    fn try_from(
        Offset {
            negative,
            hours,
            minutes,
        }: &Offset,
    ) -> Result<Self, Self::Error> {
        let secs = (*hours as i32) * 3600 + (*minutes as i32) * 60;
        let secs = if *negative { -secs } else { secs };

        let offset = jiff::tz::Offset::from_seconds(secs).map_err(|_| "offset is invalid")?;
        Ok(jiff::tz::TimeZone::fixed(offset))
    }
}

impl Display for Offset {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(
            fmt,
            "{}{:02}:{:02}",
            if self.negative { "-" } else { "+" },
            self.hours,
            self.minutes
        )
    }
}

/// Timezone abbreviations are assumed to be no longer than 6 characters.
const MAX_TZ_SIZE: usize = 6;

pub(super) fn parse(input: &mut &str) -> ModalResult<Offset> {
    timezone_name_offset.parse_next(input)
}

/// Parse the military timezone letter `j`.
///
/// Every other military letter denotes a fixed offset from UTC, but GNU `date`
/// defines `j` as local time, which cannot be expressed as an [`Offset`] at
/// all. It is therefore reported to the builder as its own item.
///
/// The whole alphabetic word is consumed before comparing, so `jan` is left to
/// the date parser rather than being read as `j` followed by `an`, and `jj` is
/// rejected rather than matching a `j` prefix.
pub(super) fn parse_local(input: &mut &str) -> ModalResult<()> {
    s(take_while(1..=MAX_TZ_SIZE, AsChar::is_alpha))
        .verify(|word: &str| word.eq_ignore_ascii_case("j"))
        .void()
        .parse_next(input)
}

/// Parse a timezone starting with `+` or `-`.
pub(super) fn timezone_offset(input: &mut &str) -> ModalResult<Offset> {
    // Like GNU, read "+8 years" as a relative item, not as "+8" and "years".
    // The costlier relative check only runs once an offset has parsed.
    let start = input.checkpoint();
    let result = alt((timezone_offset_colon, timezone_offset_colonless)).parse_next(input);
    if matches!(result, Err(ErrMode::Backtrack(_))) {
        return result;
    }

    let end = input.checkpoint();
    input.reset(&start);
    if peek(relative::parse).parse_next(input).is_ok() {
        return Err(ErrMode::Backtrack(ContextError::new()));
    }
    input.reset(&end);
    result
}

pub(super) fn clock_offset(input: &mut &str) -> ModalResult<Offset> {
    alt((timezone_offset_colon, timezone_offset_colonless)).parse_next(input)
}

/// Parse a timezone by name, with an optional numeric offset appended.
fn timezone_name_offset(input: &mut &str) -> ModalResult<Offset> {
    let nextword = s(take_while(1..=MAX_TZ_SIZE, AsChar::is_alpha)).parse_next(input)?;
    let tz = timezone_name_to_offset(nextword)?;

    // Like GNU, read "UTC +8 years" as "UTC" and "+8 years", not as "UTC+8"
    // and "years". `timezone_offset` rejects input that parses as relative.
    let start = input.checkpoint();
    if let Ok(other_tz) = timezone_offset.parse_next(input) {
        return Ok(tz.merge(other_tz));
    };
    input.reset(&start);

    Ok(tz)
}

/// Parse a timezone offset with a colon separating hours and minutes, e.g.,
/// `+08:00`, `+8:00`, `+8:0`.
fn timezone_offset_colon(input: &mut &str) -> ModalResult<Offset> {
    (plus_or_minus, s(dec_uint), s(colon), s(dec_uint))
        .parse_next(input)
        .and_then(|(sign, hours, _, minutes)| {
            (sign == '-', hours, minutes)
                .try_into()
                .map_err(|e| ErrMode::Cut(ctx_err(e)))
        })
}

/// Parse a timezone offset without colon, e.g., `+0800`, `+800`, `+08`, `+8`.
fn timezone_offset_colonless(input: &mut &str) -> ModalResult<Offset> {
    (plus_or_minus, s(dec_uint_str))
        .verify_map(|(sign, s)| {
            // GNU date accepts numeric offset strings with leading zeroes. For
            // example, `+000000110` is valid. In such cases, the string is
            // truncated to the last four characters. Thus, `+000000110` becomes
            // `+0110` (note that one leading zero is kept).
            let s = if s.len() > 4 && s.trim_start_matches('0').len() <= 4 {
                &s[s.len() - 4..]
            } else {
                s
            };

            // Hour and minute values are dependent on the length of the string.
            // For example:
            //
            // - "5"   -> 05:00
            // - "05"  -> 05:00
            // - "530" -> 05:30 (the minute is the last two characters here)
            // - "0530"-> 05:30
            // - "0000530" -> 05:30
            let (h_str, m_str) = match s.len() {
                1 | 2 => (s, "0"),
                3 => s.split_at(1),
                4 => s.split_at(2),
                _ => return None,
            };

            let hours = h_str.parse::<u8>().ok()?;
            let minutes = m_str.parse::<u8>().ok()?;
            Some((sign, hours, minutes))
        })
        .parse_next(input)
        .and_then(|(sign, hours, minutes)| {
            (sign == '-', hours, minutes)
                .try_into()
                .map_err(|e| ErrMode::Cut(ctx_err(e)))
        })
}

/// Named timezone list.
///
/// The full list of timezones can be extracted from
/// https://www.timeanddate.com/time/zones/. GNU date only supports a subset of
/// these. We support the same subset as GNU date.
fn timezone_name_to_offset(input: &str) -> ModalResult<Offset> {
    let (negative, hours, minutes) = match input {
        "z" => (false, 0, 0),
        "y" => (true, 12, 0),
        "x" => (true, 11, 0),
        "wet" => (false, 0, 0),
        "west" => (false, 1, 0),
        "wat" => (false, 1, 0),
        "w" => (true, 10, 0),
        "v" => (true, 9, 0),
        "utc" => (false, 0, 0),
        "ut" => (false, 0, 0),
        "u" => (true, 8, 0),
        "t" => (true, 7, 0),
        "sst" => (true, 12, 0),
        "sgt" => (false, 8, 0),
        "sast" => (false, 2, 0),
        "s" => (true, 6, 0),
        "r" => (true, 5, 0),
        "q" => (true, 4, 0),
        "pst" => (true, 8, 0),
        "pdt" => (true, 7, 0),
        "p" => (true, 3, 0),
        "o" => (true, 2, 0),
        "nzst" => (false, 12, 0),
        "nzdt" => (false, 13, 0),
        "nst" => (true, 3, 30),
        "ndt" => (true, 2, 30),
        "n" => (true, 1, 0),
        "mst" => (true, 7, 0),
        "msk" => (false, 3, 0),
        "msd" => (false, 4, 0),
        "mez" => (false, 1, 0),
        "mesz" => (false, 2, 0),
        "mest" => (false, 2, 0),
        "mdt" => (true, 6, 0),
        "m" => (false, 12, 0),
        "l" => (false, 11, 0),
        "kst" => (false, 9, 0),
        "k" => (false, 10, 0),
        "jst" => (false, 9, 0),
        "ist" => (false, 5, 30),
        "i" => (false, 9, 0),
        "hst" => (true, 10, 0),
        "h" => (false, 8, 0),
        "gst" => (false, 10, 0),
        "gmt" => (false, 0, 0),
        "g" => (false, 7, 0),
        "f" => (false, 6, 0),
        "est" => (true, 5, 0),
        "eet" => (false, 2, 0),
        "eest" => (false, 3, 0),
        "edt" => (true, 4, 0),
        "eat" => (false, 3, 0),
        "e" => (false, 5, 0),
        "d" => (false, 4, 0),
        "cst" => (true, 6, 0),
        "clt" => (true, 4, 0),
        "clst" => (true, 3, 0),
        "cet" => (false, 1, 0),
        "cest" => (false, 2, 0),
        "cdt" => (true, 5, 0),
        "cat" => (false, 2, 0),
        "c" => (false, 3, 0),
        "bst" => (false, 1, 0),
        "brt" => (true, 3, 0),
        "brst" => (true, 2, 0),
        "b" => (false, 2, 0),
        "ast" => (true, 4, 0),
        "art" => (true, 3, 0),
        "akst" => (true, 9, 0),
        "akdt" => (true, 8, 0),
        "adt" => (true, 3, 0),
        "a" => (false, 1, 0),
        _ => return Err(ErrMode::Backtrack(ContextError::new())),
    };

    Ok(Offset {
        negative,
        hours,
        minutes,
    })
}
