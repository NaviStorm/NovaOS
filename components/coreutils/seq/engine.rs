use crate::{
    format::Format,
    number::{self, C, Locale, Operand},
    options::{self, Diagnostic, Options, quote, usage},
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::cmp::Ordering;

pub enum Action {
    Help,
    Version,
    Done,
}
pub enum Failure<E> {
    Arguments(Diagnostic),
    Write(E),
}
impl<E> From<String> for Failure<E> {
    fn from(s: String) -> Self {
        Self::Arguments(s.into())
    }
}

impl<E> From<Diagnostic> for Failure<E> {
    fn from(s: Diagnostic) -> Self {
        Self::Arguments(s)
    }
}

fn digits(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(u8::is_ascii_digit)
}
fn canonical(s: &[u8]) -> &[u8] {
    &s[s.iter().position(|b| *b != b'0').unwrap_or(s.len() - 1)..]
}

fn integers<E>(
    start: &[u8],
    step: u32,
    end: Option<&[u8]>,
    sep: u8,
    mut write: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<(), Failure<E>> {
    let mut current = canonical(start).to_vec();
    let end = end.map(canonical);
    let mut buf = Vec::with_capacity(32768);
    while end.is_none_or(|limit| {
        current
            .len()
            .cmp(&limit.len())
            .then_with(|| current.as_slice().cmp(limit))
            != Ordering::Greater
    }) {
        buf.extend_from_slice(&current);
        let mut carry = step;
        for digit in current.iter_mut().rev() {
            let sum = u32::from(*digit - b'0') + carry;
            *digit = b'0' + (sum % 10) as u8;
            carry = sum / 10;
            if carry == 0 {
                break;
            }
        }
        if carry != 0 {
            let mut next = carry.to_string().into_bytes();
            next.extend(&current);
            current = next;
        }
        if end.is_some_and(|limit| {
            current.len() > limit.len()
                || current.len() == limit.len() && current.as_slice() > limit
        }) {
            buf.push(b'\n');
            break;
        }
        buf.push(sep);
        if buf.len() >= 32768 {
            write(&buf).map_err(Failure::Write)?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        write(&buf).map_err(Failure::Write)?;
    }
    Ok(())
}

pub fn execute<E>(
    args: &[&[u8]],
    loc: &Locale,
    mut write: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<Action, Failure<E>> {
    let (format, sep, equal, ops) = match options::parse(args)? {
        Options::Help => return Ok(Action::Help),
        Options::Version => return Ok(Action::Version),
        Options::Numbers {
            format,
            separator,
            equal,
            operands,
        } => (format, separator, equal, operands),
    };
    if ops.is_empty() {
        return Err(usage("missing operand".into()).into());
    }
    if ops.len() > 3 {
        return Err(usage(format!("extra operand {}", quote(ops[3], loc.utf8))).into());
    }
    let fmt = format.map(|s| Format::parse(s, loc.utf8)).transpose()?;
    if fmt.is_some() && equal {
        return Err(usage(
            "format string may not be specified when printing equal width strings".into(),
        )
        .into());
    }
    let fast = fmt.is_none() && !equal && sep.len() == 1;
    let start = if ops.len() == 1 { b"1" } else { ops[0] };
    let last = ops[ops.len() - 1];
    let increment = if ops.len() == 3 { ops[1] } else { b"1" };
    if fast
        && digits(start)
        && digits(last)
        && digits(increment)
        && let Ok(d) = core::str::from_utf8(canonical(increment))
            .unwrap()
            .parse::<u32>()
        && (1..=200).contains(&d)
    {
        integers(start, d, Some(last), sep[0], write)?;
        return Ok(Action::Done);
    }
    let a = if ops.len() > 1 {
        number::scan(start, loc)?
    } else {
        Operand::one()
    };
    let d = if ops.len() == 3 {
        number::scan(increment, loc)?
    } else {
        Operand::one()
    };
    if d.value == 0. {
        return Err(usage(format!(
            "invalid Zero increment value: {}",
            quote(increment, loc.utf8)
        ))
        .into());
    }
    let b = number::scan(last, loc)?;
    if fast
        && [a.precision, d.precision, b.precision] == [Some(0); 3]
        && a.value.is_finite()
        && !a.value.is_sign_negative()
        && !b.value.is_sign_negative()
        && d.value > 0.
        && d.value <= 200.
    {
        let first = if digits(start) {
            start.to_vec()
        } else {
            format!("{:.0}", a.value).into_bytes()
        };
        let end = b
            .value
            .is_finite()
            .then(|| format!("{:.0}", b.value).into_bytes());
        integers(&first, d.value as u32, end.as_deref(), sep[0], write)?;
        return Ok(Action::Done);
    }
    let fmt = fmt.unwrap_or_else(|| Format::automatic(&a, &d, &b, equal));
    let beyond = |v| {
        if d.value < 0. {
            v < b.value
        } else {
            v > b.value
        }
    };
    let mut x = a.value;
    if beyond(x) {
        return Ok(Action::Done);
    }
    fmt.check_output_size()?;
    let mut i = 0.;
    let mut final_term = false;
    loop {
        let text = fmt.numeric(x, loc);
        for part in [&fmt.prefix[..], text.as_bytes(), &fmt.suffix[..]] {
            if !part.is_empty() {
                write(part).map_err(Failure::Write)?;
            }
        }
        if final_term {
            break;
        }
        i += 1.;
        let next = a.value + i * d.value;
        if beyond(next) {
            let canonical = fmt.numeric(next, &C);
            if number::value(&canonical) != Some(b.value) || canonical == text {
                break;
            }
            final_term = true;
        }
        write(sep).map_err(Failure::Write)?;
        x = next;
    }
    write(b"\n").map_err(Failure::Write)?;
    Ok(Action::Done)
}
