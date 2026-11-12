use alloc::{string::String, vec::Vec};
use nova_cli::{runtime as rt, tty};
#[derive(Clone, Default, Debug)]
pub struct Word {
    pub text: String,
    pub active: Vec<usize>,
}

pub fn lex(line: &str) -> rt::Result<Vec<Word>> {
    let mut words = Vec::new();
    let mut word = Word::default();
    let mut quote = None;
    let mut started = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\\' && quote != Some('\'') {
            word.text.push(chars.next().ok_or(rt::BAD_ARGUMENT)?);
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
            if quote.is_none() && "*?[]!-".contains(c) {
                word.active.push(word.text.len());
            }
            word.text.push(c);
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
