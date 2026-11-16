use alloc::{format, string::String, vec::Vec};
#[derive(Default)]
pub struct Options {
    pub quiet: bool,
    pub ere: bool,
    pub sandbox: bool,
    pub zero: bool,
    pub separate: bool,
    pub unbuffered: bool,
    pub follow: bool,
    pub inplace: Option<Vec<u8>>,
    pub help: bool,
    pub version: bool,
    pub scripts: Vec<(bool, Vec<u8>)>,
    pub files: Vec<Vec<u8>>,
    pub posix: bool,
    pub width: Option<usize>,
    pub debug: bool,
}
impl Options {
    pub fn parse(args: &[Vec<u8>]) -> Result<Self, String> {
        let mut result = Self::default();
        let mut operands = Vec::new();
        let mut args = args.iter().map(Vec::as_slice);
        while let Some(arg) = args.next() {
            if arg == b"--" {
                operands.extend(args.map(<[u8]>::to_vec));
                break;
            }
            if arg == b"-" || !arg.starts_with(b"-") {
                operands.push(arg.to_vec());
                continue;
            }
            let mut flags = &arg[1..];
            let expanded;
            if let Some(long) = arg.strip_prefix(b"--") {
                let mut parts = long.splitn(2, |&c| c == b'=');
                let name = parts.next().unwrap();
                let value = parts.next();
                if value.is_some()
                    && !matches!(name, b"expression" | b"file" | b"in-place" | b"line-length")
                {
                    return Err(format!(
                        "option does not take a value: {}",
                        String::from_utf8_lossy(name)
                    ));
                }
                match name {
                    b"help" => result.help = true,
                    b"version" => result.version = true,
                    b"posix" => result.posix = true,
                    b"debug" => result.debug = true,
                    b"sandbox" => result.sandbox = true,
                    b"follow-symlinks" => result.follow = true,
                    b"quiet" | b"silent" => result.quiet = true,
                    b"regexp-extended" => result.ere = true,
                    b"separate" => result.separate = true,
                    b"unbuffered" => result.unbuffered = true,
                    b"null-data" => result.zero = true,
                    b"binary" => (),
                    b"in-place" => result.inplace = Some(value.unwrap_or_default().to_vec()),
                    b"expression" | b"file" | b"line-length" => {
                        expanded = [name[0]];
                        flags = &expanded;
                        if let Some(v) = value {
                            result.value(flags[0], v)?;
                            continue;
                        }
                    }
                    _ => {
                        return Err(format!(
                            "unknown option --{}",
                            String::from_utf8_lossy(name)
                        ));
                    }
                }
                if !matches!(name, b"expression" | b"file" | b"line-length") {
                    continue;
                }
            }
            while let Some((&flag, tail)) = flags.split_first() {
                flags = tail;
                match flag {
                    b'n' => result.quiet = true,
                    b'E' | b'r' => result.ere = true,
                    b'z' => result.zero = true,
                    b's' => result.separate = true,
                    b'u' => result.unbuffered = true,
                    b'b' => (),
                    b'i' => {
                        result.inplace = Some(flags.to_vec());
                        break;
                    }
                    b'e' | b'f' | b'l' => {
                        let v = if flags.is_empty() {
                            args.next().ok_or("option requires an argument")?
                        } else {
                            flags
                        };
                        result.value(flag, v)?;
                        break;
                    }
                    _ => return Err(format!("unknown option -{}", char::from(flag))),
                }
            }
        }
        if !result.help && !result.version && result.scripts.is_empty() {
            if operands.is_empty() {
                return Err("missing script".into());
            }
            result.scripts.push((false, operands.remove(0)));
        }
        result.files = operands;
        Ok(result)
    }
    fn value(&mut self, flag: u8, v: &[u8]) -> Result<(), String> {
        if flag == b'l' {
            self.width = Some(
                core::str::from_utf8(v)
                    .ok()
                    .and_then(|n| n.parse().ok())
                    .ok_or("invalid line length")?,
            );
        } else {
            self.scripts.push((flag == b'f', v.to_vec()));
        }
        Ok(())
    }
    pub fn flags(&self, script: &[u8]) -> i32 {
        i32::from(self.quiet || script.starts_with(b"#n"))
            | (i32::from(self.ere) << 1)
            | (i32::from(self.sandbox) << 2)
            | (i32::from(self.zero) << 3)
            | (i32::from(self.posix) << 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_operands_do_not_request_help() {
        let args = [
            b"-e".to_vec(),
            b"--help".to_vec(),
            b"--".to_vec(),
            b"--version".to_vec(),
        ];
        let opts = Options::parse(&args).unwrap();
        assert!(!opts.help && !opts.version);
        assert_eq!(opts.scripts[0].1, b"--help");
        assert_eq!(opts.files, [b"--version"]);
    }
}
