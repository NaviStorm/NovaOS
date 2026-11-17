// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Parse an ISO 8601 date and time item
//!
//! The GNU docs state:
//!
//! > The ISO 8601 date and time of day extended format consists of an ISO 8601
//! > date, a ‘T’ character separator, and an ISO 8601 time of day. This format
//! > is also recognized if the ‘T’ is replaced by a space.
//! >
//! > In this format, the time of day should use 24-hour notation. Fractional
//! > seconds are allowed, with either comma or period preceding the fraction.
//! > ISO 8601 fractional minutes and hours are not supported. Typically, hosts
//! > support nanosecond timestamp resolution; excess precision is silently
//! > discarded.
use winnow::{
    ModalResult, Parser,
    combinator::{alt, trace},
};

use crate::items::space;

use super::{date, primitive::s, time};

#[derive(PartialEq, Debug, Clone, Default)]
pub(crate) struct DateTime {
    pub(crate) date: date::Date,
    pub(crate) time: time::Time,
}

fn remaining_starts_with_meridiem(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with("am")
        || trimmed.starts_with("pm")
        || trimmed.starts_with("a.m.")
        || trimmed.starts_with("p.m.")
}

/// Parse the separator and time of a combined item whose ISO date has already
/// been parsed.
pub(crate) fn parse_time_after_date(input: &mut &str, date: date::Date) -> ModalResult<DateTime> {
    // Note: the `T` is lowercased by the main parse function
    alt((s('t').void(), (' ', space).void())).parse_next(input)?;

    let mut iso_input = *input;
    if let Ok(parsed_time) = trace("iso_time", time::iso).parse_next(&mut iso_input) {
        if !remaining_starts_with_meridiem(iso_input) {
            *input = iso_input;
            return Ok(DateTime {
                date,
                time: parsed_time,
            });
        }
    }

    let time = trace("iso_time", time::parse).parse_next(input)?;
    Ok(DateTime { date, time })
}
