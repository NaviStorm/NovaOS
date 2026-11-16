use alloc::{vec, vec::Vec};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Bytes,
    Chars,
    Fields,
}
pub struct Config {
    pub mode: Mode,
    pub ranges: Vec<(u64, u64)>,
    pub utf8: bool,
    pub partial: bool,
    pub delim: Vec<u8>,
    pub output: Option<Vec<u8>>,
    pub end: u8,
    pub suppress: bool,
    pub whitespace: bool,
    pub trim: bool,
}
impl Config {
    pub fn new(utf8: bool) -> Self {
        Self {
            mode: Mode::Fields,
            ranges: vec![],
            utf8,
            partial: false,
            delim: vec![9],
            output: None,
            end: 10,
            suppress: false,
            whitespace: false,
            trim: false,
        }
    }
    fn separator(&self) -> &[u8] {
        self.output.as_deref().unwrap_or(&self.delim)
    }
    fn range(&self, n: u64) -> Option<(u64, u64)> {
        self.ranges
            .get(self.ranges.partition_point(|r| r.1 < n))
            .copied()
            .filter(|r| r.0 <= n)
    }
}
pub fn unit(data: &[u8], utf8: bool) -> usize {
    if utf8 {
        let n = match data[0] {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 1,
        };
        if n > 1
            && data
                .get(..n)
                .is_some_and(|p| core::str::from_utf8(p).is_ok())
        {
            return n;
        }
    }
    1
}

