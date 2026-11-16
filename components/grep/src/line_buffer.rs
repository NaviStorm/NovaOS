// This file is part of the uutils grep package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use memchr::{memchr, memrchr};
use memmap2::{Advice, Mmap, MmapOptions};
use std::fs::File;
use std::io::{self, Read, Write};

pub struct LineBuffer {
    buffer: Vec<u8>,
    mapped: Option<Mmap>,
    beg: usize,
    end: usize,
    offset: u64,
    eof: bool,
    binary: bool,
    line_terminator: u8,
}

impl LineBuffer {
    pub fn new(line_terminator: u8) -> Self {
        Self {
            buffer: vec![0; 128 * 1024],
            mapped: None,
            beg: 0,
            end: 0,
            offset: 0,
            eof: false,
            binary: false,
            line_terminator,
        }
    }

    pub fn reset(&mut self) {
        self.mapped = None;
        self.beg = 0;
        self.end = 0;
        self.offset = 0;
        self.eof = false;
        self.binary = false;
    }

    pub fn read_line(
        &mut self,
        file: &mut impl Read,
        mode: crate::BinaryMode,
    ) -> io::Result<Option<(&[u8], u64, bool)>> {
        self.next(file, mode, false)
    }

    pub fn read_chunk(&mut self, file: &mut impl Read) -> io::Result<Option<(&[u8], u64)>> {
        Ok(self
            .next(file, crate::BinaryMode::Text, true)?
            .map(|(bytes, pos, _)| (bytes, pos)))
    }

    fn next(
        &mut self,
        reader: &mut impl Read,
        mode: crate::BinaryMode,
        chunk: bool,
    ) -> io::Result<Option<(&[u8], u64, bool)>> {
        if self.eof {
            return Ok(None);
        }
        self.mapped = None;
        let mut spool: Option<File> = None;
        let start = self.offset;
        let mut scan = self.beg;
        loop {
            let bytes = &self.buffer[scan..self.end];
            let found = if chunk {
                memrchr(self.line_terminator, bytes)
            } else {
                memchr(self.line_terminator, bytes)
            };
            if found.is_some() || self.eof {
                let stop = found.map_or(self.end, |i| scan + i);
                let next = stop + usize::from(found.is_some());
                let end = if chunk { next } else { stop };
                let tail = &self.buffer[self.beg..end];
                self.offset += (next - self.beg) as u64;
                self.beg = next;
                if let Some(mut file) = spool {
                    file.write_all(tail)?;
                    // Unlinked private file, no other writer or surviving writable mapping.
                    let map = unsafe { MmapOptions::new().map(&file)? };
                    map.advise(Advice::Sequential)?;
                    self.mapped = Some(map);
                    return Ok(Some((self.mapped.as_deref().unwrap(), start, self.binary)));
                }
                if found.is_none() && tail.is_empty() {
                    return Ok(None);
                }
                return Ok(Some((tail, start, self.binary)));
            }
            if self.beg != 0 {
                self.buffer.copy_within(self.beg..self.end, 0);
                self.end -= self.beg;
                self.beg = 0;
            }
            scan = self.end;
            if self.end == self.buffer.len() {
                if self.end < 8 * 1024 * 1024 {
                    self.buffer
                        .try_reserve_exact(self.end)
                        .map_err(io::Error::other)?;
                    self.buffer.resize(self.end * 2, 0);
                } else {
                    if spool.is_none() {
                        spool = Some(tempfile::tempfile_in(std::env::temp_dir())?);
                    }
                    spool
                        .as_mut()
                        .unwrap()
                        .write_all(&self.buffer[..self.end])?;
                    self.offset += self.end as u64;
                    self.end = 0;
                    scan = 0;
                }
            }
            let n = loop {
                match reader.read(&mut self.buffer[self.end..]) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    res => break res?,
                }
            };
            self.eof = n == 0;
            let incoming = &mut self.buffer[self.end..self.end + n];
            if self.line_terminator != 0
                && mode != crate::BinaryMode::Text
                && memchr(0, incoming).is_some()
            {
                self.binary = true;
                if mode == crate::BinaryMode::WithoutMatch {
                    self.eof = true;
                    return Ok(None);
                }
                for byte in incoming {
                    if *byte == 0 {
                        *byte = self.line_terminator;
                    }
                }
            }
            self.end += n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek as _, SeekFrom};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A temp file pre-loaded with `content`, rewound to the start, and removed
    /// from disk when dropped.
    struct TempInput {
        file: File,
        path: std::path::PathBuf,
    }

