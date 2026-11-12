use super::lexer::{self, Token};
use alloc::vec::Vec;
use nova_cli::{runtime as rt, tty};

pub type List<'a> = Vec<Vec<Term<'a>>>;
pub struct Term<'a> {
    pub when: Option<bool>,
    pub negate: bool,
    pub command: Command<'a>,
}
pub enum Command<'a> {
    Simple(Vec<&'a str>),
    If(Vec<(List<'a>, List<'a>)>, List<'a>),
    While(List<'a>, List<'a>),
    For(&'a str, Vec<&'a str>, List<'a>),
}

#[derive(Debug)]
pub enum Error {
    Incomplete,
    Invalid,
    Limit,
}
impl Error {
    pub fn status(&self) -> u64 {
        if matches!(self, Self::Limit) {
            rt::LIMIT
        } else {
            rt::BAD_ARGUMENT
        }
    }
}
type Result<T> = core::result::Result<T, Error>;
struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    pos: usize,
}
impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.pos).copied()
    }
    fn take(&mut self, t: Token) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn need(&self) -> Error {
        if self.peek().is_none() {
            Error::Incomplete
        } else {
            Error::Invalid
        }
    }
    fn keyword(&mut self, s: &str) -> Result<()> {
        if self.take(Token::Name(s)) {
            Ok(())
        } else {
            Err(self.need())
        }
    }
    fn lines(&mut self) {
        while self.take(Token::Newline) {}
    }
    fn words(&mut self, max: usize) -> Result<Vec<&'a str>> {
        let mut w = Vec::new();
        while let Some(Token::Name(s)) = self.peek() {
            if w.len() == max {
                return Err(Error::Limit);
            }
            w.push(s);
            self.pos += 1
        }
        Ok(w)
    }
    fn list(&mut self, stops: &[&str], depth: usize) -> Result<List<'a>> {
        if depth > 8 {
            return Err(Error::Limit);
        }
        self.lines();
        let mut list = Vec::new();
        while self.peek().is_some()
            && !matches!(self.peek(),Some(Token::Name(s)) if stops.contains(&s))
        {
            let mut chain = Vec::new();
            let mut when = None;
            loop {
                let negate = self.take(Token::Name("!"));
                if negate {
                    self.lines()
                }
                chain.push(Term {
                    when,
                    negate,
                    command: self.command(depth)?,
                });
                when = if self.take(Token::And) {
                    Some(true)
                } else if self.take(Token::Or) {
                    Some(false)
                } else {
                    break;
                };
                self.lines();
            }
            list.push(chain);
            if self.peek().is_none() {
                break;
            }
            if !self.take(Token::Semi) && !self.take(Token::Newline) {
                return Err(Error::Invalid);
            }
            self.lines();
        }
        if !stops.is_empty() && list.is_empty() {
            return Err(self.need());
        }
        Ok(list)
    }
    fn command(&mut self, depth: usize) -> Result<Command<'a>> {
        if self.take(Token::Name("if")) {
            let mut arms = Vec::new();
            loop {
                let cond = self.list(&["then"], depth + 1)?;
                self.keyword("then")?;
                arms.push((cond, self.list(&["elif", "else", "fi"], depth + 1)?));
                if !self.take(Token::Name("elif")) {
                    break;
                }
            }
            let other = if self.take(Token::Name("else")) {
                self.list(&["fi"], depth + 1)?
            } else {
                Vec::new()
            };
            self.keyword("fi")?;
            return Ok(Command::If(arms, other));
        }
        if self.take(Token::Name("while")) {
            let cond = self.list(&["do"], depth + 1)?;
            self.keyword("do")?;
            let body = self.list(&["done"], depth + 1)?;
            self.keyword("done")?;
            return Ok(Command::While(cond, body));
        }
        if self.take(Token::Name("for")) {
            let Some(Token::Name(var)) = self.peek() else {
                return Err(self.need());
            };
            if !lexer::identifier(var) {
                return Err(Error::Invalid);
            }
            self.pos += 1;
            self.keyword("in")?;
            let words = self.words(tty::ARG_COUNT)?;
            if !self.take(Token::Semi) && !self.take(Token::Newline) {
                return Err(self.need());
            }
            self.lines();
            self.keyword("do")?;
            let body = self.list(&["done"], depth + 1)?;
            self.keyword("done")?;
            return Ok(Command::For(var, words, body));
        }
        if matches!(
            self.peek(),
            Some(Token::Name(
                "then"
                    | "else"
                    | "elif"
                    | "fi"
                    | "do"
                    | "done"
                    | "!"
                    | "function"
                    | "{"
                    | "}"
                    | "case"
                    | "esac"
                    | "until"
                    | "select"
            ))
        ) {
            return Err(Error::Invalid);
        }
        let w = self.words(tty::ARG_COUNT + 1)?;
        if w.is_empty() {
            return Err(self.need());
        }
        Ok(Command::Simple(w))
    }
}
pub fn parse(s: &str) -> Result<List<'_>> {
    let tokens = lexer::tokens(s).map_err(|e| {
        if e == rt::LIMIT {
            Error::Limit
        } else {
            Error::Invalid
        }
    })?;
    Parser { tokens, pos: 0 }.list(&[], 0)
}
