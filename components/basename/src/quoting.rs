use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq)]
pub enum Style {
    Literal,
    Shell,
    ShellAlways,
    ShellEscape,
    ShellEscapeAlways,
    C,
    CMaybe,
    Escape,
    Locale,
    CLocale,
}

impl Style {
    #[cfg(not(target_os = "none"))]
    pub fn parse(word: &[u8]) -> Option<Self> {
        use Style::*;
        let styles: &[(&[u8], Style)] = &[
            (b"literal", Literal),
            (b"shell", Shell),
            (b"shell-always", ShellAlways),
            (b"shell-escape", ShellEscape),
            (b"shell-escape-always", ShellEscapeAlways),
            (b"c", C),
            (b"c-maybe", CMaybe),
            (b"escape", Escape),
            (b"locale", Locale),
            (b"clocale", CLocale),
        ];
        let mut matches = styles.iter().filter(|(name, _)| name.starts_with(word));
        let &(name, style) = matches.next()?;
        (name == word || matches.next().is_none()).then_some(style)
    }
}

fn units(input: &[u8], utf8: bool, mut visit: impl FnMut(&[u8], bool)) {
    #[cfg(not(target_os = "none"))]
    if !utf8 {
        return crate::host::native_units(input, visit);
    }
    for chunk in input.utf8_chunks() {
        for c in chunk.valid().chars() {
            let mut buf = [0; 4];
            let printable = if c.is_ascii() {
                !c.is_ascii_control()
            } else {
                utf8 && unicode_printable(c)
            };
            visit(c.encode_utf8(&mut buf).as_bytes(), printable);
        }
        for b in chunk.invalid() {
            visit(core::slice::from_ref(b), false);
        }
    }
}

#[cfg(target_os = "none")]
fn unicode_printable(c: char) -> bool {
    nova_cli::tty::printable(c)
}
#[cfg(not(target_os = "none"))]
fn unicode_printable(c: char) -> bool {
    unsafe extern "C" {
        fn iswprint(c: i32) -> i32;
    }
    // LC_CTYPE est initialisée avant toute sortie par le programme monothread.
    unsafe { iswprint(c as i32) != 0 }
}

fn escape(out: &mut Vec<u8>, bytes: &[u8]) {
    for &b in bytes {
        out.push(b'\\');
        match b {
            7..=13 => out.push(b"abtnvfr"[(b - 7) as usize]),
            _ => out.extend([b'0' + (b >> 6), b'0' + ((b >> 3) & 7), b'0' + (b & 7)]),
        }
    }
}

pub fn quote(name: &[u8], style: Style, utf8: bool) -> Vec<u8> {
    use Style::*;
    if style == Literal {
        return name.to_vec();
    }
    let mut control = false;
    units(name, utf8, |_, printable| control |= !printable);
    if matches!(style, Shell | ShellAlways | ShellEscape | ShellEscapeAlways) {
        let escaped = matches!(style, ShellEscape | ShellEscapeAlways);
        let needs = name.is_empty()
            || matches!(name, b"{" | b"}")
            || name.first().is_some_and(|c| b"#~".contains(c))
            || name
                .iter()
                .any(|c| b" \t\r\n'\"!$&()*;<=>?[\\^\x60|".contains(c))
            || (escaped && control);
        if !needs && matches!(style, Shell | ShellEscape) {
            return name.to_vec();
        }
        if name.contains(&b'\'') && !control && !name.iter().any(|c| b"!$\\\"\x60".contains(c)) {
            return [b"\"".as_slice(), name, b"\""].concat();
        }
        let mut out = alloc::vec![b'\''];
        let mut in_escape = false;
        units(name, utf8, |unit, printable| {
            if escaped && !printable {
                if !in_escape {
                    out.extend_from_slice(b"'$'");
                }
                escape(&mut out, unit);
                in_escape = true;
            } else {
                if unit == b"'" {
                    out.extend_from_slice(b"'\\''");
                } else {
                    if in_escape {
                        out.extend_from_slice(b"''");
                    }
                    out.extend_from_slice(unit);
                }
                in_escape = false;
            }
        });
        out.push(b'\'');
        return out;
    }
    if style == CMaybe && !control && !name.contains(&b'"') {
        return name.to_vec();
    }
    let (left, right): (&[u8], &[u8]) = match style {
        Escape => (b"", b""),
        Locale | CLocale if utf8 => ("‘".as_bytes(), "’".as_bytes()),
        Locale => (b"'", b"'"),
        _ => (b"\"", b"\""),
    };
    let mut out = left.to_vec();
    units(name, utf8, |unit, printable| {
        if printable {
            if unit == right || unit == b"\\" {
                out.push(b'\\');
            }
            out.extend_from_slice(unit);
        } else {
            escape(&mut out, unit);
        }
    });
    out.extend_from_slice(right);
    out
}
