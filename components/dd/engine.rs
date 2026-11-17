use crate::{args::Config, tables::*};
use alloc::{format, string::String, vec::Vec};
pub type Result<T> = core::result::Result<T, String>;
#[derive(Default, Debug, Clone, Copy)]
pub struct Stats {
    pub input: [u64; 2],
    pub output: [u64; 2],
    pub bytes: u64,
    pub truncated: u64,
}
impl Stats {
    pub fn records(&self) -> String {
        let mut s = format!(
            "{}+{} records in\n{}+{} records out\n",
            self.input[0], self.input[1], self.output[0], self.output[1]
        );
        if self.truncated > 0 {
            s.push_str(&format!(
                "{} truncated record{}\n",
                self.truncated,
                if self.truncated == 1 { "" } else { "s" }
            ))
        }
        s
    }
}
pub trait Io {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn write(&mut self, data: &[u8]) -> Result<()>;
    fn input_seek(&mut self, _bytes: u64) -> Result<Option<bool>> {
        Ok(None)
    }
    fn report(&mut self, _stats: &Stats) {}
    fn warning(&mut self, _message: &str) {}
    fn case(&self, b: u8, upper: bool) -> u8 {
        if upper {
            b.to_ascii_uppercase()
        } else {
            b.to_ascii_lowercase()
        }
    }
}
fn buffer(size: usize) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    v.try_reserve_exact(size)
        .map_err(|_| format!("cannot allocate {size} bytes"))?;
    Ok(v)
}
fn read_block(io: &mut impl Io, buf: &mut [u8], full: bool) -> Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = io.read(&mut buf[total..])?;
        total += n;
        if n == 0 || !full {
            break;
        }
    }
    Ok(total)
}
fn output(io: &mut impl Io, s: &mut Stats, data: &[u8], obs: usize) -> Result<()> {
    io.write(data)?;
    s.output[usize::from(data.len() != obs)] += 1;
    s.bytes += data.len() as u64;
    Ok(())
}
struct Blocks<'a, I> {
    io: &'a mut I,
    stats: &'a mut Stats,
    pending: Vec<u8>,
    obs: usize,
}
impl<I: Io> Blocks<'_, I> {
    fn feed(&mut self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            if self.pending.is_empty() && data.len() >= self.obs {
                let (block, rest) = data.split_at(self.obs);
                output(self.io, self.stats, block, self.obs)?;
                data = rest
            } else {
                let n = (self.obs - self.pending.len()).min(data.len());
                self.pending.extend_from_slice(&data[..n]);
                data = &data[n..];
                if self.pending.len() == self.obs {
                    output(self.io, self.stats, &self.pending, self.obs)?;
                    self.pending.clear()
                }
            }
        }
        Ok(())
    }
    fn repeat(&mut self, b: u8, mut n: usize) -> Result<()> {
        let chunk = [b; 256];
        while n > 0 {
            let k = n.min(chunk.len());
            self.feed(&chunk[..k])?;
            n -= k
        }
        Ok(())
    }
}
struct Record {
    block: bool,
    unblock: bool,
    width: usize,
    col: usize,
    spaces: usize,
    space: u8,
    newline: u8,
}
impl Record {
    fn feed(&mut self, b: u8, out: &mut Blocks<impl Io>) -> Result<()> {
        if self.block {
            if b == self.newline {
                out.repeat(self.space, self.width.saturating_sub(self.col))?;
                self.col = 0
            } else {
                if self.col < self.width {
                    out.feed(&[b])?
                } else if self.col == self.width {
                    out.stats.truncated += 1
                }
                self.col = self.col.saturating_add(1)
            }
        } else if self.unblock {
            if b == self.space {
                self.spaces += 1
            } else {
                out.repeat(self.space, self.spaces)?;
                self.spaces = 0;
                out.feed(&[b])?
            }
            self.col += 1;
            if self.col == self.width {
                out.feed(&[self.newline])?;
                self.col = 0;
                self.spaces = 0
            }
        } else {
            out.feed(&[b])?
        }
        Ok(())
    }
}
pub fn copy(c: &Config, io: &mut impl Io, stats: &mut Stats) -> Result<()> {
    let full = c.inf.contains(&"fullblock");
    let skip = c
        .skip
        .checked_mul(c.ibs as u64)
        .and_then(|n| n.checked_add(c.skip_tail as u64))
        .ok_or("skip offset overflow")?;
    let mut read_skip = false;
    if skip > 0 {
        match io.input_seek(skip)? {
            Some(false) if c.status != "none" => io.warning("skip"),
            None => read_skip = true,
            _ => (),
        }
    }
    if !read_skip && c.count == Some(0) && c.tail == 0 {
        return Ok(());
    }
    let mut input = buffer(c.ibs)?;
    input.resize(c.ibs, 0);
    if read_skip {
        let mut total = 0u64;
        for i in 0..c.skip + u64::from(c.skip_tail != 0) {
            let n = if i == c.skip { c.skip_tail } else { c.ibs };
            let k = read_block(io, &mut input[..n], full)?;
            if k == 0 {
                break;
            }
            total += k as u64
        }
        if total != skip && c.status != "none" {
            io.warning("skip")
        }
    }
    let mut table = [0u8; 256];
    let charset = if c.has("ebcdic") {
        Some(&ASCII_TO_EBCDIC)
    } else if c.has("ibm") {
        Some(&ASCII_TO_IBM)
    } else {
        None
    };
    let translate = c.has("ascii") || charset.is_some() || c.has("ucase") || c.has("lcase");
    if translate {
        for (i, d) in table.iter_mut().enumerate() {
            let mut b = if c.has("ascii") {
                EBCDIC_TO_ASCII[i]
            } else {
                i as u8
            };
            if c.has("ucase") || c.has("lcase") {
                b = io.case(b, c.has("ucase"))
            }
            *d = charset.map_or(b, |t| t[b as usize]);
        }
    }
    let mut record = Record {
        block: c.has("block"),
        unblock: c.has("unblock"),
        width: c.cbs,
        col: 0,
        spaces: 0,
        space: charset.map_or(b' ', |t| t[32]),
        newline: charset.map_or(10, |t| t[10]),
    };
    let lines = record.block || record.unblock;
    let mut out = Blocks {
        io,
        stats,
        pending: buffer(if c.direct_blocks { 0 } else { c.obs })?,
        obs: c.obs,
    };
    let mut saved = None;
    let mut previous = 0;
    let mut warn = c.direct_blocks && !full && (c.skip > 0 || c.count.is_some_and(|n| n > 0));
    let mut failure = Ok(());
    let mut blocks = 0;
    loop {
        out.io.report(out.stats);
        let want = match c.count {
            Some(n) if blocks == n => c.tail,
            Some(n) if blocks > n => 0,
            _ => c.ibs,
        };
        if want == 0 {
            break;
        }
        let mut n = match read_block(out.io, &mut input[..want], full) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) => {
                if !c.has("noerror") {
                    failure = Err(error);
                    break;
                }
                if c.status != "none" {
                    out.io.warning(&error);
                    out.io.warning(&out.stats.records())
                }
                // Seul un repositionnement confirmé autorise une nouvelle tentative au bloc suivant.
                if out.io.input_seek(c.ibs as u64)? != Some(true) {
                    failure = Err(error);
                    break;
                }
                if !c.has("sync") {
                    continue;
                }
                0
            }
        };
        if warn && previous > 0 && previous < c.ibs {
            if c.status != "none" {
                out.io.warning(&format!(
                    "warning: partial read ({} byte{}); suggest iflag=fullblock",
                    previous,
                    if previous == 1 { "" } else { "s" }
                ))
            }
            warn = false
        }
        previous = n;
        blocks += 1;
        out.stats.input[usize::from(n != c.ibs)] += 1;
        if c.has("sync") {
            input[n..].fill(if lines { 32 } else { 0 });
            n = c.ibs
        }
        let data = &mut input[..n];
        if c.direct_blocks {
            output(out.io, out.stats, data, c.obs)?;
            continue;
        }
        if translate {
            for b in data.iter_mut() {
                *b = table[*b as usize]
            }
        }
        if c.has("swab") {
            let mut slice = &data[..];
            if let Some(b) = saved.take() {
                if let Some((&first, rest)) = slice.split_first() {
                    record.feed(first, &mut out)?;
                    record.feed(b, &mut out)?;
                    slice = rest
                }
            }
            let mut pairs = slice.chunks_exact(2);
            for pair in &mut pairs {
                record.feed(pair[1], &mut out)?;
                record.feed(pair[0], &mut out)?
            }
            saved = pairs.remainder().first().copied();
        } else if lines {
            for &b in data.iter() {
                record.feed(b, &mut out)?
            }
        } else {
            out.feed(data)?
        }
    }
    if let Some(b) = saved {
        record.feed(b, &mut out)?
    }
    if record.col > 0 {
        if record.block {
            out.repeat(record.space, record.width.saturating_sub(record.col))?
        } else if record.unblock {
            out.feed(&[record.newline])?
        }
    }
    if !out.pending.is_empty() {
        output(out.io, out.stats, &out.pending, c.obs)?
    }
    failure
}
