use crate::{
    number::{Locale, Operand},
    options::{Diagnostic, quote},
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

#[derive(Default)]
pub struct Format {
    pub prefix: Vec<u8>,
    pub suffix: Vec<u8>,
    flags: Vec<u8>,
    width: usize,
    precision: Option<usize>,
    kind: u8,
}

fn field_size(digits: &[u8]) -> usize {
    digits.iter().fold(0usize, |size, &digit| {
        size.saturating_mul(10)
            .saturating_add(usize::from(digit - b'0'))
    })
}

impl Format {
    pub fn parse(src: &[u8], utf8: bool) -> Result<Self, Diagnostic> {
        let mut result = Self::default();
        let mut i = 0;
        let error = |msg: &str| format!("format {} {msg}", quote(src, utf8));
        while i < src.len() {
            let b = src[i];
            i += 1;
            if b != b'%' || src.get(i) == Some(&b'%') {
                if result.kind == 0 {
                    result.prefix.push(b)
                } else {
                    result.suffix.push(b)
                }
                if b == b'%' {
                    i += 1;
                }
                continue;
            }
            if result.kind != 0 {
                return Err(error("has too many % directives").into());
            }
            while i < src.len() && b"-+#0 '".contains(&src[i]) {
                result.flags.push(src[i]);
                i += 1;
            }
            let start = i;
            while src.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            result.width = field_size(&src[start..i]);
            if src.get(i) == Some(&b'.') {
                i += 1;
                let start = i;
                while src.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                result.precision = Some(field_size(&src[start..i]));
            }
            if src.get(i) == Some(&b'L') {
                i += 1;
            }
            let Some(&k) = src.get(i) else {
                return Err(error("ends in %").into());
            };
            if !b"aAeEfFgG".contains(&k) {
                return Err(Diagnostic::bytes(
                    error("has unknown %").as_bytes(),
                    &[k],
                    b" directive",
                ));
            }
            result.kind = k;
            i += 1;
        }
        if result.kind == 0 {
            return Err(error("has no % directive").into());
        }
        Ok(result)
    }

    pub fn automatic(a: &Operand, d: &Operand, b: &Operand, equal: bool) -> Self {
        let mut fmt = Self {
            kind: b'g',
            ..Self::default()
        };
        if let (Some(x), Some(y), Some(_)) = (a.precision, d.precision, b.precision) {
            let p = x.max(y);
            if p < i32::MAX as i64 {
                fmt.kind = b'f';
                fmt.precision = Some(p as usize);
                if equal {
                    let w = a
                        .integral_width
                        .max(b.integral_width)
                        .saturating_add(p)
                        .saturating_add(i64::from(p > 0));
                    if w <= i32::MAX as i64 {
                        fmt.width = w as usize;
                        fmt.flags.push(b'0');
                    }
                }
            }
        }
        fmt
    }

    // Une plage vide ne demande aucune conversion, quelle que soit la largeur.
    pub fn check_output_size(&self) -> Result<(), String> {
        if self.width > i32::MAX as usize {
            return Err("write error: Value too large to be stored in data type".into());
        }
        if self.precision.is_some_and(|p| p > i32::MAX as usize) {
            return Err("write error: Cannot allocate memory".into());
        }
        Ok(())
    }

    pub fn numeric(&self, value: f64, loc: &Locale) -> String {
        let x = value.abs();
        let k = self.kind.to_ascii_lowercase();
        let alt = self.flags.contains(&b'#');
        let mut body = if !x.is_finite() {
            if x.is_nan() { "nan" } else { "inf" }.to_string()
        } else {
            let p = self.precision.unwrap_or(6);
            match k {
                b'a' => hexadecimal(x, self.precision, alt),
                b'e' => scientific(x, p, alt, false),
                b'f' => fixed(x, p, alt),
                _ => {
                    let sig = if alt { p.max(1) } else { p.clamp(1, 1075) };
                    let (_, exp) = exponential(x, sig - 1);
                    if exp < -4 || i64::from(exp) >= sig as i64 {
                        scientific(x, sig - 1, alt, !alt)
                    } else {
                        let decimals = (sig as i64 - i64::from(exp) - 1) as usize;
                        let mut s = fixed(x, decimals, alt);
                        if !alt {
                            trim_fraction(&mut s);
                        }
                        s
                    }
                }
            }
        };
        if self.kind.is_ascii_uppercase() {
            body.make_ascii_uppercase();
        }
        let mut prefix = String::from(if value.is_sign_negative() {
            "-"
        } else if self.flags.contains(&b'+') {
            "+"
        } else if self.flags.contains(&b' ') {
            " "
        } else {
            ""
        });
        if k == b'a' && x.is_finite() {
            prefix.push_str(&body[..2]);
            body.drain(..2);
        }
        if self.flags.contains(&b'\'')
            && !loc.thousands.is_empty()
            && matches!(k, b'f' | b'g')
            && !body.contains(['e', 'E'])
        {
            let integer = body.bytes().take_while(u8::is_ascii_digit).count();
            if loc.point != "." {
                body = body.replace('.', loc.point);
            }
            let mut i = integer;
            let mut groups = loc.grouping.iter().copied();
            let mut group = groups.next().unwrap_or(127);
            while group > 0 && group < 127 && i > usize::from(group) {
                i -= usize::from(group);
                body.insert_str(i, loc.thousands);
                group = groups.next().unwrap_or(group);
            }
        } else if loc.point != "." {
            body = body.replace('.', loc.point);
        }
        let pad = self.width.saturating_sub(body.len() + prefix.len());
        let left = self.flags.contains(&b'-');
        let zero = self.flags.contains(&b'0') && x.is_finite() && !left;
        if zero {
            prefix.extend(core::iter::repeat_n('0', pad));
        }
        prefix.push_str(&body);
        if left {
            prefix.extend(core::iter::repeat_n(' ', pad));
        } else if !zero && pad > 0 {
            prefix = format!("{}{prefix}", " ".repeat(pad));
        }
        prefix
    }
}

fn fixed(x: f64, p: usize, alt: bool) -> String {
    // Un binaire64 a au plus 1074 décimales exactes.
    let mut s = format!("{:.*}", p.min(1074), x);
    if alt && p == 0 {
        s.push('.');
    }
    s.extend(core::iter::repeat_n('0', p.saturating_sub(1074)));
    s
}

fn exponential(x: f64, p: usize) -> (String, i32) {
    let s = format!("{:.*e}", p.min(1074), x);
    let (m, e) = s.split_once('e').unwrap();
    let mut m = m.to_string();
    m.extend(core::iter::repeat_n('0', p.saturating_sub(1074)));
    (m, e.parse().unwrap())
}
fn trim_fraction(s: &mut String) {
    if s.contains('.') {
        s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
    }
}
fn scientific(x: f64, p: usize, alt: bool, trim: bool) -> String {
    let (mut m, e) = exponential(x, p);
    if alt && p == 0 {
        m.push('.');
    }
    if trim {
        trim_fraction(&mut m);
    }
    format!("{m}e{e:+03}")
}
fn hexadecimal(x: f64, precision: Option<usize>, alt: bool) -> String {
    let bits = x.to_bits();
    let biased = (bits >> 52) as i32;
    let mut m = bits & ((1 << 52) - 1);
    if biased != 0 {
        m |= 1 << 52;
    }
    let e = if bits == 0 { 0 } else { biased.max(1) - 1023 };
    let p = precision.unwrap_or(13);
    let digits = p.min(13);
    let shift = 4 * (13 - digits);
    if shift != 0 {
        let half = 1u64 << (shift - 1);
        let lost = m & ((1 << shift) - 1);
        m >>= shift;
        m += u64::from(lost > half || lost == half && m & 1 != 0);
    }
    let mut s = format!("{m:0width$x}", width = digits + 1);
    s.insert(1, '.');
    if precision.is_none() {
        s.truncate(s.trim_end_matches('0').len());
    }
    s.extend(core::iter::repeat_n('0', p.saturating_sub(13)));
    if !alt && s.ends_with('.') {
        s.pop();
    }
    format!("0x{s}p{e:+}")
}
