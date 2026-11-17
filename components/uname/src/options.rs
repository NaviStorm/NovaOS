use alloc::{format, vec::Vec};
pub const FIELDS: &[u8; 8] = b"snrvmpio";
const LONG: &[(&[u8], u8)] = &[
    (b"all", b'a'),
    (b"all-labeled", b'A'),
    (b"kernel-name", b's'),
    (b"sysname", b's'),
    (b"nodename", b'n'),
    (b"kernel-release", b'r'),
    (b"release", b'r'),
    (b"kernel-version", b'v'),
    (b"machine", b'm'),
    (b"processor", b'p'),
    (b"hardware-platform", b'i'),
    (b"operating-system", b'o'),
    (b"help", b'H'),
    (b"version", b'V'),
];

pub enum Action {
    Fields(Selection),
    Help,
    Version,
}
#[derive(Default)]
pub struct Selection {
    pub mask: u8,
    pub all: bool,
    pub labeled: bool,
}
pub enum Error<'a> {
    Option(Vec<u8>),
    Operand(&'a [u8]),
}
impl Error<'_> {
    pub fn message(&self, utf8: bool) -> Vec<u8> {
        let Self::Operand(arg) = self else {
            let Self::Option(message) = self else {
                unreachable!()
            };
            return message.clone();
        };
        let (left, right) = if utf8 { ("‘", "’") } else { ("'", "'") };
        let mut out = format!("extra operand {left}").into_bytes();
        let mut rest = *arg;
        while let Some((&b, tail)) = rest.split_first() {
            if rest.starts_with(right.as_bytes()) {
                out.push(b'\\');
            }
            match b {
                b'\\' => out.extend(b"\\\\"),
                7..=13 => out.extend([b'\\', b"abtnvfr"[(b - 7) as usize]]),
                32..=126 => out.push(b),
                _ => {
                    if utf8
                        && let Some(s) = rest
                            .get(..utf8_width(b))
                            .and_then(|s| core::str::from_utf8(s).ok())
                        && !s.chars().next().unwrap().is_control()
                    {
                        out.extend(s.as_bytes());
                        rest = &rest[s.len()..];
                        continue;
                    }
                    out.extend(format!("\\{b:03o}").bytes());
                }
            }
            rest = tail;
        }
        out.extend(right.bytes());
        out
    }
}
fn utf8_width(b: u8) -> usize {
    match b {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 1,
    }
}

fn long(arg: &[u8]) -> Result<u8, Error<'_>> {
    let mut parts = arg[2..].splitn(2, |&b| b == b'=');
    let prefix = parts.next().unwrap();
    let mut matches = LONG.iter().filter(|(name, _)| name.starts_with(prefix));
    let Some(&(mut name, mut code)) = matches.next() else {
        return Err(Error::Option(
            [b"unrecognized option '".as_slice(), arg, b"'"].concat(),
        ));
    };
    if name != prefix
        && let Some(&(exact, key)) = LONG.iter().find(|(name, _)| *name == prefix)
    {
        name = exact;
        code = key;
    } else if name != prefix && matches.any(|&(_, other)| other != code) {
        let mut msg = [
            b"option '".as_slice(),
            arg,
            b"' is ambiguous; possibilities:",
        ]
        .concat();
        for &(name, _) in LONG.iter().filter(|(name, _)| name.starts_with(prefix)) {
            msg.extend(b" '--");
            msg.extend(name);
            msg.push(b'\'');
        }
        return Err(Error::Option(msg));
    }
    if parts.next().is_some() {
        return Err(Error::Option(
            [
                b"option '--".as_slice(),
                name,
                b"' doesn't allow an argument",
            ]
            .concat(),
        ));
    }
    Ok(code)
}

pub fn parse<'a>(args: &[&'a [u8]], posix: bool) -> Result<Action, Error<'a>> {
    let mut fields = Selection::default();
    let mut operand = None;
    let mut options = true;
    for &arg in args {
        if !options || !arg.starts_with(b"-") || arg == b"-" {
            operand.get_or_insert(arg);
            options &= !posix;
            continue;
        }
        if arg == b"--" {
            options = false;
            continue;
        }
        let single;
        let codes = if arg.starts_with(b"--") {
            single = long(arg)?;
            match single {
                b'H' => return Ok(Action::Help),
                b'V' => return Ok(Action::Version),
                _ => {}
            }
            core::slice::from_ref(&single)
        } else {
            &arg[1..]
        };
        for &code in codes {
            if matches!(code, b'a' | b'A') {
                fields.all = true;
                fields.mask = u8::MAX;
                fields.labeled = code == b'A';
            } else if let Some(i) = FIELDS.iter().position(|&b| b == code) {
                fields.mask |= 1 << i;
            } else {
                return Err(Error::Option(
                    [b"invalid option -- '".as_slice(), &[code], b"'"].concat(),
                ));
            }
        }
    }
    if let Some(arg) = operand {
        return Err(Error::Operand(arg));
    }
    if fields.mask == 0 {
        fields.mask = 1
    }
    Ok(Action::Fields(fields))
}
