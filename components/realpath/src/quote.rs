use alloc::vec::Vec;

#[cfg(not(target_os = "none"))]
pub const STYLES: [&str; 10] = [
    "literal",
    "shell",
    "shell-always",
    "shell-escape",
    "shell-escape-always",
    "c",
    "c-maybe",
    "escape",
    "locale",
    "clocale",
];

#[cfg(not(target_os = "none"))]
pub fn style(name: &str) -> Option<&'static str> {
    let mut candidates = STYLES.into_iter().filter(|s| s.starts_with(name));
    let first = candidates.next()?;
    (first == name || candidates.next().is_none()).then_some(first)
}

fn escaped(out: &mut Vec<u8>, b: u8) {
    out.push(b'\\');
    if let Some(i) = [7, 8, 12, 10, 13, 9, 11].iter().position(|v| *v == b) {
        out.push(b"abfnrtv"[i]);
    } else {
        out.extend_from_slice(&[b'0' + (b >> 6), b'0' + ((b >> 3) & 7), b'0' + (b & 7)]);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ByteClass {
    Graphic,
    Hidden,
    Continuation,
}

pub fn mark(classes: &mut Vec<ByteClass>, len: usize, visible: bool) {
    if visible {
        classes.push(ByteClass::Graphic);
        classes.extend(core::iter::repeat_n(ByteClass::Continuation, len - 1));
    } else {
        classes.extend(core::iter::repeat_n(ByteClass::Hidden, len));
    }
}

/// Regroupe les octets non affichables, y compris les séquences UTF-8 invalides.
#[cfg(target_os = "none")]
pub fn quoted(bytes: &[u8], style: &str, utf8: bool, printable: impl Fn(char) -> bool) -> Vec<u8> {
    if style == "literal" {
        return bytes.to_vec();
    }
    let mut hidden = Vec::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            let visible = (utf8 || c.is_ascii()) && printable(c);
            mark(&mut hidden, c.len_utf8(), visible);
        }
        hidden.extend(core::iter::repeat_n(
            ByteClass::Hidden,
            chunk.invalid().len(),
        ));
    }
    render(bytes, style, utf8, &hidden)
}

pub fn render(bytes: &[u8], style: &str, utf8: bool, hidden: &[ByteClass]) -> Vec<u8> {
    if style == "literal" {
        return bytes.to_vec();
    }
    if style.starts_with("shell") {
        let escape = style.contains("escape");
        let controls = escape && hidden.contains(&ByteClass::Hidden);
        let special = bytes
            .iter()
            .any(|b| b" \t\n\r!\"$&()*;<>?[\\^|=\x60".contains(b));
        let force = bytes.is_empty()
            || special
            || bytes.starts_with(b"#")
            || bytes.starts_with(b"~")
            || matches!(bytes, b"{" | b"}");
        if !controls
            && bytes.contains(&b'\'')
            && bytes.iter().enumerate().all(|(i, b)| {
                b.is_ascii_alphanumeric()
                    || b" '/.,_+@%:-]".contains(b)
                    || (i == 0 && matches!(b, b'#' | b'~'))
                    || (!b.is_ascii() && hidden[i] != ByteClass::Hidden)
                    || hidden[i] == ByteClass::Continuation
            })
        {
            return [b"\"".as_slice(), bytes, b"\""].concat();
        }
        if !(force || controls || style.ends_with("always") || bytes.contains(&b'\'')) {
            return bytes.to_vec();
        }
        let mut out = Vec::from(b"'");
        let mut escaping = false;
        for (&b, &hidden) in bytes.iter().zip(hidden) {
            if hidden == ByteClass::Hidden && escape {
                if !escaping {
                    out.extend_from_slice(b"'$'");
                }
                escaped(&mut out, b);
                escaping = true;
            } else {
                if escaping {
                    out.extend_from_slice(b"''");
                }
                if b == b'\'' {
                    out.extend_from_slice(b"'\\''");
                } else {
                    out.push(b);
                }
                escaping = false;
            }
        }
        out.push(b'\'');
        return out;
    }
    if style == "c-maybe" && !hidden.contains(&ByteClass::Hidden) && !bytes.contains(&b'"') {
        return bytes.to_vec();
    }
    let (open, close): (&[u8], &[u8]) = match style {
        "escape" => (b"", b""),
        "locale" | "clocale" if utf8 => ("‘".as_bytes(), "’".as_bytes()),
        "locale" => (b"'", b"'"),
        _ => (b"\"", b"\""),
    };
    let mut out = open.to_vec();
    for (i, (&b, &hide)) in bytes.iter().zip(hidden).enumerate() {
        if hide == ByteClass::Hidden {
            escaped(&mut out, b);
        } else {
            if hide != ByteClass::Continuation
                && (b == b'\\' || (!close.is_empty() && bytes[i..].starts_with(close)))
            {
                out.push(b'\\');
            }
            out.push(b);
        }
    }
    out.extend_from_slice(close);
    out
}
