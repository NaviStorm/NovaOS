use alloc::{string::String, vec::Vec};
use nova_cli::{runtime as rt, tty};

pub const SOURCE_MAX: usize = 8192;
const TOKEN_MAX: usize = 512;

#[derive(Clone, Default, Debug)]
pub struct Word {
    pub text: String,
    pub active: Vec<usize>,
}

pub fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b == b'_' || b.is_ascii_alphabetic())
        && bytes.all(|b| b == b'_' || b.is_ascii_alphanumeric())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Token<'a> {
    Name(&'a str),
    Newline,
    Semi,
    And,
    Or,
}

fn parameter(s: &str) -> rt::Result<Option<(&str, usize)>> {
    let b = s.as_bytes();
    if let Some(rest) = s.strip_prefix('{') {
        let end = rest.find('}').ok_or(rt::BAD_ARGUMENT)?;
        let name = &rest[..end];
        if !identifier(name) && !matches!(name, "?" | "$") {
            return Err(rt::BAD_ARGUMENT);
        }
        return Ok(Some((name, end + 2)));
    }
    if matches!(b.first(), Some(b'?' | b'$')) {
        return Ok(Some((&s[..1], 1)));
    }
    if b.first()
        .is_some_and(|v| v.is_ascii_alphabetic() || *v == b'_')
    {
        let end = b
            .iter()
            .position(|v| !v.is_ascii_alphanumeric() && *v != b'_')
            .unwrap_or(b.len());
        return Ok(Some((&s[..end], end)));
    }
    if matches!(b.first(), Some(b'(' | b'0'..=b'9')) {
        return Err(rt::BAD_ARGUMENT);
    }
    Ok(None)
}

// Une seule lecture des protections pour le découpage et l'expansion.
// Les octets ajoutés par une variable ne repassent jamais par le lexer.
fn scan(s: &str, mut literal: impl FnMut(&str, bool, bool) -> rt::Result<()>) -> rt::Result<usize> {
    let mut pos = 0;
    let mut quote = None;
    while pos < s.len() {
        let c = s[pos..].chars().next().unwrap();
        if quote.is_none() && matches!(c, ' ' | '\t' | '\n' | '\r' | ';' | '&' | '|') {
            break;
        }
        if c == '\\' && quote != Some('\'') {
            pos += 1;
            let next = s[pos..].chars().next().ok_or(rt::BAD_ARGUMENT)?;
            let end = pos + next.len_utf8();
            if next != '\n' {
                literal(&s[pos..end], false, false)?
            }
            pos = end;
        } else if quote == Some(c) {
            quote = None;
            pos += 1
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
            pos += 1
        } else if c == '$'
            && quote != Some('\'')
            && let Some((name, n)) = parameter(&s[pos + 1..])?
        {
            literal(name, quote.is_none(), true)?;
            pos += n + 1;
        } else {
            if (quote.is_none() && matches!(c, '<' | '>' | '(' | ')'))
                || (quote != Some('\'') && c == '\u{60}')
            {
                return Err(rt::BAD_ARGUMENT);
            }
            let end = pos + c.len_utf8();
            literal(&s[pos..end], quote.is_none(), false)?;
            pos = end
        }
    }
    if quote.is_some() {
        return Err(rt::BAD_ARGUMENT);
    }
    Ok(pos)
}

pub fn tokens(mut s: &str) -> rt::Result<Vec<Token<'_>>> {
    if s.len() > SOURCE_MAX {
        return Err(rt::LIMIT);
    }
    if s.chars()
        .any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n'))
    {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut out = Vec::new();
    while !s.is_empty() {
        let (tok, n) = match s.as_bytes()[0] {
            b' ' | b'\t' => {
                s = &s[1..];
                continue;
            }
            b'#' => {
                s = &s[s.find('\n').unwrap_or(s.len())..];
                continue;
            }
            b'\r' if s.starts_with("\r\n") => (Token::Newline, 2),
            b'\r' => return Err(rt::BAD_ARGUMENT),
            b'\n' => (Token::Newline, 1),
            b';' => (Token::Semi, 1),
            b'&' if s.starts_with("&&") => (Token::And, 2),
            b'|' if s.starts_with("||") => (Token::Or, 2),
            b'&' | b'|' => return Err(rt::BAD_ARGUMENT),
            _ => {
                let n = scan(s, |_, _, _| Ok(()))?;
                (Token::Name(&s[..n]), n)
            }
        };
        if out.len() == TOKEN_MAX {
            return Err(rt::LIMIT);
        }
        out.push(tok);
        s = &s[n..]
    }
    Ok(out)
}

pub fn expand<'a>(s: &str, lookup: impl Fn(&str) -> &'a str) -> rt::Result<Word> {
    let mut w = Word::default();
    scan(s, |part, active, variable| {
        let value = if variable { lookup(part) } else { part };
        if w.text.len() + value.len() > tty::ARG_MAX {
            return Err(rt::LIMIT);
        }
        if active {
            w.active.extend(
                value
                    .char_indices()
                    .filter(|(_, c)| "*?[]!-".contains(*c))
                    .map(|(i, _)| w.text.len() + i),
            );
        }
        w.text.push_str(value);
        Ok(())
    })?;
    Ok(w)
}
