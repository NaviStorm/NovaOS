use crate::{args::Options, custom::custom_str_cmp, numeric::*, version::version_cmp};
use alloc::{borrow::Cow, string::String, vec::Vec};
use core::cmp::Ordering;
use md5::{Digest, Md5};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    modes: u16,
    pub mode: u8,
    pub reverse: bool,
    pub blank: bool,
    pub fold: bool,
    pub dictionary: bool,
    pub printable: bool,
}
impl Flags {
    pub fn has_transform(self) -> bool {
        self.mode != 0 || self.blank || self.fold || self.dictionary || self.printable
    }
    pub fn add(&mut self, c: u8) -> Result<(), String> {
        match c {
            b'b' => self.blank = true,
            b'r' => self.reverse = true,
            b'f' => self.fold = true,
            b'd' => {
                self.dictionary = true;
                self.printable = false
            }
            b'i' => self.printable = !self.dictionary,
            b'n' | b'g' | b'h' | b'M' | b'V' | b'R' => {
                let index = b"ghMnRV".iter().position(|&b| b == c).unwrap();
                self.modes |= 1 << index;
                self.mode = if self.modes & (1 << 4) != 0 { b'R' } else { c }
            }
            _ => return Err("stray character in field spec".into()),
        };
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Position {
    field: usize,
    column: usize,
    blank: bool,
}
pub struct Key {
    start: Position,
    end: Option<Position>,
    flags: Flags,
    local: bool,
    empty_end: bool,
}
fn position(s: &str, start: bool, flags: &mut Flags) -> Result<Position, String> {
    let n = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    let (f, c) = s[..n]
        .split_once('.')
        .map_or((&s[..n], None), |(a, b)| (a, Some(b)));
    let parse = |s: &str| {
        if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) {
            Ok(s.parse::<usize>().unwrap_or(usize::MAX))
        } else {
            Err("invalid number")
        }
    };
    let field = parse(f)?.checked_sub(1).ok_or("field number is zero")?;
    let col = c
        .map(parse)
        .transpose()
        .map_err(|_| "invalid character position")?
        .unwrap_or(usize::from(start));
    if start && col == 0 {
        return Err("character offset is zero".into());
    }
    let mods = &s.as_bytes()[n..];
    for &m in mods {
        flags.add(m)?
    }
    Ok(Position {
        field,
        column: col,
        blank: mods.contains(&b'b'),
    })
}
impl Key {
    pub fn legacy(start: &str, end: Option<&str>) -> Option<Self> {
        fn pos(s: &str, start: bool, flags: &mut Flags) -> Option<Position> {
            let n = s
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .unwrap_or(s.len());
            let (f, c) = s[..n]
                .split_once('.')
                .map_or((&s[..n], None), |(a, b)| (a, Some(b)));
            let field = f.parse::<usize>().ok()?;
            let col = c.map(str::parse::<usize>).transpose().ok()?.unwrap_or(0);
            for &m in &s.as_bytes()[n..] {
                flags.add(m).ok()?
            }
            Some(Position {
                field,
                column: if start { col.saturating_add(1) } else { col },
                blank: s[n..].contains('b'),
            })
        }
        let mut flags = Flags::default();
        let start = pos(start, true, &mut flags)?;
        let mut zero = false;
        let end = match end {
            Some(s) => {
                let mut p = pos(s, false, &mut flags)?;
                if p.column == 0 {
                    if p.field == 0 {
                        zero = true
                    } else {
                        p.field -= 1
                    }
                }
                Some(p)
            }
            None => None,
        };
        Some(Self {
            start,
            end,
            flags,
            local: flags != Flags::default(),
            empty_end: zero,
        })
    }
    pub fn parse(s: &str) -> Result<Self, String> {
        let (a, b) = s.split_once(',').map_or((s, None), |(a, b)| (a, Some(b)));
        let mut flags = Flags::default();
        let start = position(a, true, &mut flags)?;
        let end = b.map(|b| position(b, false, &mut flags)).transpose()?;
        Ok(Self {
            start,
            end,
            flags,
            local: flags != Flags::default(),
            empty_end: false,
        })
    }
    fn bounds(&self, line: &[u8], sep: Option<u8>, global: Flags) -> (usize, usize, Flags) {
        let flags = if self.local { self.flags } else { global };
        let locate = |p: Position, start: bool| {
            let mut pos = 0;
            for _ in 0..p.field {
                if pos == line.len() {
                    break;
                }
                pos = field_end(line, pos, sep);
                if sep.is_some() && pos < line.len() {
                    pos += 1
                }
            }
            if !start && p.column == 0 {
                return field_end(line, pos, sep);
            }
            if p.blank || !self.local && global.blank {
                pos += blanks(&line[pos..])
            }
            pos.saturating_add(p.column - usize::from(start))
                .min(line.len())
        };
        let a = locate(self.start, true);
        let b = if self.empty_end {
            0
        } else {
            self.end.map_or(line.len(), |p| locate(p, false))
        };
        (a, b.max(a), flags)
    }
}
fn blanks(s: &[u8]) -> usize {
    s.iter().take_while(|&&b| is_blank(b)).count()
}
fn field_end(s: &[u8], pos: usize, sep: Option<u8>) -> usize {
    if let Some(sep) = sep {
        return pos
            + s[pos..]
                .iter()
                .position(|&b| b == sep)
                .unwrap_or(s.len() - pos);
    }
    let i = pos + blanks(&s[pos..]);
    i + s[i..]
        .iter()
        .position(|&b| is_blank(b))
        .unwrap_or(s.len() - i)
}

pub type Collate = fn(&[u8], &[u8]) -> Ordering;
pub struct Order<'a> {
    pub options: &'a Options,
    pub collate: Option<Collate>,
    pub general: fn(&[u8]) -> Option<f64>,
}
fn filtered(s: &[u8], f: Flags) -> Cow<'_, [u8]> {
    if !(f.fold || f.dictionary || f.printable) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.iter()
            .copied()
            .filter(|&b| {
                (!f.dictionary || b.is_ascii_alphanumeric() || is_blank(b))
                    && (!f.printable || (32..127).contains(&b))
            })
            .map(|b| if f.fold { b.to_ascii_uppercase() } else { b })
            .collect(),
    )
}
impl Order<'_> {
    pub fn key(&self, line: &[u8], idx: usize) -> (usize, usize, Flags) {
        let o = self.options;
        if let Some(k) = o.keys.get(idx) {
            k.bounds(line, o.sep, o.flags)
        } else {
            (
                if o.flags.blank { blanks(line) } else { 0 },
                line.len(),
                o.flags,
            )
        }
    }
    pub fn annotation(&self, line: &[u8], idx: usize) -> (usize, usize) {
        let (mut begin, end, f) = self.key(line, idx);
        if !matches!(f.mode, b'g' | b'h' | b'n' | b'M') {
            return (begin, end);
        }
        begin += blanks(&line[begin..end]);
        let bytes = &line[begin..end];
        let mut n = 0;
        match f.mode {
            b'M' => {
                if bytes.len() >= 3
                    && b"JANFEBMARAPRMAYJUNJULAUGSEPOCTNOVDEC"
                        .chunks_exact(3)
                        .any(|m| m.eq_ignore_ascii_case(&bytes[..3]))
                {
                    n = 3
                }
            }
            b'g' => {
                n = bytes.len();
                while n > 0 && (self.general)(&bytes[..n]) == (self.general)(&bytes[..n - 1]) {
                    n -= 1
                }
            }
            _ => {
                let sign = usize::from(bytes.first() == Some(&b'-'));
                n = sign;
                while bytes.get(n).is_some_and(u8::is_ascii_digit) {
                    n += 1
                }
                let mut digits = n - sign;
                if bytes.get(n) == Some(&b'.') {
                    n += 1;
                    let first = n;
                    while bytes.get(n).is_some_and(u8::is_ascii_digit) {
                        n += 1
                    }
                    digits += n - first
                }
                if digits == 0 {
                    n = 0
                } else if f.mode == b'h' && bytes.get(n).is_some_and(|c| b"kKMGTPEZYRQ".contains(c))
                {
                    n += 1
                }
            }
        }
        (begin, begin + n)
    }
    fn by(&self, a: &[u8], b: &[u8], f: Flags) -> Ordering {
        if f.mode == 0 && self.collate.is_none() {
            return custom_str_cmp(a, b, f.printable, f.dictionary, f.fold);
        }
        let a = filtered(a, f);
        let b = filtered(b, f);
        match f.mode {
            b'n' | b'h' => {
                let settings = NumInfoParseSettings {
                    accept_si_units: f.mode == b'h',
                    decimal_pt: self.options.decimal.or(Some(b'.')),
                    thousands_separator: self.options.thousands,
                    ..Default::default()
                };
                let (ai, ar) = NumInfo::parse(&a, &settings);
                let (bi, br) = NumInfo::parse(&b, &settings);
                let cmp = if f.mode == b'h' {
                    human_numeric_str_cmp
                } else {
                    numeric_str_cmp
                };
                cmp((&a[ar], &ai), (&b[br], &bi))
            }
            b'M' => {
                let month = |s: &[u8]| {
                    let tail = &s[blanks(s)..];
                    if !self.options.months.is_empty() {
                        return self.options.months.iter().position(|m| {
                            !m.is_empty()
                                && tail
                                    .get(..m.len())
                                    .is_some_and(|v| v.eq_ignore_ascii_case(m))
                        });
                    }
                    let head = tail.get(..3);
                    b"JANFEBMARAPRMAYJUNJULAUGSEPOCTNOVDEC"
                        .chunks_exact(3)
                        .position(|m| head.is_some_and(|h| m.eq_ignore_ascii_case(h)))
                };
                month(&a).cmp(&month(&b))
            }
            b'g' => match ((self.general)(&a), (self.general)(&b)) {
                (Some(x), Some(y)) => x
                    .partial_cmp(&y)
                    .unwrap_or_else(|| y.is_nan().cmp(&x.is_nan())),
                (x, y) => x.is_some().cmp(&y.is_some()),
            },
            b'V' => version_cmp(&a, &b),
            b'R' => {
                let hash = |s: &[u8]| {
                    Md5::new()
                        .chain_update(self.options.seed)
                        .chain_update(s)
                        .finalize()
                };
                hash(&a).cmp(&hash(&b)).then_with(|| a.cmp(&b))
            }
            _ => self.collate.map_or_else(|| a.cmp(&b), |cmp| cmp(&a, &b)),
        }
    }
    pub fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        let o = self.options;
        for i in 0..o.keys.len().max(1) {
            let (aa, az, f) = self.key(a, i);
            let (ba, bz, _) = self.key(b, i);
            let c = self.by(&a[aa..az], &b[ba..bz], f);
            if !c.is_eq() {
                return if f.reverse { c.reverse() } else { c };
            }
        }
        if o.unique || o.stable {
            return Ordering::Equal;
        }
        let c = self.collate.map_or_else(|| a.cmp(b), |cmp| cmp(a, b));
        if o.flags.reverse { c.reverse() } else { c }
    }
    pub fn sort(&self, rows: &mut [Vec<u8>]) {
        rows.sort_by(|a, b| self.compare(a, b))
    }
}

