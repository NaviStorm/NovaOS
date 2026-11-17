use alloc::{format, vec::Vec};

#[derive(Clone, Copy, PartialEq)]
pub enum Existence {
    Existing,
    Last,
    Missing,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Links {
    Physical,
    Logical,
    Strip,
}

pub struct Settings<'a> {
    pub exist: Existence,
    pub links: Links,
    pub quiet: bool,
    pub zero: bool,
    pub base: Option<&'a [u8]>,
    pub to: Option<&'a [u8]>,
    pub names: Vec<&'a [u8]>,
}
pub enum Action<'a> {
    Help,
    Version,
    Paths(Settings<'a>),
}

pub fn parse<'a>(args: &[&'a [u8]], posix: bool) -> Result<Action<'a>, Vec<u8>> {
    let mut opt = Settings {
        exist: Existence::Last,
        links: Links::Physical,
        quiet: false,
        zero: false,
        base: None,
        to: None,
        names: Vec::new(),
    };
    let longs = &[
        ("canonicalize", b'E'),
        ("canonicalize-existing", b'e'),
        ("canonicalize-missing", b'm'),
        ("relative-to", 1),
        ("relative-base", 2),
        ("quiet", b'q'),
        ("strip", b's'),
        ("no-symlinks", b's'),
        ("zero", b'z'),
        ("logical", b'L'),
        ("physical", b'P'),
        ("help", 3),
        ("version", 4),
    ];
    let mut iter = args.iter().copied();
    let mut options = true;
    while let Some(arg) = iter.next() {
        if !options || !arg.starts_with(b"-") || arg == b"-" {
            opt.names.push(arg);
            options &= !posix;
            continue;
        }
        if arg == b"--" {
            options = false;
            continue;
        }
        let short;
        let flags = if let Some(long) = arg.strip_prefix(b"--") {
            let mut parts = long.splitn(2, |&c| c == b'=');
            let key = parts.next().unwrap();
            let mut matches: Vec<_> = longs
                .iter()
                .filter(|(s, _)| s.as_bytes().starts_with(key))
                .collect();
            if let Some(exact) = matches.iter().copied().find(|(s, _)| s.as_bytes() == key) {
                matches = alloc::vec![exact]
            }
            let Some(&&(name, flag)) = matches.first() else {
                return Err([b"unrecognized option '".as_slice(), arg, b"'"].concat());
            };
            if matches.len() > 1 {
                let mut msg = [
                    b"option '".as_slice(),
                    arg,
                    b"' is ambiguous; possibilities:",
                ]
                .concat();
                for (s, _) in matches {
                    msg.extend(format!(" '--{s}'").bytes())
                }
                return Err(msg);
            }
            let value = parts.next();
            if flag <= 2 {
                let value = value.or_else(|| iter.next()).ok_or_else(|| {
                    format!("option '--{name}' requires an argument").into_bytes()
                })?;
                if flag == 1 {
                    opt.to = Some(value)
                } else {
                    opt.base = Some(value)
                }
                continue;
            }
            if value.is_some() {
                return Err(format!("option '--{name}' doesn't allow an argument").into_bytes());
            }
            if flag == 3 {
                return Ok(Action::Help);
            }
            if flag == 4 {
                return Ok(Action::Version);
            }
            short = [flag];
            &short[..]
        } else {
            &arg[1..]
        };
        for &f in flags {
            match f {
                b'E' => opt.exist = Existence::Last,
                b'e' => opt.exist = Existence::Existing,
                b'm' => opt.exist = Existence::Missing,
                b'P' => opt.links = Links::Physical,
                b'L' => opt.links = Links::Logical,
                b's' => opt.links = Links::Strip,
                b'q' => opt.quiet = true,
                b'z' => opt.zero = true,
                _ => return Err([b"invalid option -- '".as_slice(), &[f], b"'"].concat()),
            }
        }
    }
    if opt.names.is_empty() {
        return Err(b"missing operand".to_vec());
    }
    Ok(Action::Paths(opt))
}
