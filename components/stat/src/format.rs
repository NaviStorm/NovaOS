use crate::options::Options;
use alloc::{format, string::String, vec::Vec};

pub enum Value {
    Text(Vec<u8>),
    #[cfg(target_os = "macos")]
    Link(Vec<u8>, Vec<u8>),
    Number(u64, u8),
    Epoch(i64, u32),
    Unknown,
}
impl Value {
    pub fn text(s: impl AsRef<[u8]>) -> Self {
        Self::Text(s.as_ref().to_vec())
    }
    pub fn dec(n: u64) -> Self {
        Self::Number(n, 10)
    }
    pub fn hex(n: u64) -> Self {
        Self::Number(n, 16)
    }
}
pub fn template(o: &Options, device: bool) -> Vec<u8> {
    if let Some(f) = o.format {
        return f.to_vec();
    }
    match (o.fs,o.terse) {
        (true,true)=>b"%Qn %i %l %t %s %S %b %f %a %c %d".to_vec(),
        (false,true)=>b"%Qn %s %b %f %u %g %D %i %h %t %T %X %Y %Z %W %o".to_vec(),
        (true,false)=>b"  File: %Qn\n    ID: %-8i Namelen: %-7l Type: %T\nBlock size: %-10s Fundamental block size: %S\nBlocks: Total: %-10b Free: %-10f Available: %a\nInodes: Total: %-10c Free: %d".to_vec(),
        (false,false)=>[b"  File: %N\n  Size: %-10s\tBlocks: %-10b IO Block: %-6o %F\nDevice: %Hd,%Ld\tInode: %-10i  Links: ".as_slice(),if device{b"%-5h Device type: %Hr,%Lr"}else{b"%h"},b"\nAccess: (%04a/%10.10A)  Uid: (%5u/%8U)   Gid: (%5g/%8G)\nAccess: %x\nModify: %y\nChange: %z\n Birth: %w"].concat()
    }
}
fn number(fmt: &[u8], p: &mut usize) -> Result<usize, String> {
    let mut value = 0;
    while let Some(b'0'..=b'9') = fmt.get(*p) {
        value = value * 10 + (fmt[*p] - b'0') as usize;
        *p += 1;
        if value > 65536 {
            return Err("format width or precision exceeds 65536".into());
        }
    }
    Ok(value)
}
fn escaped(f: &[u8], i: &mut usize, warn: &mut impl FnMut(String)) -> u8 {
    let Some(&c) = f.get(*i) else {
        warn("warning: backslash at end of format".into());
        return b'\\';
    };
    *i += 1;
    if b"01234567x".contains(&c) {
        let hex = c == b'x';
        let start = if hex { *i } else { *i - 1 };
        let mut end = start;
        let mut n = 0u32;
        while end < f.len() && end - start < (if hex { 2 } else { 3 }) {
            let radix = if hex { 16 } else { 8 };
            let Some(d) = (f[end] as char).to_digit(radix) else {
                break;
            };
            n = n * radix + d;
            end += 1
        }
        if end > start {
            *i = end;
            return n as u8;
        }
    }
    match c {
        b'a' => 7,
        b'b' => 8,
        b'e' => 27,
        b'f' => 12,
        b'n' => 10,
        b'r' => 13,
        b't' => 9,
        b'v' => 11,
        b'"' | b'\\' => c,
        _ => {
            warn(format!("warning: unrecognized escape '\\{}'", c as char));
            c
        }
    }
}
pub fn render(
    f: &[u8],
    escapes: bool,
    filesystem: bool,
    mut field: impl FnMut(&[u8]) -> Value,
    out: &mut Vec<u8>,
    mut warn: impl FnMut(String),
) -> Result<(), String> {
    let mut i = 0;
    while i < f.len() {
        #[cfg(target_os = "none")]
        if out.len() > 65536 {
            return Err("format output exceeds 65536 bytes".into());
        }
        let start = i;
        let c = f[i];
        i += 1;
        if c != b'%' {
            out.push(if c == b'\\' && escapes {
                escaped(f, &mut i, &mut warn)
            } else {
                c
            });
            continue;
        }
        let flags = i;
        while f.get(i).is_some_and(|c| b"'-+ #0I".contains(c)) {
            i += 1
        }
        let flags = &f[flags..i];
        let width = number(f, &mut i)?;
        let mut implicit_precision = false;
        let precision = if f.get(i) == Some(&b'.') {
            i += 1;
            implicit_precision = !f.get(i).is_some_and(u8::is_ascii_digit);
            Some(number(f, &mut i)?)
        } else {
            None
        };
        if i == f.len() || f[i] == b'%' {
            if i - start > 1 {
                return Err(format!(
                    "'{}': invalid directive",
                    String::from_utf8_lossy(&f[start..(i + 1).min(f.len())])
                ));
            }
            out.push(b'%');
            i = (i + 1).min(f.len());
            continue;
        }
        let key = i;
        i += 1;
        if (f[key] == b'Q' && f.get(i) == Some(&b'n'))
            || (!filesystem && matches!((f[key], f.get(i)), (b'H' | b'L', Some(b'd' | b'r'))))
        {
            i += 1
        }
        let value = field(&f[key..i]);
        let epoch = matches!(value, Value::Epoch(..));
        let mut numeric = false;
        let mut prefix = Vec::new();
        let mut data = match value {
            Value::Unknown => {
                out.push(b'?');
                continue;
            }
            #[cfg(target_os = "macos")]
            Value::Link(a, b) => {
                for (index, mut part) in [a, b].into_iter().enumerate() {
                    if index != 0 {
                        out.extend_from_slice(b" -> ")
                    }
                    if let Some(p) = precision {
                        part.truncate(p)
                    }
                    let pad = width.saturating_sub(part.len());
                    if !flags.contains(&b'-') {
                        out.resize(out.len() + pad, b' ')
                    }
                    out.extend(part);
                    if flags.contains(&b'-') {
                        out.resize(out.len() + pad, b' ')
                    }
                }
                continue;
            }
            Value::Text(mut b) => {
                if let Some(p) = precision {
                    b.truncate(p)
                }
                b
            }
            Value::Epoch(sec, ns) => {
                numeric = true;
                let digits = if implicit_precision {
                    9
                } else {
                    precision.unwrap_or(0)
                };
                let negative = sec < 0;
                let fraction = if negative && ns != 0 {
                    1_000_000_000 - ns
                } else {
                    ns
                };
                let whole =
                    if negative && ns != 0 && fraction / 10u32.pow(9 - digits.min(9) as u32) != 0 {
                        sec.unsigned_abs() - 1
                    } else {
                        sec.unsigned_abs()
                    };
                if negative {
                    prefix.push(b'-')
                } else if flags.contains(&b'+') {
                    prefix.push(b'+')
                } else if flags.contains(&b' ') {
                    prefix.push(b' ')
                }
                let mut b = format!("{whole}").into_bytes();
                if digits > 0 {
                    b.push(b'.');
                    b.extend_from_slice(&format!("{fraction:09}").as_bytes()[..digits.min(9)]);
                    b.resize(b.len() + digits.saturating_sub(9), b'0')
                }
                b
            }
            Value::Number(n, radix) => {
                numeric = true;
                let mut b = match radix {
                    8 => format!("{n:o}"),
                    16 => format!("{n:x}"),
                    _ => format!("{n}"),
                }
                .into_bytes();
                if precision == Some(0) && n == 0 {
                    b.clear()
                }
                if let Some(p) = precision {
                    let mut z = alloc::vec![b'0';p.saturating_sub(b.len())];
                    z.extend_from_slice(&b);
                    b = z
                }
                if flags.contains(&b'#') {
                    if radix == 8 && b.first() != Some(&b'0') {
                        prefix.push(b'0')
                    } else if radix == 16 && n != 0 {
                        prefix.extend(b"0x")
                    }
                }
                b
            }
        };
        let padding = width.saturating_sub(prefix.len() + data.len());
        let left = flags.contains(&b'-');
        let zeros = numeric && flags.contains(&b'0') && !left && (epoch || precision.is_none());
        if !left && !zeros {
            out.resize(out.len() + padding, b' ')
        }
        out.append(&mut prefix);
        if zeros {
            out.resize(out.len() + padding, b'0')
        }
        out.append(&mut data);
        if left {
            out.resize(out.len() + padding, b' ')
        }
    }
    if !escapes {
        out.push(b'\n')
    }
    Ok(())
}
