use alloc::{
    format,
    string::{String, ToString},
};
use core::fmt::Write;
use jiff::{Zoned, tz::TimeZone};
use parse_datetime::ParsedDateTime;

pub struct Fields {
    pub zoned: Zoned,
    year: i64,
    pub seconds: i64,
}
impl Fields {
    pub fn new(parsed: &ParsedDateTime, zone: &TimeZone) -> Result<Self, String> {
        let seconds = parsed.unix_epoch_second();
        let (zoned, year) = match parsed {
            ParsedDateTime::InRange(z) => {
                let z = z.with_time_zone(zone.clone());
                let year = z.year().into();
                (z, year)
            }
            ParsedDateTime::Extended(dt) => {
                // Un cycle grégorien conserve jours de semaine et années bissextiles.
                let y = (2000 + dt.year % 400) as i16;
                let civil = jiff::civil::DateTime::new(
                    y,
                    dt.month as i8,
                    dt.day as i8,
                    dt.hour as i8,
                    dt.minute as i8,
                    dt.second as i8,
                    dt.nanosecond as i32,
                )
                .map_err(|e| e.to_string())?;
                let source =
                    jiff::tz::Offset::from_seconds(dt.offset_seconds).map_err(|e| e.to_string())?;
                let z = civil
                    .to_zoned(source.to_time_zone())
                    .map_err(|e| e.to_string())?
                    .with_time_zone(zone.clone());
                let year = i64::from(dt.year) + i64::from(z.year() - y);
                (z, year)
            }
        };
        Ok(Self {
            zoned,
            year,
            seconds,
        })
    }

