use alloc::{string::String, vec::Vec};
pub struct Invocation {
    pub name: String,
    pub args: Vec<super::lexer::Word>,
}
pub fn parse(words: Vec<super::lexer::Word>) -> Option<Invocation> {
    let mut it = words.into_iter();
    Some(Invocation {
        name: it.next()?.text,
        args: it.collect(),
    })
}
