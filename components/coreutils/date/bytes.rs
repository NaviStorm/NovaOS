//! Transport réversible des octets Darwin au travers du parseur d'options UTF-8.
use std::borrow::Cow;
const MARK: char = '\u{f0000}';
pub fn encode(input: &[u8]) -> Cow<'_, str> {
    if let Ok(s) = str::from_utf8(input)
        && !s.contains(MARK)
    {
        return Cow::Borrowed(s);
    }
    let mut s = String::new();
    for c in input.utf8_chunks() {
        for ch in c.valid().chars() {
            s.push(ch);
            if ch == MARK {
                s.push(ch)
            }
        }
        for &b in c.invalid() {
            s.push(MARK);
            s.push(char::from(b))
        }
    }
    Cow::Owned(s)
}
pub fn decode(s: &str) -> Cow<'_, [u8]> {
    if !s.contains(MARK) {
        return Cow::Borrowed(s.as_bytes());
    }
    let mut out = Vec::new();
    let mut chars = s.chars();
    while let Some(mut c) = chars.next() {
        if c == MARK
            && let Some(next) = chars.next()
        {
            if next != MARK {
                out.push(next as u8);
                continue;
            }
            c = next;
        }
        out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes())
    }
    Cow::Owned(out)
}
