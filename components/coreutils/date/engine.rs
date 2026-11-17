use crate::{
    documentation,
    format::Fields,
    options::{self, Action, Options},
    platform,
};
use alloc::{borrow::Cow, format, string::String};
use jiff::{Timestamp, Zoned, tz::TimeZone};
use parse_datetime::{ParsedDateTime, parse_absolute, parse_datetime_at_date};

pub type Result<T> = core::result::Result<T, String>;
pub trait Output {
    fn line(&mut self, text: &str) -> Result<()>;
    fn error(&mut self, text: &str) -> Result<()>;
}

pub fn quote_bytes(bytes: &[u8]) -> String {
    let mut value = String::new();
    for &b in bytes {
        match b {
            b'\n' => value.push_str("\\n"),
            b'\t' => value.push_str("\\t"),
            b'\r' => value.push_str("\\r"),
            b'\\' | b'\'' => {
                value.push('\\');
                value.push(char::from(b))
            }
            32..=126 => value.push(char::from(b)),
            _ => value.push_str(&format!("\\{b:03o}")),
        }
    }
    value
}
fn numeric_date(word: &str, base: Option<&Zoned>, zone: &TimeZone) -> Option<ParsedDateTime> {
    let (digits, seconds) = match word.split_once('.') {
        Some((d, s)) if s.len() == 2 && s.bytes().all(|c| c.is_ascii_digit()) => {
            (d, s.parse::<i8>().ok()?)
        }
        None => (word, 0),
        _ => return None,
    };
    if !digits.bytes().all(|b| b.is_ascii_digit()) || !matches!(digits.len(), 8 | 10 | 12) {
        return None;
    }
    let num = |i: usize| digits[i..i + 2].parse::<i8>().ok();
    let year = match digits.len() {
        8 => base?.year(),
        10 => {
            let y = i16::from(num(8)?);
            if y < 69 { 2000 + y } else { 1900 + y }
        }
        _ => digits[8..].parse().ok()?,
    };
    let civil =
        jiff::civil::DateTime::new(year, num(0)?, num(2)?, num(4)?, num(6)?, seconds, 0).ok()?;
    Some(ParsedDateTime::InRange(civil.to_zoned(zone.clone()).ok()?))
}
pub(crate) fn parse(input: &str, base: Option<&Zoned>, zone: &TimeZone) -> Option<ParsedDateTime> {
    let mut s = Cow::Borrowed(input.trim());

    // Les commentaires entre parenthèses peuvent être imbriqués.
    if s.contains('(') {
        let mut cleaned = String::new();
        let mut depth = 0usize;
        for c in s.chars() {
            match c {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                _ if depth == 0 => cleaned.push(c),
                _ => {}
            }
        }
        s = Cow::Owned(cleaned)
    }
    if matches!(s.trim(), "" | "+" | "-") {
        s = Cow::Borrowed("00:00")
    }
    if let Some(rest) = s.strip_prefix("TZ=\"") {
        let (name, tail) = rest.split_once('"')?;
        let zone = TimeZone::get(name)
            .or_else(|_| TimeZone::posix(name))
            .ok()?;
        let b = base.map(|t| t.with_time_zone(zone.clone()));
        return parse(tail.trim(), b.as_ref(), &zone);
    }
    if let Ok(ts) = s.parse::<Timestamp>() {
        return Some(ParsedDateTime::InRange(ts.to_zoned(zone.clone())));
    }
    if let Some((date, time)) = s.split_once([' ', 'T'])
        && date.contains('-')
        && !time.contains(['+', '-', 'Z', 'z', ' '])
        && let Ok(civil) = s.parse::<jiff::civil::DateTime>()
    {
        let mapping = zone.to_ambiguous_zoned(civil);
        if matches!(mapping.offset(), jiff::tz::AmbiguousOffset::Gap { .. }) {
            return None;
        }
        return mapping.compatible().ok().map(ParsedDateTime::InRange);
    }
    if s.contains('.') && !s.starts_with('@') {
        let mut words = alloc::vec::Vec::new();
        let mut changed = false;
        for word in s.split_ascii_whitespace() {
            let mut parts = word.split('.');
            let mapped = match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some(day), Some(month), Some(year), None)
                    if (1..=2).contains(&day.len())
                        && (1..=2).contains(&month.len())
                        && (year.is_empty() || year.len() == 4)
                        && [day, month, year]
                            .iter()
                            .all(|v| v.bytes().all(|c| c.is_ascii_digit())) =>
                {
                    changed = true;
                    if year.is_empty() {
                        format!("{month}/{day}")
                    } else {
                        format!("{year}-{month}-{day}")
                    }
                }
                _ => word.into(),
            };
            words.push(mapped)
        }
        if changed {
            s = Cow::Owned(words.join(" "))
        }
    }
    match base {
        Some(b) => parse_datetime_at_date(b.clone(), s.as_ref()),
        None => parse_absolute(&s),
    }
    .ok()
}
fn show(
    opts: &Options,
    parsed: ParsedDateTime,
    zone: &TimeZone,
    out: &mut impl Output,
) -> Result<bool> {
    let fields = Fields::new(&parsed, zone)?;
    let mut ok = true;
    if (opts.set.is_some() || opts.numeric.is_some())
        && let Err(e) = platform::set(&fields)
    {
        out.error(&format!("date: cannot set date: {e}\n"))?;
        ok = false;
    }
    if opts.debug {
        out.error(&format!("date: output format: '{}'\n", opts.template()))?
    }
    let mut value = fields.render(opts.template(), opts.invariant)?;
    value.push('\n');
    out.line(&value)?;
    Ok(ok)
}
pub fn execute<O: Output>(
    args: &[&str],
    lang: documentation::Language,
    out: &mut O,
) -> Result<bool> {
    let opts = match options::parse(args)? {
        Action::Run(o) => o,
        doc => {
            let page = documentation::lookup("date").unwrap();
            let text = match doc {
                Action::Help => page.render(&[], false, lang).unwrap(),
                _ => page.version(lang),
            };
            out.line(&text)?;
            return Ok(true);
        }
    };
    let (zone, name) = platform::zone(opts.utc)?;
    if opts.debug
        && let Some(c) = opts.repeated
    {
        out.error(&format!("date: only using last of multiple -{c} options\n"))?
    }
    let convert = |s: &str, out: &mut O| -> Result<bool> {
        let base = platform::now().ok().map(|t| t.to_zoned(zone.clone()));
        let parsed = if opts.numeric.is_some() {
            numeric_date(s, base.as_ref(), &zone)
        } else {
            parse(s, base.as_ref(), &zone)
        };
        if opts.debug && opts.numeric.is_none() {
            crate::debug::trace(s, base.as_ref(), parsed.as_ref(), &zone, &name, out)?
        }
        match parsed {
            Some(p) => show(&opts, p, &zone, out),
            None => {
                out.error(&format!(
                    "date: invalid date '{}'\n",
                    platform::quote(if opts.file.is_some() {
                        s.strip_suffix('\n').unwrap_or(s)
                    } else {
                        s
                    })
                ))?;
                Ok(false)
            }
        }
    };
    if let Some(path) = opts.file {
        let mut ok = true;
        platform::lines(path, |line| {
            ok &= convert(line, out)?;
            Ok(())
        })?;
        return Ok(ok);
    }
    if let Some(s) = opts.date.or(opts.set).or(opts.numeric) {
        return convert(s, out);
    }
    let t: Timestamp = if let Some(path) = opts.reference {
        platform::reference(path)?
    } else if opts.resolution {
        platform::resolution()?
    } else {
        platform::now()?
    };
    show(
        &opts,
        ParsedDateTime::InRange(t.to_zoned(zone.clone())),
        &zone,
        out,
    )
}
