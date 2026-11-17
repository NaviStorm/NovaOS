use alloc::{format, vec::Vec};

#[derive(Default)]
pub struct Options<'a> {
    pub directory: bool,
    pub dry: bool,
    pub quiet: bool,
    pub legacy: bool,
    pub parent: Option<&'a [u8]>,
    pub suffix: Option<&'a [u8]>,
    pub template: Option<&'a [u8]>,
}

pub enum Action<'a> {
    Help,
    Version,
    Create(Options<'a>),
}

pub fn parse<'a>(args: &[&'a [u8]], posix: bool) -> Result<Action<'a>, Vec<u8>> {
    let mut opt = Options::default();
    let mut scan = true;
    let mut extra = false;
    let mut words = args.iter().copied();
    while let Some(arg) = words.next() {
        if !scan || arg == b"-" || !arg.starts_with(b"-") {
            extra |= opt.template.replace(arg).is_some();
            scan &= !posix;
        } else if arg == b"--" {
            scan = false;
        } else if let Some(long) = arg.strip_prefix(b"--") {
            let mut pair = long.splitn(2, |c| *c == b'=');
            let prefix = pair.next().unwrap();
            let value = pair.next();
            let mut names = [
                "directory",
                "quiet",
                "dry-run",
                "suffix",
                "tmpdir",
                "help",
                "version",
            ]
            .into_iter()
            .filter(|s| s.as_bytes().starts_with(prefix));
            let Some(name) = names.next() else {
                return Err([b"unrecognized option '".as_slice(), arg, b"'"].concat());
            };
            if let Some(second) = names.next() {
                let mut reason = [
                    b"option '".as_slice(),
                    arg,
                    b"' is ambiguous; possibilities:",
                ]
                .concat();
                for s in [name, second].into_iter().chain(names) {
                    reason.extend_from_slice(format!(" '--{s}'").as_bytes());
                }
                return Err(reason);
            }
            if value.is_some() && !matches!(name, "suffix" | "tmpdir") {
                return Err(format!("option '--{name}' doesn't allow an argument").into_bytes());
            }
            match name {
                "directory" => opt.directory = true,
                "quiet" => opt.quiet = true,
                "dry-run" => opt.dry = true,
                "suffix" => {
                    opt.suffix = Some(
                        value
                            .or_else(|| words.next())
                            .ok_or(b"option '--suffix' requires an argument".to_vec())?,
                    )
                }
                "tmpdir" => opt.parent = Some(value.unwrap_or_default()),
                "help" => return Ok(Action::Help),
                _ => return Ok(Action::Version),
            }
        } else {
            for (i, &c) in arg.iter().enumerate().skip(1) {
                match c {
                    b'd' => opt.directory = true,
                    b'q' => opt.quiet = true,
                    b't' => opt.legacy = true,
                    b'u' => opt.dry = true,
                    b'V' => return Ok(Action::Version),
                    b'p' => {
                        opt.parent = Some(if i + 1 < arg.len() {
                            &arg[i + 1..]
                        } else {
                            words
                                .next()
                                .ok_or(b"option requires an argument -- 'p'".to_vec())?
                        });
                        break;
                    }
                    _ => return Err([b"invalid option -- '".as_slice(), &[c], b"'"].concat()),
                }
            }
        }
    }
    if extra {
        return Err(b"too many templates".to_vec());
    }
    Ok(Action::Create(opt))
}
