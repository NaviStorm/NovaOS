use alloc::vec::Vec;

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
        return Some(if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
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
    Some(value)
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

fn value(text: &str) -> Option<f64> {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let is_hex = unsigned.starts_with("0x") || unsigned.starts_with("0X");
    let n = if is_hex {
        hex(text)?
    } else {
        text.parse::<f64>().ok()?
    };
    if n.is_nan() || n < 0.0 {
        return None;
    }
    if n == 0.0 {
        // Une valeur négative non nulle sous-fluant vers -0 reste invalide.
        let digits = if is_hex {
            unsigned[2..].split(['p', 'P']).next()?
        } else {
            unsigned.split(['e', 'E']).next()?
        };
        if digits.bytes().any(|b| b != b'0' && b != b'.') {
            return (!text.starts_with('-')).then_some(f64::from_bits(1));
        }
    }
    Some(n)
}

pub fn seconds(raw: &[u8], point: &str) -> Option<f64> {
    let text = core::str::from_utf8(raw)
        .ok()?
        .trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}');
    let local;
    let text = if point != "." && !point.is_empty() && text.contains(point) {
        if text.contains('.') {
            return None;
        }
        local = text.replace(point, ".");
        local.as_str()
    } else {
        text
    };
    // Dans 0x.01d, d est un chiffre de la mantisse. Essayer le nombre entier d’abord.
    let (n, scale) = if let Some(n) = value(text) {
        (n, 1.0)
    } else {
        let factor = match text.as_bytes().last()? {
            b's' => 1.0,
            b'm' => 60.0,
            b'h' => 3600.0,
            b'd' => 86400.0,
            _ => return None,
        };
        (value(&text[..text.len() - 1])?, factor)
    };
    Some(upper(upper(n) * scale))
}

pub fn upper(n: f64) -> f64 {
    if n > 0.0 && n.is_finite() {
        n.next_up()
    } else {
        n
    }
}
