// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Compare two version strings.

use core::cmp::Ordering;

/// Compares the non-digit parts of a version.
/// Special cases: ~ are before everything else, even ends ("a~" < "a")
/// Letters are before non-letters
fn version_non_digit_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let mut a_chars = a.iter();
    let mut b_chars = b.iter();
    loop {
        match (a_chars.next(), b_chars.next()) {
            (Some(c1), Some(c2)) if c1 == c2 => {}
            (None, None) => return Ordering::Equal,
            (_, Some(b'~')) => return Ordering::Greater,
            (Some(b'~'), _) | (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c1), Some(c2)) if c1.is_ascii_alphabetic() && !c2.is_ascii_alphabetic() => {
                return Ordering::Less;
            }
            (Some(c1), Some(c2)) if !c1.is_ascii_alphabetic() && c2.is_ascii_alphabetic() => {
                return Ordering::Greater;
            }
            (Some(c1), Some(c2)) => return c1.cmp(c2),
        }
    }
}

/// Remove file endings matching the regex `(\.[A-Za-z~][A-Za-z0-9~]*)*$`
fn remove_file_ending(a: &[u8]) -> &[u8] {
    let mut ending_start = None;
    let mut prev_was_dot = false;
    for (idx, &char) in a.iter().enumerate() {
        if char == b'.' {
            if ending_start.is_none() || prev_was_dot {
                ending_start = Some(idx);
            }
            prev_was_dot = true;
        } else if prev_was_dot {
            prev_was_dot = false;
            if !char.is_ascii_alphabetic() && char != b'~' {
                ending_start = None;
            }
        } else if !char.is_ascii_alphanumeric() && char != b'~' {
            ending_start = None;
        }
    }
    if prev_was_dot {
        ending_start = None;
    }
    if let Some(ending_start) = ending_start {
        &a[..ending_start]
    } else {
        a
    }
}

/// Compare two version strings.
pub fn version_cmp(mut a: &[u8], mut b: &[u8]) -> Ordering {
    let str_cmp = a.cmp(b);
    if str_cmp == Ordering::Equal {
        return str_cmp;
    }

    // Special cases:
    // 1. Empty strings
    match (a.is_empty(), b.is_empty()) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (true, true) => unreachable!(),
        (false, false) => {}
    }
    // 2. Dots
    match (a == b".", b == b".") {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (true, true) => unreachable!(),
        (false, false) => {}
    }
    // 3. Two Dots
    match (a == b"..", b == b"..") {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (true, true) => unreachable!(),
        (false, false) => {}
    }
    // 4. Strings starting with a dot
    match (a.starts_with(b"."), b.starts_with(b".")) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (true, true) => {
            // Strip the leading dot for later comparisons
            a = &a[1..];
            b = &b[1..];
        }
        _ => {}
    }

    // Try to strip file extensions
    let (mut a, mut b) = match (remove_file_ending(a), remove_file_ending(b)) {
        (a_stripped, b_stripped) if a_stripped == b_stripped => {
            // If both would be the same after stripping file extensions, don't strip them.
            (a, b)
        }
        stripped => stripped,
    };

    // 1. Compare leading non-numerical part
    // 2. Compare leading numerical part
    // 3. Repeat
    while !a.is_empty() || !b.is_empty() {
        let a_numerical_start = a.iter().position(u8::is_ascii_digit).unwrap_or(a.len());
        let b_numerical_start = b.iter().position(u8::is_ascii_digit).unwrap_or(b.len());

        let a_str = &a[..a_numerical_start];
        let b_str = &b[..b_numerical_start];

        match version_non_digit_cmp(a_str, b_str) {
            Ordering::Equal => {}
            ord => return ord,
        }

        a = &a[a_numerical_start..];
        b = &b[a_numerical_start..];

        let a_numerical_end = a
            .iter()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(a.len());
        let b_numerical_end = b
            .iter()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(b.len());

        let a_str = &a[a.iter().position(|&c| c != b'0').unwrap_or(a.len())..a_numerical_end];
        let b_str = &b[b.iter().position(|&c| c != b'0').unwrap_or(b.len())..b_numerical_end];

        match a_str.len().cmp(&b_str.len()) {
            Ordering::Equal => {}
            ord => return ord,
        }

        match a_str.cmp(b_str) {
            Ordering::Equal => {}
            ord => return ord,
        }

        a = &a[a_numerical_end..];
        b = &b[b_numerical_end..];
    }

    Ordering::Equal
}
