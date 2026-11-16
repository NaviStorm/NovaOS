// MIT. Sélection commune aux deux architectures, sans E/S ni bibliothèque C.
use alloc::vec::Vec;

#[derive(Clone, Copy)]
pub struct Segment<'a> {
    pub bytes: &'a [u8],
    pub first: bool,
    pub last: bool,
}

pub trait Pattern {
    type Error;
    const MATCH_BYTES: bool = false;
    /// Leftmost match, longest on a tie. Spans are relative to the segment.
    fn find(&self, part: Segment<'_>, from: usize) -> Result<Option<(usize, usize)>, Self::Error>;
}

#[derive(Clone, Copy)]
pub struct Selection {
    pub utf8: bool,
    pub word: bool,
    pub whole: bool,
    pub inverted: bool,
}

impl Selection {
    pub fn select<P: Pattern>(
        &self,
        patterns: &[P],
        line: &[u8],
        positions: bool,
    ) -> Result<Option<Vec<(usize, usize)>>, P::Error> {
        let mut matched = false;
        let mut spans = Vec::new();
        let mut visit = |bytes: &[u8], offset: usize| -> Result<bool, P::Error> {
            let part = Segment {
                bytes,
                first: offset == 0,
                last: offset + bytes.len() == line.len(),
            };
            if self.whole && !(part.first && part.last) {
                return Ok(false);
            }
            // Each pattern keeps its next match until the shared cursor passes it.
            let mut pending = Vec::with_capacity(patterns.len());
            for pattern in patterns {
                pending.push(pattern.find(part, 0)?);
            }
            while let Some((start, end)) = pending
                .iter()
                .flatten()
                .copied()
                .min_by_key(|&(s, e)| (s, core::cmp::Reverse(e)))
            {
                let selected = (!self.whole || start == 0 && end == bytes.len())
                    && (!self.word || self.whole || word_boundary(bytes, start, end, self.utf8));
                if selected {
                    matched = true;
                    if !positions || self.inverted {
                        return Ok(true);
                    }
                    if start < end {
                        spans.push((offset + start, offset + end));
                    }
                }
                // Rejected and empty matches must leave overlapping candidates visible.
                let from = if selected && end > start {
                    end
                } else {
                    next_byte(bytes, start, self.utf8)
                };
                if from > bytes.len() {
                    break;
                }
                for (slot, pattern) in pending.iter_mut().zip(patterns) {
                    if slot.is_some_and(|(s, _)| s < from) {
                        *slot = pattern.find(part, from)?;
                    }
                }
            }
            Ok(false)
        };
        if !P::MATCH_BYTES && self.utf8 && core::str::from_utf8(line).is_err() {
            // Invalid bytes cannot participate in a character match. Preserve the
            // actual anchors, original byte offsets and inversion of the entire record.
            let mut offset = 0;
            for chunk in line.utf8_chunks() {
                if visit(chunk.valid().as_bytes(), offset)? {
                    break;
                }
                offset += chunk.valid().len() + chunk.invalid().len();
                if offset == line.len() && !chunk.invalid().is_empty() {
                    visit(&[], offset)?;
                }
            }
        } else {
            visit(line, 0)?;
        }
        Ok((matched != self.inverted).then_some(spans))
    }
}

fn next_byte(bytes: &[u8], start: usize, utf8: bool) -> usize {
    let mut next = start + 1;
    while utf8 && bytes.get(next).is_some_and(|b| b & 0xc0 == 0x80) {
        next += 1;
    }
    next
}

pub fn word_boundary(bytes: &[u8], start: usize, end: usize, utf8: bool) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    if utf8 {
        // The segment is valid. Decode at most one adjacent scalar on each side.
        let tail = bytes.get(end..).unwrap_or_default();
        let right = tail
            .first()
            .map(|&first| match first {
                0..=127 => 1,
                192..=223 => 2,
                224..=239 => 3,
                _ => 4,
            })
            .and_then(|len| core::str::from_utf8(tail.get(..len)?).ok()?.chars().next());
        let left = bytes[start.saturating_sub(4)..start]
            .iter()
            .rposition(|b| b & 0xc0 != 0x80)
            .and_then(|i| {
                core::str::from_utf8(&bytes[start.saturating_sub(4) + i..start])
                    .ok()?
                    .chars()
                    .next()
            });
        return !left.is_some_and(word) && !right.is_some_and(word);
    }
    let ascii = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_';
    !bytes.get(end).is_some_and(ascii)
        && !start
            .checked_sub(1)
            .and_then(|i| bytes.get(i))
            .is_some_and(ascii)
}
