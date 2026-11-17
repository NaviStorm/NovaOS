use alloc::{format, vec::Vec};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Mode {
    #[default]
    Raw,
    Existing,
    Last,
    Missing,
}
#[derive(Default)]
pub struct Flags {
    pub mode: Mode,
    pub zero: bool,
    pub no_newline: bool,
    pub verbose: bool,
}
pub enum Action<'a> {
    Help,
    Version,
    Paths { names: Vec<&'a [u8]>, flags: Flags },
}
const LONG: &[(&str, u8)] = &[
    ("canonicalize", b'f'),
    ("canonicalize-existing", b'e'),
    ("canonicalize-missing", b'm'),
    ("no-newline", b'n'),
    ("quiet", b'q'),
    ("silent", b's'),
    ("verbose", b'v'),
    ("zero", b'z'),
    ("help", b'H'),
    ("version", b'V'),
];

fn long_option(arg: &[u8]) -> Result<u8, Vec<u8>> {
    let mut parts = arg[2..].splitn(2, |&b| b == b'=');
    let key = parts.next().unwrap();
    let mut matching = LONG
        .iter()
        .filter(|(name, _)| name.as_bytes().starts_with(key));
    let Some(&(name, flag)) = matching.next() else {
        return Err([b"unrecognized option '".as_slice(), arg, b"'"].concat());
    };
    if name.as_bytes() != key && matching.next().is_some() {
        let mut msg = [
            b"option '".as_slice(),
            arg,
            b"' is ambiguous; possibilities:",
        ]
        .concat();
        for (name, _) in LONG
            .iter()
            .filter(|(name, _)| name.as_bytes().starts_with(key))
        {
            msg.extend(format!(" '--{name}'").bytes());
        }
        return Err(msg);
    }
    if parts.next().is_some() {
        return Err(format!("option '--{name}' doesn't allow an argument").into_bytes());
    }
    Ok(flag)
}

pub fn parse<'a>(args: &[&'a [u8]], posix: bool) -> Result<Action<'a>, Vec<u8>> {
    let mut names = Vec::new();
    let mut flags = Flags::default();
    let mut options = true;
    for &arg in args {
        if !options || !arg.starts_with(b"-") || arg == b"-" {
            names.push(arg);
            options &= !posix;
        } else if arg == b"--" {
            options = false;
        } else {
            let long;
            let cluster = if arg.starts_with(b"--") {
                long = [long_option(arg)?];
                match long[0] {
                    b'H' => return Ok(Action::Help),
                    b'V' => return Ok(Action::Version),
                    _ => &long[..],
                }
            } else {
                &arg[1..]
            };
            for &c in cluster {
                match c {
                    b'f' => flags.mode = Mode::Last,
                    b'e' => flags.mode = Mode::Existing,
                    b'm' => flags.mode = Mode::Missing,
                    b'n' => flags.no_newline = true,
                    b'z' => flags.zero = true,
                    b'q' | b's' => flags.verbose = false,
                    b'v' => flags.verbose = true,
                    _ => return Err([b"invalid option -- '".as_slice(), &[c], b"'"].concat()),
                }
            }
        }
    }
    if names.is_empty() {
        return Err(b"missing operand".to_vec());
    }
    flags.verbose |= posix;
    Ok(Action::Paths { names, flags })
}
