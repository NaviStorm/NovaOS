use super::engine::{Options, Separator};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
pub enum Action<'a> {
    Help,
    Version,
    Run(Options, Vec<&'a [u8]>),
}
// Préfixe privé d'argv construit par nsh. Les deux côtés emploient le même contexte.
#[allow(dead_code)]
pub fn native<'a>(args: &[&'a [u8]]) -> Result<Action<'a>, String> {
    if let Some((first, rest)) = args.split_first()
        && let Some([mode @ b'0'..=b'7']) = first.strip_prefix(b"\0uniq:")
    {
        let flags = mode - b'0';
        return parse(rest, flags & 1 != 0, flags & 2 != 0, flags & 4 != 0);
    }
    parse(args, false, false, false)
}
const NAMES: &[(&str, u8)] = &[
    ("count", b'c'),
    ("repeated", b'd'),
    ("all-repeated", b'D'),
    ("group", b'g'),
    ("ignore-case", b'i'),
    ("unique", b'u'),
    ("skip-fields", b'f'),
    ("skip-chars", b's'),
    ("check-chars", b'w'),
    ("zero-terminated", b'z'),
    ("help", b'h'),
    ("version", b'v'),
];

fn quoted(s: &str, utf8: bool) -> String {
    if utf8 {
        format!("‘{s}’")
    } else {
        format!("'{s}'")
    }
}
fn method(group: bool, value: Option<&[u8]>, utf8: bool) -> Result<Separator, String> {
    let methods: &[(&str, Separator)] = if group {
        &[
            ("prepend", Separator::Prepend),
            ("append", Separator::Append),
            ("separate", Separator::Separate),
            ("both", Separator::Both),
        ]
    } else {
        &[
            ("none", Separator::None),
            ("prepend", Separator::Prepend),
            ("separate", Separator::Separate),
        ]
    };
    let Some(v) = value else {
        return Ok(if group {
            Separator::Separate
        } else {
            Separator::None
        });
    };
    let mut hits = methods.iter().filter(|(m, _)| m.as_bytes().starts_with(v));
    let first = hits.next();
    if let Some((_, m)) = first
        && hits.next().is_none()
    {
        return Ok(*m);
    }
    let err = if first.is_some() {
        "ambiguous"
    } else {
        "invalid"
    };
    let name = if group { "group" } else { "all-repeated" };
    let mut msg = format!(
        "{err} argument {} for {}\nValid arguments are:",
        quoted(&String::from_utf8_lossy(v), utf8),
        quoted(&format!("--{name}"), utf8)
    );
    for (m, _) in methods {
        msg.push_str(&format!("\n  - {}", quoted(m, utf8)))
    }
    Err(usage(msg))
}
fn usage(s: String) -> String {
    format!("{s}\nTry 'uniq --help' for more information.")
}
fn decimal(raw: &[u8]) -> Option<usize> {
    let mut s = raw;
    while s.first().is_some_and(|b| b.is_ascii_whitespace()) {
        s = &s[1..]
    }
    let negative = s.first() == Some(&b'-');
    if negative || s.first() == Some(&b'+') {
        s = &s[1..]
    }
    if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let n = s.iter().fold(0usize, |n, b| {
        n.saturating_mul(10).saturating_add(usize::from(b - b'0'))
    });
    (!negative || n == 0).then_some(n)
}

