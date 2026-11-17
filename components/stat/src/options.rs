use alloc::{format, string::String, vec::Vec};
#[derive(Default)]
pub struct Options<'a> {
    pub files: Vec<&'a [u8]>,
    pub fs: bool,
    pub follow: bool,
    pub terse: bool,
    pub format: Option<&'a [u8]>,
    pub escapes: bool,
    pub doc: Option<bool>,
}
pub fn parse<'a>(args: &[&'a [u8]], posix: bool) -> Result<Options<'a>, String> {
    let mut o = Options::default();
    let mut rest = args.iter().copied();
    let mut end = false;
    while let Some(arg) = rest.next() {
        if end || !arg.starts_with(b"-") || arg == b"-" {
            o.files.push(arg);
            end |= posix;
            continue;
        }
        if arg == b"--" {
            end = true;
            continue;
        }
        if let Some(long) = arg.strip_prefix(b"--") {
            let mut pair = long.splitn(2, |b| *b == b'=');
            let key = pair.next().unwrap();
            let inline = pair.next();
            let names = [
                "dereference",
                "file-system",
                "terse",
                "format",
                "printf",
                "cached",
                "help",
                "version",
            ];
            if key == b"f" {
                return Err(format!(
                    "option '{}' is ambiguous; possibilities: '--file-system' '--format'",
                    String::from_utf8_lossy(arg)
                ));
            }
            let name = names
                .into_iter()
                .find(|n| !key.is_empty() && n.as_bytes().starts_with(key))
                .ok_or_else(|| format!("unrecognized option '{}'", String::from_utf8_lossy(arg)))?;
            if matches!(name, "format" | "printf" | "cached") {
                let value = inline
                    .or_else(|| rest.next())
                    .ok_or_else(|| format!("option '--{name}' requires an argument"))?;
                if name == "cached" {
                    if !["always", "never", "default"]
                        .iter()
                        .any(|s| !value.is_empty() && s.as_bytes().starts_with(value))
                    {
                        return Err(format!(
                            "{} argument '{}' for '--cached'\nValid arguments are:\n  - 'default'\n  - 'never'\n  - 'always'",
                            if value.is_empty() {
                                "ambiguous"
                            } else {
                                "invalid"
                            },
                            String::from_utf8_lossy(value)
                        ));
                    }
                } else {
                    o.format = Some(value);
                    o.escapes = name == "printf"
                }
            } else {
                if inline.is_some() {
                    return Err(format!("option '--{name}' doesn't allow an argument"));
                }
                match name {
                    "terse" => o.terse = true,
                    "file-system" => o.fs = true,
                    "dereference" => o.follow = true,
                    _ => {
                        o.doc = Some(name == "help");
                        return Ok(o);
                    }
                }
            }
        } else {
            for (i, c) in arg.iter().copied().enumerate().skip(1) {
                match c {
                    b'L' => o.follow = true,
                    b'f' => o.fs = true,
                    b't' => o.terse = true,
                    b'c' => {
                        o.format = Some(if i + 1 < arg.len() {
                            &arg[i + 1..]
                        } else {
                            rest.next().ok_or("option requires an argument -- 'c'")?
                        });
                        o.escapes = false;
                        break;
                    }
                    _ => return Err(format!("invalid option -- '{}'", c as char)),
                }
            }
        }
    }
    if o.files.is_empty() {
        return Err("missing operand".into());
    }
    Ok(o)
}
