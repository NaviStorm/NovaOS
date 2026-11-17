//! Explications des éléments réellement consommés et des étapes calendaires.
use crate::{
    engine::{Output, Result, parse},
    format::{Fields, offset},
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use jiff::{SignedDuration, Zoned, tz::TimeZone};
use parse_datetime::{ParsedDateTime, Piece};

fn emit(out: &mut impl Output, s: &str) -> Result<()> {
    out.error(&format!("date: {s}\n"))
}
fn moment(z: &Zoned, zone: &str) -> String {
    format!("(Y-M-D) {}{zone}", z.strftime("%F %T"))
}

pub fn trace(
    text: &str,
    base: Option<&Zoned>,
    parsed: Option<&ParsedDateTime>,
    display: &TimeZone,
    name: &str,
    out: &mut impl Output,
) -> Result<()> {
    let trimmed = text.trim();
    let prefix = trimmed
        .strip_prefix("TZ=\"")
        .and_then(|v| v.split_once('"'));
    let source_name = prefix.map_or(name, |v| v.0);
    let raw = prefix.map_or(trimmed, |v| v.1.trim());
    let source = if prefix.is_some() {
        TimeZone::get(source_name)
            .or_else(|_| TimeZone::posix(source_name))
            .unwrap_or(TimeZone::UTC)
    } else {
        display.clone()
    };
    let date = parsed.map(|v| Fields::new(v, &source)).transpose()?;
    if raw.starts_with('@') && date.is_some() {
        let sec = date.as_ref().unwrap().seconds;
        emit(
            out,
            &format!("parsed number of seconds part: number of seconds: {sec}"),
        )?;
        emit(out, "input timezone: '@timespec' - always UTC")?;
    } else {
        let tokens = parse_datetime::inspect(raw);
        let mut absolute = Vec::new();
        let mut relative = [0i64; 7];
        let mut calendar = None;
        let mut clock = None;
        let mut weekday = None;
        let mut fixed = None;
        let mut adjusted = None;
        let mut local_dst = None;
        let mut has_relative = false;
        let current = base.cloned().map(|b| b.with_time_zone(source.clone()));
        let mut messages = Vec::new();
        for token in &tokens {
            let mut line = String::new();
            if !matches!(token.parts.first(), Some(Piece::Relative(_))) {
                absolute.push(token.raw)
            }
            for part in &token.parts {
                match *part {
                    Piece::Date(year, m, d) => {
                        let y = year
                            .map(i64::from)
                            .or_else(|| current.as_ref().map(|b| i64::from(b.year())))
                            .unwrap_or(0);
                        let input_year = token
                            .raw
                            .split('-')
                            .next()
                            .filter(|v| v.len() == 2)
                            .and_then(|v| v.parse::<i64>().ok())
                            .unwrap_or(y);
                        if y != input_year {
                            adjusted = Some((input_year, y))
                        }
                        calendar = Some((y, i64::from(m), i64::from(d)));
                        let val = format!("(Y-M-D) {input_year:04}-{m:02}-{d:02}");
                        let combined = token
                            .raw
                            .as_bytes()
                            .get(10)
                            .is_some_and(|&c| c == b'T' || c == b't');
                        if combined {
                            line = format!("parsed datetime part: {val} ")
                        } else {
                            let part = if token.raw.trim().bytes().all(|b| b.is_ascii_digit()) {
                                "number"
                            } else {
                                "date"
                            };
                            messages.push(format!("parsed {part} part: {val}"))
                        }
                    }
                    Piece::Time(h, m, s, ns, tz) => {
                        let val = format!("{h:02}:{m:02}:{s:02}");
                        clock = Some(val.clone());
                        if line.is_empty() {
                            line = "parsed time part: ".into()
                        }
                        line.push_str(&val);
                        if ns != 0 {
                            line.push_str(&format!(".{ns:09}"))
                        }
                        if let Some(off) = tz {
                            fixed = Some(off);
                            line.push_str(&format!(" UTC{}", offset(off, 3)))
                        }
                        messages.push(core::mem::take(&mut line));
                    }
                    Piece::Weekday(n, rank) => {
                        let day = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][n as usize];
                        let label = match rank {
                            0 => day.into(),
                            1 => format!("next/first {day}"),
                            -1 => format!("last {day}"),
                            _ => format!("{rank} {day}"),
                        };
                        messages.push(format!(
                            "parsed day part: {label} (day ordinal={rank} number={n})"
                        ));
                        weekday = Some(label)
                    }
                    Piece::Number => {
                        if calendar.is_some() && token.raw.len() >= 3 {
                            messages.push(format!("parsed number part: year: {}", token.raw))
                        } else if let Some(f) = &date {
                            if token.raw.len() >= 6 {
                                calendar = Some((
                                    f.zoned.year().into(),
                                    f.zoned.month().into(),
                                    f.zoned.day().into(),
                                ));
                                messages.push(format!(
                                    "parsed number part: (Y-M-D) {}",
                                    f.zoned.strftime("%F")
                                ))
                            } else {
                                clock = Some(f.zoned.strftime("%T").to_string());
                                messages.push(format!(
                                    "parsed number part: {}",
                                    clock.as_ref().unwrap()
                                ))
                            }
                        }
                    }
                    Piece::Offset(off) => {
                        let local = calendar
                            .and_then(|(y, m, d)| {
                                jiff::civil::Date::new(y.try_into().ok()?, m as i8, d as i8).ok()
                            })
                            .and_then(|d| d.at(12, 0, 0, 0).to_zoned(source.clone()).ok());
                        let info = local.as_ref().map(|z| source.to_offset_info(z.timestamp()));
                        if !token.raw.eq_ignore_ascii_case("UTC")
                            && info
                                .as_ref()
                                .is_some_and(|i| i.abbreviation().eq_ignore_ascii_case(token.raw))
                        {
                            let dst = info.unwrap().dst().is_dst();
                            local_dst = Some(dst);
                            messages
                                .push(format!("parsed local_zone part: isdst={}", u8::from(dst)))
                        } else {
                            fixed = Some(off);
                            messages.push(format!("parsed zone part: UTC{}", offset(off, 3)));
                        }
                    }
                    Piece::Relative(delta) => {
                        has_relative = true;
                        for (sum, n) in relative.iter_mut().zip(delta) {
                            *sum = sum.saturating_add(n)
                        }
                        let units = [
                            "year(s)", "month(s)", "day(s)", "hour(s)", "minutes", "seconds", "ns",
                        ];
                        let values: Vec<_> = relative
                            .iter()
                            .zip(units)
                            .filter(|(n, _)| **n != 0)
                            .map(|(n, u)| format!("{n:+} {u}"))
                            .collect();
                        let desc = if values.is_empty() {
                            "today/this/now".into()
                        } else {
                            values.join(" ")
                        };
                        messages.push(format!("parsed relative part: {desc}"))
                    }
                }
            }
        }
        let unknown = if tokens.is_empty() {
            Some(raw)
        } else {
            raw.rsplit_once(tokens.last().unwrap().raw)
                .map(|(_, tail)| tail.trim())
                .filter(|s| !s.is_empty())
        };
        if parsed.is_none()
            && let Some(word) = unknown
                .and_then(|s| s.split_ascii_whitespace().next())
                .filter(|s| s.bytes().all(|b| b.is_ascii_alphabetic()))
        {
            emit(
                out,
                &format!("error: unknown word '{}'", word.to_ascii_uppercase()),
            )?;
        }
        // GNU termine la réduction du fuseau après l'élément relatif qui le suit.
        for i in 1..messages.len() {
            if messages[i].starts_with("parsed relative")
                && messages[i - 1].starts_with("parsed zone")
            {
                messages.swap(i - 1, i)
            }
        }
        for msg in messages {
            emit(out, &msg)?
        }
        if parsed.is_none() && (calendar.is_none() || unknown.is_some()) {
            return emit(
                out,
                if text.ends_with('\n') {
                    "error: parsing failed, stopped at '\n'"
                } else {
                    "error: parsing failed"
                },
            );
        }
        let src = if let Some(off) = fixed {
            format!("parsed date/time string ({})", offset(off, 3))
        } else if prefix.is_some() {
            format!("TZ=\"{source_name}\" in date string")
        } else if source_name == "UTC0" {
            "TZ=\"UTC0\" environment value or -u".into()
        } else {
            format!(
                "TZ=\"{source_name}\" environment value{}",
                if local_dst == Some(true) { ", dst" } else { "" }
            )
        };
        emit(out, &format!("input timezone: {src}"))?;
        if let Some((a, b)) = adjusted {
            emit(out, &format!("warning: adjusting year value {a} to {b}"))?
        }
        let mut initial_zone = source.clone();
        if let Some(n) = fixed
            && let Ok(o) = jiff::tz::Offset::from_seconds(n)
        {
            initial_zone = o.to_time_zone()
        }
        let absolute = absolute.join(" ");
        let start = if absolute.is_empty() && has_relative {
            current.clone()
        } else {
            parse(&absolute, current.as_ref(), &source)
                .and_then(|p| p.into_zoned())
                .map(|z| z.with_time_zone(initial_zone.clone()))
        };
        if let Some(time) = &clock {
            emit(
                out,
                &format!("using specified time as starting value: '{time}'"),
            )?
        } else if has_relative
            && calendar.is_none()
            && weekday.is_none()
            && let Some(b) = &current
        {
            emit(
                out,
                &format!(
                    "using current time as starting value: '{}'",
                    b.strftime("%T")
                ),
            )?
        } else {
            emit(out, "warning: using midnight as starting time: 00:00:00")?
        }
        if calendar.is_none()
            && weekday.is_none()
            && let Some(b) = &current
        {
            emit(
                out,
                &format!(
                    "using current date as starting value: '(Y-M-D) {}'",
                    b.strftime("%F")
                ),
            )?
        }
        let suffix = fixed.map_or_else(String::new, |n| format!(" TZ={}", offset(n, 3)));
        if parsed.is_none() {
            if let Some((y, m, d)) = calendar {
                emit(out, "error: invalid date/time value:")?;
                let t = clock.as_deref().unwrap_or("00:00:00");
                emit(
                    out,
                    &format!("    user provided time: '(Y-M-D) {y:04}-{m:02}-{d:02} {t}{suffix}'"),
                )?;
                let target = format!("{y:04}-{m:02}-{d:02}T{t}");
                if let Ok(civil) = target.parse::<jiff::civil::DateTime>()
                    && let Ok(z) = civil.to_zoned(initial_zone)
                {
                    emit(
                        out,
                        &format!("       normalized time: '{}'", moment(&z, &suffix)),
                    )?;
                } else {
                    emit(out, "       time could not be normalized")?
                }
                for msg in [
                    "     possible reasons:",
                    "       nonexistent due to daylight-saving time;",
                    "       invalid day/month combination;",
                    "       missing timezone",
                ] {
                    emit(out, msg)?
                }
            }
            return Ok(());
        }
        if let Some(start) = start {
            if let Some(w) = weekday {
                if calendar.is_some() {
                    emit(
                        out,
                        &format!("warning: day ({w}) ignored when explicit dates are given"),
                    )?
                } else {
                    emit(
                        out,
                        &format!("new start date: '{w}' is '{}'", moment(&start, &suffix)),
                    )?
                }
            }
            emit(
                out,
                &format!("starting date/time: '{}'", moment(&start, &suffix)),
            )?;
            let [y, m, d, h, min, s, n] = relative;
            let duration = SignedDuration::new(
                h.saturating_mul(3600)
                    .saturating_add(min.saturating_mul(60))
                    .saturating_add(s),
                0,
            )
            .checked_add(SignedDuration::from_nanos(n));
            let f = date.as_ref().unwrap();
            if let Some(ts) = duration.and_then(|dt| f.zoned.timestamp().checked_sub(dt).ok()) {
                let after = ts.to_zoned(initial_zone.clone());
                if y != 0 || m != 0 || d != 0 {
                    if (y != 0 || m != 0) && start.day() != 15 {
                        emit(
                            out,
                            "warning: when adding relative months/years, it is recommended to specify the 15th of the months",
                        )?
                    }
                    if d != 0 && start.hour() != 12 {
                        emit(
                            out,
                            "warning: when adding relative days, it is recommended to specify noon",
                        )?
                    }
                    emit(
                        out,
                        &format!("after date adjustment ({y:+} years, {m:+} months, {d:+} days),"),
                    )?;
                    emit(
                        out,
                        &format!("    new date/time = '{}'", moment(&after, &suffix)),
                    )?;
                    if start.offset() != after.offset() {
                        emit(
                            out,
                            "warning: daylight saving time changed after date adjustment",
                        )?
                    }
                    if (y != 0 || m != 0) && start.day() != after.day() {
                        let months = i64::from(start.month()) - 1 + m;
                        emit(
                            out,
                            "warning: month/year adjustment resulted in shifted dates:",
                        )?;
                        emit(
                            out,
                            &format!(
                                "     adjusted Y M D: {:04} {:02} {:02}",
                                i64::from(start.year()) + y + months.div_euclid(12),
                                months.rem_euclid(12) + 1,
                                start.day()
                            ),
                        )?;
                        emit(
                            out,
                            &format!("   normalized Y M D: {}", after.strftime("%Y %m %d")),
                        )?;
                    }
                }
                emit(
                    out,
                    &format!(
                        "'{}' = {} epoch-seconds",
                        moment(&after, &suffix),
                        ts.as_nanosecond().div_euclid(1_000_000_000)
                    ),
                )?;
                if h != 0 || min != 0 || s != 0 || n != 0 {
                    emit(
                        out,
                        &format!(
                            "after time adjustment ({h:+} hours, {min:+} minutes, {s:+} seconds, {n:+} ns),"
                        ),
                    )?;
                    emit(out, &format!("    new time = {} epoch-seconds", f.seconds))?;
                }
            }
        }
    }
    if let (Some(parsed), Some(f)) = (parsed, date) {
        let utc = Fields::new(parsed, &TimeZone::UTC)?;
        let tz = if source_name == "UTC0" {
            "Universal Time".into()
        } else {
            format!("TZ=\"{source_name}\" environment value")
        };
        emit(out, &format!("timezone: {tz}"))?;
        emit(
            out,
            &format!(
                "final: {}.{:09} (epoch-seconds)",
                f.seconds,
                f.zoned.subsec_nanosecond()
            ),
        )?;
        emit(
            out,
            &format!("final: (Y-M-D) {} (UTC)", utc.render("%F %T", true)?),
        )?;
        emit(
            out,
            &format!(
                "final: (Y-M-D) {} (UTC{})",
                f.render("%F %T", true)?,
                offset(f.zoned.offset().seconds(), 3)
            ),
        )?;
    }
    Ok(())
}
