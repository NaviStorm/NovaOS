use crate::engine::{Config, Encoding, Mode};
use alloc::{format, string::String, vec, vec::Vec};

pub enum Action<'a> {
    Help,
    Version,
    Run(Config, Vec<&'a [u8]>),
}
const LONG: &[(&str, u8)] = &[
    ("bytes", b'b'),
    ("characters", b'c'),
    ("complement", b'C'),
    ("delimiter", b'd'),
    ("fields", b'f'),
    ("no-partial", b'n'),
    ("only-delimited", b's'),
    ("output-delimiter", b'O'),
    ("whitespace-delimited", b'w'),
    ("zero-terminated", b'z'),
    ("help", b'H'),
    ("version", b'V'),
];
fn show(s: &[u8]) -> String {
    String::from_utf8_lossy(s).into_owned()
}

fn quote(s: &str, utf8: bool) -> String {
    if utf8 {
        format!("‘{s}’")
    } else {
        format!("'{s}'")
    }
}
fn ranges(
    list: &[u8],
    field: bool,
    complement: bool,
    utf8: bool,
) -> Result<Vec<(u64, u64)>, String> {
    let zero = if field {
        "fields are numbered from 1"
    } else {
        "byte/character positions are numbered from 1"
    };
    let invalid = if field {
        "invalid field range"
    } else {
        "invalid byte or character range"
    };
    let mut result = Vec::new();
    let mut offset = 0;
    for item in list.split(|b| matches!(b, b',' | b' ' | b'\t')) {
        let mut ends = [None, None];
        let mut side = 0;
        let mut i = 0;
        while i < item.len() {
            if item[i] == b'-' {
                if side == 1 {
                    return Err(invalid.into());
                }
                if ends[0] == Some(0) {
                    return Err(zero.into());
                }
                side = 1;
                i += 1;
                continue;
            }
            if !item[i].is_ascii_digit() {
                let label = if field {
                    "invalid field value"
                } else {
                    "invalid byte/character position"
                };
                return Err(format!(
                    "{label} {}",
                    quote(&show(&list[offset + i..]), utf8)
                ));
            }
            let start = i;
            while i < item.len() && item[i].is_ascii_digit() {
                i += 1
            }
            let digits = core::str::from_utf8(&item[start..i]).unwrap();
            let value = digits
                .parse::<u64>()
                .ok()
                .filter(|n| *n < u64::MAX)
                .ok_or_else(|| {
                    let label = if field {
                        "field number"
                    } else {
                        "byte/character offset"
                    };
                    format!("{label} {} is too large", quote(digits, utf8))
                })?;
            ends[side] = Some(value);
        }
        let (lo, hi) = if side == 0 {
            let n = ends[0].unwrap_or(0);
            if n == 0 {
                return Err(zero.into());
            }
            (n, n)
        } else {
            if ends == [None, None] {
                return Err("invalid range with no endpoint: -".into());
            }
            (ends[0].unwrap_or(1), ends[1].unwrap_or(u64::MAX))
        };
        if hi < lo {
            return Err("invalid decreasing range".into());
        }
        result.push((lo, hi));
        offset += item.len() + 1;
    }
    result.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (lo, hi) in result {
        if let Some(last) = merged.last_mut()
            && last.1 >= lo
        {
            last.1 = last.1.max(hi)
        } else {
            merged.push((lo, hi))
        }
    }
    if !complement {
        return Ok(merged);
    }
    let mut gaps = Vec::new();
    let mut next = 1;
    for (lo, hi) in merged {
        if next < lo {
            gaps.push((next, lo - 1))
        }
        if hi == u64::MAX {
            return Ok(gaps);
        }
        next = hi + 1;
    }
    gaps.push((next, u64::MAX));
    Ok(gaps)
}

