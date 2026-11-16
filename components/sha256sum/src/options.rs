use alloc::{format, string::String, vec::Vec};

#[derive(Default, PartialEq)]
pub enum Report {
    #[default]
    Normal,
    Quiet,
    Status,
    Warn,
}

#[derive(Default)]
pub struct Options<'a> {
    pub paths: Vec<&'a [u8]>,
    pub check: bool,
    pub mode: Option<u8>,
    pub tag: bool,
    pub zero: bool,
    pub ignore: bool,
    pub strict: bool,
    pub report: Report,
    pub doc: Option<bool>,
}

const NAMES: &[(&str, u8)] = &[
    ("binary", b'b'),
    ("check", b'c'),
    ("tag", b'T'),
    ("text", b't'),
    ("zero", b'z'),
    ("ignore-missing", b'i'),
    ("quiet", b'q'),
    ("status", b's'),
    ("strict", b'S'),
    ("warn", b'w'),
    ("help", b'h'),
    ("version", b'v'),
];

impl<'a> Options<'a> {
    pub fn parse(args: &[&'a [u8]]) -> Result<Self, String> {
        let mut opt = Self::default();
        let mut positional = false;
        for &arg in args {
            if positional || arg == b"-" || !arg.starts_with(b"-") {
                opt.paths.push(arg);
                continue;
            }
            if arg == b"--" {
                positional = true;
                continue;
            }
            if let Some(key) = arg.strip_prefix(b"--") {
                let key = key.split(|&c| c == b'=').next().unwrap();
                let mut matches = NAMES
                    .iter()
                    .filter(|(name, _)| name.as_bytes().starts_with(key));
                let Some(&(name, flag)) = matches.next() else {
                    return Err(format!(
                        "unrecognized option '{}'",
                        String::from_utf8_lossy(arg)
                    ));
                };
                if matches.next().is_some() {
                    let names = NAMES
                        .iter()
                        .filter(|(n, _)| n.as_bytes().starts_with(key))
                        .map(|(n, _)| format!(" '--{n}'"))
                        .collect::<String>();
                    return Err(format!(
                        "option '{}' is ambiguous; possibilities:{names}",
                        String::from_utf8_lossy(arg)
                    ));
                }
                if arg.contains(&b'=') {
                    return Err(format!("option '--{name}' doesn't allow an argument"));
                }
                opt.set(flag);
                if opt.doc.is_some() {
                    return Ok(opt);
                }
            } else {
                for &flag in &arg[1..] {
                    if !b"bctwz".contains(&flag) {
                        return Err(format!("invalid option -- '{}'", char::from(flag)));
                    }
                    opt.set(flag);
                }
            }
        }
        if opt.paths.is_empty() {
            opt.paths.push(b"-");
        }
        let invalid = if opt.check && opt.zero {
            Some("the --zero option is not supported when verifying checksums")
        } else if opt.check && opt.tag {
            Some("the --tag option is meaningless when verifying checksums")
        } else if opt.check && opt.mode.is_some() {
            Some("the --binary and --text options are meaningless when verifying checksums")
        } else if opt.tag && opt.mode == Some(b' ') {
            Some("--tag does not support --text mode")
        } else {
            None
        };
        if let Some(msg) = invalid {
            return Err(msg.into());
        }
        if !opt.check {
            for (used, name) in [
                (opt.ignore, "ignore-missing"),
                (opt.report == Report::Status, "status"),
                (opt.report == Report::Warn, "warn"),
                (opt.report == Report::Quiet, "quiet"),
                (opt.strict, "strict"),
            ] {
                if used {
                    return Err(format!(
                        "the --{name} option is meaningful only when verifying checksums"
                    ));
                }
            }
        }
        Ok(opt)
    }
    fn set(&mut self, f: u8) {
        match f {
            b'b' => self.mode = Some(b'*'),
            b't' => self.mode = Some(b' '),
            b'c' => self.check = true,
            b'T' => self.tag = true,
            b'z' => self.zero = true,
            b'i' => self.ignore = true,
            b'S' => self.strict = true,
            b'w' => self.report = Report::Warn,
            b'q' => self.report = Report::Quiet,
            b's' => self.report = Report::Status,
            b'h' | b'v' => self.doc = Some(f == b'h'),
            _ => unreachable!(),
        }
    }
}