    impl Drop for TempInput {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn temp_input(content: &[u8]) -> TempInput {
        let mut path = std::env::temp_dir();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push(format!("uu_grep_lb_{}_{n}.tmp", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(content).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        TempInput { file, path }
    }

    /// Drain `read_chunk` into a list of (owned bytes, start offset) pairs.
    fn chunks(term: u8, content: &[u8]) -> Vec<(Vec<u8>, u64)> {
        let mut lb = LineBuffer::new(term);
        let mut input = temp_input(content);
        let mut out = Vec::new();
        while let Some((chunk, start)) = lb.read_chunk(&mut input.file).unwrap() {
            out.push((chunk.to_vec(), start));
        }
        out
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(chunks(b'\n', b"").is_empty());
    }

    #[test]
    fn whole_complete_lines_come_back_as_one_chunk() {
        // Small input arrives in a single read, so everything up to the final
        // terminator is one chunk starting at offset 0.
        assert_eq!(
            chunks(b'\n', b"a\nbb\nccc\n"),
            vec![(b"a\nbb\nccc\n".to_vec(), 0)]
        );
    }

    #[test]
    fn unterminated_tail_is_a_final_chunk_with_its_own_offset() {
        // "a\n" is the complete-line chunk; "bb" is flushed at EOF at offset 2.
        assert_eq!(
            chunks(b'\n', b"a\nbb"),
            vec![(b"a\n".to_vec(), 0), (b"bb".to_vec(), 2)]
        );
    }

    #[test]
    fn input_without_any_terminator_is_one_chunk() {
        assert_eq!(chunks(b'\n', b"abc"), vec![(b"abc".to_vec(), 0)]);
    }

    #[test]
    fn honors_a_custom_terminator() {
        assert_eq!(
            chunks(b'\0', b"a\0bb\0c"),
            vec![(b"a\0bb\0".to_vec(), 0), (b"c".to_vec(), 5)]
        );
    }

    #[test]
    fn reassembles_input_larger_than_the_buffer() {
        // Force many reads and at least one chunk boundary mid-file.
        let mut content = Vec::new();
        for i in 0..50_000u32 {
            content.extend_from_slice(format!("line number {i}\n").as_bytes());
        }
        assert!(content.len() > 128 * 1024);

        let got = chunks(b'\n', &content);
        assert!(got.len() > 1, "expected multiple chunks, got {}", got.len());

        // Chunks must tile the input exactly, contiguously, each ending on a
        // line boundary (the input ends with a terminator).
        let mut expected_start = 0u64;
        let mut joined = Vec::new();
        for (bytes, start) in &got {
            assert_eq!(*start, expected_start);
            assert_eq!(*bytes.last().unwrap(), b'\n');
            expected_start += bytes.len() as u64;
            joined.extend_from_slice(bytes);
        }
        assert_eq!(joined, content);
    }

    #[test]
    fn grows_to_hold_a_single_overlong_line() {
        // One line far bigger than the initial 128 KiB buffer, then a short one.
        let mut content = vec![b'x'; 300 * 1024];
        content.push(b'\n');
        content.extend_from_slice(b"tail\n");

        let got = chunks(b'\n', &content);
        let joined: Vec<u8> = got.iter().flat_map(|(b, _)| b.clone()).collect();
        assert_eq!(joined, content);
        assert_eq!(got[0].1, 0);
    }
}
