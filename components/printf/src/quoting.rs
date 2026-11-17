use crate::Locale;
use alloc::{format, vec::Vec};

fn escaped(b: u8) -> Vec<u8> {
    match b {
        7 => br"\a".to_vec(),
        8 => br"\b".to_vec(),
        9 => br"\t".to_vec(),
        10 => br"\n".to_vec(),
        11 => br"\v".to_vec(),
        12 => br"\f".to_vec(),
        13 => br"\r".to_vec(),
        _ => format!("\\{b:03o}").into_bytes(),
    }
}
pub fn diagnostic(mut s: &[u8], l: Locale) -> Vec<u8> {
    let mut out = if l.curved_quotes {
        "‘".as_bytes().to_vec()
    } else {
        b"'".to_vec()
    };
    while !s.is_empty() {
        let (_, n, print) = (l.decode)(s).unwrap_or((0, 1, false));
        if print {
            if s[0] == b'\\'
                || (!l.curved_quotes && s[0] == b'\'')
                || l.curved_quotes && s.starts_with("’".as_bytes())
            {
                out.push(b'\\')
            }
            out.extend(&s[..n]);
        } else {
            for &b in &s[..n] {
                out.extend(escaped(b));
            }
        }
        s = &s[n..];
    }
    out.extend(if l.curved_quotes {
        "’".as_bytes()
    } else {
        b"'"
    });
    out
}

// Each non-printable run forms one ANSI-C quoted fragment.
pub fn shell(s: &[u8], l: Locale) -> Vec<u8> {
    let mut single =
        s.is_empty() || matches!(s, b"{" | b"}") || matches!(s.first(), Some(b'#' | b'~'));
    let mut double = s.contains(&b'\'');
    let mut tail = s;
    while !tail.is_empty() {
        let (_, n, print) = (l.decode)(tail).unwrap_or((0, 1, false));
        let c = tail[0];
        if !print || b" \t!?\"$&()*;<=>[^|\x60\\'".contains(&c) {
            single = true;
        }
        if !print
            || b"!?\"$&()*;<=>[^|\x60\\{}".contains(&c)
            || b"#~".contains(&c) && tail.len() != s.len()
        {
            double = false;
        }
        tail = &tail[n..];
    }
    if !single {
        return s.to_vec();
    }
    if double {
        return [b"\"".as_slice(), s, b"\""].concat();
    }
    let mut out = b"'".to_vec();
    let mut ansi = false;
    tail = s;
    while !tail.is_empty() {
        let (_, n, print) = (l.decode)(tail).unwrap_or((0, 1, false));
        if !print {
            if !ansi {
                out.extend(b"'$'");
                ansi = true;
            }
            for &b in &tail[..n] {
                out.extend(escaped(b))
            }
        } else {
            if ansi {
                if tail[0] != b'\'' {
                    out.extend(b"''");
                }
                ansi = false;
            }
            if tail[0] == b'\'' {
                out.extend(b"'\\''")
            } else {
                out.extend(&tail[..n])
            }
        }
        tail = &tail[n..];
    }
    out.push(b'\'');
    out
}
