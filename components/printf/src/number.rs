use crate::{
    Locale, Output,
    engine::{Field, Parts, Printer, Result},
    quoting,
};
use alloc::{format, string::ToString, vec::Vec};

pub fn sign(negative: bool, f: &Field) -> &'static [u8] {
    if negative {
        b"-"
    } else if f.flag(b'+') {
        b"+"
    } else if f.flag(b' ') {
        b" "
    } else {
        b""
    }
}
pub fn group(digits: &[u8], l: Locale) -> Vec<u8> {
    let mut end = digits.len();
    let mut cuts = Vec::new();
    if !l.separator.is_empty() {
        let mut sizes = l.groups.iter();
        let mut last = 0;
        loop {
            let size = match sizes.next() {
                Some(&127) => break,
                Some(&0) | None => last,
                Some(&n) => n as usize,
            };
            if size == 0 || end <= size {
                break;
            }
            end -= size;
            cuts.push(end);
            last = size;
        }
    }
    let mut out = Vec::new();
    let mut start = 0;
    for end in cuts.into_iter().rev() {
        out.extend(&digits[start..end]);
        out.extend(l.separator);
        start = end
    }
    out.extend(&digits[start..]);
    out
}
impl<O: Output> Printer<'_, O> {
    pub fn character(&mut self, v: &[u8]) -> Result<Option<u32>> {
        if !matches!(v.first(), Some(b'\'' | b'"')) || v.len() < 2 {
            return Ok(None);
        }
        let (ch, n, _) = (self.locale.decode)(&v[1..]).unwrap_or((v[1] as u32, 1, false));
        if v.len() > n + 1 && !self.locale.posix {
            self.message(&[
                b"warning: ",
                &v[n + 1..],
                b": character(s) following character constant have been ignored",
            ])?
        }
        Ok(Some(ch))
    }
    pub fn verify(&mut self, v: &[u8], used: usize, range: bool) -> Result<()> {
        let why: &[u8] = if used == 0 {
            b": expected a numeric value"
        } else if range {
            b": Result too large"
        } else if used != v.len() {
            b": value not completely converted"
        } else {
            return Ok(());
        };
        self.status = 1;
        self.message(&[&quoting::diagnostic(v, self.locale), why])
    }
    pub fn integer(&mut self, arg: Option<&[u8]>, signed: bool) -> Result<u64> {
        let Some(v) = arg else { return Ok(0) };
        if let Some(ch) = self.character(v)? {
            return Ok(ch as u64);
        }
        let (negative, mut i) = start(v);
        let mut base = 10;
        if v.get(i) == Some(&b'0') {
            base = 8;
            if matches!(v.get(i + 1), Some(b'x' | b'X'))
                && v.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
            {
                i += 2;
                base = 16
            }
        }
        let first = i;
        let mut n = 0u64;
        let mut range = false;
        let limit = if signed {
            i64::MAX as u64 + u64::from(negative)
        } else {
            u64::MAX
        };
        while let Some(digit) = v.get(i).and_then(|&b| (b as char).to_digit(base)) {
            n = n
                .checked_mul(base.into())
                .and_then(|x| x.checked_add(digit.into()))
                .filter(|&x| x <= limit)
                .unwrap_or_else(|| {
                    range = true;
                    limit
                });
            i += 1;
        }
        self.verify(v, if i == first { 0 } else { i }, range)?;
        Ok(if negative && (signed || !range) {
            n.wrapping_neg()
        } else {
            n
        })
    }
}
pub fn start(v: &[u8]) -> (bool, usize) {
    let i = v
        .iter()
        .position(|b| !matches!(b, b' ' | 9..=13))
        .unwrap_or(v.len());
    (
        v.get(i) == Some(&b'-'),
        i + usize::from(matches!(v.get(i), Some(b'-' | b'+'))),
    )
}
// The last locale grouping size repeats. Describe a run once, even for
// precision INT_MAX, and stream the virtual leading zeros.
fn grouped_integer<O: Output>(
    p: &mut Printer<O>,
    f: &Field,
    sign: &[u8],
    zeros: usize,
    digits: &[u8],
) -> Result<()> {
    let locale = p.locale;
    let total = zeros + digits.len();
    if total > isize::MAX as usize || f.width > isize::MAX as usize {
        p.overflow = true;
        return Ok(());
    }
    let mut left = total;
    let mut runs = Vec::new();
    for (index, &size) in locale.groups.iter().enumerate() {
        let size = size as usize;
        if size == 0 || size == 127 || left <= size {
            break;
        }
        let repeat = if index + 1 == locale.groups.len() {
            (left - 1) / size
        } else {
            1
        };
        runs.push((size, repeat));
        left -= size * repeat;
    }
    let separators: usize = runs.iter().map(|&(_, n)| n).sum();
    if sign.len() as u128 + total as u128 + separators as u128 * locale.separator.len() as u128
        > isize::MAX as u128
    {
        p.overflow = true;
        return Ok(());
    }
    let padding = f
        .width
        .saturating_sub(sign.len() + total + separators * locale.separator.len());
    let zero = f.flag(b'0') && f.precision.is_none() && !f.left;
    if !f.left && !zero {
        p.pad(padding, b' ')?
    }
    p.write(sign)?;
    if zero {
        p.pad(padding, b'0')?
    }
    let mut position = 0;
    let mut emit = |p: &mut Printer<O>, size: usize| -> Result<()> {
        let count = zeros.saturating_sub(position).min(size);
        p.pad(count, b'0')?;
        let start = position.saturating_sub(zeros);
        p.write(&digits[start..start + size - count])?;
        position += size;
        Ok(())
    };
    emit(p, left)?;
    for (size, repeat) in runs.into_iter().rev() {
        for _ in 0..repeat {
            p.write(locale.separator)?;
            emit(p, size)?
        }
    }
    if f.left {
        p.pad(padding, b' ')?
    }
    Ok(())
}
pub fn print<O: Output>(p: &mut Printer<O>, f: &Field, arg: Option<&[u8]>) -> Result<()> {
    let signed = matches!(f.conv, b'd' | b'i');
    let raw = p.integer(arg, signed)?;
    let negative = signed && (raw as i64) < 0;
    let n = if negative { raw.wrapping_neg() } else { raw };
    let digits = if n == 0 && f.precision == Some(0) {
        Vec::new()
    } else {
        match f.conv {
            b'o' => format!("{n:o}"),
            b'x' => format!("{n:x}"),
            b'X' => format!("{n:X}"),
            _ => n.to_string(),
        }
        .into_bytes()
    };
    let mut leading = f.precision.unwrap_or(0).saturating_sub(digits.len());
    if f.conv == b'o' && f.flag(b'#') && digits.first() != Some(&b'0') {
        leading = leading.max(1)
    }
    let sign = if n != 0 && f.flag(b'#') && matches!(f.conv, b'x' | b'X') {
        if f.conv == b'x' {
            b"0x".as_slice()
        } else {
            b"0X"
        }
    } else if signed {
        sign(negative, f)
    } else {
        b""
    };
    if f.flag(b'\'') && !p.locale.separator.is_empty() {
        return grouped_integer(p, f, sign, leading, &digits);
    }
    p.field(
        f,
        Parts {
            sign,
            leading,
            body: &digits,
            trailing: 0,
            suffix: b"",
            zero_width: f.flag(b'0') && f.precision.is_none(),
        },
    )
}
