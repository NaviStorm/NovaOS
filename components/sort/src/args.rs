use crate::engine::{Flags, Key};
use alloc::{format, string::String, vec::Vec};

const LONG: &[(&str, u8)] = &[
    ("ignore-leading-blanks", b'b'),
    ("dictionary-order", b'd'),
    ("ignore-case", b'f'),
    ("general-numeric-sort", b'g'),
    ("ignore-nonprinting", b'i'),
    ("month-sort", b'M'),
    ("human-numeric-sort", b'h'),
    ("numeric-sort", b'n'),
    ("random-sort", b'R'),
    ("random-source", 1),
    ("reverse", b'r'),
    ("sort", 2),
    ("version-sort", b'V'),
    ("batch-size", 3),
    ("check", b'c'),
    ("compress-program", 4),
    ("debug", 5),
    ("files0-from", 6),
    ("key", b'k'),
    ("merge", b'm'),
    ("output", b'o'),
    ("stable", b's'),
    ("buffer-size", b'S'),
    ("field-separator", b't'),
    ("temporary-directory", b'T'),
    ("parallel", 7),
    ("unique", b'u'),
    ("zero-terminated", b'z'),
    ("help", 8),
    ("version", 9),
];

#[derive(Default)]
pub struct Options {
    pub flags: Flags,
    pub keys: Vec<Key>,
    pub files: Vec<String>,
    pub sep: Option<u8>,
    pub zero: bool,
    pub unique: bool,
    pub stable: bool,
    pub merge: bool,
    pub check: Option<bool>,
    pub debug: bool,
    pub help: bool,
    pub version: bool,
    pub output: Option<String>,
    pub temps: Vec<String>,
    pub list: Option<String>,
    pub source: Option<String>,
    pub compressor: Option<String>,
    pub parallel: Option<usize>,
    pub batch: Option<usize>,
    pub sizes: Vec<String>,
    pub seed: [u8; 16],
    pub decimal: Option<u8>,
    pub thousands: Option<u8>,
    pub months: Vec<Vec<u8>>,
}
fn once(slot: &mut Option<String>, s: &str) -> Result<(), String> {
    if slot.as_ref().is_some_and(|old| old != s) {
        return Err("conflicting arguments".into());
    }
    *slot = Some(s.into());
    Ok(())
}
fn positive(s: &str) -> Result<usize, String> {
    s.parse::<usize>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| format!("invalid number: '{s}'"))
}
pub fn budget(s: &str, ram: usize) -> Result<usize, String> {
    let original = s;
    let s = s
        .trim_ascii_start()
        .strip_prefix('+')
        .unwrap_or(s.trim_ascii_start());
    let n = s.bytes().take_while(u8::is_ascii_digit).count();
    let v = if n == 0 && s.len() == 1 && b"EgGkKmMPQRtTYZ".contains(&s.as_bytes()[0]) {
        1
    } else {
        s[..n]
            .parse::<u128>()
            .map_err(|_| format!("invalid -S argument '{original}'"))?
    };
    let suffix = &s[n..];
    let units = b"KMGTPEZYRQ";
    let bytes = match suffix {
        "%" => v.saturating_mul(ram as u128) / 100,
        "b" => v,
        "" => v.saturating_mul(1024),
        _ if suffix.len() == 1 && b"EgGkKmMPQRtTYZ".contains(&suffix.as_bytes()[0]) => v
            .saturating_mul(
                1u128
                    << (10
                        * (units
                            .iter()
                            .position(|&b| b == suffix.as_bytes()[0].to_ascii_uppercase())
                            .unwrap()
                            + 1)),
            ),
        _ => return Err(format!("invalid suffix in -S argument '{original}'")),
    };
    usize::try_from(bytes).map_err(|_| format!("-S argument '{original}' too large"))
}
impl Options {
    pub fn buffer(&self, ram: usize, default: usize) -> Result<usize, String> {
        self.sizes
            .iter()
            .map(|s| budget(s, ram))
            .try_fold(None, |largest, n| Ok(Some(largest.unwrap_or(0).max(n?))))
            .map(|v| v.unwrap_or(default).max(4096))
    }
    pub fn delim(&self) -> u8 {
        if self.zero { 0 } else { b'\n' }
    }
    pub fn parse(words: &[&str]) -> Result<Self, String> {
        Self::parse_legacy(words, false)
    }
    pub fn parse_legacy(words: &[&str], old: bool) -> Result<Self, String> {
        let mut o = Self::default();
        let mut it = words.iter().copied().peekable();
        let mut end = false;
        while let Some(arg) = it.next() {
            if !end && let Some(start) = arg.strip_prefix('+') {
                let stop = it
                    .peek()
                    .and_then(|s| s.strip_prefix('-'))
                    .filter(|s| s.as_bytes().first().is_some_and(u8::is_ascii_digit));
                if (old || stop.is_some())
                    && let Some(key) = Key::legacy(start, stop)
                {
                    o.keys.push(key);
                    if stop.is_some() {
                        it.next();
                    }
                    continue;
                }
            }
            if end || arg == "-" || !arg.starts_with('-') {
                o.files.push(arg.into());
                continue;
            }
            if arg == "--" {
                end = true;
                continue;
            }
            let (codes, attached);
            if let Some(long) = arg.strip_prefix("--") {
                let (name, value) = long
                    .split_once('=')
                    .map_or((long, None), |(a, b)| (a, Some(b)));
                let mut matches = LONG.iter().filter(|(n, _)| n.starts_with(name));
                let exact = LONG.iter().find(|(n, _)| *n == name);
                let c = if let Some(x) = exact {
                    x.1
                } else {
                    let x = matches.next().ok_or_else(|| {
                        format!(
                            "unrecognized option '{arg}'\nTry 'sort --help' for more information."
                        )
                    })?;
                    if matches.next().is_some() {
                        return Err(format!("option '{arg}' is ambiguous"));
                    }
                    x.1
                };
                codes = alloc::vec![c];
                attached = value;
            } else {
                codes = arg.as_bytes()[1..].to_vec();
                attached = None;
            }
            for (i, &c) in codes.iter().enumerate() {
                // Solaris compatibility retained by GNU: a separate non-number
                // following -y is processed again as an option or operand.
                if c == b'y'
                    && i + 1 == codes.len()
                    && it
                        .peek()
                        .is_some_and(|v| !v.bytes().all(|b| b.is_ascii_digit()))
                {
                    break;
                }
                let takes = b"ktoSTy".contains(&c) || matches!(c, 1 | 2 | 3 | 4 | 6 | 7);
                let value = if takes {
                    if let Some(v) = attached {
                        v
                    } else if i + 1 < codes.len() {
                        core::str::from_utf8(&codes[i + 1..]).map_err(|_| "invalid argument")?
                    } else {
                        it.next().ok_or_else(||if arg.starts_with("--"){format!("option '{arg}' requires an argument\nTry 'sort --help' for more information.")}else{format!("option requires an argument -- '{}'\nTry 'sort --help' for more information.",char::from(c))})?
                    }
                } else {
                    if attached.is_some() && c != b'c' {
                        return Err(format!("option does not take an argument: {arg}"));
                    }
                    ""
                };
                match c {
                    b'b' | b'd' | b'f' | b'g' | b'h' | b'i' | b'M' | b'n' | b'R' | b'r' | b'V' => {
                        o.flags.add(c)?
                    }
                    b'k' => {
                        for (idx, c) in value.char_indices() {
                            if matches!(c, '.' | ',')
                                && !value
                                    .as_bytes()
                                    .get(idx + 1)
                                    .is_some_and(u8::is_ascii_digit)
                            {
                                return Err(format!(
                                    "invalid number after '{c}': invalid count at start of '{}'",
                                    &value[idx + 1..]
                                ));
                            }
                        }
                        o.keys.push(
                            Key::parse(value).map_err(|e| {
                                format!("{e}: invalid field specification '{value}'")
                            })?,
                        )
                    }
                    b't' => {
                        let sep = if value == "\\0" {
                            0
                        } else if value.len() == 1 {
                            value.as_bytes()[0]
                        } else {
                            return Err(if value.is_empty() {
                                "empty tab".into()
                            } else {
                                format!("multi-character tab '{value}'")
                            });
                        };
                        if o.sep.is_some_and(|s| s != sep) {
                            return Err("multiple field separators".into());
                        }
                        o.sep = Some(sep)
                    }
                    b'o' => {
                        once(&mut o.output, value).map_err(|_| "multiple output files specified")?
                    }
                    b'y' => (),
                    b'T' => o.temps.push(value.into()),
                    b'S' => {
                        budget(value, 1)?;
                        o.sizes.push(value.into())
                    }
                    b'C' | b'c' => {
                        let quiet = match attached {
                            None => c == b'C',
                            Some(s)
                                if !s.is_empty()
                                    && ("quiet".starts_with(s) || "silent".starts_with(s)) =>
                            {
                                true
                            }
                            Some(s) if !s.is_empty() && "diagnose-first".starts_with(s) => false,
                            Some(s) => {
                                return Err(format!(
                                    "invalid argument '{s}' for '--check'\nValid arguments are:\n  - 'quiet', 'silent'\n  - 'diagnose-first'\nTry 'sort --help' for more information."
                                ));
                            }
                        };
                        if o.check.is_some_and(|q| q != quiet) {
                            return Err("options '-cC' are incompatible".into());
                        }
                        o.check = Some(quiet)
                    }
                    b's' => o.stable = true,
                    b'u' => o.unique = true,
                    b'm' => o.merge = true,
                    b'z' => o.zero = true,
                    1 => once(&mut o.source, value)
                        .map_err(|_| "multiple random sources specified")?,
                    2 => {
                        let kinds = [
                            ("general-numeric", b'g'),
                            ("human-numeric", b'h'),
                            ("month", b'M'),
                            ("numeric", b'n'),
                            ("random", b'R'),
                            ("version", b'V'),
                        ];
                        let kind = kinds
                            .iter()
                            .find(|(n, _)| !value.is_empty() && n.starts_with(value))
                            .ok_or_else(||format!("invalid argument '{value}' for '--sort'\nValid arguments are:\n  - 'general-numeric'\n  - 'human-numeric'\n  - 'month'\n  - 'numeric'\n  - 'random'\n  - 'version'\nTry 'sort --help' for more information."))?;
                        o.flags.add(kind.1)?
                    }
                    3 => {
                        let b = match value.parse::<usize>() {
                            Ok(n) => n,
                            Err(_)
                                if !value.is_empty()
                                    && value.bytes().all(|b| b.is_ascii_digit()) =>
                            {
                                usize::MAX
                            }
                            Err(_) => {
                                return Err(format!("invalid --batch-size argument '{value}'"));
                            }
                        };
                        if b < 2 {
                            return Err(format!(
                                "invalid --batch-size argument '{value}'\nsort: minimum --batch-size argument is '2'"
                            ));
                        }
                        o.batch = Some(b)
                    }
                    4 => once(&mut o.compressor, value)
                        .map_err(|_| "multiple compress programs specified")?,
                    5 => o.debug = true,
                    6 => once(&mut o.list, value)?,
                    7 => {
                        o.parallel = Some(if value.parse::<usize>() == Ok(0) {
                            return Err("number in parallel must be nonzero".into());
                        } else {
                            positive(value)
                                .map_err(|_| format!("invalid --parallel argument '{value}'"))?
                        })
                    }
                    8 => {
                        o.help = true;
                        return Ok(o);
                    }
                    9 => {
                        o.version = true;
                        return Ok(o);
                    }
                    _ => {
                        return Err(format!(
                            "invalid option -- '{}'\nTry 'sort --help' for more information.",
                            char::from(c)
                        ));
                    }
                }
                if takes {
                    break;
                }
            }
        }
        if o.help || o.version {
            return Ok(o);
        }
        if o.list.is_some() && !o.files.is_empty() {
            return Err(format!(
                "extra operand '{}'\nfile operands cannot be combined with --files0-from\nTry 'sort --help' for more information.",
                o.files[0]
            ));
        }
        if let Some(q) = o.check {
            let c = if q { 'C' } else { 'c' };
            if o.files.len() > 1 {
                return Err(format!(
                    "extra operand '{}' not allowed with -{c}",
                    o.files[1]
                ));
            }
            if o.output.is_some() {
                return Err(format!("options '-{c}o' are incompatible"));
            }
            if o.debug {
                return Err(format!("options '-{c} --debug' are incompatible"));
            }
        }
        if o.debug && o.output.is_some() {
            return Err("options '-o --debug' are incompatible".into());
        }
        o.validate_flags()?;
        Ok(o)
    }
}
