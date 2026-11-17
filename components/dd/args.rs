#[path = "doc_flag.rs"]
mod doc_flag;
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
pub const CONVS: &[&str] = &[
    "ascii",
    "ebcdic",
    "ibm",
    "block",
    "unblock",
    "lcase",
    "ucase",
    "sparse",
    "swab",
    "sync",
    "excl",
    "nocreat",
    "notrunc",
    "noerror",
    "fdatasync",
    "fsync",
];
pub const FLAGS: &[&str] = &[
    "append",
    "directory",
    "dsync",
    "sync",
    "fullblock",
    "nonblock",
    "noctty",
    "nofollow",
    "nocache",
    "count_bytes",
    "skip_bytes",
    "seek_bytes",
];
pub enum Action<'a> {
    Help,
    Version,
    Run(Config<'a>),
}
#[derive(Default)]
pub struct Config<'a> {
    pub input: Option<&'a [u8]>,
    pub output: Option<&'a [u8]>,
    pub ibs: usize,
    pub obs: usize,
    pub cbs: usize,
    pub direct_blocks: bool,
    pub count: Option<u64>,
    pub tail: usize,
    pub skip: u64,
    pub seek: u64,
    pub skip_tail: usize,
    pub conv: Vec<&'static str>,
    pub inf: Vec<&'static str>,
    pub outf: Vec<&'static str>,
    pub status: &'static str,
    pub warnings: usize,
}
impl Config<'_> {
    pub fn has(&self, key: &str) -> bool {
        self.conv.contains(&key)
    }
}
pub struct Error(pub String, pub bool);
fn invalid(key: &str, raw: &[u8], usage: bool) -> Error {
    Error(format!("{key} '{}'", String::from_utf8_lossy(raw)), usage)
}
fn number(raw: &[u8], min: u64, max: u64) -> Result<(u64, usize), Error> {
    let bad = || invalid("invalid number:", raw, false);
    let s = core::str::from_utf8(raw).map_err(|_| bad())?;
    let mut product = 1u128;
    let mut warnings = 0;
    for part in s.split('x') {
        let trimmed = part.trim_start_matches(|c: char| c.is_ascii_whitespace());
        let t = trimmed.strip_prefix('+').unwrap_or(trimmed);
        let digits = t.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 && (part != t || t.is_empty() || t == "B") {
            return Err(bad());
        }
        let value = if digits == 0 {
            1
        } else {
            t[..digits].parse::<u128>().unwrap_or(u128::MAX)
        };
        let suffix = &t[digits..];
        let scale = match suffix {
            "" | "B" | "c" | "cB" => 1,
            "b" | "bB" => 512,
            "w" | "wB" => 2,
            _ => {
                let (&first, rest) = suffix.as_bytes().split_first().ok_or_else(bad)?;
                let power = match first {
                    b'k' => 1,
                    c => b"KMGTPEZYRQ".iter().position(|v| *v == c).ok_or_else(bad)? + 1,
                };
                let base: u128 = match rest {
                    b"" | b"iB" => 1024,
                    b"B" | b"D" | b"DB" => 1000,
                    _ => return Err(bad()),
                };
                base.pow(power as u32)
            }
        };
        product = product.saturating_mul(value.saturating_mul(scale));
    }
    if s.split('x')
        .take(s.bytes().filter(|&b| b == b'x').count())
        .any(|p| p == "0")
    {
        warnings = 1
    }
    if product > max as u128 {
        return Err(Error(
            format!(
                "invalid number: '{}': Value too large to be stored in data type",
                s
            ),
            false,
        ));
    }
    if product < min as u128 {
        return Err(bad());
    }
    Ok((product as u64, warnings))
}
fn symbols(
    dst: &mut Vec<&'static str>,
    raw: &[u8],
    choices: &[&'static str],
    label: &str,
) -> Result<(), Error> {
    for item in raw.split(|b| *b == b',') {
        let value = choices
            .iter()
            .copied()
            .find(|k| k.as_bytes() == item)
            .ok_or_else(|| invalid(label, item, true))?;
        if !dst.contains(&value) {
            dst.push(value)
        }
    }
    Ok(())
}
pub fn parse<'a>(argv: &[&'a [u8]], posix: bool) -> Result<Action<'a>, Error> {
    let mut cfg = Config {
        ibs: 512,
        obs: 512,
        ..Default::default()
    };
    let mut bs = 0;
    let mut end = false;
    let mut operands = Vec::new();
    for &word in argv {
        if !end && word == b"--" {
            end = true;
            continue;
        }
        if !end && word.starts_with(b"-") && word != b"-" {
            if let Some(long) = word.strip_prefix(b"--") {
                let mut pieces = long.splitn(2, |b| *b == b'=');
                let prefix = pieces.next().unwrap();
                if let Some(name) = doc_flag::name(prefix) {
                    if pieces.next().is_some() {
                        return Err(Error(
                            format!("option '--{name}' doesn't allow an argument"),
                            true,
                        ));
                    }
                    return Ok(if name == "help" {
                        Action::Help
                    } else {
                        Action::Version
                    });
                }
                return Err(invalid("unrecognized option", word, true));
            }
            return Err(invalid("invalid option --", &word[1..2], true));
        }
        operands.push(word);
        if posix {
            end = true
        }
    }
    let mut byte_units = [false; 3];
    for word in operands {
        let mut parts = word.splitn(2, |b| *b == b'=');
        let key = parts.next().unwrap();
        let unknown = || invalid("unrecognized operand", word, true);
        let val = parts.next().ok_or_else(unknown)?;
        match key {
            b"if" => cfg.input = Some(val),
            b"of" => cfg.output = Some(val),
            b"conv" => symbols(&mut cfg.conv, val, CONVS, "invalid conversion:")?,
            b"iflag" => symbols(&mut cfg.inf, val, FLAGS, "invalid input flag:")?,
            b"oflag" => symbols(&mut cfg.outf, val, FLAGS, "invalid output flag:")?,
            b"status" => {
                for s in val.split(|b| *b == b',') {
                    cfg.status = match s {
                        b"none" => "none",
                        b"noxfer" => "noxfer",
                        b"progress" => "progress",
                        _ => return Err(invalid("invalid status level:", s, true)),
                    }
                }
            }
            b"count" | b"skip" | b"iseek" | b"seek" | b"oseek" | b"bs" | b"ibs" | b"obs"
            | b"cbs" => {
                let block = matches!(key, b"bs" | b"ibs" | b"obs" | b"cbs");
                let (n, w) = number(
                    val,
                    block as u64,
                    (i64::MAX as u64) - u64::from(block && key != b"cbs"),
                )?;
                cfg.warnings += w;
                let bytes = val.contains(&b'B');
                match key {
                    b"bs" => bs = n as usize,
                    b"ibs" => cfg.ibs = n as usize,
                    b"obs" => cfg.obs = n as usize,
                    b"cbs" => cfg.cbs = n as usize,
                    b"count" => {
                        cfg.count = Some(n);
                        byte_units[0] = bytes
                    }
                    b"skip" | b"iseek" => {
                        cfg.skip = n;
                        byte_units[1] = bytes
                    }
                    _ => {
                        cfg.seek = n;
                        byte_units[2] = bytes
                    }
                }
            }
            _ => return Err(unknown()),
        }
    }
    if bs != 0 {
        cfg.ibs = bs;
        cfg.obs = bs
    }
    if cfg.has("ascii") {
        cfg.conv.push("unblock")
    }
    if cfg.has("ebcdic") || cfg.has("ibm") {
        cfg.conv.push("block")
    }
    cfg.direct_blocks = bs > 0
        && !cfg.conv.iter().any(|c| {
            matches!(
                *c,
                "ascii" | "ebcdic" | "ibm" | "block" | "unblock" | "swab" | "ucase" | "lcase"
            )
        });
    if cfg.cbs == 0 {
        cfg.conv.retain(|c| *c != "block" && *c != "unblock")
    }
    if cfg.outf.contains(&"fullblock") {
        return Err(invalid("invalid output flag:", b"fullblock", true));
    }
    for (names, msg) in [
        (
            &["ascii", "ebcdic", "ibm"][..],
            "any two of {ascii,ebcdic,ibm}",
        ),
        (&["block", "unblock"], "block and unblock"),
        (&["ucase", "lcase"], "lcase and ucase"),
        (&["excl", "nocreat"], "excl and nocreat"),
    ] {
        if names.iter().filter(|s| cfg.has(s)).count() > 1 {
            return Err(Error(format!("cannot combine {msg}"), false));
        }
    }
    if let Some(n) = cfg.count
        && (byte_units[0] || cfg.inf.contains(&"count_bytes"))
    {
        cfg.count = Some(n / cfg.ibs as u64);
        cfg.tail = (n % cfg.ibs as u64) as usize
    }
    if byte_units[1] || cfg.inf.contains(&"skip_bytes") {
        cfg.skip_tail = (cfg.skip % cfg.ibs as u64) as usize;
        cfg.skip /= cfg.ibs as u64
    }
    if !byte_units[2] && !cfg.outf.contains(&"seek_bytes") {
        cfg.seek = cfg
            .seek
            .checked_mul(cfg.obs as u64)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| Error("seek offset overflow".to_string(), false))?
    }
    Ok(Action::Run(cfg))
}
