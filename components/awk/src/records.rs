use crate::{Error, Result};
use alloc::{string::String, vec::Vec};
use no_std_io2::io::BufRead;

// UTF-8 separators have a unique final byte only for ASCII. Check the full
// suffix when an encoded separator straddles two input buffers.
fn separated(reader: &mut dyn BufRead, sep: &[u8], bytes: bool) -> Result<Option<String>> {
    let mut record = Vec::new();
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            if record.is_empty() {
                return Ok(None);
            }
            break;
        }
        let end = memchr::memchr(sep[sep.len() - 1], buf);
        let consumed = end.map_or(buf.len(), |i| i + 1);
        record
            .try_reserve(consumed)
            .map_err(|_| Error::runtime("out of memory reading record"))?;
        record.extend_from_slice(&buf[..consumed]);
        reader.consume(consumed);
        if end.is_some() && record.ends_with(sep) {
            record.truncate(record.len() - sep.len());
            break;
        }
    }
    decode(record, bytes).map(Some)
}

fn decode(record: Vec<u8>, bytes: bool) -> Result<String> {
    if bytes && !record.is_ascii() {
        return Ok(record.into_iter().map(char::from).collect());
    }
    String::from_utf8(record).map_err(|_| Error::runtime("invalid UTF-8 input"))
}
fn next_literal(reader: &mut dyn BufRead, rs: &str, bytes: bool) -> Result<Option<String>> {
    if rs.is_empty() {
        let mut record = String::new();
        while let Some(line) = separated(reader, b"\n", bytes)? {
            if line.is_empty() {
                if !record.is_empty() {
                    break;
                }
            } else {
                if !record.is_empty() {
                    record.push('\n');
                }
                record.push_str(&line);
            }
        }
        return Ok((!record.is_empty()).then_some(record));
    }
    if rs.chars().count() != 1 {
        return Err(Error::runtime("regular-expression RS is unavailable"));
    }
    if bytes {
        let byte = u8::try_from(rs.chars().next().unwrap() as u32)
            .map_err(|_| Error::runtime("invalid byte separator"))?;
        separated(reader, &[byte], true)
    } else {
        separated(reader, rs.as_bytes(), false)
    }
}

pub(crate) fn emit(writer: &mut dyn no_std_io2::io::Write, text: &str, bytes: bool) -> Result<()> {
    if !bytes || text.is_ascii() {
        writer.write_all(text.as_bytes())?;
    } else {
        for c in text.chars() {
            let b = u8::try_from(c as u32)
                .map_err(|_| Error::runtime("character outside byte range"))?;
            writer.write_all(&[b])?;
        }
    }
    Ok(())
}

pub(crate) struct Sink;
impl no_std_io2::io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> no_std_io2::io::Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> no_std_io2::io::Result<()> {
        Ok(())
    }
}

/// Regex separators may need lookahead beyond the last matching byte. Keep
/// those bytes with this stream even when the program changes RS mid-file.
pub(crate) struct Records<R> {
    pub inner: R,
    pending: Vec<u8>,
    offset: usize,
    previous: Option<u8>,
    regex: Option<(String, bool, crate::ere::Ere)>,
}

impl<R: BufRead> Records<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            offset: 0,
            previous: None,
            regex: None,
        }
    }

    pub fn next(&mut self, rs: &str, bytes: bool, insensitive: bool) -> Result<Option<String>> {
        if rs.len() <= 1 || rs.chars().count() == 1 {
            return next_literal(self, rs, bytes);
        }
        let cached = self.regex.take();
        let entry = match cached {
            Some(e) if e.0 == rs && e.1 == insensitive => e,
            _ => (
                String::from(rs),
                insensitive,
                crate::ere::Ere::new(rs, bytes, insensitive)?,
            ),
        };
        let result = self.regex_record(&entry.2, bytes);
        self.regex = Some(entry);
        result
    }

    fn byte(&mut self, index: usize) -> Result<Option<u8>> {
        if index == self.pending.len() {
            let buf = self.inner.fill_buf()?;
            self.pending
                .try_reserve(buf.len())
                .map_err(|_| Error::runtime("out of memory reading RS"))?;
            self.pending.extend_from_slice(buf);
            let count = buf.len();
            self.inner.consume(count);
        }
        Ok(self.pending.get(index).copied())
    }

    fn regex_record(&mut self, re: &crate::ere::Ere, bytes: bool) -> Result<Option<String>> {
        if self.offset >= 8192 {
            self.pending.drain(..self.offset);
            self.offset = 0;
        }
        let mut begin = self.offset;
        while self.byte(begin)?.is_some() {
            let before = if begin == self.offset {
                self.previous
            } else {
                Some(self.pending[begin - 1])
            };
            let mut state = re.start(before)?;
            let mut end = begin;
            let mut pos = begin;
            loop {
                let byte = self.byte(pos)?;
                state = re.step(state, byte);
                if re.matched(state) {
                    end = pos;
                }
                if byte.is_none() || re.dead(state) {
                    break;
                }
                pos += 1;
            }
            if end > begin {
                let text = decode(self.pending[self.offset..begin].to_vec(), bytes)?;
                self.offset = end;
                self.previous = Some(self.pending[end - 1]);
                return Ok(Some(text));
            }
            begin += 1;
        }
        let data = &self.pending[self.offset..];
        let result = if data.is_empty() {
            None
        } else {
            Some(decode(data.to_vec(), bytes)?)
        };
        self.previous = self.pending.last().copied().or(self.previous);
        self.offset = self.pending.len();
        Ok(result)
    }
}

impl<R: BufRead> no_std_io2::io::Read for Records<R> {
    fn read(&mut self, dst: &mut [u8]) -> no_std_io2::io::Result<usize> {
        let buf = self.fill_buf()?;
        let n = dst.len().min(buf.len());
        dst[..n].copy_from_slice(&buf[..n]);
        self.consume(n);
        Ok(n)
    }
}
impl<R: BufRead> BufRead for Records<R> {
    fn fill_buf(&mut self) -> no_std_io2::io::Result<&[u8]> {
        if self.offset < self.pending.len() {
            Ok(&self.pending[self.offset..])
        } else {
            self.inner.fill_buf()
        }
    }
    fn consume(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        if self.offset < self.pending.len() {
            self.offset += n;
            self.previous = Some(self.pending[self.offset - 1]);
        } else {
            // The last byte was already obtained by fill_buf.
            self.previous = self
                .inner
                .fill_buf()
                .ok()
                .and_then(|b| b.get(n - 1).copied());
            self.inner.consume(n);
        }
    }
}
