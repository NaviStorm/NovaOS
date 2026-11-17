use super::engine::{Entries, Error, Word, cat, quoted, value};
use alloc::vec::Vec;

pub fn words(
    s: &[u8],
    inherited: Option<&Entries>,
    mut trace: impl FnMut(Word),
) -> Result<Vec<Word>, Error> {
    let mut words = Vec::new();
    let mut word: Option<Word> = None;
    let mut quote = 0;
    let mut i = 0;
    while i < s.len() {
        let start = i;
        let c = s[i];
        i += 1;
        if quote == 0 && (c.is_ascii_whitespace() || c == 11) {
            if let Some(w) = word.take() {
                words.push(w)
            }
            continue;
        }
        if matches!(c, b'\'' | b'"') && (quote == 0 || quote == c) {
            quote = if quote == 0 { c } else { 0 };
            word.get_or_insert_with(Vec::new);
            continue;
        }
        if quote == 0 && c == b'#' && word.is_none() {
            break;
        }
        if c == b'$' && quote != b'\'' {
            let tail = &s[i..];
            let name = tail
                .strip_prefix(b"{")
                .and_then(|t| t.split(|c| *c == b'}').next())
                .filter(|n| {
                    !n.is_empty()
                        && (n[0].is_ascii_alphabetic() || n[0] == b'_')
                        && n.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
                        && tail.get(n.len() + 1) == Some(&b'}')
                })
                .ok_or_else(|| {
                    Error::new(cat(&[
                        b"only \x24{VARNAME} expansion is supported, error at: ",
                        &s[start..],
                    ]))
                })?;
            i += name.len() + 2;
            let inherited = inherited
                .ok_or_else(|| Error::new(b"inherited environment unavailable".to_vec()))?;
            if let Some(bytes) = value(inherited, name) {
                trace(cat(&[
                    b"expanding \x24{",
                    name,
                    b"} into ",
                    &quoted(bytes),
                    b"\n",
                ]));
                word.get_or_insert_with(Vec::new).extend_from_slice(bytes);
            } else {
                trace(cat(&[b"replacing \x24{", name, b"} with empty string\n"]));
            }
            continue;
        }
        let byte = if c == b'\\' && (quote != b'\'' || matches!(s.get(i), Some(b'\'' | b'\\'))) {
            let next = *s
                .get(i)
                .ok_or_else(|| Error::new(b"invalid backslash at end of string in -S".to_vec()))?;
            i += 1;
            match next {
                b'c' if quote == b'"' => {
                    return Err(Error::new(
                        br"'\c' must not appear in double-quoted -S string".to_vec(),
                    ));
                }
                b'c' => break,
                b'_' if quote == 0 => {
                    if let Some(w) = word.take() {
                        words.push(w)
                    }
                    continue;
                }
                b'_' => b' ',
                b't' => 9,
                b'n' => 10,
                b'v' => 11,
                b'f' => 12,
                b'r' => 13,
                b'\\' | b'#' | b'$' | b'\'' | b'"' => next,
                _ => {
                    return Err(Error::new(cat(&[
                        b"invalid sequence '\\",
                        &[next],
                        b"' in -S",
                    ])));
                }
            }
        } else {
            c
        };
        word.get_or_insert_with(Vec::new).push(byte);
    }
    if quote != 0 {
        return Err(Error::new(b"no terminating quote in -S string".to_vec()));
    }
    if let Some(w) = word {
        words.push(w)
    }
    Ok(words)
}
