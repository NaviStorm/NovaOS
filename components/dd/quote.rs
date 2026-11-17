//! Rendu des octets d’un diagnostic en locale C.
use alloc::{format, string::String};
fn printable(b: u8) -> bool {
    (32..=126).contains(&b)
}
fn escape(s: &mut String, b: u8) {
    let simple = match b {
        7 => 'a',
        8 => 'b',
        9 => 't',
        10 => 'n',
        11 => 'v',
        12 => 'f',
        13 => 'r',
        _ => {
            s.push_str(&format!("\\{b:03o}"));
            return;
        }
    };
    s.push('\\');
    s.push(simple);
}
pub fn symbol(bytes: &[u8]) -> String {
    let mut s = String::from("'");
    for &b in bytes {
        if !printable(b) {
            escape(&mut s, b)
        } else {
            if matches!(b, b'\\' | b'\'') {
                s.push('\\')
            }
            s.push(b as char)
        }
    }
    s.push('\'');
    s
}
pub fn file(bytes: &[u8], always: bool) -> String {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"_+%,.:/@-".contains(&b);
    if !always && !bytes.is_empty() && bytes.iter().copied().all(plain) {
        return String::from_utf8(bytes.to_vec()).unwrap();
    }
    if bytes.contains(&b'\'')
        && bytes
            .iter()
            .all(|b| plain(*b) || matches!(*b, b'\'' | b' '))
    {
        return format!("\"{}\"", core::str::from_utf8(bytes).unwrap());
    }
    let mut s = String::from("'");
    let mut escaped = false;
    for &b in bytes {
        if printable(b) {
            if escaped {
                s.push_str("''");
                escaped = false
            }
            if b == b'\'' {
                s.push_str("'\\''")
            } else {
                s.push(b as char)
            }
        } else {
            if !escaped {
                s.push_str("'$'");
                escaped = true
            }
            escape(&mut s, b)
        }
    }
    s.push('\'');
    s
}
