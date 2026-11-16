use crate::options::{quote, usage};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

pub struct Locale<'a> {
    pub point: &'a str,
    pub thousands: &'a str,
    pub grouping: &'a [u8],
    pub utf8: bool,
}
pub const C: Locale<'static> = Locale {
    point: ".",
    thousands: "",
    grouping: &[],
    utf8: false,
};

pub struct Operand {
    pub value: f64,
    pub precision: Option<i64>,
    pub integral_width: i64,
}
impl Operand {
    pub fn one() -> Self {
        Self {
            value: 1.,
            precision: Some(0),
            integral_width: 1,
        }
    }
}

// Accumule les 53 bits utiles et garde les bits de garde pour l'arrondi pair,
// même si la mantisse dépasse la taille d'un entier machine.
fn hex(text: &str) -> Option<f64> {
    let negative = text.starts_with('-');
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    let significand = unsigned.get(2..)?;
    let (digits, power) = split_exp(significand, ['p', 'P'])?;
    let mut bits = Vec::new();
    let mut fractional = None;
    for c in digits.chars() {
        if c == '.' && fractional.is_none() {
            fractional = Some(0i64);
            continue;
        }
        let d = c.to_digit(16)?;
        for shift in (0..4).rev() {
            bits.push(((d >> shift) & 1) as u64);
        }
        if let Some(n) = &mut fractional {
            *n += 4;
        }
    }
    if bits.is_empty() {
        return None;
    }
    let sign = (negative as u64) << 63;
    let Some(start) = bits.iter().position(|&b| b == 1) else {
        return Some(f64::from_bits(sign));
    };
    let bits = &bits[start..];
    let exp = power
        .saturating_sub(fractional.unwrap_or(0))
        .saturating_add(bits.len() as i64 - 1);
    if exp > 1023 {
        return None;
    }
    if exp < -1075 {
        return Some(f64::from_bits(sign));
    }
    let keep = (exp + 1075).min(53) as usize;
    let mut m = 0;
    for i in 0..keep {
        m = (m << 1) | bits.get(i).copied().unwrap_or(0);
    }
    if bits.get(keep) == Some(&1) && (m & 1 != 0 || bits[keep + 1..].contains(&1)) {
        m += 1;
    }
    let biased = if exp >= -1022 {
        ((exp + 1022) as u64) << 52
    } else {
        0
    };
    let value = f64::from_bits(sign | (biased + m));
    value.is_finite().then_some(value)
}

fn split_exp(text: &str, marks: [char; 2]) -> Option<(&str, i64)> {
    let Some((mantissa, e)) = text.split_once(marks) else {
        return Some((text, 0));
    };
    let digits = e.strip_prefix(['-', '+']).unwrap_or(e);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = e.parse().unwrap_or(if e.starts_with('-') {
        i64::MIN
    } else {
        i64::MAX
    });
    Some((mantissa, n))
}

pub fn value(text: &str) -> Option<f64> {
    let t = text.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}');
    let unsigned = t.strip_prefix(['-', '+']).unwrap_or(t);
    if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
        return hex(t);
    }
    if let Some(payload) = unsigned.get(3..)
        && unsigned[..3].eq_ignore_ascii_case("nan")
        && payload.starts_with('(')
        && payload.ends_with(')')
        && payload[1..payload.len() - 1]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Some(f64::NAN);
    }
    let n: f64 = t.parse().ok()?;
    (n.is_finite()
        || n.is_nan()
        || unsigned.eq_ignore_ascii_case("inf")
        || unsigned.eq_ignore_ascii_case("infinity"))
    .then_some(n)
}

pub fn scan(raw: &[u8], locale: &Locale) -> Result<Operand, String> {
    let text = core::str::from_utf8(raw).unwrap_or("");
    let normalized = if locale.point != "." {
        text.replace(locale.point, ".")
    } else {
        text.to_string()
    };
    let n = value(&normalized).ok_or_else(|| {
        usage(format!(
            "invalid floating point argument: {}",
            quote(raw, locale.utf8)
        ))
    })?;
    if n.is_nan() {
        return Err(usage(format!(
            "invalid {} argument: {}",
            quote(b"not-a-number", locale.utf8),
            quote(raw, locale.utf8)
        )));
    }
    let s = text.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}' || c == '+');
    let mut p = if s.contains(['.', 'p']) {
        None
    } else {
        Some(0)
    };
    let mut width = 0;
    if n.is_finite() && !s.contains(['x', 'X']) {
        let (mantissa, e) = split_exp(s, ['e', 'E']).unwrap_or((s, 0));
        let (left, right) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let int = (left.len() as i64).max(if left.starts_with('-') { 2 } else { 1 });
        let frac = right.len() as i64;
        let precision = frac.saturating_sub(e).max(0);
        p = Some(precision);
        // Les zéros écrits dans la partie entière comptent pour -w.
        width = if e < 0 { int } else { int.saturating_add(e) };
    }
    Ok(Operand {
        value: n,
        precision: p,
        integral_width: width,
    })
}
