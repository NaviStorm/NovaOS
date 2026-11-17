// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore multispace0

//! From the GNU docs:
//!
//! > A date is a string, possibly empty, containing many items separated by
//! > whitespace. The whitespace may be omitted when no ambiguity arises. The
//! > empty string means the beginning of today (i.e., midnight). Order of the
//! > items is immaterial. A date string may contain many flavors of items:
//! >  - calendar date items
//! >  - time of day items
//! >  - time zone items
//! >  - combined date and time of day items
//! >  - day of the week items
//! >  - relative items
//! >  - pure numbers.
//!
//! We put all of those in separate modules:
//!  - [`combined`]
//!  - [`date`]
//!  - [`epoch`]
//!  - [`offset`]
//!  - [`pure`]
//!  - [`relative`]
//!  - [`time`]
//!  - [`timezone`]
//!  - [`weekday`]
//!  - [`year`]

// date and time items
mod combined;
mod date;
mod epoch;
mod offset;
mod pure;
mod relative;
mod time;
mod timezone;
mod weekday;
mod year;

// utility modules
mod builder;
mod ordinal;
mod primitive;

pub(crate) mod error;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::ParsedDateTime;
use jiff::Zoned;
use primitive::space;
use winnow::{
    ModalResult, Parser,
    combinator::{alt, eof, preceded, repeat_till, terminated, trace},
    error::{AddContext, ContextError, ErrMode, StrContext, StrContextValue},
    stream::Stream,
};

use builder::DateTimeBuilder;
use error::Error;

#[derive(PartialEq, Debug)]
enum Item {
    DateTime(combined::DateTime),
    Date(date::Date),
    Time(time::Time),
    Weekday(weekday::Weekday),
    Relative(relative::Relative),
    Offset(offset::Offset),
    /// The military timezone letter `j`, which GNU `date` defines as local
    /// time. It is a timezone item like [`Item::Offset`], but carries no
    /// offset, so it cannot be represented as one.
    LocalZone,
    TimeZone(jiff::tz::TimeZone),
    Pure(String),
}

/// Parse a date and time string and resolve it against the given base date and
/// time, returning a [`ParsedDateTime`] result.
pub(crate) fn parse_at_date<S: AsRef<str> + Clone>(
    base: Zoned,
    input: S,
) -> Result<ParsedDateTime, Error> {
    match parse(&mut input.as_ref()) {
        Ok(builder) => builder.set_base(base).build(),
        Err(e) => Err(e.into()),
    }
}

/// Parse a date and time string and resolve it against the current local date
/// and time, returning a [`ParsedDateTime`] result.
pub(crate) fn parse_at_local<S: AsRef<str> + Clone>(input: S) -> Result<ParsedDateTime, Error> {
    match parse(&mut input.as_ref()) {
        Ok(builder) => builder.build(), // the builder uses current local date and time if no base is given.
        Err(e) => Err(e.into()),
    }
}

/// Parse a date and time string.
///
/// Grammar:
///
/// ```ebnf
/// spec                = [ tz_rule ] ( timestamp | items ) ;
///
/// tz_rule            = "TZ=" , "\"" , ( posix_tz | iana_tz ) , "\"" ;
///
/// timestamp           = "@" , float ;
///
/// items               = item , { item } ;
/// item                = datetime | date | time | relative | weekday | offset | pure ;
///
/// datetime            = date , [ "t" | whitespace ] , iso_time ;
///
/// date                = iso_date | us_date | literal1_date | literal2_date ;
///
/// iso_date            = year , [ iso_date_delim ] , month , [ iso_date_delim ] , day ;
/// iso_date_delim      = optional_whitespace , "-" , optional_whitespace ;
///
/// us_date             = month , [ us_date_delim ] , day , [ us_date_delim , year ];
/// us_date_delim       = optional_whitespace , "/" , optional_whitespace ;
///
/// literal1_date       = day , [ literal1_date_delim ] , literal_month , [ literal1_date_delim , year ] ;
/// literal1_date_delim = (optional_whitespace , "-" , optional_whitespace) | optional_whitespace ;
///
/// literal2_date       = literal_month , optional_whitespace , day , [ literal2_date_delim , year ] ;
/// literal2_date_delim = (optional_whitespace , "," , optional_whitespace) | optional_whitespace ;
///
/// year                = dec_uint ;
/// month               = dec_uint ;
/// day                 = dec_uint ;
///
/// literal_month       = "january" | "jan"
///                     | "february" | "feb"
///                     | "march" | "mar"
///                     | "april" | "apr"
///                     | "may"
///                     | "june" | "jun"
///                     | "july" | "jul"
///                     | "august" | "aug"
///                     | "september" | "sept" | "sep"
///                     | "october" | "oct"
///                     | "november" | "nov"
///                     | "december" | "dec" ;
///
/// time                = iso_time | meridiem_time ;
///
/// iso_time            = hour24 , [ ":" , minute , [ ":" , second ] ] , [ time_offset ] ;
///
/// meridiem_time       = hour12 , [ ":" , minute , [ ":" , second ] ] , meridiem ;
/// meridiem            = "am" | "pm" | "a.m." | "p.m." ;
///
/// hour24              = dec_uint ;
/// hour12              = dec_uint ;
/// minute              = dec_uint ;
/// second              = dec_uint ;
///
/// time_offset         = ( "+" | "-" ) , dec_uint , [ ":" , dec_uint ] ;
///
/// relative            = [ numeric_ordinal  ] , unit , [ "ago" | "hence" ] | day_shift ;
///
/// unit                = "year" | "years"
///                     | "month" | "months"
///                     | "fortnight" | "fortnights"
///                     | "week" | "weeks"
///                     | "day" | "days"
///                     | "hour" | "hours"
///                     | "minute" | "minutes" | "min" | "mins"
///                     | "second" | "seconds" | "sec" | "secs" ;
///
/// day_shift           = "tomorrow" | "yesterday" | "today" | "now" ;
///
/// weekday             = [ ordinal ] , day , [ "," ] ;
///
/// ordinal             = numeric_ordinal | text_ordinal ;
/// numeric_ordinal     = [ "+" | "-" ] , dec_uint ;
/// text_ordinal        = "last" | "this" | "next" | "first"
///                     | "third" | "fourth" | "fifth" | "sixth"
///                     | "seventh" | "eighth" | "ninth" | "tenth"
///                     | "eleventh" | "twelfth" ;
///
/// day                 = "monday" | "mon" | "mon."
///                     | "tuesday" | "tue" | "tue." | "tues"
///                     | "wednesday" | "wed" | "wed." | "wednes"
///                     | "thursday" | "thu" | "thu." | "thur" | "thurs"
///                     | "friday" | "fri" | "fri."
///                     | "saturday" | "sat" | "sat."
///                     | "sunday" | "sun" | "sun." ;
///
/// offset             = named_zone , [ time_offset ] ;
///
/// pure               = { digit }
///
/// optional_whitespace = { whitespace } ;
/// ```
fn parse(input: &mut &str) -> ModalResult<DateTimeBuilder> {
    trace("parse", alt((parse_timestamp, parse_items))).parse_next(input)
}