    pub fn render(&self, template: &str, invariant: bool) -> Result<String, String> {
        let mut out = String::with_capacity(template.len() + 24);
        let (year, month, day) = if invariant {
            None
        } else {
            crate::platform::calendar_parts(&self.zoned)
        }
        .unwrap_or((
            self.year,
            self.zoned.month().into(),
            self.zoned.day().into(),
        ));
        let mut it = template.char_indices().peekable();
        while let Some((start, ch)) = it.next() {
            if ch != '%' {
                out.push(ch);
                continue;
            }
            let mut pad = None;
            let mut upper = false;
            let mut flip = false;
            while let Some(&(_, c)) = it.peek() {
                match c {
                    '-' | '_' | '0' | '+' => pad = Some(c),
                    '^' => upper = true,
                    '#' => flip = true,
                    _ => break,
                }
                it.next();
            }
            let mut width = 0usize;
            while let Some(d) = it.peek().and_then(|(_, c)| c.to_digit(10)) {
                width = width * 10 + d as usize;
                if width > 1_048_576 {
                    return Err("field width exceeds 1048576".into());
                }
                it.next();
            }
            let modifier = it.next_if(|(_, c)| matches!(c, 'E' | 'O')).map(|(_, c)| c);
            let mut colons = 0;
            while it.next_if(|(_, c)| *c == ':').is_some() {
                colons += 1
            }
            let Some((end, code)) = it.next() else {
                out.push_str(&template[start..]);
                break;
            };
            let literal = &template[start..end + code.len_utf8()];
            let z = &self.zoned;
            if colons > 0 && (code != 'z' || colons > 3)
                || modifier == Some('E') && !"cCxXyYnpPqrRstTuzZ".contains(code)
            {
                out.push_str(literal);
                continue;
            }
            if modifier == Some('O') && "aAcDFxX".contains(code) {
                out.push_str(literal);
                continue;
            }
            if code == '%' && literal != "%%" {
                let n = width.saturating_sub(literal.len() - 1);
                out.extend(core::iter::repeat_n(
                    if matches!(pad, Some('0' | '+')) {
                        '0'
                    } else {
                        ' '
                    },
                    n,
                ));
                out.push_str(literal);
                continue;
            }
            if code == 'z' {
                let raw = offset(z.offset().seconds(), colons);
                let digits = raw[1..].trim_start_matches('0');
                let compact = if digits.is_empty() || digits.starts_with(':') {
                    format!("0{digits}")
                } else {
                    digits.into()
                };
                let default = if width == 0 { raw.len() } else { width };
                let sign = &raw[..1];
                if pad == Some('-') {
                    write!(out, "{sign}{compact}").unwrap()
                } else if pad == Some('_') {
                    write!(out, "{:>default$}", format!("{sign}{compact}")).unwrap()
                } else {
                    out.push_str(sign);
                    out.extend(core::iter::repeat_n('0', default.saturating_sub(raw.len())));
                    out.push_str(&raw[1..])
                }
                continue;
            }
            if code == 'N' {
                let fraction = format!("{:09}", z.subsec_nanosecond());
                let n = if literal == "%-N" {
                    crate::platform::fraction_digits()
                } else if width == 0 {
                    9
                } else {
                    width
                };
                let digits = &fraction[..n.min(9)];
                let trimmed = digits.trim_end_matches('0');
                let value = if matches!(pad, Some('-' | '_')) && literal != "%-N" {
                    if trimmed.is_empty() { "0" } else { trimmed }
                } else {
                    digits
                };
                out.push_str(value);
                if pad != Some('-') {
                    out.extend(core::iter::repeat_n(
                        if pad == Some('_') { ' ' } else { '0' },
                        n - value.len(),
                    ))
                }
                continue;
            }
            let iso = z.date().iso_week_date();
            let iso_year = self.year + i64::from(iso.year() - z.year());
            let numeric = match code {
                'C' => Some((self.year.div_euclid(100), 2)),
                'y' => Some((self.year.rem_euclid(100), 2)),
                'Y' => Some((
                    if modifier == Some('E') {
                        self.year
                    } else {
                        year
                    },
                    4,
                )),
                'G' => Some((iso_year, 4)),
                'g' => Some((iso_year.rem_euclid(100), 2)),
                'm' => Some((month, 2)),
                'd' | 'e' => Some((day, 2)),
                'H' | 'k' => Some((z.hour().into(), 2)),
                'I' | 'l' => Some((((z.hour() + 11) % 12 + 1).into(), 2)),
                'M' => Some((z.minute().into(), 2)),
                'S' => Some((z.second().into(), 2)),
                'j' => Some((z.date().day_of_year().into(), 3)),
                'q' => Some((((z.month() - 1) / 3 + 1).into(), 1)),
                'u' => Some((z.weekday().to_monday_one_offset().into(), 1)),
                'w' => Some((z.weekday().to_sunday_zero_offset().into(), 1)),
                'V' => Some((iso.week().into(), 2)),
                'U' | 'W' => {
                    let dow = if code == 'U' {
                        z.weekday().to_sunday_zero_offset()
                    } else {
                        z.weekday().to_monday_zero_offset()
                    };
                    Some((
                        ((z.date().day_of_year() + 6 - i16::from(dow)) / 7).into(),
                        2,
                    ))
                }
                's' => Some((self.seconds, 1)),
                _ => None,
            };
            if let Some((number, default)) = numeric {
                let width = if pad == Some('-') {
                    0
                } else if width == 0 {
                    default
                } else {
                    width
                };
                let space = pad == Some('_') || pad.is_none() && "ekl".contains(code);
                let plus = pad == Some('+')
                    && matches!(code, 'Y' | 'G' | 'C' | 'y' | 'g')
                    && (width > default || number >= 10i64.pow(default as u32));
                if space {
                    write!(out, "{number:width$}").unwrap()
                } else if plus {
                    write!(out, "{number:+0width$}").unwrap()
                } else {
                    write!(out, "{number:0width$}").unwrap()
                }
                continue;
            }
            let mut value = if !invariant && "aAbBhcprPxX".contains(code) {
                crate::platform::localized(code, z)
            } else {
                None
            };
            if value.is_none() {
                let sub = match code {
                    'F' => Some("%+4Y-%m-%d"),
                    'D' | 'x' => Some("%m/%d/%y"),
                    'T' | 'X' => Some("%H:%M:%S"),
                    'R' => Some("%H:%M"),
                    'r' => Some("%I:%M:%S %p"),
                    'c' => Some("%a %b %e %T %Y"),
                    _ => None,
                };
                value = Some(if let Some(sub) = sub {
                    self.render(sub, invariant)?
                } else {
                    match code {
                        '%' => "%".into(),
                        'n' => "\n".into(),
                        't' => "\t".into(),
                        'z' => offset(z.offset().seconds(), colons),
                        'Z' => z
                            .time_zone()
                            .to_offset_info(z.timestamp())
                            .abbreviation()
                            .into(),
                        'a' | 'A' | 'b' | 'B' | 'h' | 'p' | 'P' => jiff::fmt::strtime::format(
                            format!("%{}", if code == 'h' { 'b' } else { code }),
                            z,
                        )
                        .map_err(|e| e.to_string())?,
                        _ => literal.into(),
                    }
                });
            }
            let mut s = value.unwrap();
            if code == 'P' || flip && matches!(code, 'p' | 'Z') {
                s = s.to_lowercase()
            } else if upper || flip && "aAbBh".contains(code) {
                s = s.to_uppercase()
            }
            if pad != Some('-') {
                out.extend(core::iter::repeat_n(
                    if matches!(pad, Some('0' | '+')) {
                        '0'
                    } else {
                        ' '
                    },
                    width.saturating_sub(s.len()),
                ))
            }
            out.push_str(&s);
        }
        Ok(out)
    }
}
pub fn offset(seconds: i32, colons: usize) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let abs = seconds.unsigned_abs();
    let (h, m, s) = (abs / 3600, abs / 60 % 60, abs % 60);
    match colons {
        0 => format!("{sign}{h:02}{m:02}"),
        1 => format!("{sign}{h:02}:{m:02}"),
        3 if m == 0 && s == 0 => format!("{sign}{h:02}"),
        3 if s == 0 => format!("{sign}{h:02}:{m:02}"),
        _ => format!("{sign}{h:02}:{m:02}:{s:02}"),
    }
}