pub fn parse<'a>(
    args: &[&'a [u8]],
    encoding: impl Into<Encoding>,
    posix: bool,
) -> Result<Action<'a>, String> {
    let encoding = encoding.into();
    let utf8 = matches!(encoding, Encoding::Utf8);
    let mut cfg = Config::new(encoding);
    let mut list = None;
    let mut complement = false;
    let mut ws = false;
    let mut custom = false;
    let mut shorthand = false;
    let mut files = Vec::new();
    let mut it = args.iter().copied();
    while let Some(word) = it.next() {
        if word == b"--" {
            files.extend(it);
            break;
        }
        if word == b"-" || !word.starts_with(b"-") {
            files.push(word);
            if posix {
                files.extend(it);
                break;
            }
            continue;
        }
        let mut short = &word[1..];
        let long = short.starts_with(b"-");
        while !short.is_empty() {
            let (key, name, mut arg) = if long {
                let spec = &short[1..];
                let n = spec.iter().position(|b| *b == b'=').unwrap_or(spec.len());
                let prefix = &spec[..n];
                let mut names = LONG
                    .iter()
                    .filter(|(name, _)| name.as_bytes().starts_with(prefix));
                let Some(&(name, key)) = names.next() else {
                    return Err(format!("unrecognized option '{}'", show(word)));
                };
                if names.next().is_some() && name.as_bytes() != prefix {
                    let choices: Vec<_> = LONG
                        .iter()
                        .filter(|(s, _)| s.as_bytes().starts_with(prefix))
                        .map(|(s, _)| format!("'--{s}'"))
                        .collect();
                    return Err(format!(
                        "option '{}' is ambiguous; possibilities: {}",
                        show(word),
                        choices.join(" ")
                    ));
                }
                short = &[];
                (key, name, spec.get(n + 1..))
            } else {
                let key = short[0];
                short = &short[1..];
                if !b"bcdfFnOszw".contains(&key) {
                    return Err(format!("invalid option -- '{}'", key as char));
                }
                (key, "", None)
            };
            if b"bcdfFO".contains(&key) {
                if !short.is_empty() {
                    arg = Some(short);
                    short = &[]
                }
                if arg.is_none() {
                    arg = it.next()
                }
                if arg.is_none() {
                    return Err(if long {
                        format!("option '--{name}' requires an argument")
                    } else {
                        format!("option requires an argument -- '{}'", key as char)
                    });
                }
            } else if arg.is_some() && key != b'w' {
                return Err(format!("option '--{name}' doesn't allow an argument"));
            }
            let v = arg.unwrap_or_default();
            match key {
                b'H' => return Ok(Action::Help),
                b'V' => return Ok(Action::Version),
                b'b' | b'c' | b'f' | b'F' => {
                    if list.replace(v).is_some() {
                        return Err("only one list may be specified".into());
                    }
                    cfg.mode = match key {
                        b'b' => Mode::Bytes,
                        b'c' => Mode::Chars,
                        _ => Mode::Fields,
                    };
                    shorthand = key == b'F';
                }
                b'd' => {
                    if !v.is_empty() && encoding.width(v) != v.len() {
                        return Err("the delimiter must be a single character".into());
                    }
                    custom = true;
                    cfg.delim = nonempty(v);
                }
                b'O' => cfg.output = Some(nonempty(v)),
                b'C' => complement = true,
                b's' => cfg.suppress = true,
                b'z' => cfg.end = 0,
                b'n' => cfg.partial = true,
                b'w' => {
                    ws = true;
                    cfg.trim = arg.is_some();
                    if cfg.trim && !b"trimmed".starts_with(v) {
                        return Err(format!(
                            "invalid argument {} for {}\nValid arguments are:\n  - {}",
                            quote(&show(v), utf8),
                            quote("--whitespace-delimited", utf8),
                            quote("trimmed", utf8)
                        ));
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    let list = list.ok_or("you must specify a list of bytes, characters, or fields")?;
    if cfg.mode != Mode::Fields {
        if custom || ws {
            return Err("an input delimiter makes sense\n\tonly when operating on fields".into());
        }
        if cfg.suppress {
            return Err(
                "suppressing non-delimited lines makes sense\n\tonly when operating on fields"
                    .into(),
            );
        }
    }
    if custom && ws {
        return Err("-d and -w are mutually exclusive".into());
    }
    cfg.whitespace = ws || shorthand && !custom;
    if shorthand && cfg.output.is_none() {
        cfg.output = Some(vec![b' '])
    }
    cfg.ranges = ranges(list, cfg.mode == Mode::Fields, complement, utf8)?;
    if files.is_empty() {
        files.push(b"-")
    }
    Ok(Action::Run(cfg, files))
}
fn nonempty(s: &[u8]) -> Vec<u8> {
    if s.is_empty() { vec![0] } else { s.to_vec() }
}
