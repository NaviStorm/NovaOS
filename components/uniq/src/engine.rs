//! Moteur en flux. Une clé référence le tampon du groupe courant.
use alloc::{format, vec::Vec};
use core::ops::Range;

#[derive(Clone, Copy, PartialEq)]
pub enum Separator {
    None,
    Separate,
    Prepend,
    Append,
    Both,
}

pub struct Options {
    pub count: bool,
    pub repeated: bool,
    pub unique: bool,
    pub all: Option<Separator>,
    pub group: Option<Separator>,
    pub ignore_case: bool,
    pub fields: usize,
    pub skip: usize,
    pub width: usize,
    pub delimiter: u8,
    pub utf8: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            count: false,
            repeated: false,
            unique: false,
            all: None,
            group: None,
            ignore_case: false,
            fields: 0,
            skip: 0,
            width: usize::MAX,
            delimiter: b'\n',
            utf8: false,
        }
    }
}

fn unit(data: &[u8], utf8: bool) -> (u32, usize) {
    let b = data[0];
    if !utf8 || b < 128 {
        return (u32::from(b), 1);
    }
    let n = match b {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 1,
    };
    if let Some(c) = data
        .get(..n)
        .and_then(|s| core::str::from_utf8(s).ok())
        .and_then(|s| s.chars().next())
    {
        return (c as u32, n);
    }
    (u32::MAX, 1)
}

fn blank(c: u32) -> bool {
    matches!(c,9|10|32|0x1680|0x2000..=0x2006|0x2008..=0x200a|0x205f|0x3000)
}
fn advance(mut s: &[u8], mut n: usize, utf8: bool) -> usize {
    if !utf8 {
        return s.len().min(n);
    }
    let len = s.len();
    while !s.is_empty() && n > 0 {
        s = &s[unit(s, true).1..];
        n -= 1;
    }
    len - s.len()
}

impl Options {
    fn key(&self, s: &[u8]) -> Range<usize> {
        let mut at = 0;
        for _ in 0..self.fields {
            if at == s.len() {
                break;
            }
            for separators in [true, false] {
                while at < s.len() {
                    let (c, n) = unit(&s[at..], self.utf8);
                    if blank(c) != separators {
                        break;
                    }
                    at += n;
                }
            }
        }
        at += advance(&s[at..], self.skip, self.utf8);
        let len = s.len() - at;
        at..at
            + if self.width >= len {
                len
            } else {
                advance(&s[at..], self.width, self.utf8)
            }
    }
}

pub struct Engine {
    opt: Options,
    saved: Vec<u8>,
    key: Range<usize>,
    count: u64,
    emitted: bool,
}
impl Engine {
    pub fn new(opt: Options) -> Self {
        Self {
            opt,
            saved: Vec::new(),
            key: 0..0,
            count: 0,
            emitted: false,
        }
    }
    fn line<E>(&self, data: &[u8], out: &mut impl FnMut(&[u8]) -> Result<(), E>) -> Result<(), E> {
        out(data)?;
        out(core::slice::from_ref(&self.opt.delimiter))
    }
    fn close<E>(&self, out: &mut impl FnMut(&[u8]) -> Result<(), E>) -> Result<(), E> {
        if self.count == 0 || self.opt.group.is_some() {
            return Ok(());
        }
        if self.opt.all.is_some() {
            if self.count > 1 && !self.opt.unique {
                self.line(&self.saved, out)?
            }
        } else if self.opt.count || self.opt.repeated || self.opt.unique {
            if (self.count == 1 && !self.opt.repeated) || (self.count > 1 && !self.opt.unique) {
                if self.opt.count {
                    out(format!("{:7} ", self.count).as_bytes())?
                }
                self.line(&self.saved, out)?;
            }
        }
        Ok(())
    }
    fn separator<E>(
        &self,
        mode: Separator,
        out: &mut impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        if matches!(mode, Separator::Prepend | Separator::Both)
            || self.emitted && mode != Separator::None
        {
            out(core::slice::from_ref(&self.opt.delimiter))?;
        }
        Ok(())
    }
    pub fn record<E>(
        &mut self,
        data: &[u8],
        out: &mut impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        let key = self.opt.key(data);
        let same = self.count != 0
            && if self.opt.ignore_case {
                self.saved[self.key.clone()].eq_ignore_ascii_case(&data[key.clone()])
            } else {
                self.saved[self.key.clone()] == data[key.clone()]
            };
        if same {
            self.count = self.count.saturating_add(1);
            if let Some(mode) = self.opt.all {
                if self.count == 2 {
                    self.separator(mode, out)?;
                    self.emitted = true;
                }
                self.line(&self.saved, out)?;
            } else {
                if self.opt.group.is_some() {
                    self.line(data, out)?
                }
                return Ok(());
            }
        } else {
            self.close(out)?;
            self.count = 1;
            if let Some(mode) = self.opt.group {
                self.separator(mode, out)?;
                self.line(data, out)?;
                self.emitted = true;
            } else if !(self.opt.count
                || self.opt.repeated
                || self.opt.unique
                || self.opt.all.is_some())
            {
                self.line(data, out)?;
            }
        }
        self.saved.clear();
        self.saved.extend_from_slice(data);
        self.key = key;
        Ok(())
    }
    pub fn finish<E>(&self, out: &mut impl FnMut(&[u8]) -> Result<(), E>) -> Result<(), E> {
        self.close(out)?;
        if self.emitted && matches!(self.opt.group, Some(Separator::Append | Separator::Both)) {
            out(core::slice::from_ref(&self.opt.delimiter))?
        }
        Ok(())
    }
}
