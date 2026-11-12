//! Le curseur est un offset UTF-8. Un CRLF compte comme un seul saut lors de l'édition.
use alloc::string::String;
use core::ops::Range;

pub use nova_cli::vi_wire::BYTES;
pub const LINES: usize = 512;

pub struct Buffer {
    pub text: String,
    pub at: usize,
    saved: String,
    previous: String,
    previous_at: Option<usize>,
    checkpoint: bool,
    eol: &'static str,
}

fn reserved() -> Result<String, &'static str> {
    let mut s = String::new();
    s.try_reserve_exact(BYTES)
        .map_err(|_| "mémoire insuffisante")?;
    Ok(s)
}

impl Buffer {
    pub fn new(input: &str) -> Result<Self, &'static str> {
        if input.len() > BYTES || input.bytes().filter(|&b| b == b'\n').count() >= LINES {
            return Err("limite du tampon");
        }
        if input
            .match_indices('\r')
            .any(|(i, _)| !input[i..].starts_with("\r\n"))
            || input
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
        {
            return Err("contrôle interdit dans le texte");
        }
        let mut text = reserved()?;
        let mut saved = reserved()?;
        text.push_str(input);
        saved.push_str(input);
        let eol = match input.find('\n') {
            Some(n) if input[..n].ends_with('\r') => "\r\n",
            _ => "\n",
        };
        Ok(Self {
            text,
            saved,
            at: 0,
            previous: reserved()?,
            previous_at: None,
            checkpoint: true,
            eol,
        })
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }
    pub fn saved(&mut self) {
        self.saved.clear();
        self.saved.push_str(&self.text);
    }
    pub fn change(&mut self) {
        self.checkpoint = true;
    }
    pub fn undo(&mut self) -> bool {
        let Some(at) = self.previous_at.take() else {
            return false;
        };
        core::mem::swap(&mut self.previous, &mut self.text);
        self.at = at;
        self.normal();
        true
    }

    pub fn start(&self) -> usize {
        self.text[..self.at].rfind('\n').map_or(0, |i| i + 1)
    }
    pub fn end_from(&self, start: usize) -> usize {
        let line = self.text[start..].split('\n').next().unwrap_or("");
        start + line.strip_suffix('\r').unwrap_or(line).len()
    }
    pub fn end(&self) -> usize {
        self.end_from(self.start())
    }
    pub fn row(&self) -> usize {
        self.text[..self.at].bytes().filter(|&b| b == b'\n').count()
    }
    pub fn col(&self) -> usize {
        self.text[self.start()..self.at].chars().count()
    }
    fn prev(&self, at: usize) -> usize {
        self.text[..at]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.at
            + self.text[self.at..]
                .chars()
                .next()
                .map_or(0, char::len_utf8)
    }
    pub fn normal(&mut self) {
        if self.at == self.end() && self.at > self.start() {
            self.at = self.prev(self.at);
        }
    }
    pub fn left(&mut self) {
        if self.at > self.start() {
            self.at = self.prev(self.at);
        }
    }
    pub fn right(&mut self, insertion: bool) {
        if self.at < self.end() {
            self.at = self.next();
        }
        if !insertion {
            self.normal();
        }
    }
    pub fn vertical(&mut self, down: bool, insertion: bool) {
        let start = self.start();
        let other = if down {
            let Some(n) = self.text[start..].find('\n') else {
                return;
            };
            start + n + 1
        } else {
            if start == 0 {
                return;
            }
            self.text[..start - 1].rfind('\n').map_or(0, |i| i + 1)
        };
        let end = self.end_from(other);
        self.at = self.text[other..end]
            .char_indices()
            .nth(self.col())
            .map_or(end, |(i, _)| other + i);
        if !insertion {
            self.normal();
        }
    }
    pub fn last(&mut self) {
        self.at = self.text.rfind('\n').map_or(0, |i| i + 1);
    }
    fn replace(&mut self, range: Range<usize>, new: &str, at: usize) -> Result<(), &'static str> {
        if self.text.len() - range.len() + new.len() > BYTES
            || (new.contains('\n') && self.text.bytes().filter(|&b| b == b'\n').count() + 2 > LINES)
        {
            return Err("limite du tampon");
        }
        if self.checkpoint {
            self.previous.clear();
            self.previous.push_str(&self.text);
            self.previous_at = Some(self.at);
            self.checkpoint = false;
        }
        self.text.replace_range(range, new);
        self.at = at;
        Ok(())
    }
    pub fn insert(&mut self, c: char) -> Result<(), &'static str> {
        let mut buf = [0; 4];
        let s = if c == '\n' {
            self.eol
        } else {
            c.encode_utf8(&mut buf)
        };
        self.replace(self.at..self.at, s, self.at + s.len())
    }
    pub fn open_line(&mut self, below: bool) -> Result<(), &'static str> {
        let at = if below { self.end() } else { self.start() };
        self.replace(
            at..at,
            self.eol,
            if below { at + self.eol.len() } else { at },
        )
    }
    pub fn erase(&mut self) -> Result<(), &'static str> {
        if self.at == 0 {
            return Ok(());
        }
        let at = if self.text[..self.at].ends_with("\r\n") {
            self.at - 2
        } else {
            self.prev(self.at)
        };
        self.replace(at..self.at, "", at)
    }
    pub fn delete(&mut self) -> Result<(), &'static str> {
        if self.at < self.end() {
            self.replace(self.at..self.next(), "", self.at)?;
            self.normal();
        }
        Ok(())
    }
    pub fn delete_line(&mut self) -> Result<(), &'static str> {
        if self.text.is_empty() {
            return Ok(());
        }
        let start = self.start();
        if let Some(i) = self.text[start..].find('\n') {
            self.replace(start..start + i + 1, "", start)
        } else {
            let cut = if self.text[..start].ends_with("\r\n") {
                start - 2
            } else {
                start.saturating_sub(1)
            };
            self.replace(cut..self.text.len(), "", cut)?;
            self.at = self.start();
            Ok(())
        }
    }
    pub fn search(&mut self, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let after = self.next();
        let found = self.text[after..]
            .find(needle)
            .map(|i| i + after)
            .or_else(|| self.text.find(needle));
        if let Some(i) = found {
            self.at = i;
            true
        } else {
            false
        }
    }
}
