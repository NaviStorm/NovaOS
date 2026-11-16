//! Les options précèdent la première donnée. Les opérandes restent des tranches d'octets.
pub enum Action<'a> {
    Help,
    Version,
    Names {
        paths: &'a [&'a [u8]],
        suffix: &'a [u8],
        zero: bool,
    },
}
pub enum Problem<'a> {
    Missing,
    Extra(&'a [u8]),
    Unknown(&'a [u8]),
    Short(u8),
    NeedSuffix(bool),
    Unexpected(&'static [u8]),
    Ambiguous(&'a [u8]),
}
const OPTIONS: &[(&[u8], u8)] = &[
    (b"multiple", b'a'),
    (b"suffix", b's'),
    (b"zero", b'z'),
    (b"help", b'h'),
    (b"version", b'v'),
];

pub fn parse<'a>(mut args: &'a [&'a [u8]]) -> Result<Action<'a>, Problem<'a>> {
    let mut multiple = false;
    let mut suffix = &b""[..];
    let mut zero = false;
    while let Some((&word, tail)) = args.split_first() {
        if word.len() < 2 || word[0] != b'-' {
            break;
        }
        args = tail;
        if word == b"--" {
            break;
        }
        let flags: &[u8];
        let (attached, long);
        if let Some(key) = word.strip_prefix(b"--") {
            let mut parts = key.splitn(2, |&b| b == b'=');
            let key = parts.next().unwrap();
            attached = parts.next();
            if key.is_empty() {
                return Err(Problem::Ambiguous(word));
            }
            let &(name, option) = OPTIONS
                .iter()
                .find(|(name, _)| name.starts_with(key))
                .ok_or(Problem::Unknown(word))?;
            if attached.is_some() && option != b's' {
                return Err(Problem::Unexpected(name));
            }
            match option {
                b'h' => return Ok(Action::Help),
                b'v' => return Ok(Action::Version),
                _ => {}
            }
            flags = match option {
                b'a' => b"a",
                b's' => b"s",
                _ => b"z",
            };
            long = true;
        } else {
            flags = &word[1..];
            attached = None;
            long = false;
        }
        for (i, &flag) in flags.iter().enumerate() {
            match flag {
                b'a' => multiple = true,
                b'z' => zero = true,
                b's' => {
                    multiple = true;
                    suffix = if let Some(value) = attached {
                        value
                    } else if i + 1 < flags.len() {
                        &flags[i + 1..]
                    } else {
                        let (&value, rest) = args.split_first().ok_or(Problem::NeedSuffix(long))?;
                        args = rest;
                        value
                    };
                    break;
                }
                _ => return Err(Problem::Short(flag)),
            }
        }
    }
    if args.is_empty() {
        return Err(Problem::Missing);
    }
    if !multiple {
        if let Some(extra) = args.get(2) {
            return Err(Problem::Extra(extra));
        }
        if let Some(value) = args.get(1) {
            suffix = value;
        }
        args = &args[..1];
    }
    Ok(Action::Names {
        paths: args,
        suffix,
        zero,
    })
}
