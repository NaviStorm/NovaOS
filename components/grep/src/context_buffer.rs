// This file is part of the uutils grep package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

pub struct LineView<'a> {
    /// Line content (without the terminator).
    pub line: &'a [u8],
    /// 1-based line number.
    pub line_number: u64,
    /// Byte offset of this line in the input stream.
    pub byte_offset: u64,
    /// Whether this line matched (vs. being a context line).
    pub is_match: bool,
    /// Match positions within the line (start, end).
    /// Empty for context lines or for matching lines we don't need to highlight.
    pub match_positions: &'a [(usize, usize)],
}

pub enum StoredLine {
    Bytes(Vec<u8>),
    Mapped(memmap2::Mmap),
}
impl std::ops::Deref for StoredLine {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Bytes(b) => b,
            Self::Mapped(b) => b,
        }
    }
}

pub struct BufferedLine {
    pub line: StoredLine,
    pub line_number: u64,
    pub byte_offset: u64,
}

impl BufferedLine {
    pub fn view(&self) -> LineView<'_> {
        LineView {
            line: &self.line,
            line_number: self.line_number,
            byte_offset: self.byte_offset,
            is_match: false,
            match_positions: &[],
        }
    }
}

use crate::context_spool::Spool;
use std::collections::VecDeque;
use std::io;

pub struct ContextBuffer {
    lines: VecDeque<BufferedLine>,
    capacity: usize,
    bytes: usize,
    spool: Option<Spool>,
}

impl ContextBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            capacity,
            bytes: 0,
            spool: None,
        }
    }
    pub fn clear(&mut self) {
        *self = Self::new(self.capacity);
    }

    pub fn push(&mut self, line: &[u8], number: u64, offset: u64) -> io::Result<()> {
        debug_assert!(self.capacity > 0);
        if self.spool.is_none() {
            if self.lines.len() == self.capacity {
                self.pop()?;
            }
            let size = line.len() + size_of::<BufferedLine>();
            if self.bytes + size <= 8 * 1024 * 1024 {
                self.bytes += size;
                self.lines.push_back(BufferedLine {
                    line: StoredLine::Bytes(line.to_vec()),
                    line_number: number,
                    byte_offset: offset,
                });
                return Ok(());
            }
            let mut spool = Spool::new()?;
            for record in &self.lines {
                spool.push(&record.line, record.line_number, record.byte_offset)?;
            }
            self.lines = VecDeque::new();
            self.bytes = 0;
            self.spool = Some(spool);
        }
        let spool = self.spool.as_mut().unwrap();
        if spool.len == self.capacity {
            spool.discard()?;
        }
        spool.push(line, number, offset)
    }

    pub fn pop(&mut self) -> io::Result<Option<BufferedLine>> {
        if let Some(spool) = &mut self.spool {
            let record = spool.pop()?;
            if spool.len == 0 {
                self.spool = None;
            }
            return Ok(record);
        }
        let record = self.lines.pop_front();
        if let Some(record) = &record {
            self.bytes -= record.line.len() + size_of::<BufferedLine>();
        }
        Ok(record)
    }
}