/// Each reader owns one current record. Reader index breaks ties across consecutive runs.
pub fn merge<E>(
    count: usize,
    order: &Order<'_>,
    mut next: impl FnMut(usize, &mut Vec<u8>) -> Result<bool, E>,
    mut emit: impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<(), E> {
    let mut heads = alloc::vec![Vec::new();count];
    let mut heap: Vec<usize> = Vec::new();
    let less =
        |i: usize, j: usize, h: &[Vec<u8>]| order.compare(&h[i], &h[j]).then(i.cmp(&j)).is_lt();
    for i in 0..count {
        if next(i, &mut heads[i])? {
            let mut at = heap.len();
            heap.push(i);
            while at > 0 {
                let parent = (at - 1) / 2;
                if !less(i, heap[parent], &heads) {
                    break;
                }
                heap[at] = heap[parent];
                at = parent
            }
            heap[at] = i;
        }
    }
    while let Some(&winner) = heap.first() {
        emit(&heads[winner])?;
        if !next(winner, &mut heads[winner])? {
            heap.swap_remove(0);
            if heap.is_empty() {
                break;
            }
        }
        let i = heap[0];
        let mut at = 0;
        while 2 * at + 1 < heap.len() {
            let mut child = 2 * at + 1;
            if child + 1 < heap.len() && less(heap[child + 1], heap[child], &heads) {
                child += 1
            }
            if !less(heap[child], i, &heads) {
                break;
            }
            heap[at] = heap[child];
            at = child
        }
        heap[at] = i;
    }
    Ok(())
}
impl Options {
    pub fn has_random(&self) -> bool {
        self.flags.mode == b'R' || self.keys.iter().any(|k| k.flags.mode == b'R')
    }
}

impl Options {
    pub fn validate_flags(&self) -> Result<(), String> {
        for i in 0..self.keys.len().max(1) {
            let f = self
                .keys
                .get(i)
                .filter(|k| k.local)
                .map_or(self.flags, |k| k.flags);
            let numeric = f.modes & 15;
            if numeric.count_ones() + u32::from(f.dictionary || f.printable || f.modes & 48 != 0)
                > 1
            {
                let mut opts = String::new();
                for c in b"dfghiMnRV" {
                    let used = match c {
                        b'd' => f.dictionary,
                        b'f' => f.fold,
                        b'i' => f.printable,
                        _ => b"ghMnRV"
                            .iter()
                            .position(|b| b == c)
                            .is_some_and(|i| f.modes & (1 << i) != 0),
                    };
                    if used {
                        opts.push(char::from(*c))
                    }
                }
                return Err(alloc::format!("options '-{opts}' are incompatible"));
            }
        }
        Ok(())
    }
}
