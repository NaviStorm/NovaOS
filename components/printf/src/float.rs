//! Binary64 is represented by its bits. Rounding is integer round-to-nearest, ties-to-even.
use crate::{
    Output,
    engine::{Field, Parts, Printer, Result},
    number::{group, sign, start},
};
use alloc::{format, string::String, vec::Vec};
use num_bigint::BigUint;

const HIDDEN: u64 = 1 << 52;
const INF: u64 = 0x7ff0_0000_0000_0000;
const NEG: u64 = 1 << 63;
fn nearest(n: BigUint, d: BigUint) -> (u64, bool) {
    let q = &n / &d;
    let rem = n - q.clone() * &d;
    let inexact = rem.bits() != 0;
    let q = q.to_u64_digits().first().copied().unwrap_or(0);
    let twice = rem << 1;
    (
        q + u64::from(twice > d || twice == d && q % 2 != 0),
        inexact,
    )
}
fn binary(n: BigUint, d: BigUint) -> (u64, bool) {
    if n.bits() == 0 {
        return (0, false);
    }
    let mut exp = n.bits() as i64 - d.bits() as i64;
    if exp > 1024 {
        return (INF, true);
    }
    if exp < -1075 {
        return (0, true);
    }
    let below = if exp >= 0 {
        n < (&d << exp as usize)
    } else {
        (&n << (-exp) as usize) < d
    };
    exp -= i64::from(below);
    let shift = 52 - exp.max(-1022);
    let (mantissa, inexact) = if shift >= 0 {
        nearest(n << shift as usize, d)
    } else {
        nearest(n, d << (-shift) as usize)
    };
    if exp < -1022 {
        return (mantissa, inexact && mantissa < HIDDEN);
    }
    let bits = ((exp + 1022) as u64) * HIDDEN + mantissa;
    (bits.min(INF), bits >= INF)
}
fn begins(v: &[u8], p: &[u8]) -> bool {
    v.get(..p.len()).is_some_and(|s| s.eq_ignore_ascii_case(p))
}
fn exponent(s: &[u8], i: &mut usize, marker: u8) -> i32 {
    let mark = *i;
    if !s.get(mark).is_some_and(|c| c.eq_ignore_ascii_case(&marker)) {
        return 0;
    }
    let mut j = mark + 1;
    let neg = s.get(j) == Some(&b'-');
    j += usize::from(matches!(s.get(j), Some(b'-' | b'+')));
    let begin = j;
    let mut value = 0i32;
    while let Some(&b @ b'0'..=b'9') = s.get(j) {
        value = value.saturating_mul(10).saturating_add((b - b'0') as i32);
        j += 1
    }
    if j == begin {
        return 0;
    }
    *i = j;
    if neg { -value } else { value }
}
fn scan(s: &[u8], decimal: &[u8]) -> (u64, usize, bool) {
    let (neg, mut i) = start(s);
    let sign = if neg { NEG } else { 0 };
    let rest = &s[i..];
    if begins(rest, b"nan") {
        i += 3;
        if s.get(i) == Some(&b'(')
            && let Some(end) = s[i + 1..].iter().position(|&c| c == b')')
        {
            i += end + 2
        }
        return (INF + 1, i, false);
    }
    if begins(rest, b"inf") {
        return (
            INF | sign,
            i + if begins(rest, b"infinity") { 8 } else { 3 },
            false,
        );
    }
    let hex = begins(rest, b"0x")
        && (rest.get(2).is_some_and(u8::is_ascii_hexdigit)
            || rest[2..]
                .strip_prefix(decimal)
                .and_then(|v| v.first())
                .is_some_and(u8::is_ascii_hexdigit));
    if hex {
        i += 2
    }
    let base = if hex { 16 } else { 10 };
    let mut digits = Vec::new();
    let mut fraction = None;
    while i < s.len() {
        if (s[i] as char).is_digit(base) {
            digits.push(s[i]);
            i += 1
        } else if fraction.is_none() && s[i..].starts_with(decimal) {
            fraction = Some(digits.len());
            i += decimal.len()
        } else {
            break;
        }
    }
    if digits.is_empty() {
        return (0, 0, false);
    }
    let exp = exponent(s, &mut i, if hex { b'p' } else { b'e' }) as i64;
    let fractional = digits.len() - fraction.unwrap_or(digits.len());
    let exp = exp - (fractional as i64) * if hex { 4 } else { 1 };
    let Some(first) = digits.iter().position(|&v| v != b'0') else {
        return (sign, i, false);
    };
    let significant = &digits[first..];
    // Reject remote exponents before allocating their powers.
    let order = exp + significant.len() as i64 * if hex { 4 } else { 1 };
    let (bits, range) = if order > if hex { 1028 } else { 310 } {
        (INF, true)
    } else if order < if hex { -1076 } else { -324 } {
        (0, true)
    } else {
        let n = BigUint::parse_bytes(significant, base).unwrap();
        let power = BigUint::from(if hex { 2u8 } else { 10 }).pow(exp.unsigned_abs() as u32);
        if exp >= 0 {
            binary(n * power, 1u8.into())
        } else {
            binary(n, power)
        }
    };
    (sign | bits, i, range)
}