pub struct Stream<'a> {
    cfg: &'a Config,
    pending: Vec<u8>,
    position: u64,
    printed: bool,
    started: bool,
    carry: Vec<u8>,
    selected: bool,
}
impl<'a> Stream<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Self {
            cfg,
            pending: Vec::new(),
            carry: Vec::new(),
            position: 1,
            printed: false,
            started: false,
            selected: cfg.range(1).is_some(),
        }
    }
    fn plain(&self) -> bool {
        self.cfg.mode == Mode::Fields
            && !self.cfg.whitespace
            && self.cfg.delim.len() == 1
            && self.cfg.delim[0] != self.cfg.end
            && !(self.cfg.utf8 && (0x80..=0xf4).contains(&self.cfg.delim[0]))
    }
    fn same_delim(&self) -> bool {
        self.cfg.mode == Mode::Fields && !self.cfg.whitespace && self.cfg.delim == [self.cfg.end]
    }
    fn deferred(&self) -> bool {
        self.position == 1 && self.cfg.suppress == self.selected
    }
    fn plain_part(&mut self, mut data: &[u8], out: &mut Vec<u8>) {
        self.started |= !data.is_empty();
        while !data.is_empty() {
            let n = data
                .iter()
                .position(|b| *b == self.cfg.delim[0])
                .unwrap_or(data.len());
            if self.deferred() {
                self.pending.extend_from_slice(&data[..n])
            } else if self.selected {
                out.extend_from_slice(&data[..n])
            }
            if n == data.len() {
                break;
            }
            if self.position == 1 && self.deferred() {
                if self.selected {
                    out.extend_from_slice(&self.pending)
                }
                self.pending.clear();
            }
            self.printed |= self.selected;
            self.position += 1;
            self.selected = self.cfg.range(self.position).is_some();
            if self.selected && self.printed {
                out.extend_from_slice(self.cfg.separator())
            }
            data = &data[n + 1..];
        }
    }
    fn positions(&mut self, mut data: &[u8], last: bool, out: &mut Vec<u8>) {
        self.started |= !data.is_empty();
        let decode = self.cfg.utf8 && (self.cfg.partial || self.cfg.mode == Mode::Chars);
        // Keep at most the prefix of one UTF-8 scalar across a read boundary.
        if !self.carry.is_empty() {
            let mut prefix = core::mem::take(&mut self.carry);
            let n = (4 - prefix.len()).min(data.len());
            prefix.extend_from_slice(&data[..n]);
            if !last && prefix.len() < 4 {
                self.carry = prefix;
                return;
            }
            let width = unit(&prefix, true);
            self.item(&prefix[..width], out);
            let tail = &prefix[width..];
            self.positions(tail, last && n == data.len(), out);
            self.positions(&data[n..], last, out);
            return;
        }
        while !data.is_empty() {
            if decode && !last && data[0] >= 0xc2 && data.len() < 4 {
                self.carry.extend_from_slice(data);
                break;
            }
            let width = unit(data, decode);
            self.item(&data[..width], out);
            data = &data[width..];
        }
    }
    fn item(&mut self, bytes: &[u8], out: &mut Vec<u8>) {
        let n = if self.cfg.mode == Mode::Bytes {
            bytes.len()
        } else {
            1
        };
        let mut first = None;
        let mut gap = false;
        for _ in 0..n {
            if let Some((lo, _)) = self.cfg.range(self.position) {
                first.get_or_insert(lo == self.position);
            } else if first.is_some() {
                gap = true
            }
            self.position += 1;
        }
        if let Some(start) = first
            && !gap
        {
            if self.printed
                && start
                && let Some(sep) = &self.cfg.output
            {
                out.extend_from_slice(sep)
            }
            out.extend_from_slice(bytes);
            self.printed = true
        }
    }
    fn empty_byte_search(&self) -> bool {
        self.cfg.mode == Mode::Fields
            && self.cfg.ranges.is_empty()
            && !self.same_delim()
            && if self.cfg.whitespace {
                !self.cfg.utf8 && !self.cfg.trim
            } else {
                !self.cfg.utf8
                    || self.cfg.delim.len() > 1
                    || !(0x80..=0xf4).contains(&self.cfg.delim[0])
            }
    }
    // With an empty field selection the 9.12 byte scanner takes a different
    // path for blocks containing a separator. Preserve its observable EOF rule.
    fn empty_block(&mut self, data: &[u8], out: &mut Vec<u8>) {
        if self.cfg.suppress {
            return;
        }
        let mut joined = core::mem::take(&mut self.pending);
        let block = if joined.is_empty() {
            data
        } else {
            joined.extend_from_slice(data);
            &joined
        };
        let end = self.cfg.end;
        if self.cfg.whitespace
            || block
                .windows(self.cfg.delim.len())
                .any(|s| s == self.cfg.delim)
        {
            out.extend(block.iter().copied().filter(|&b| b == end));
        } else if let Some(i) = block.iter().rposition(|&b| b == end) {
            out.extend_from_slice(&block[..=i]);
            if block.len() - i - 1 < 4 {
                self.pending.extend_from_slice(&block[i + 1..]);
            }
        }
    }
    pub fn feed(&mut self, data: &[u8], out: &mut Vec<u8>) {
        if self.empty_byte_search() {
            self.empty_block(data, out);
            return;
        }

        if self.same_delim() && self.cfg.ranges.is_empty() {
            self.started |= !data.is_empty();
            return;
        }
        if self.same_delim() {
            self.pending.extend_from_slice(data);
            return;
        }
        let mut parts = data.split(|&b| b == self.cfg.end).peekable();
        while let Some(part) = parts.next() {
            let end = parts.peek().is_some();
            if self.plain() {
                self.plain_part(part, out)
            } else if self.cfg.mode != Mode::Fields {
                self.positions(part, end, out)
            } else {
                self.pending.extend_from_slice(part);
                self.started |= !part.is_empty()
            }
            if end {
                self.end_record(out)
            }
        }
    }
    fn end_record(&mut self, out: &mut Vec<u8>) {
        if self.plain() {
            if self.position == 1 && self.deferred() && !self.cfg.suppress {
                out.extend_from_slice(&self.pending)
            }
            if self.position > 1 || !self.cfg.suppress {
                out.push(self.cfg.end)
            }
        } else if self.cfg.mode == Mode::Fields {
            self.fields(&self.pending, out)
        } else {
            out.push(self.cfg.end)
        }
        self.pending.clear();
        self.position = 1;
        self.printed = false;
        self.started = false;
        self.selected = self.cfg.range(1).is_some()
    }
    fn blank(&self, data: &[u8]) -> bool {
        matches!(data, b" " | b"\t")
            || self.cfg.utf8
                && matches!(
                    core::str::from_utf8(data)
                        .ok()
                        .and_then(|s| s.chars().next()),
                    Some(
                        '\u{1680}'
                        | '\u{2000}'..='\u{2006}'
                        | '\u{2008}'..='\u{200a}'
                        | '\u{205f}'
                        | '\u{3000}',
                    )
                )
    }
    fn fields(&self, data: &[u8], out: &mut Vec<u8>) {
        let mut row = data;
        if self.cfg.trim {
            let mut start = None;
            let mut end = 0;
            let mut p = 0;
            while p < row.len() {
                let n = unit(&row[p..], self.cfg.utf8);
                if !self.blank(&row[p..p + n]) {
                    start.get_or_insert(p);
                    end = p + n
                }
                p += n;
            }
            row = &row[start.unwrap_or(0)..end]
        }
        let mut p = 0;
        let mut begin = 0;
        let mut index = 1;
        let mut printed = false;
        while p < row.len() {
            let n = unit(&row[p..], self.cfg.utf8);
            let ch = &row[p..p + n];
            let split = if self.cfg.whitespace {
                self.blank(ch)
            } else {
                ch == self.cfg.delim
            };
            if !split {
                p += n;
                continue;
            }
            if self.cfg.range(index).is_some() {
                if printed {
                    out.extend_from_slice(self.cfg.separator())
                }
                out.extend_from_slice(&row[begin..p]);
                printed = true
            }
            p += n;
            if self.cfg.whitespace {
                while p < row.len() {
                    let w = unit(&row[p..], self.cfg.utf8);
                    if !self.blank(&row[p..p + w]) {
                        break;
                    }
                    p += w
                }
            }
            begin = p;
            index += 1;
        }
        if index == 1 {
            if self.cfg.suppress {
                return;
            }
            out.extend_from_slice(row);
        } else if self.cfg.range(index).is_some() {
            if printed {
                out.extend_from_slice(self.cfg.separator())
            }
            out.extend_from_slice(&row[begin..]);
        }
        out.push(self.cfg.end)
    }
    pub fn finish(&mut self, out: &mut Vec<u8>) {
        if self.empty_byte_search() {
            if !self.pending.is_empty() {
                out.extend_from_slice(&self.pending);
                out.push(self.cfg.end)
            }
            return;
        }
        if self.cfg.trim && !self.pending.is_empty() {
            let mut rest = self.pending.as_slice();
            while !rest.is_empty() {
                let n = unit(rest, self.cfg.utf8);
                if !self.blank(&rest[..n]) {
                    break;
                }
                rest = &rest[n..];
            }
            if rest.is_empty() {
                return;
            }
        }

        if self.same_delim() && self.cfg.ranges.is_empty() {
            if self.started && !self.cfg.suppress {
                out.push(self.cfg.end)
            }
            return;
        }
        if self.same_delim() {
            let data = &self.pending;
            if data.is_empty() {
                return;
            }
            if let Some(row) = data.strip_suffix(&[self.cfg.end])
                && !row.contains(&self.cfg.end)
            {
                if self.cfg.range(1).is_some() {
                    out.extend_from_slice(row);
                    out.push(self.cfg.end)
                } else if !self.cfg.suppress {
                    out.push(self.cfg.end)
                }
            } else {
                self.fields(data.strip_suffix(&[self.cfg.end]).unwrap_or(data), out)
            }
        } else if self.started {
            if self.cfg.mode != Mode::Fields {
                self.positions(&[], true, out)
            }
            self.end_record(out)
        }
    }
}
