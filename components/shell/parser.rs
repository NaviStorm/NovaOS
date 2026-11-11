use alloc::{string::String, vec::Vec};
pub struct Invocation {
    pub name: String,
    pub args: Vec<String>,
}
pub fn parse(words: Vec<String>) -> Option<Invocation> {
    let mut it = words.into_iter();
    Some(Invocation {
        name: it.next()?,
        args: it.collect(),
    })
}
