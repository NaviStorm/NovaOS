use crate::{EN, FR, Locale, Output, VERSION, float, number, quoting};
use alloc::{format, vec::Vec};

pub enum Stop {
    Cancel,
    Error,
    Write,
}
pub type Result<T> = core::result::Result<T, Stop>;
pub struct Printer<'a, O> {
    pub io: &'a mut O,
    pub locale: Locale<'a>,
    pub status: u8,
    pub overflow: bool,
}
#[derive(Default)]
pub struct Field<'a> {
    pub flags: &'a [u8],
    pub width: usize,
    pub precision: Option<usize>,
    pub left: bool,
    pub conv: u8,
}
impl Field<'_> {
    pub fn flag(&self, b: u8) -> bool {
        self.flags.contains(&b)
    }
}
pub struct Parts<'a> {
    pub sign: &'a [u8],
    pub leading: usize,
    pub body: &'a [u8],
    pub trailing: usize,
    pub suffix: &'a [u8],
    pub zero_width: bool,
}
impl<'a> Parts<'a> {
    pub fn text(body: &'a [u8]) -> Self {
        Self {
            sign: b"",
            leading: 0,
            body,
            trailing: 0,
            suffix: b"",
            zero_width: false,
        }
    }
}
impl<O: Output> Printer<'_, O> {
    pub fn write(&mut self, s: &[u8]) -> Result<()> {
        if s.is_empty() {
            Ok(())
        } else {
            self.io.write(false, s).map_err(|_| Stop::Write)
        }
    }
    fn error(&mut self, s: &[u8]) -> Result<()> {
        self.io.write(true, s).map_err(|_| Stop::Error)
    }
    pub fn message(&mut self, parts: &[&[u8]]) -> Result<()> {
        self.error(b"printf: ")?;
        for b in parts {
            self.error(b)?
        }
        self.error(b"\n")
    }
    pub fn fail<T>(&mut self, parts: &[&[u8]]) -> Result<T> {
        self.message(parts)?;
        Err(Stop::Error)
    }
    pub fn pad(&mut self, mut n: usize, b: u8) -> Result<()> {
        let buf = [b; 512];
        while n != 0 {
            let chunk = n.min(buf.len());
            self.write(&buf[..chunk])?;
            n -= chunk
        }
        Ok(())
    }
    pub fn field(&mut self, f: &Field, p: Parts) -> Result<()> {
        let total = p.sign.len() as u128
            + p.leading as u128
            + p.body.len() as u128
            + p.trailing as u128
            + p.suffix.len() as u128;
        if total > isize::MAX as u128 || f.width > isize::MAX as usize {
            self.overflow = true;
            return Ok(());
        }
        let len = total as usize;
        let pad = f.width.saturating_sub(len);
        if !f.left && !p.zero_width {
            self.pad(pad, b' ')?
        }
        self.write(p.sign)?;
        self.pad(
            p.leading + if !f.left && p.zero_width { pad } else { 0 },
            b'0',
        )?;
        self.write(p.body)?;
        self.pad(p.trailing, b'0')?;
        self.write(p.suffix)?;
        if f.left {
            self.pad(pad, b' ')?
        }
        Ok(())
    }
    fn escape(&mut self, s: &mut &[u8], arg: bool) -> Result<()> {
        let Some((&kind, rest)) = s.split_first() else {
            return self.write(br"\");
        };
        *s = rest;
        let value = match kind {
            b'0'..=b'7' => {
                // A leading zero in %b introduces up to three additional digits.
                let (mut val, limit) = if arg && kind == b'0' {
                    (0, 3)
                } else {
                    (kind - b'0', 2)
                };
                for _ in 0..limit {
                    if let Some(&b @ b'0'..=b'7') = s.first() {
                        val = val.wrapping_mul(8) + (b - b'0');
                        *s = &s[1..]
                    } else {
                        break;
                    }
                }
                val
            }
            b'x' | b'u' | b'U' => {
                let max = match kind {
                    b'x' => 2,
                    b'u' => 4,
                    _ => 8,
                };
                let n = s
                    .iter()
                    .take(max)
                    .take_while(|c| c.is_ascii_hexdigit())
                    .count();
                if n == 0 || kind != b'x' && n != max {
                    return self.fail(&[b"missing hexadecimal number in escape"]);
                }
                let val = u32::from_str_radix(core::str::from_utf8(&s[..n]).unwrap(), 16).unwrap();
                *s = &s[n..];
                if kind == b'x' {
                    val as u8
                } else {
                    if (0xd800..=0xdfff).contains(&val) {
                        return self.fail(&[format!(
                            "invalid universal character name \\{}{val:0max$x}",
                            kind as char
                        )
                        .as_bytes()]);
                    }
                    let mut v = Vec::new();
                    if !char::from_u32(val).is_some_and(|ch| (self.locale.encode)(ch, &mut v)) {
                        v = if val <= 0xffff {
                            format!("\\u{val:04X}")
                        } else {
                            format!("\\U{val:08X}")
                        }
                        .into_bytes();
                    }
                    return self.write(&v);
                }
            }
            b'c' => return Err(Stop::Cancel),
            b'a' => 7,
            b'b' => 8,
            b'e' => 27,
            b'f' => 12,
            b'n' => 10,
            b'r' => 13,
            b't' => 9,
            b'v' => 11,
            b'"' | b'\\' => kind,
            _ => {
                self.write(br"\")?;
                kind
            }
        };
        self.write(&[value])
    }
    fn span(&mut self, s: &mut &[u8], arg: bool) -> Result<()> {
        let n = s
            .iter()
            .position(|&b| b == b'\\' || (!arg && b == b'%'))
            .unwrap_or(s.len());
        self.write(&s[..n])?;
        *s = &s[n..];
        if let Some(rest) = s.strip_prefix(br"\") {
            *s = rest;
            self.escape(s, arg)?
        }
        Ok(())
    }
    fn convert(&mut self, original: &[u8], s: &mut &[u8], a: &mut Arguments) -> Result<()> {
        if eat(s, b'%') {
            return self.write(b"%");
        }
        let pos = index(s);
        if let Some(&c @ (b'b' | b'q')) = s.first() {
            *s = &s[1..];
            if let Some(mut data) = a.get(pos) {
                if c == b'q' {
                    self.write(&quoting::shell(data, self.locale))?
                } else {
                    while !data.is_empty() {
                        self.span(&mut data, true)?
                    }
                }
            }
            return Ok(());
        }
        let nf = s.iter().take_while(|c| b"+- 0#'".contains(c)).count();
        let flags = &s[..nf];
        *s = &s[nf..];
        let mut f = Field {
            flags,
            left: flags.contains(&b'-'),
            ..Field::default()
        };
        if eat(s, b'*') {
            let arg = a.get(index(s));
            let w = self.integer(arg, true)? as i64;
            if i32::try_from(w).is_err() {
                return self.fail(&[
                    b"invalid field width: ",
                    &quoting::diagnostic(arg.unwrap(), self.locale),
                ]);
            }
            f.left |= w < 0;
            f.width = w.unsigned_abs() as usize;
        } else {
            f.width = digits(s)
        }
        let precision_present = eat(s, b'.');
        if precision_present {
            f.precision = if eat(s, b'*') {
                let arg = a.get(index(s));
                let p = self.integer(arg, true)? as i64;
                if p > i32::MAX as i64 {
                    return self.fail(&[
                        b"invalid precision: ",
                        &quoting::diagnostic(arg.unwrap(), self.locale),
                    ]);
                }
                (p >= 0).then_some(p as usize)
            } else {
                Some(digits(s))
            }
        }
        while s.first().is_some_and(|c| b"hlLjtz".contains(c)) {
            *s = &s[1..]
        }
        let c = s.first().copied().unwrap_or(0);
        f.conv = c;
        if c != 0 {
            *s = &s[1..]
        }
        let valid = b"diouxXcsaAeEfFgG".contains(&c)
            && !(f.flag(b'#') && b"csdiu".contains(&c)
                || f.flag(b'0') && b"cs".contains(&c)
                || f.flag(b'\'') && !b"diufFgG".contains(&c)
                || precision_present && c == b'c');
        if !valid {
            return self.fail(&[
                &original[..original.len() - s.len()],
                b": invalid conversion specification",
            ]);
        }
        let arg = a.get(pos);
        match c {
            b'c' => self.field(
                &f,
                Parts::text(&[arg.and_then(|v| v.first()).copied().unwrap_or(0)]),
            ),
            b's' => {
                let v = arg.unwrap_or_default();
                self.field(
                    &f,
                    Parts::text(&v[..v.len().min(f.precision.unwrap_or(usize::MAX))]),
                )
            }
            b'd' | b'i' | b'o' | b'u' | b'x' | b'X' => number::print(self, &f, arg),
            _ => float::print(self, &f, arg),
        }
    }
    fn format(&mut self, fmt: &[u8], mut values: &[&[u8]]) -> Result<()> {
        loop {
            let mut a = Arguments {
                values,
                next: 0,
                used: 0,
            };
            let mut s = fmt;
            while !s.is_empty() {
                let start = s;
                if eat(&mut s, b'%') {
                    self.convert(start, &mut s, &mut a)?
                } else {
                    self.span(&mut s, false)?
                }
            }
            if a.used == 0 {
                if let Some(v) = values.first() {
                    self.message(&[
                        b"warning: ignoring excess arguments, starting with ",
                        &quoting::diagnostic(v, self.locale),
                    ])?
                }
                break;
            }
            values = &values[a.used..];
            if values.is_empty() {
                break;
            }
        }
        Ok(())
    }
}
fn eat(s: &mut &[u8], b: u8) -> bool {
    if let Some(t) = s.strip_prefix(&[b]) {
        *s = t;
        true
    } else {
        false
    }
}
fn digits(s: &mut &[u8]) -> usize {
    let mut n = 0usize;
    while let Some(&c @ b'0'..=b'9') = s.first() {
        n = n.saturating_mul(10).saturating_add((c - b'0') as usize);
        *s = &s[1..]
    }
    n
}
fn index(s: &mut &[u8]) -> Option<usize> {
    let mut t = *s;
    let n = digits(&mut t);
    if n > 0 && eat(&mut t, b'$') {
        *s = t;
        Some(n - 1)
    } else {
        None
    }
}
struct Arguments<'a> {
    values: &'a [&'a [u8]],
    next: usize,
    used: usize,
}
impl<'a> Arguments<'a> {
    fn get(&mut self, i: Option<usize>) -> Option<&'a [u8]> {
        let n = i.unwrap_or_else(|| {
            let n = self.next;
            self.next += 1;
            n
        });
        self.used = self.used.max(n.saturating_add(1).min(self.values.len()));
        self.values.get(n).copied()
    }
}
pub fn run(out: &mut impl Output, args: &[&[u8]], l: Locale) -> u8 {
    let mut p = Printer {
        io: out,
        locale: l,
        status: 0,
        overflow: false,
    };
    let result = match args {
        [b"--help"] => p.write(if l.french {
            FR.as_bytes()
        } else {
            EN.as_bytes()
        }),
        [b"--version"] => p.write(VERSION.as_bytes()),
        _ => match args
            .strip_prefix(&[b"--".as_slice()])
            .unwrap_or(args)
            .split_first()
        {
            Some((f, values)) => p.format(f, values),
            None => {
                let _ = p.message(&[b"missing operand"]);
                let _ = p.error(b"Try 'printf --help' for more information.\n");
                Err(Stop::Error)
            }
        },
    };
    if p.overflow {
        let _ = p.message(&[b"write error"]);
        return 1;
    }
    match result {
        Ok(()) => p.status,
        Err(Stop::Cancel) => 0,
        Err(Stop::Error) => 1,
        Err(Stop::Write) => {
            #[cfg(target_os = "none")]
            let _ = p.message(&[b"write error"]);
            1
        }
    }
}
