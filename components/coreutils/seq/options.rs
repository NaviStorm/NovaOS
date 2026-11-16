use alloc::{format, string::String};

// Les diagnostics getopt et %c peuvent contenir un octet non UTF-8 sur Darwin.
pub struct Diagnostic(pub alloc::vec::Vec<u8>);
impl From<String> for Diagnostic {
    fn from(text: String) -> Self {
        Self(text.into_bytes())
    }
}
impl Diagnostic {
    pub fn bytes(prefix: &[u8], token: &[u8], suffix: &[u8]) -> Self {
        Self([prefix, token, suffix].concat())
    }
    fn hint(mut self) -> Self {
        self.0
            .extend_from_slice(b"\nTry 'seq --help' for more information.");
        self
    }
}

pub enum Options<'a> {
    Help,
    Version,
    Numbers {
        format: Option<&'a [u8]>,
        separator: &'a [u8],
        equal: bool,
        operands: &'a [&'a [u8]],
    },
}

pub fn usage(reason: String) -> String {
    format!("{reason}\nTry 'seq --help' for more information.")
}

pub fn quote(value: &[u8], utf8: bool) -> String {
    let mut out = String::from(if utf8 { "‘" } else { "'" });
    let mut rest = value;
    while !rest.is_empty() {
        let b = rest[0];
        rest = &rest[1..];
        match b {
            7 => out.push_str("\\a"),
            8 => out.push_str("\\b"),
            11 => out.push_str("\\v"),
            12 => out.push_str("\\f"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b'\\' => out.push_str("\\\\"),
            b'\'' if !utf8 => out.push_str("\\'"),
            32..=126 => out.push(b as char),
            128..=255 if utf8 => {
                let n = match b {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => 0,
                };
                let mut bytes = [0u8; 4];
                bytes[0] = b;
                if n > 0 && rest.len() >= n - 1 {
                    bytes[1..n].copy_from_slice(&rest[..n - 1]);
                    if let Ok(s) = core::str::from_utf8(&bytes[..n]) {
                        if s == "’" {
                            out.push('\\');
                        }
                        if s.chars().next().is_some_and(char::is_control) {
                            for x in &bytes[..n] {
                                out.push_str(&format!("\\{x:03o}"));
                            }
                        } else {
                            out.push_str(s);
                        }
                        rest = &rest[n - 1..];
                        continue;
                    }
                }
                out.push_str(&format!("\\{b:03o}"));
            }
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push(if utf8 { '’' } else { '\'' });
    out
}

pub fn parse<'a>(args: &'a [&'a [u8]]) -> Result<Options<'a>, Diagnostic> {
    let mut words = args.iter();
    let (mut format, mut equal, mut separator) = (None, false, &b"\n"[..]);
    let names = ["equal-width", "format", "separator", "help", "version"];
    while let Some(&arg) = words.as_slice().first() {
        if arg == b"--" {
            words.next();
            break;
        }
        if arg.len() < 2 || arg[0] != b'-' || arg[1].is_ascii_digit() || arg[1] == b'.' {
            break;
        }
        words.next();
        if let Some(long) = arg.strip_prefix(b"--") {
            let mut parts = long.splitn(2, |&b| b == b'=');
            let name = parts.next().unwrap();
            let attached = parts.next();
            if name.is_empty() {
                return Err(Diagnostic::bytes(b"option '",arg,b"' is ambiguous; possibilities: '--equal-width' '--format' '--separator' '--help' '--version'").hint());
            }
            let full = names
                .iter()
                .find(|s| s.as_bytes().starts_with(name))
                .ok_or_else(|| Diagnostic::bytes(b"unrecognized option '", arg, b"'").hint())?;
            if matches!(*full, "format" | "separator") {
                let value = attached
                    .or_else(|| words.next().copied())
                    .ok_or_else(|| usage(format!("option '--{full}' requires an argument")))?;
                if *full == "format" {
                    format = Some(value)
                } else {
                    separator = value
                }
            } else {
                if attached.is_some() {
                    return Err(
                        usage(format!("option '--{full}' doesn't allow an argument")).into(),
                    );
                }
                match *full {
                    "help" => return Ok(Options::Help),
                    "version" => return Ok(Options::Version),
                    _ => equal = true,
                }
            }
        } else {
            for (i, &flag) in arg.iter().enumerate().skip(1) {
                match flag {
                    b'w' => equal = true,
                    b'f' | b's' => {
                        let value = if i + 1 < arg.len() {
                            &arg[i + 1..]
                        } else {
                            words.next().copied().ok_or_else(|| {
                                usage(format!("option requires an argument -- '{}'", flag as char))
                            })?
                        };
                        if flag == b'f' {
                            format = Some(value)
                        } else {
                            separator = value
                        }
                        break;
                    }
                    _ => {
                        return Err(Diagnostic::bytes(b"invalid option -- '", &[flag], b"'").hint());
                    }
                }
            }
        }
    }
    Ok(Options::Numbers {
        format,
        separator,
        equal,
        operands: words.as_slice(),
    })
}
