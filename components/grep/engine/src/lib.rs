// This file is part of the uutils grep package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

#![no_std]
extern crate alloc;
use alloc::vec::Vec;

/// KMP over raw bytes. The bare soft-float targets require scalar code.
pub struct Literal {
    needle: Vec<u8>,
    prefix: Vec<usize>,
}

impl Literal {
    pub fn new(needle: Vec<u8>) -> Self {
        let mut prefix = alloc::vec![0; needle.len()];
        let mut k = 0;
        for i in 1..needle.len() {
            while k > 0 && needle[i] != needle[k] {
                k = prefix[k - 1];
            }
            if needle[i] == needle[k] {
                k += 1;
            }
            prefix[i] = k;
        }
        Self { needle, prefix }
    }

    pub fn find(&self, bytes: &[u8]) -> Option<usize> {
        if self.needle.is_empty() {
            return Some(0);
        }
        self.advance(&mut 0, bytes)
            .map(|end| end - self.needle.len())
    }

    /// Return the exclusive end in this fragment, even when a match began in the last.
    pub fn advance(&self, prefix: &mut usize, bytes: &[u8]) -> Option<usize> {
        if self.needle.is_empty() {
            return Some(0);
        }
        for (i, &byte) in bytes.iter().enumerate() {
            while *prefix > 0 && byte != self.needle[*prefix] {
                *prefix = self.prefix[*prefix - 1];
            }
            if byte == self.needle[*prefix] {
                *prefix += 1;
            }
            if *prefix == self.needle.len() {
                *prefix = self.prefix[*prefix - 1];
                return Some(i + 1);
            }
        }
        None
    }
}

mod records;
pub use records::{Pattern, Segment, Selection, word_boundary};

impl Pattern for Literal {
    type Error = core::convert::Infallible;
    const MATCH_BYTES: bool = true;
    fn find(&self, part: Segment<'_>, from: usize) -> Result<Option<(usize, usize)>, Self::Error> {
        Ok(self
            .find(&part.bytes[from..])
            .map(|p| (from + p, from + p + self.needle.len())))
    }
}

/// Offset of the earliest occurrence of any needle in `hay`, or `None`.
pub fn leftmost_match(finders: &[Literal], hay: &[u8]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for finder in finders {
        if let Some(pos) = finder.find(hay) {
            best = Some(best.map_or(pos, |b| b.min(pos)));
            if best == Some(0) {
                break; // Can't start any earlier.
            }
        }
    }
    best
}

/// Count line terminators in `bytes`.
pub fn count_terminators(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|&&byte| byte == b'\n').count() as u64
}

/// Byte range `[start, end)` of the line containing `pos` in `buf`, excluding
/// the trailing terminator. `start` follows the previous terminator (or 0);
/// `end` is the next terminator (or end of buffer).
pub fn line_bounds(buf: &[u8], pos: usize) -> (usize, usize) {
    let start = buf[..pos]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let end = buf[pos..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(buf.len(), |i| pos + i);
    (start, end)
}
