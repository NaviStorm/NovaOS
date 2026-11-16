use super::context_buffer::{BufferedLine, StoredLine};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

/// Private FIFO for context that does not fit the in-memory window.
pub(crate) struct Spool {
    file: File,
    head: u64,
    tail: u64,
    pub len: usize,
}

impl Spool {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            file: tempfile::tempfile_in(std::env::temp_dir())?,
            head: 0,
            tail: 0,
            len: 0,
        })
    }

    pub fn push(&mut self, bytes: &[u8], number: u64, offset: u64) -> io::Result<()> {
        // Reclaim discarded records with bounded copying and amortized linear I/O.
        if self.head >= 16 * 1024 * 1024 && self.head >= self.tail - self.head {
            let mut replacement = tempfile::tempfile_in(std::env::temp_dir())?;
            self.file.seek(SeekFrom::Start(self.head))?;
            let copied = io::copy(&mut self.file, &mut replacement)?;
            if copied != self.tail - self.head {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            self.file = replacement;
            self.head = 0;
            self.tail = copied;
        }
        self.file.seek(SeekFrom::Start(self.tail))?;
        for value in [bytes.len() as u64, number, offset] {
            self.file.write_all(&value.to_le_bytes())?;
        }
        self.file.write_all(bytes)?;
        self.tail += 24 + bytes.len() as u64;
        self.len += 1;
        Ok(())
    }

    fn header(&mut self) -> io::Result<(usize, u64, u64)> {
        self.file.seek(SeekFrom::Start(self.head))?;
        let mut data = [0u8; 24];
        self.file.read_exact(&mut data)?;
        let word = |at| u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
        let size = usize::try_from(word(0)).map_err(io::Error::other)?;
        if size as u64 > (self.tail - self.head).saturating_sub(24) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.head += 24 + size as u64;
        self.len -= 1;
        Ok((size, word(8), word(16)))
    }

    pub fn discard(&mut self) -> io::Result<()> {
        self.header().map(|_| ())
    }

    pub fn pop(&mut self) -> io::Result<Option<BufferedLine>> {
        if self.len == 0 {
            return Ok(None);
        }
        let (size, line_number, byte_offset) = self.header()?;
        let line = if size > 128 * 1024 {
            // Queue data is append-only. The anonymous file remains alive in its mapping.
            StoredLine::Mapped(unsafe {
                memmap2::MmapOptions::new()
                    .offset(self.head - size as u64)
                    .len(size)
                    .map(&self.file)?
            })
        } else {
            let mut bytes = vec![0; size];
            self.file.read_exact(&mut bytes)?;
            StoredLine::Bytes(bytes)
        };
        Ok(Some(BufferedLine {
            line,
            line_number,
            byte_offset,
        }))
    }
}
