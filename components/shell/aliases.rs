use super::{lexer, parser::Invocation};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use nova_cli::{console, runtime as rt, tty};

#[derive(Default)]
pub struct Aliases(BTreeMap<String, Vec<lexer::Word>>);

impl Aliases {
    fn valid(name: &str) -> bool {
        !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    }

    pub fn define(&mut self, args: &[lexer::Word]) -> rt::Result<()> {
        if args.is_empty() {
            for (name, words) in &self.0 {
                console::printable(
                    tty::OUTPUT,
                    &alloc::format!(
                        "{name} = {:?}\n",
                        words.iter().map(|w| &w.text).collect::<Vec<_>>()
                    ),
                )?;
            }
            return Ok(());
        }
        let [assignment] = args else {
            return Err(rt::BAD_ARGUMENT);
        };
        let (name, value) = assignment.text.split_once('=').ok_or(rt::BAD_ARGUMENT)?;
        if !Self::valid(name) || matches!(name, "alias" | "unalias") {
            return Err(rt::BAD_ARGUMENT);
        }
        let words = lexer::lex(value)?;
        if words.first().is_none_or(|w| w.text.is_empty()) {
            return Err(rt::BAD_ARGUMENT);
        }
        self.0.insert(name.into(), words);
        Ok(())
    }

    pub fn remove(&mut self, args: &[lexer::Word]) -> rt::Result<()> {
        let [name] = args else {
            return Err(rt::BAD_ARGUMENT);
        };
        self.0
            .remove(&name.text)
            .ok_or(nova_cli::protocol::NOT_FOUND)?;
        Ok(())
    }

    pub fn expand(&self, mut cmd: Invocation) -> rt::Result<Invocation> {
        let mut seen = alloc::collections::BTreeSet::new();
        while let Some(words) = self.0.get(&cmd.name) {
            if !seen.insert(cmd.name) {
                return Err(rt::BAD_ARGUMENT);
            }
            if words.len() - 1 + cmd.args.len() > tty::ARG_COUNT {
                return Err(rt::LIMIT);
            }
            cmd.name = words[0].text.clone();
            cmd.args.splice(0..0, words[1..].iter().cloned());
        }
        Ok(cmd)
    }
}
