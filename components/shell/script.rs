use super::{
    aliases::Aliases,
    builtins, executor, glob,
    lexer::{self, Word},
    parser::{self, Command, List},
    paths,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use nova_cli::{command::Program, console, fileapi::Client, protocol as w, runtime as rt, tty};

const STEPS: usize = 1024;
const FRAMES: usize = 4;
const VAR_BYTES: usize = 4096;

#[derive(Default)]
pub struct Frame {
    vars: BTreeMap<String, (String, bool)>,
    aliases: Aliases,
    pub status: u64,
}
impl Frame {
    fn child(&self) -> Self {
        Self {
            vars: self
                .vars
                .iter()
                .filter(|(_, (_, export))| *export)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ..Self::default()
        }
    }
    fn set(&mut self, name: &str, value: &str, export: bool) -> rt::Result<()> {
        if !lexer::identifier(name) {
            return Err(rt::BAD_ARGUMENT);
        }
        let prev = self.vars.get(name);
        let used: usize = self.vars.iter().map(|(k, (v, _))| k.len() + v.len()).sum();
        if (prev.is_none() && self.vars.len() >= 32)
            || used - prev.map_or(0, |(v, _)| name.len() + v.len()) + name.len() + value.len()
                > VAR_BYTES
        {
            return Err(rt::LIMIT);
        }
        let inherit = export || prev.is_some_and(|(_, e)| *e);
        self.vars.insert(name.into(), (value.into(), inherit));
        Ok(())
    }
    fn word(&self, raw: &str, pid: &str) -> rt::Result<Word> {
        let status = self.status.to_string();
        lexer::expand(raw, |s| match s {
            "?" => &status,
            "$" => pid,
            name => self.vars.get(name).map_or("", |(v, _)| v.as_str()),
        })
    }
}
#[derive(Clone, Copy)]
pub enum Flow {
    Status(u64),
    Exit(u64),
    Return(u64),
    Break,
    Continue,
}
pub struct Engine<'a> {
    fs: &'a mut Client,
    test: bool,
    pid: String,
    left: usize,
}
impl<'a> Engine<'a> {
    pub fn new(fs: &'a mut Client, test: bool) -> rt::Result<Self> {
        Ok(Self {
            fs,
            test,
            pid: rt::value(rt::call(rt::PROCESS_ID, 0, 0, 0))?.to_string(),
            left: STEPS,
        })
    }
    fn step(&mut self) -> rt::Result<()> {
        self.left = self.left.checked_sub(1).ok_or(rt::LIMIT)?;
        Ok(())
    }
    pub fn list(
        &mut self,
        frame: &mut Frame,
        list: &List,
        depth: usize,
        loops: usize,
    ) -> rt::Result<Flow> {
        for chain in list {
            for term in chain {
                if term.when.is_some_and(|ok| ok != (frame.status == 0)) {
                    continue;
                }
                self.step()?;
                let result = match self.command(frame, &term.command, depth, loops) {
                    Err(e)
                        if !matches!(e, rt::LIMIT | rt::REVOKED | rt::CLOSED | rt::NO_MEMORY) =>
                    {
                        console::write(
                            tty::ERROR,
                            &alloc::format!("erreur : {}\n", console::reason(e)),
                        )?;
                        Flow::Status(e)
                    }
                    other => other?,
                };
                let Flow::Status(s) = result else {
                    return Ok(result);
                };
                frame.status = if term.negate { u64::from(s == 0) } else { s };
            }
        }
        Ok(Flow::Status(frame.status))
    }
    fn command(
        &mut self,
        f: &mut Frame,
        cmd: &Command,
        depth: usize,
        loops: usize,
    ) -> rt::Result<Flow> {
        match cmd {
            Command::Simple(words) => self.simple(f, words, depth, loops),
            Command::If(arms, other) => {
                for (cond, body) in arms {
                    match self.list(f, cond, depth, loops)? {
                        Flow::Status(0) => return self.list(f, body, depth, loops),
                        Flow::Status(_) => (),
                        flow => return Ok(flow),
                    }
                }
                if other.is_empty() {
                    Ok(Flow::Status(0))
                } else {
                    self.list(f, other, depth, loops)
                }
            }
            Command::While(cond, body) => {
                let mut last = 0;
                loop {
                    match self.list(f, cond, depth, loops + 1)? {
                        Flow::Status(0) => (),
                        Flow::Status(_) => break,
                        Flow::Break => return Ok(Flow::Status(0)),
                        Flow::Continue => {
                            last = 0;
                            continue;
                        }
                        flow => return Ok(flow),
                    }
                    match self.list(f, body, depth, loops + 1)? {
                        Flow::Status(n) => last = n,
                        Flow::Break => return Ok(Flow::Status(0)),
                        Flow::Continue => {
                            last = 0;
                            continue;
                        }
                        flow => return Ok(flow),
                    }
                }
                Ok(Flow::Status(last))
            }
            Command::For(name, raw, body) => {
                let words = self.words(f, raw)?;
                let mut last = 0;
                for val in glob::arguments(self.fs, &words)? {
                    f.set(name, &val, false)?;
                    match self.list(f, body, depth, loops + 1)? {
                        Flow::Status(n) => last = n,
                        Flow::Break => return Ok(Flow::Status(0)),
                        Flow::Continue => {
                            last = 0;
                            continue;
                        }
                        flow => return Ok(flow),
                    }
                }
                Ok(Flow::Status(last))
            }
        }
    }
    fn words(&self, f: &Frame, raw: &[&str]) -> rt::Result<Vec<Word>> {
        let mut bytes = 1;
        raw.iter()
            .map(|s| {
                let w = f.word(s, &self.pid)?;
                bytes += w.text.len() + 2;
                if bytes > tty::ARG_MAX {
                    return Err(rt::LIMIT);
                }
                Ok(w)
            })
            .collect()
    }
    fn simple(
        &mut self,
        f: &mut Frame,
        raw: &[&str],
        depth: usize,
        loops: usize,
    ) -> rt::Result<Flow> {
        if let Some((name, value)) = raw[0].split_once('=')
            && lexer::identifier(name)
        {
            if raw.len() != 1 {
                return Err(rt::BAD_ARGUMENT);
            }
            f.set(name, &f.word(value, &self.pid)?.text, false)?;
            return Ok(Flow::Status(0));
        }
        let name = f.word(raw[0], &self.pid)?.text;
        let mut words = self.words(f, &raw[1..])?;
        let name = f
            .aliases
            .expand(name, &mut words, |s| f.word(s, &self.pid))?;
        match name.as_str() {
            "alias" => {
                f.aliases.define(&words, depth != 0)?;
                return Ok(Flow::Status(0));
            }
            "unalias" => {
                f.aliases.remove(&words)?;
                return Ok(Flow::Status(0));
            }
            "export" | "unset" => {
                if words.is_empty() {
                    return Err(rt::BAD_ARGUMENT);
                }
                for word in &words {
                    if name == "unset" {
                        if !lexer::identifier(&word.text) {
                            return Err(rt::BAD_ARGUMENT);
                        }
                        f.vars.remove(&word.text);
                    } else if let Some((key, value)) = word.text.split_once('=') {
                        f.set(key, value, true)?
                    } else if let Some((_, export)) = f.vars.get_mut(&word.text) {
                        *export = true
                    } else {
                        f.set(&word.text, "", true)?
                    }
                }
                return Ok(Flow::Status(0));
            }
            _ => (),
        }
        let args = glob::arguments(self.fs, &words)?;
        let argv: Vec<_> = args.iter().map(String::as_str).collect();
        let code = match name.as_str() {
            "exit" | "return" => {
                let n = match argv.as_slice() {
                    [] if depth == 0 => 0,
                    [] => f.status,
                    [s] => s.parse::<u8>().map_err(|_| rt::BAD_ARGUMENT)? as u64,
                    _ => return Err(rt::BAD_ARGUMENT),
                };
                return match name.as_str() {
                    "exit" => Ok(Flow::Exit(n)),
                    _ if depth > 0 => Ok(Flow::Return(n)),
                    _ => Err(rt::BAD_ARGUMENT),
                };
            }
            "break" | "continue" => {
                if loops == 0 || !args.is_empty() {
                    return Err(rt::BAD_ARGUMENT);
                }
                f.status = 0;
                return Ok(if name == "break" {
                    Flow::Break
                } else {
                    Flow::Continue
                });
            }
            "echo" => {
                console::printable(tty::OUTPUT, &args.join(" "))?;
                console::write(tty::OUTPUT, "\n")?;
                0
            }
            "test" | "[" => {
                let operands = if name == "[" {
                    argv.strip_suffix(&["]"]).ok_or(rt::BAD_ARGUMENT)?
                } else {
                    &argv
                };
                u64::from(!self.test_expr(operands)?)
            }
            "true" | "false" if args.is_empty() => u64::from(name == "false"),
            "cd" => {
                builtins::cd(self.fs, &args)?;
                0
            }
            "nsh" => match argv.as_slice() {
                ["-c", src] => self.script(f, src, depth)?,
                [path] if !path.starts_with('-') => self.file(f, path, depth, false)?,
                _ => return Err(rt::BAD_ARGUMENT),
            },
            _ if let Some(p) = Program::named(&name, self.test) => {
                executor::execute(self.fs, p, &args, self.test)?
            }
            _ if name.contains('/') && args.is_empty() => match self.file(f, &name, depth, true) {
                Err(w::NOT_FOUND) => {
                    console::write(tty::ERROR, "commande inconnue\n")?;
                    127
                }
                v => v?,
            },
            _ => {
                console::write(tty::ERROR, "commande inconnue\n")?;
                127
            }
        };
        Ok(Flow::Status(code))
    }
    fn test_expr(&mut self, words: &[&str]) -> rt::Result<bool> {
        let num = |s: &str| s.parse::<i64>().map_err(|_| rt::BAD_ARGUMENT);
        Ok(match words {
            [] => false,
            [s] => !s.is_empty(),
            ["-n", s] => !s.is_empty(),
            ["-z", s] => s.is_empty(),
            [kind @ ("-e" | "-f" | "-d"), path] => {
                let st = paths::resolve(self.fs, path)
                    .and_then(|p| self.fs.at(w::LOOKUP_AT, 0, p.as_bytes(), 0));
                match st {
                    Err(w::NOT_FOUND | w::NOT_DIR) => false,
                    Err(e) => return Err(e),
                    Ok(st) => {
                        *kind == "-e" || st[w::PAYLOAD + 24] == if *kind == "-d" { 2 } else { 1 }
                    }
                }
            }
            [a, op, b] => match *op {
                "=" | "==" => a == b,
                "!=" => a != b,
                "-lt" => num(a)? < num(b)?,
                "-le" => num(a)? <= num(b)?,
                "-eq" => num(a)? == num(b)?,
                "-ne" => num(a)? != num(b)?,
                "-ge" => num(a)? >= num(b)?,
                "-gt" => num(a)? > num(b)?,
                _ => return Err(rt::BAD_ARGUMENT),
            },
            _ => return Err(rt::BAD_ARGUMENT),
        })
    }
    fn file(&mut self, f: &Frame, path: &str, depth: usize, direct: bool) -> rt::Result<u64> {
        if depth >= FRAMES {
            return Err(rt::LIMIT);
        }
        let path = paths::resolve(self.fs, path)?;
        let h = self.fs.open_at(0, path.as_bytes(), w::READABLE)?;
        let read = (|| {
            let before = self.fs.simple(w::STAT, h)?;
            let len = w::word(&before, w::PAYLOAD + 16);
            if len > lexer::SOURCE_MAX as u64 {
                return Err(rt::LIMIT);
            }
            let mut bytes = vec![0; len as usize];
            let n = self.fs.read(h, 0, &mut bytes)?;
            let after = self.fs.simple(w::STAT, h)?;
            if n != bytes.len()
                || w::word(&before, w::PAYLOAD) != w::word(&after, w::PAYLOAD)
                || w::word(&after, w::PAYLOAD + 16) != len
            {
                return Err(rt::REVOKED);
            }
            // READ peut publier atime sur P9. Le service ne sert aucun autre
            // écrivain pendant cette lecture synchrone sur le handle épinglé.
            // En cas de révision nouvelle, comparer aussi le contenu jusqu'à EOF.
            if w::word(&before, w::PAYLOAD + 8) != w::word(&after, w::PAYLOAD + 8) {
                let mut check = [0; w::FRAGMENT];
                for (i, chunk) in bytes.chunks(check.len()).enumerate() {
                    let count = self.fs.read(h, (i * w::FRAGMENT) as u64, &mut check)?;
                    if count != chunk.len() || &check[..count] != chunk {
                        return Err(rt::REVOKED);
                    }
                }
                if self.fs.read(h, len, &mut check[..1])? != 0 {
                    return Err(rt::REVOKED);
                }
            }
            String::from_utf8(bytes).map_err(|_| rt::BAD_ARGUMENT)
        })();
        self.fs.close(h)?;
        let source = read?;
        if direct || source.starts_with("#!") {
            if source.lines().next() != Some("#!/bin/nsh") {
                return Err(rt::BAD_ARGUMENT);
            }
        }
        self.script(f, &source, depth)
    }
    fn script(&mut self, outer: &Frame, src: &str, depth: usize) -> rt::Result<u64> {
        if depth >= FRAMES {
            return Err(rt::LIMIT);
        }
        let program = parser::parse(src).map_err(|e| e.status())?;
        let cwd = self.fs.directory(0, b".", w::DIR_ALL)?;
        let result = self.list(&mut outer.child(), &program, depth + 1, 0);
        let restore = self.fs.simple(w::CHDIR, cwd);
        self.fs.simple(w::DIR_CLOSE, cwd)?;
        restore?;
        match result? {
            Flow::Status(n) | Flow::Return(n) | Flow::Exit(n) => Ok(n),
            _ => Err(rt::BAD_ARGUMENT),
        }
    }
}
