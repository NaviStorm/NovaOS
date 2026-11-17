use alloc::{format, string::String};

pub fn quote(value: &[u8], utf8: bool) -> String {
    let mut out = String::from(if utf8 { "‘" } else { "'" });
    let mut rest = value;
    while !rest.is_empty() {
        let b = rest[0];
        rest = &rest[1..];
        match b {
            7 => out.push_str("\\a"),
            8 => out.push_str("\\b"),
            11 => out.push_str("\\v"),
            12 => out.push_str("\\f"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b'\\' => out.push_str("\\\\"),
            b'\'' if !utf8 => out.push_str("\\'"),
            32..=126 => out.push(b as char),
            128..=255 if utf8 => {
                let n = match b {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => 0,
                };
                let mut bytes = [0u8; 4];
                bytes[0] = b;
                if n > 0 && rest.len() >= n - 1 {
                    bytes[1..n].copy_from_slice(&rest[..n - 1]);
                    if let Ok(s) = core::str::from_utf8(&bytes[..n]) {
                        if s == "’" {
                            out.push('\\');
                        }
                        if s.chars().next().is_some_and(char::is_control) {
                            for x in &bytes[..n] {
                                out.push_str(&format!("\\{x:03o}"));
                            }
                        } else {
                            out.push_str(s);
                        }
                        rest = &rest[n - 1..];
                        continue;
                    }
                }
                out.push_str(&format!("\\{b:03o}"));
            }
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push(if utf8 { '’' } else { '\'' });
    out
}
