// Les deux options documentaires s’arrêtent au premier résultat getopt.
use alloc::vec::Vec;

pub enum Action {
    Help,
    Version,
    Wait(Option<usize>),
}

pub fn options<'a>(args: impl Iterator<Item = &'a [u8]>, posix: bool) -> Result<Action, Vec<u8>> {
    for (index, arg) in args.enumerate() {
        if arg == b"--" {
            return Ok(Action::Wait(Some(index)));
        }
        if arg == b"-" || !arg.starts_with(b"-") {
            if posix {
                break;
            }
            continue;
        }
        let Some(long) = arg.strip_prefix(b"--") else {
            return Err([&b"invalid option -- '"[..], &arg[1..2], b"'"].concat());
        };
        let name = long.split(|b| *b == b'=').next().unwrap();
        if name.is_empty() {
            return Err([
                &b"option '"[..],
                arg,
                b"' is ambiguous; possibilities: '--help' '--version'",
            ]
            .concat());
        }
        for (full, action) in [
            (b"help".as_slice(), Action::Help),
            (b"version", Action::Version),
        ] {
            if full.starts_with(name) {
                if long.len() != name.len() {
                    return Err([&b"option '--"[..], full, b"' doesn't allow an argument"].concat());
                }
                return Ok(action);
            }
        }
        return Err([&b"unrecognized option '"[..], arg, b"'"].concat());
    }
    Ok(Action::Wait(None))
}