pub fn parse<'a>(
    args: &[&'a [u8]],
    utf8: bool,
    posix: bool,
    legacy_plus: bool,
) -> Result<Action<'a>, String> {
    let mut o = Options {
        utf8,
        ..Default::default()
    };
    let mut files = Vec::new();
    let mut end = false;
    let mut i = 0;
    let mut modern_fields = false;
    while i < args.len() {
        let a = args[i];
        i += 1;
        if !end && a == b"--" {
            end = true;
            continue;
        }
        if end || a == b"-" || !a.starts_with(b"-") {
            if !end
                && legacy_plus
                && a.starts_with(b"+")
                && let Some(n) = decimal(a)
            {
                o.skip = n;
                continue;
            }
            if files.len() == 2 {
                return Err(usage(format!(
                    "extra operand {}",
                    quoted(&String::from_utf8_lossy(a), utf8)
                )));
            }
            files.push(a);
            end |= posix;
            continue;
        }
        let long = a.starts_with(b"--");
        let mut cluster = &a[1..];
        let mut value = None;
        let mut lname = "";
        if long {
            let word = &a[2..];
            let key = if let Some(k) = word.iter().position(|&b| b == b'=') {
                value = Some(&word[k + 1..]);
                &word[..k]
            } else {
                word
            };
            let matches: Vec<_> = NAMES
                .iter()
                .filter(|(n, _)| !key.is_empty() && n.as_bytes().starts_with(key))
                .collect();
            let &(name, ref code) = match matches.as_slice() {
                [one] => *one,
                [] => {
                    return Err(usage(format!(
                        "unrecognized option '{}'",
                        String::from_utf8_lossy(a)
                    )));
                }
                _ => {
                    let choices = matches
                        .iter()
                        .map(|(n, _)| format!(" '--{n}'"))
                        .collect::<String>();
                    return Err(usage(format!(
                        "option '{}' is ambiguous; possibilities:{choices}",
                        String::from_utf8_lossy(a)
                    )));
                }
            };
            lname = name;
            cluster = core::slice::from_ref(code);
            if value.is_some() && !b"Dgfsw".contains(code) {
                return Err(usage(format!(
                    "option '--{name}' doesn't allow an argument"
                )));
            }
        }
        while let Some((&c, rest)) = cluster.split_first() {
            cluster = rest;
            match c {
                b'h' if long => return Ok(Action::Help),
                b'v' if long => return Ok(Action::Version),
                b'c' => o.count = true,
                b'd' => o.repeated = true,
                b'u' => o.unique = true,
                b'z' => o.delimiter = 0,
                b'i' => o.ignore_case = true,
                b'D' => o.all = Some(method(false, value, utf8)?),
                b'g' if long => o.group = Some(method(true, value, utf8)?),
                b'0'..=b'9' => {
                    if modern_fields {
                        o.fields = 0;
                        modern_fields = false
                    }
                    o.fields = o
                        .fields
                        .saturating_mul(10)
                        .saturating_add(usize::from(c - b'0'))
                }
                b'f' | b's' | b'w' => {
                    let raw = if let Some(v) = value {
                        v
                    } else if !cluster.is_empty() {
                        let v = cluster;
                        cluster = &[];
                        v
                    } else if i < args.len() {
                        i += 1;
                        args[i - 1]
                    } else {
                        return Err(usage(if long {
                            format!("option '--{lname}' requires an argument")
                        } else {
                            format!("option requires an argument -- '{}'", char::from(c))
                        }));
                    };
                    let n = decimal(raw).ok_or_else(|| {
                        format!(
                            "{}: invalid number of {}",
                            String::from_utf8_lossy(raw),
                            match c {
                                b'f' => "fields to skip",
                                b's' => "bytes to skip",
                                _ => "bytes to compare",
                            }
                        )
                    })?;
                    match c {
                        b'f' => {
                            modern_fields = true;
                            o.fields = n
                        }
                        b's' => o.skip = n,
                        _ => o.width = n,
                    }
                }
                _ => return Err(usage(format!("invalid option -- '{}'", char::from(c)))),
            }
        }
    }
    if o.group.is_some() && (o.count || o.unique || o.repeated || o.all.is_some()) {
        return Err(usage(
            "--group is mutually exclusive with -c/-d/-D/-u".to_string(),
        ));
    }
    if o.count && o.all.is_some() {
        return Err(usage(
            "printing all duplicated lines and repeat counts is meaningless".to_string(),
        ));
    }
    Ok(Action::Run(o, files))
}
