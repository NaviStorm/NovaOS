use alloc::{format, string::String, vec::Vec};

pub enum Action<'a> {
    Help,
    Version,
    Paths { names: Vec<&'a [u8]>, zero: bool },
}

pub fn parse<'a>(args: &[&'a [u8]], posix: bool) -> Result<Action<'a>, Vec<u8>> {
    let mut names = Vec::new();
    let mut zero = false;
    let mut options = true;
    for &arg in args {
        if !options || arg == b"-" || !arg.starts_with(b"-") {
            names.push(arg);
            options &= !posix;
        } else if arg == b"--" {
            options = false;
        } else if let Some(long) = arg.strip_prefix(b"--") {
            let mut parts = long.splitn(2, |b| *b == b'=');
            let prefix = parts.next().unwrap();
            if prefix.is_empty() {
                return Err([
                    b"option '".as_slice(),
                    arg,
                    b"' is ambiguous; possibilities: '--zero' '--help' '--version'",
                ]
                .concat());
            }
            let Some(name) = ["zero", "help", "version"]
                .into_iter()
                .find(|name| name.as_bytes().starts_with(prefix))
            else {
                return Err([b"unrecognized option '".as_slice(), arg, b"'"].concat());
            };
            if parts.next().is_some() {
                return Err(format!("option '--{name}' doesn't allow an argument").into_bytes());
            }
            match name {
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                _ => zero = true,
            }
        } else {
            if let Some(b) = arg[1..].iter().find(|&&b| b != b'z') {
                return Err([
                    b"invalid option -- '".as_slice(),
                    core::slice::from_ref(b),
                    b"'",
                ]
                .concat());
            }
            zero = true;
        }
    }
    if names.is_empty() {
        return Err(String::from("missing operand").into_bytes());
    }
    Ok(Action::Paths { names, zero })
}
