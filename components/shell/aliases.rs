use super::lexer::{self, Token, Word};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use nova_cli::{console, runtime as rt, tty};

#[derive(Default)]
pub struct Aliases(BTreeMap<String, Vec<String>>);
impl Aliases {
    pub fn define(&mut self, args: &[Word], script: bool) -> rt::Result<()> {
        if args.is_empty() {
            for (name, words) in &self.0 {
                console::printable(tty::OUTPUT, &alloc::format!("{name} = {words:?}\n"))?
            }
            return Ok(());
        }
        let [arg] = args else {
            return Err(rt::BAD_ARGUMENT);
        };
        let (name, value) = arg.text.split_once('=').ok_or(rt::BAD_ARGUMENT)?;
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || matches!(name, "alias" | "unalias")
        {
            return Err(rt::BAD_ARGUMENT);
        }
        if script && !self.0.contains_key(name) && self.0.len() >= 64 {
            return Err(rt::LIMIT);
        }
        let words = lexer::tokens(value)?
            .into_iter()
            .map(|t| match t {
                Token::Name(s) => Ok(String::from(s)),
                _ => Err(rt::BAD_ARGUMENT),
            })
            .collect::<rt::Result<Vec<_>>>()?;
        if words.is_empty() || words.len() > tty::ARG_COUNT + 1 {
            return Err(rt::BAD_ARGUMENT);
        }
        self.0.insert(name.into(), words);
        Ok(())
    }
    pub fn remove(&mut self, args: &[Word]) -> rt::Result<()> {
        let [arg] = args else {
            return Err(rt::BAD_ARGUMENT);
        };
        self.0
            .remove(&arg.text)
            .ok_or(nova_cli::protocol::NOT_FOUND)?;
        Ok(())
    }
    pub fn expand(
        &self,
        mut name: String,
        args: &mut Vec<Word>,
        expand: impl Fn(&str) -> rt::Result<Word>,
    ) -> rt::Result<String> {
        let mut seen = BTreeSet::new();
        while let Some(words) = self.0.get(&name) {
            if !seen.insert(name) {
                return Err(rt::BAD_ARGUMENT);
            }
            if words.len() - 1 + args.len() > tty::ARG_COUNT {
                return Err(rt::LIMIT);
            }
            name = expand(&words[0])?.text;
            let mut bytes = 1 + args.iter().map(|w| w.text.len() + 2).sum::<usize>();
            let mut prefix = Vec::new();
            for raw in &words[1..] {
                let word = expand(raw)?;
                bytes += word.text.len() + 2;
                if bytes > tty::ARG_MAX {
                    return Err(rt::LIMIT);
                }
                prefix.push(word);
            }
            args.splice(0..0, prefix);
        }
        Ok(name)
    }
}