/// Parse a standalone epoch timestamp (e.g., `@1758724019`).
///
/// GNU `date` specifies that a timestamp item is *complete* and *must not* be
/// combined with any other date/time item.
///
/// Notes:
///
/// - If a timezone rule (`TZ="..."`) appears at the beginning of the input, it
///   has no effect on the epoch value. We intentionally parse and ignore it.
/// - Trailing input (aside from optional whitespaces) is rejected.
fn parse_timestamp(input: &mut &str) -> ModalResult<DateTimeBuilder> {
    // Parse and ignore an optional leading timezone rule.
    let _ = timezone::parse(input);

    trace(
        "parse_timestamp",
        // Expect exactly one timestamp and then EOF (allowing trailing spaces).
        terminated(epoch::parse, preceded(space, eof)),
    )
    .verify_map(|ts| DateTimeBuilder::new().set_timestamp(ts).ok())
    .parse_next(input)
}

/// Parse a sequence of date/time items, honoring an optional leading TZ rule.
///
/// Notes:
///
/// - If a timezone rule (`TZ="..."`) appears at the beginning of the input,
///   parse it first. The timezone rule is case-sensitive.
/// - After the optional timezone rule is parsed, we convert the input to
///   lowercase to allow case-insensitive parsing of the remaining items.
/// - Trailing input (aside from optional whitespaces) is rejected.
fn parse_items(input: &mut &str) -> ModalResult<DateTimeBuilder> {
    // Parse and consume an optional leading timezone rule.
    let tz = timezone::parse(input).map(Item::TimeZone);

    // Convert input to lowercase for case-insensitive parsing.
    let lower: Cow<str> = if input.bytes().any(|b| b.is_ascii_uppercase()) {
        input.to_ascii_lowercase().into()
    } else {
        (*input).into()
    };
    let input = &mut lower.as_ref();

    let (mut items, _): (Vec<Item>, _) = trace(
        "parse_items",
        // Parse zero or more items until EOF (allowing trailing spaces).
        repeat_till(0.., parse_item, preceded(space, eof)),
    )
    .parse_next(input)?;

    if let Ok(tz) = tz {
        items.push(tz);
    }

    items.try_into().map_err(|e| expect_error(input, e))
}

/// Parse an item.
fn parse_item(input: &mut &str) -> ModalResult<Item> {
    // An ISO date is either the start of a combined date and time item or a
    // date item on its own. Parse it once instead of once per alternative.
    let start = input.checkpoint();
    if let Ok(date) = alt((date::iso1, date::iso2)).parse_next(input) {
        let after_date = input.checkpoint();
        match combined::parse_time_after_date(input, date.clone()) {
            Ok(dt) => return Ok(Item::DateTime(dt)),
            Err(ErrMode::Backtrack(_)) => {
                input.reset(&after_date);
                return Ok(Item::Date(date));
            }
            Err(e) => return Err(e),
        }
    }
    input.reset(&start);

    trace(
        "parse_item",
        alt((
            date::parse_non_iso.map(Item::Date),
            time::parse.map(Item::Time),
            relative::parse.map(Item::Relative),
            weekday::parse.map(Item::Weekday),
            offset::parse.map(Item::Offset),
            offset::parse_local.map(|()| Item::LocalZone),
            pure::parse.map(Item::Pure),
        )),
    )
    .parse_next(input)
}

/// Create an error with context for unexpected input.
fn expect_error(input: &mut &str, reason: &'static str) -> ErrMode<ContextError> {
    ErrMode::Cut(ContextError::new()).add_context(
        input,
        &input.checkpoint(),
        StrContext::Expected(StrContextValue::Description(reason)),
    )
}
pub(crate) mod trace;
