// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Parse a timezone item. The timezone item must be at the beginning of the
//! input string and in the `TZ="..."` format.
//!
//! From the GNU docs:
//!
//! > Normally, dates are interpreted using the rules of the current time zone,
//! > which in turn are specified by the ‘TZ’ environment variable, or by a
//! > system default if ‘TZ’ is not set. To specify a different set of default
//! > time zone rules that apply just to one date, start the date with a string
//! > of the form ‘TZ="RULE"’. The two quote characters (‘"’) must be present in
//! > the date, and any quotes or backslashes within RULE must be escaped by a
//! > backslash.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use jiff::tz::{Offset, TimeZone};
use winnow::{
    ModalResult, Parser,
    combinator::{alt, delimited, opt, preceded, repeat},
    stream::AsChar,
    token::{one_of, take_while},
};

use super::primitive::{dec_uint, plus_or_minus};

pub(super) fn parse(input: &mut &str) -> ModalResult<TimeZone> {
    delimited("TZ=\"", preceded(opt(':'), alt((posix, iana))), '"').parse_next(input)
}

/// Parse a posix (proleptic) timezone string (e.g., "UTC7", "JST-9").
///
/// TODO: This implementation is incomplete. It currently only parses the
/// `STDOFFSET` part of the format.
///
/// From the GNU docs:
///
/// > The proleptic format is:
/// >
/// >   STDOFFSET[DST[OFFSET][,START[/TIME],END[/TIME]]]
/// >
/// > The STD string specifies the time zone abbreviation, which must be at
/// > least three bytes long. ...
/// >
/// > The OFFSET specifies the time value you must add to the local time to
/// > get a UTC value.  It has syntax like:
/// >
/// >   [+|-]HH[:MM[:SS]]
/// >
/// > This is positive if the local time zone is west of the Prime Meridian
/// > and negative if it is east; this is opposite from the usual convention
/// > that positive time zone offsets are east of the Prime Meridian.  The
/// > hour HH must be between 0 and 24 and may be a single digit, and the
/// > minutes MM and seconds SS, if present, must be between 0 and 59.
fn posix(input: &mut &str) -> ModalResult<TimeZone> {
    (take_while(3.., AsChar::is_alpha), posix_offset)
        .verify_map(|(_, offset)| Offset::from_seconds(offset).ok().map(|o| o.to_time_zone()))
        .parse_next(input)
}

/// Parse an IANA (geographical) timezone string (e.g., "Europe/Paris"). If the
/// string is not a valid IANA timezone name, the UTC timezone is returned.
///
/// Compatibility notes:
///
/// - The implementation uses `jiff::tz::TimeZone::get()` to resolve time zones.
///   Only canonical/aliased IANA names are accepted. Absolute file paths are
///   not supported.
/// - GNU `date` resolves time zones from the tzdata files under
///   `/usr/share/zoneinfo` (respecting `TZDIR`) and also accepts an absolute
///   path when the string starts with `/`.
///
/// From the GNU docs:
///
/// > If the format's CHARACTERS begin with ‘/’ it is an absolute file
/// > name; otherwise the library looks for the file
/// > ‘/usr/share/zoneinfo/CHARACTERS’.  The ‘zoneinfo’ directory contains
/// > data files describing time zone rulesets in many different parts of the
/// > world.  The names represent major cities, with subdirectories for
/// > geographical areas; for example, ‘America/New_York’, ‘Europe/London’,
/// > ‘Asia/Tokyo’.  These data files are installed by the system
/// > administrator, who also sets ‘/etc/localtime’ to point to the data file
/// > for the local time zone ruleset.
fn iana(input: &mut &str) -> ModalResult<TimeZone> {
    repeat(
        0..,
        alt((
            preceded('\\', one_of(['\\', '"'])).map(|c: char| c.to_string()),
            take_while(1, |c| c != '"' && c != '\\').map(str::to_string),
        )),
    )
    .map(|parts: Vec<String>| parts.concat())
    .map(|s| TimeZone::get(&s).unwrap_or(TimeZone::UTC))
    .parse_next(input)
}

fn posix_offset(input: &mut &str) -> ModalResult<i32> {
    let uint = dec_uint::<u32, _>;

    (
        opt(plus_or_minus),
        alt((
            (uint, preceded(':', uint), preceded(':', uint)).map(|(h, m, s)| (h, m, s)),
            (uint, preceded(':', uint)).map(|(h, m)| (h, m, 0)),
            uint.map(|h| (h, 0, 0)),
        )),
    )
        .map(|(sign, (h, m, s))| {
            // The sign is opposite from the usual convention:
            // - Positive offsets are west of UTC.
            // - Negative offsets are east of UTC.
            let sign = if sign == Some('-') { 1 } else { -1 };

            // - If hour is greater than 24, clamp it to 24.
            // - If minute is greater than 59, clamp it to 59.
            // - If second is greater than 59, clamp it to 59.
            let h = h.min(24) as i32;
            let m = m.min(59) as i32;
            let s = s.min(59) as i32;

            sign * (h * 3600 + m * 60 + s)
        })
        .parse_next(input)
}
