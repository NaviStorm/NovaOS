use alloc::{string::String, vec::Vec};
use nova_cli::{runtime as rt, tty};
pub fn lex(line: &str) -> rt::Result<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\\' && quote != Some('\'') {
            word.push(chars.next().ok_or(rt::BAD_ARGUMENT)?);
            started = true;
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
            started = true;
        } else if quote.is_none() && c == ' ' {
            if started {
                words.push(core::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(c);
            started = true;
        }
        if words.len() > tty::ARG_COUNT + 1 {
            return Err(rt::LIMIT);
        }
    }
    if quote.is_some() {
        return Err(rt::BAD_ARGUMENT);
    }
    if started {
        words.push(word)
    }
    if words.len() > tty::ARG_COUNT + 1 {
        return Err(rt::LIMIT);
    }
    Ok(words)
}
