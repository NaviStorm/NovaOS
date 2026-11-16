use alloc::{format, string::String, vec::Vec};
pub const LIMIT: u64 = i64::MAX as u64;
const NAMES: &[(&str, u8)] = &[
    ("print-bytes", b'b'),
    ("print-chars", b'c'),
    ("ignore-initial", b'i'),
    ("verbose", b'l'),
    ("bytes", b'n'),
    ("silent", b's'),
    ("quiet", b's'),
    ("version", b'v'),
    ("help", b'h'),
];

pub enum Action<'a> {
    Help,
    Version,
    Compare(Options<'a>),
}
pub struct Options<'a> {
    pub names: [&'a [u8]; 2],
    pub skip: [u64; 2],
    pub limit: u64,
    pub visible: bool,
    pub list: bool,
    pub quiet: bool,
}
fn number(raw: &[u8]) -> Option<u64> {
    let text = core::str::from_utf8(raw).ok()?;
    let trimmed = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let sign = trimmed.starts_with('-');
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    let mut digits = unsigned;
    let radix = if digits.starts_with("0x") || digits.starts_with("0X") {
        digits = &digits[2..];
        16
    } else if digits.starts_with('0') {
        8
    } else {
        10
    };
    let mut value = 0u64;
    let mut end = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(radix) else { break };
        value = value.saturating_mul(radix.into()).saturating_add(d.into());
        end += 1;
    }
    if end == 0 {
        if digits != text || digits.is_empty() {
            return None;
        }
        value = 1;
    }
    if sign && value != 0 {
        return None;
    }
    let suffix = &digits[end..];
    if !suffix.is_empty() {
        let (&unit, tail) = suffix.as_bytes().split_first()?;
        let power = b"KMGTPEZY"
            .iter()
            .position(|&b| b == if unit == b'k' { b'K' } else { unit })?
            + 1;
        let scale = match tail {
            b"" | b"iB" => 1024,
            b"B" | b"D" => 1000,
            _ => return None,
        };
        for _ in 0..power {
            value = value.saturating_mul(scale)
        }
    }
    Some(value.min(LIMIT + 1))
}
pub fn quote(raw: &[u8]) -> String {
    let mut text = String::from("'");
    for &b in raw {
        match b {
            b'\'' | b'\\' => {
                text.push('\\');
                text.push(b as char)
            }
            b'\n' => text.push_str("\\n"),
            b'\t' => text.push_str("\\t"),
            b'\r' => text.push_str("\\r"),
            0x20..=0x7e => text.push(b as char),
            _ => text.push_str(&format!("\\{b:03o}")),
        }
    }
    text.push('\'');
    text
}
fn numeric(kind: &str, raw: &[u8]) -> Result<u64, String> {
    number(raw).ok_or_else(|| format!("invalid --{kind} value {}", quote(raw)))
}
pub fn parse<'a>(args: &[&'a [u8]], invocation: &[u8], posix: bool) -> Result<Action<'a>, String> {
    let mut o = Options {
        names: [b"-"; 2],
        skip: [0; 2],
        limit: LIMIT,
        visible: false,
        list: false,
        quiet: false,
    };
    let mut operands = Vec::new();
    let mut end = false;
    let mut iter = args.iter().copied();
    while let Some(arg) = iter.next() {
        if end || !arg.starts_with(b"-") || arg == b"-" {
            operands.push(arg);
            end |= posix;
            continue;
        }
        if arg == b"--" {
            end = true;
            continue;
        }
        let mut short = &arg[1..];
        let mut attached = None;
        let long = short.starts_with(b"-");
        let singleton;
        if long {
            let mut parts = short[1..].splitn(2, |&b| b == b'=');
            let key = parts.next().unwrap();
            attached = parts.next();
            let mut matches = NAMES.iter().filter(|(n, _)| n.as_bytes().starts_with(key));
            let Some(&(name, flag)) = matches.next() else {
                return Err(format!("unrecognized option {}", quote(arg)));
            };
            if matches.next().is_some() {
                let mut names = Vec::new();
                for &(n, _) in NAMES {
                    if n.as_bytes().starts_with(key) {
                        names.push(format!("'--{n}'"));
                    }
                }
                return Err(format!(
                    "option {} is ambiguous; possibilities: {}",
                    quote(arg),
                    names.join(" ")
                ));
            }
            if attached.is_some() && !matches!(flag, b'i' | b'n') {
                return Err(format!("option '--{name}' doesn't allow an argument"));
            }
            singleton = [flag];
            short = &singleton;
        }
        while let Some((&flag, rest)) = short.split_first() {
            short = rest;
            match flag {
                b'h' if long => return Ok(Action::Help),
                b'v' => return Ok(Action::Version),
                b'b' | b'c' => o.visible = true,
                b'l' => o.list = true,
                b's' => o.quiet = true,
                b'i' | b'n' => {
                    let arg = attached
                        .or_else(|| (!short.is_empty()).then_some(short))
                        .or_else(|| iter.next())
                        .ok_or_else(|| {
                            if long {
                                format!(
                                    "option '--{}' requires an argument",
                                    if flag == b'i' {
                                        "ignore-initial"
                                    } else {
                                        "bytes"
                                    }
                                )
                            } else {
                                format!("option requires an argument -- '{}'", flag as char)
                            }
                        })?;
                    if flag == b'n' {
                        o.limit = o.limit.min(numeric("bytes", arg)?);
                    } else {
                        let mut halves = arg.splitn(2, |&b| b == b':');
                        let first = halves.next().unwrap();
                        let left = number(first).ok_or_else(|| {
                            format!("invalid --ignore-initial value {}", quote(arg))
                        })?;
                        o.skip[0] = o.skip[0].max(left);
                        let right = match halves.next() {
                            Some(v) => numeric("ignore-initial", v)?,
                            None => o.skip[0],
                        };
                        o.skip[1] = o.skip[1].max(right);
                    }
                    break;
                }
                _ => return Err(format!("invalid option -- '{}'", flag as char)),
            }
            if o.list && o.quiet {
                return Err("options -l and -s are incompatible".into());
            }
        }
    }
    if operands.is_empty() {
        return Err(format!(
            "missing operand after {}",
            quote(args.last().copied().unwrap_or(invocation))
        ));
    }
    o.names[0] = operands[0];
    if operands.len() > 1 {
        o.names[1] = operands[1]
    }
    for (i, raw) in operands.iter().skip(2).take(2).enumerate() {
        o.skip[i] = o.skip[i].max(numeric("ignore-initial", raw)?)
    }
    if operands.len() > 4 {
        return Err(format!("extra operand {}", quote(operands[4])));
    }
    Ok(Action::Compare(o))
}
