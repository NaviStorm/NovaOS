// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
//! A Rust crate for parsing human-readable relative time strings and
//! human-readable datetime strings.
//! The function supports the following formats for time:
//!
//! * ISO formats
//! * timezone offsets, e.g., "UTC-0100"
//! * unix timestamps, e.g., "@12"
//! * relative time to now, e.g. "+1 hour"
//!
#![no_std]
extern crate alloc;
use core::error::Error;
use core::fmt::{self, Display};

use jiff::Zoned;

mod extended;
mod items;

pub use extended::{DateParts, ExtendedDateTime, TimeParts};

/// Maximum year accepted by GNU `date`.
pub const GNU_MAX_YEAR: u32 = 2_147_485_547;

/// Parsed datetime output.
///
/// - [`ParsedDateTime::InRange`] contains a standard [`jiff::Zoned`] value.
/// - [`ParsedDateTime::Extended`] contains an out-of-range year representation
///   (for example, years greater than `9999`) that cannot be represented by
///   `jiff::Zoned`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedDateTime {
    InRange(Zoned),
    Extended(ExtendedDateTime),
}

impl ParsedDateTime {
    pub fn as_zoned(&self) -> Option<&Zoned> {
        match self {
            ParsedDateTime::InRange(z) => Some(z),
            ParsedDateTime::Extended(_) => None,
        }
    }

    pub fn into_zoned(self) -> Option<Zoned> {
        match self {
            ParsedDateTime::InRange(z) => Some(z),
            ParsedDateTime::Extended(_) => None,
        }
    }

    /// Returns the number of seconds since the Unix epoch, using floor
    /// division (rounding toward negative infinity).
    ///
    /// This matches the behavior of GNU `date +%s` for negative fractional
    /// timestamps. For example, a timestamp of −1.5 seconds returns −2
    /// (not −1 as truncation toward zero would give).
    ///
    /// Note: `jiff::Timestamp::as_second()` uses truncation toward zero,
    /// which differs from the POSIX/GNU convention for `%s`. This method
    /// corrects for that.
    pub fn unix_epoch_second(&self) -> i64 {
        match self {
            ParsedDateTime::InRange(z) => {
                let ts = z.timestamp();
                let s = ts.as_second();
                let ns = ts.subsec_nanosecond();
                // jiff stores negative fractional timestamps with a negative
                // subsec_nanosecond (e.g. -1.5s → as_second=-1, subsec=-500ms).
                // Floor semantics require subtracting 1 from the second when
                // the nanosecond component is negative.
                if ns < 0 { s - 1 } else { s }
            }
            ParsedDateTime::Extended(ext) => ext.unix_seconds(),
        }
    }

    /// Returns the sub-second nanosecond component, always in `0..1_000_000_000`.
    ///
    /// This is the complement of [`unix_epoch_second`](Self::unix_epoch_second):
    /// the full timestamp equals `unix_epoch_second() + subsec_nanosecond() / 1e9`.
    ///
    /// For negative fractional timestamps the nanosecond is re-normalized to be
    /// non-negative (matching the GNU `timespec` convention).
    pub fn subsec_nanosecond(&self) -> i32 {
        match self {
            ParsedDateTime::InRange(z) => {
                let ns = z.timestamp().subsec_nanosecond();
                if ns < 0 { 1_000_000_000 + ns } else { ns }
            }
            ParsedDateTime::Extended(_) => 0,
        }
    }

    /// Unwraps the `InRange` variant, panicking if this is an `Extended` value.
    ///
    /// This is a convenience for contexts where the caller is certain the result
    /// is in range (e.g., tests). Prefer [`into_zoned`](Self::into_zoned) or
    /// pattern matching in production code.
    pub fn expect_in_range(self) -> Zoned {
        self.into_zoned()
            .expect("ParsedDateTime is not representable as jiff::Zoned")
    }
}

impl fmt::Display for ParsedDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParsedDateTime::InRange(z) => {
                let ns = z.datetime().time().subsec_nanosecond();
                if ns != 0 {
                    write!(f, "{}", z.strftime("%Y-%m-%d %H:%M:%S%.9f%:z"))
                } else {
                    write!(f, "{}", z.strftime("%Y-%m-%d %H:%M:%S%:z"))
                }
            }
            ParsedDateTime::Extended(dt) => write!(f, "{dt}"),
        }
    }
}

/// An `Extended` value never compares equal to a `Zoned`.
impl PartialEq<Zoned> for ParsedDateTime {
    fn eq(&self, other: &Zoned) -> bool {
        match self {
            ParsedDateTime::InRange(z) => z == other,
            ParsedDateTime::Extended(_) => false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseDateTimeError {
    InvalidInput,
}

impl Display for ParseDateTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseDateTimeError::InvalidInput => write!(f, "Invalid input string"),
        }
    }
}

impl Error for ParseDateTimeError {}

impl From<items::error::Error> for ParseDateTimeError {
    fn from(_: items::error::Error) -> Self {
        ParseDateTimeError::InvalidInput
    }
}

/// Resolve a fully specified date without access to a civil clock.
pub fn parse_absolute(input: &str) -> Result<ParsedDateTime, ParseDateTimeError> {
    items::parse_at_local(input).map_err(Into::into)
}

/// Parses a time string at a specific date and returns a [`ParsedDateTime`]
/// representing the absolute time of the string.
///
/// # Arguments
///
/// * date - The date represented in local time
/// * `input` - A string slice representing the time.
///
/// # Examples
///
/// ```
/// use jiff::Zoned;
/// use parse_datetime::{parse_datetime_at_date, ParsedDateTime};
///
///  let now = Zoned::now();
///  let after = parse_datetime_at_date(now, "2024-09-13UTC +3 days").unwrap();
///
///  match after {
///      ParsedDateTime::InRange(z) => assert_eq!("2024-09-16", z.strftime("%F").to_string()),
///      ParsedDateTime::Extended(_) => unreachable!("unexpected for this input"),
///  }
/// ```
///
/// # Returns
///
/// * `Ok(ParsedDateTime)` - If the input string can be parsed as a time
/// * `Err(ParseDateTimeError)` - If the input string cannot be parsed
///
/// # Errors
///
/// This function will return `Err(ParseDateTimeError::InvalidInput)` if the
/// input string cannot be parsed.
pub fn parse_datetime_at_date<S: AsRef<str> + Clone>(
    date: Zoned,
    input: S,
) -> Result<ParsedDateTime, ParseDateTimeError> {
    items::parse_at_date(date, input).map_err(|e| e.into())
}
pub use items::trace::{Piece, inspect};
