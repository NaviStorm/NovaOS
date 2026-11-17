// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Parse a year from a string.
//!
//! The year must be parsed to a string first, this is to handle a specific GNU
//! compatibility quirk. According to the GNU documentation: "if the year is 68
//! or smaller, then 2000 is added to it; otherwise, if year is less than 100,
//! then 1900 is added to it." This adjustment only applies to two-digit year
//! strings. For example, `"00"` is interpreted as `2000`, whereas `"0"`,
//! `"000"`, or `"0000"` are interpreted as `0`.

use winnow::{ModalResult, Parser, stream::AsChar, token::take_while};

use super::primitive::s;

const GNU_MAX_YEAR: u32 = 2_147_485_547;
// TODO: Leverage `TryFrom` trait.
pub(super) fn year_from_str(year_str: &str) -> Result<u32, &'static str> {
    let mut year = year_str
        .parse::<u32>()
        .map_err(|_| "year must be a non-negative integer")?;

    // If year is 68 or smaller, then 2000 is added to it; otherwise, if year
    // is less than 100, then 1900 is added to it.
    //
    // GNU quirk: this only applies to two-digit years. For example,
    // "98-01-01" will be parsed as "1998-01-01", whereas "098-01-01" will be
    // parsed as "0098-01-01".
    if year_str.len() == 2 {
        if year <= 68 {
            year += 2000
        } else {
            year += 1900
        }
    }

    if year > GNU_MAX_YEAR {
        return Err("year exceeds GNU maximum");
    }

    Ok(year)
}

pub(super) fn year_str<'a>(input: &mut &'a str) -> ModalResult<&'a str> {
    s(take_while(1.., AsChar::is_dec_digit)).parse_next(input)
}
