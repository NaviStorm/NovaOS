use crate::options::Options;
use alloc::{format, vec::Vec};
pub fn hex(hash: &[u8; 32]) -> [u8; 64] {
    let mut result = [0; 64];
    let digits = b"0123456789abcdef";
    for (out, &value) in result.chunks_exact_mut(2).zip(hash) {
        out.copy_from_slice(&[digits[(value >> 4) as usize], digits[(value & 15) as usize]]);
    }
    result
}

pub fn record(hash: &[u8; 32], name: &[u8], opt: &Options) -> Vec<u8> {
    let escape = !opt.zero && name.iter().any(|c| b"\\\n\r".contains(c));
    let mut line = Vec::new();
    if escape {
        line.push(b'\\');
    }
    if opt.tag {
        line.extend_from_slice(b"SHA256 (");
    } else {
        line.extend_from_slice(&hex(hash));
        line.extend([b' ', opt.mode.unwrap_or(b' ')]);
    }
    for &c in name {
        match (escape, c) {
            (true, b'\n') => line.extend_from_slice(br"\n"),
            (true, b'\r') => line.extend_from_slice(br"\r"),
            (true, b'\\') => line.extend_from_slice(br"\\"),
            _ => line.push(c),
        }
    }
    if opt.tag {
        line.extend_from_slice(b") = ");
        line.extend_from_slice(&hex(hash));
    }
    line.push(if opt.zero { 0 } else { b'\n' });
    line
}

fn trim_start(mut s: &[u8]) -> &[u8] {
    while matches!(s.first(), Some(b' ' | b'\t')) {
        s = &s[1..];
    }
    s
}

pub fn entry<'a>(s: &'a [u8], reversed: &mut Option<bool>) -> Option<(&'a [u8], Vec<u8>)> {
    let s = trim_start(s);
    let escape = s.starts_with(br"\");
    let s = &s[usize::from(escape)..];
    let (hash, name) = if let Some(rest) = s.strip_prefix(b"SHA256") {
        let rest = rest.strip_prefix(b" ").unwrap_or(rest).strip_prefix(b"(")?;
        let end = rest.iter().rposition(|&c| c == b')')?;
        let hash = trim_start(trim_start(&rest[end + 1..]).strip_prefix(b"=")?);
        (hash, &rest[..end])
    } else {
        let (hash, rest) = s.split_at_checked(64)?;
        if !matches!(rest.first(), Some(b' ' | b'\t')) {
            return None;
        }
        let tail = &rest[1..];
        if tail.is_empty() {
            return None;
        }
        let legacy = tail.len() == 1 || !matches!(tail[0], b' ' | b'*');
        if legacy && *reversed == Some(false) {
            return None;
        }
        if !hash.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        let legacy = *reversed.get_or_insert(legacy);
        (hash, &tail[usize::from(!legacy)..])
    };
    if hash.len() != 64 || !hash.iter().all(u8::is_ascii_hexdigit) || name.contains(&0) {
        return None;
    }
    let mut decoded = Vec::new();
    let mut bytes = name.iter().copied();
    while let Some(c) = bytes.next() {
        decoded.push(if escape && c == b'\\' {
            match bytes.next()? {
                b'n' => b'\n',
                b'r' => b'\r',
                b'\\' => b'\\',
                _ => return None,
            }
        } else {
            c
        });
    }
    Some((hash, decoded))
}

// Présentation des noms dans les diagnostics en locale C, indépendante du format sur disque.
pub fn quote(s: &[u8]) -> Vec<u8> {
    if !s.is_empty()
        && s.iter().enumerate().all(|(i, c)| {
            c.is_ascii_alphanumeric() || b"_+,-./@%".contains(c) || i > 0 && b"#~".contains(c)
        })
    {
        return s.into();
    }
    if s.contains(&b'\'')
        && s.iter()
            .all(|&c| (32..127).contains(&c) && !b"\"$\\\x60".contains(&c))
    {
        return [b"\"".as_slice(), s, b"\""].concat();
    }
    let mut out = alloc::vec![b'\''];
    let mut control = false;
    for &c in s {
        if !(32..127).contains(&c) {
            if !control {
                out.extend_from_slice(b"'$'");
                control = true;
            }
            match c {
                7 => out.extend_from_slice(br"\a"),
                8 => out.extend_from_slice(br"\b"),
                9 => out.extend_from_slice(br"\t"),
                10 => out.extend_from_slice(br"\n"),
                11 => out.extend_from_slice(br"\v"),
                12 => out.extend_from_slice(br"\f"),
                13 => out.extend_from_slice(br"\r"),
                _ => out.extend(format!("\\{c:03o}").bytes()),
            }
        } else {
            if control {
                out.extend_from_slice(b"''");
                control = false;
            }
            if c == b'\'' {
                out.extend_from_slice(br"'\''");
            } else {
                out.push(c);
            }
        }
    }
    out.push(b'\'');
    out
}
