//! Les positions sont des offsets UTF-8. Les opérations de ligne préservent les CRLF.
use super::{gap::Gap, history::History};
use alloc::string::String;
use core::ops::Range;

pub struct Buffer {
    text: Gap,
    pub at: usize,
    saved: Option<String>,
    history: History,
    eol: &'static str,
}
impl Buffer {
    pub fn validate(input: &str) -> Result<(), &'static str> {
        if input
            .match_indices('\r')
            .any(|(i, _)| !input[i..].starts_with("\r\n"))
            || input
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
        {
            return Err("contrôle interdit dans le texte");
        }
        Ok(())
    }
    pub fn source_eol(&mut self, crlf: bool) {
        self.eol = if crlf { "\r\n" } else { "\n" };
    }
    pub fn extend_source(&mut self, tail: &str) -> Result<(), &'static str> {
        // Réserver les deux représentations avant de toucher aux octets. Le journal
        // décrit uniquement des éditions du préfixe déjà présent.
        if let Some(saved) = &mut self.saved {
            saved
                .try_reserve(tail.len())
                .map_err(|_| "mémoire insuffisante")?;
        }
        self.text.reserve(tail.len())?;
        self.text.replace(self.len(), 0, tail);
        if let Some(saved) = &mut self.saved {
            saved.push_str(tail);
        }
        Ok(())
    }
    pub fn new(input: String) -> Result<Self, &'static str> {
        Self::validate(&input)?;
        let eol = match input.find('\n') {
            Some(n) if input[..n].ends_with('\r') => "\r\n",
            _ => "\n",
        };
        Ok(Self {
            text: Gap::new(input),
            at: 0,
            saved: None,
            history: History::default(),
            eol,
        })
    }
    pub fn len(&self) -> usize {
        self.text.len()
    }
    pub fn parts(&self, r: Range<usize>) -> [&str; 2] {
        self.text.parts(r)
    }
    pub fn compact(&mut self) -> &str {
        self.text.compact()
    }
    pub fn dirty(&self) -> bool {
        let Some(saved) = &self.saved else {
            return false;
        };
        if self.len() != saved.len() {
            return true;
        }
        let [a, b] = self.parts(0..self.len());
        !saved.starts_with(a) || !saved.ends_with(b)
    }
    pub fn saved(&mut self) {
        self.saved = None;
    }
    pub fn change(&mut self) {
        self.history.boundary();
    }
    pub fn finish(&mut self) {
        self.history.finish(self.at);
    }
    pub fn truncated(&self) -> bool {
        self.history.truncated()
    }
    fn remember(&self) -> Result<String, &'static str> {
        let mut s = String::new();
        s.try_reserve_exact(self.len())
            .map_err(|_| "mémoire insuffisante")?;
        for part in self.parts(0..self.len()) {
            s.push_str(part);
        }
        Ok(s)
    }
    pub fn undo_redo(&mut self, redo: bool) -> Result<(), &'static str> {
        if !self.history.available(redo) {
            return Err(if redo {
                "aucun rétablissement"
            } else {
                "aucune annulation"
            });
        }
        if self.saved.is_none() {
            self.saved = Some(self.remember()?);
        }
        self.text.reserve(self.history.growth(redo))?;
        self.at = self.history.replay(&mut self.text, redo).unwrap();
        self.normal();
        Ok(())
    }
    pub fn start_at(&self, at: usize) -> usize {
        let [a, b] = self.parts(0..at);
        b.rfind('\n')
            .map(|i| a.len() + i + 1)
            .or_else(|| a.rfind('\n').map(|i| i + 1))
            .unwrap_or(0)
    }
    pub fn start(&self) -> usize {
        self.start_at(self.at)
    }
    pub fn newline(&self, at: usize) -> Option<usize> {
        let [a, b] = self.parts(at..self.len());
        a.find('\n')
            .or_else(|| b.find('\n').map(|i| a.len() + i))
            .map(|n| n + at)
    }
    fn before(&self, at: usize) -> Option<char> {
        let [a, b] = self.parts(0..at);
        b.chars().next_back().or_else(|| a.chars().next_back())
    }
    fn current(&self) -> Option<char> {
        let [a, b] = self.parts(self.at..self.len());
        a.chars().next().or_else(|| b.chars().next())
    }
    pub fn end_from(&self, start: usize) -> usize {
        let n = self.newline(start).unwrap_or(self.len());
        n - usize::from(self.before(n) == Some('\r'))
    }
    pub fn end(&self) -> usize {
        self.end_from(self.at)
    }
    pub fn row(&self) -> usize {
        self.text.row(self.at)
    }
    pub fn col(&self) -> usize {
        self.parts(self.start()..self.at)
            .iter()
            .map(|s| s.chars().count())
            .sum()
    }
    fn next(&self) -> usize {
        self.at + self.current().map_or(0, char::len_utf8)
    }
    pub fn normal(&mut self) {
        if matches!(self.current(), None | Some('\r' | '\n')) {
            self.left();
        }
    }
    pub fn left(&mut self) {
        if let Some(c) = self.before(self.at)
            && c != '\n'
        {
            self.at -= c.len_utf8();
        }
    }
    pub fn right(&mut self, insertion: bool) {
        if !matches!(self.current(), None | Some('\r' | '\n')) {
            self.at = self.next();
        }
        if !insertion {
            self.normal();
        }
    }
    pub fn vertical(&mut self, down: bool, insertion: bool) {
        let other = if down {
            let Some(n) = self.newline(self.at) else {
                return;
            };
            n + 1
        } else {
            let start = self.start();
            if start == 0 {
                return;
            }
            self.start_at(start - 1)
        };
        let mut n = self.col();
        let end = self.end_from(other);
        let mut pos = other;
        for part in self.parts(other..end) {
            if let Some((i, _)) = part.char_indices().nth(n) {
                pos += i;
                break;
            }
            pos += part.len();
            n -= part.chars().count();
        }
        self.at = pos;
        if !insertion {
            self.normal();
        }
    }
    pub fn last(&mut self) {
        self.at = self.start_at(self.len());
    }
    fn replace(&mut self, range: Range<usize>, new: &str, at: usize) -> Result<(), &'static str> {
        if self.saved.is_none() {
            self.saved = Some(self.remember()?);
        }
        self.text.reserve(new.len().saturating_sub(range.len()))?;
        self.history
            .record(&self.text, range.clone(), new, self.at, at)?;
        self.text.replace(range.start, range.len(), new);
        self.at = at;
        Ok(())
    }
    #[cfg(test)]
    pub fn allocated(&self) -> usize {
        self.text.capacity()
            + self.saved.as_ref().map_or(0, String::capacity)
            + self.history.bytes()
    }
    #[cfg(test)]
    pub fn history_bytes(&self) -> usize {
        self.history.bytes()
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
        let at = if self.before(self.at) == Some('\n') && self.before(self.at - 1) == Some('\r') {
            self.at - 2
        } else {
            self.at - self.before(self.at).unwrap().len_utf8()
        };
        self.replace(at..self.at, "", at)
    }
    pub fn delete(&mut self) -> Result<(), &'static str> {
        if self.at < self.end() {
            self.replace(self.at..self.next(), "", self.at)?;
            self.normal();
            self.finish();
        }
        Ok(())
    }
    pub fn delete_line(&mut self) -> Result<(), &'static str> {
        if self.len() == 0 {
            return Ok(());
        }
        let start = self.start();
        if let Some(i) = self.newline(start) {
            self.replace(start..i + 1, "", start)
        } else {
            let cut = if start >= 2 && self.before(start - 1) == Some('\r') {
                start - 2
            } else {
                start.saturating_sub(1)
            };
            self.replace(cut..self.len(), "", cut)?;
            self.at = self.start();
            self.finish();
            Ok(())
        }
    }
    pub fn search(&mut self, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let after = self.next();
        let text = self.compact();
        let found = text[after..]
            .find(needle)
            .map(|i| i + after)
            .or_else(|| text.find(needle));
        if let Some(i) = found {
            self.at = i;
            true
        } else {
            false
        }
    }
}