// Finite binary fractions have at most 1074 decimal places.
fn exact(m: u64, power: i32) -> (Vec<u8>, i32) {
    if m == 0 {
        return (b"0".to_vec(), 1);
    }
    let k = (-power).max(0);
    let n = if power >= 0 {
        BigUint::from(m) << power as usize
    } else {
        BigUint::from(m) * BigUint::from(5u8).pow(k as u32)
    };
    let mut digits = n.to_str_radix(10).into_bytes();
    let point = digits.len() as i32 - k;
    while digits.len() > 1 && digits.last() == Some(&b'0') {
        digits.pop();
    }
    (digits, point)
}
fn rounded(d: &mut Vec<u8>, point: &mut i32, keep: i64) {
    if keep >= d.len() as i64 {
        return;
    }
    let up = keep >= 0 && {
        let k = keep as usize;
        d[k] > b'5'
            || d[k] == b'5' && (d[k + 1..].iter().any(|&c| c != b'0') || k > 0 && d[k - 1] % 2 == 1)
    };
    d.truncate(keep.max(0) as usize);
    if up {
        for c in d.iter_mut().rev() {
            if *c < b'9' {
                *c += 1;
                return;
            }
            *c = b'0'
        }
        d.insert(0, b'1');
        *point += 1
    }
    if d.is_empty() {
        d.push(b'0');
        *point = 1
    }
}
fn hex_parts(
    m: u64,
    exp: i32,
    precision: Option<usize>,
    alt: bool,
    upper: bool,
    decimal: &[u8],
) -> (Vec<u8>, usize, Vec<u8>) {
    let count = precision.unwrap_or(13);
    let keep = count.min(13);
    let shift = (13 - keep) * 4;
    let mut value = m >> shift;
    if shift > 0 {
        let rem = m - (value << shift);
        let half = 1u64 << (shift - 1);
        if rem > half || rem == half && value % 2 == 1 {
            value += 1
        }
    }
    let mut s = format!("{value:0width$x}", width = 1 + keep).into_bytes();
    if precision.is_none() {
        while s.len() > 1 && s.last() == Some(&b'0') {
            s.pop();
        }
    }
    if upper {
        s.make_ascii_uppercase()
    }
    let mut body = s[..1].to_vec();
    if s.len() > 1 || count > 13 || alt {
        body.extend(decimal);
        body.extend(&s[1..])
    }
    (
        body,
        count.saturating_sub(13),
        format!("{}{exp:+}", if upper { 'P' } else { 'p' }).into_bytes(),
    )
}
pub fn print<O: Output>(p: &mut Printer<O>, f: &Field, arg: Option<&[u8]>) -> Result<()> {
    let bits = if let Some(v) = arg {
        if let Some(ch) = p.character(v)? {
            binary(ch.into(), 1u8.into()).0
        } else {
            let local = scan(v, p.locale.decimal);
            let (bits, n, range) = if p.locale.decimal != b"." && local.1 < v.len() {
                let c = scan(v, b".");
                if c.1 > local.1 { c } else { local }
            } else {
                local
            };
            p.verify(v, n, range)?;
            bits
        }
    } else {
        0
    };
    let upper = f.conv.is_ascii_uppercase();
    let kind = f.conv.to_ascii_lowercase();
    let abs = bits & !NEG;
    let mut prefix = sign(bits & NEG != 0, f).to_vec();
    if abs >= INF {
        let s = if abs == INF {
            if upper { b"INF" } else { b"inf" }
        } else {
            prefix.clear();
            if upper { b"NAN" } else { b"nan" }
        };
        return p.field(
            f,
            Parts {
                sign: &prefix,
                ..Parts::text(s)
            },
        );
    }
    let e = (abs >> 52) as i32;
    let m = abs % (1 << 52) + if e == 0 { 0 } else { HIDDEN };
    let exponent = e.max(1) - 1023;
    let (body, trailing, suffix) = if kind == b'a' {
        prefix.extend(if upper { b"0X" } else { b"0x" });
        hex_parts(
            m,
            if m == 0 { 0 } else { exponent },
            f.precision,
            f.flag(b'#'),
            upper,
            p.locale.decimal,
        )
    } else {
        let precision = f.precision.unwrap_or(6);
        let general = kind == b'g';
        let (mut digits, mut point) = exact(m, exponent - 52);
        let bounded = precision.min(i64::MAX as usize) as i64;
        let keep = match kind {
            b'f' => bounded.saturating_add(point.into()),
            b'e' => bounded.saturating_add(1),
            _ => bounded.max(1),
        };
        rounded(&mut digits, &mut point, keep);
        let sci = kind == b'e' || general && (point <= -4 || point as i64 > keep);
        let exponent = point - 1;
        if sci {
            point = 1
        }
        let mut decimals = if general {
            keep.saturating_sub(i64::from(point)).max(0) as usize
        } else {
            precision
        };
        if general && !f.flag(b'#') {
            while digits.len() > 1 && digits.last() == Some(&b'0') {
                digits.pop();
            }
            decimals = decimals.min((digits.len() as i32 - point).max(0) as usize);
        }
        let mut whole = if point <= 0 {
            b"0".to_vec()
        } else {
            let mut n = digits[..digits.len().min(point as usize)].to_vec();
            n.resize(point as usize, b'0');
            n
        };
        if f.flag(b'\'') && !sci {
            whole = group(&whole, p.locale)
        }
        let mut frac = Vec::new();
        if decimals > 0 || f.flag(b'#') {
            whole.extend(p.locale.decimal);
            frac.resize(((-point).max(0) as usize).min(decimals), b'0');
            let first = (point.max(0) as usize).min(digits.len());
            let n = (digits.len() - first).min(decimals - frac.len());
            frac.extend(&digits[first..first + n]);
            whole.extend(&frac);
        }
        let suffix = if sci {
            format!("{}{exponent:+03}", if upper { 'E' } else { 'e' })
        } else {
            String::new()
        };
        (whole, decimals - frac.len(), suffix.into_bytes())
    };
    p.field(
        f,
        Parts {
            sign: &prefix,
            body: &body,
            suffix: &suffix,
            leading: 0,
            trailing,
            zero_width: f.flag(b'0'),
        },
    )
}
