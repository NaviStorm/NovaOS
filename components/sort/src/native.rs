use crate::private::Run;
use alloc::{format, vec::Vec};
use nova_cli::{console, fileapi::Client, process, protocol as fs, runtime as rt, tty};
use nova_sort::{
    args::Options,
    engine::{Order, merge},
};
pub trait Stream {
    fn read(&mut self, buf: &mut [u8]) -> rt::Result<usize>;
}
enum Source {
    File(Client, u64, u64),
    Pipe(u64),
    Terminal,
}
impl Stream for Source {
    fn read(&mut self, buf: &mut [u8]) -> rt::Result<usize> {
        match self {
            Self::File(c, offset, end) => {
                let n = (*end - *offset).min(buf.len() as u64) as usize;
                if n == 0 {
                    return Ok(0);
                }
                let read = c.read(1, *offset, &mut buf[..n])?;
                if read == 0 || read > n {
                    return Err(fs::IO);
                }
                *offset += read as u64;
                Ok(read)
            }
            Self::Pipe(p) => process::pipe_read(*p, buf),
            Self::Terminal => {
                let b = rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? as u8;
                if b == 4 {
                    return Ok(0);
                }
                buf[0] = if b == 13 { 10 } else { b };
                Ok(1)
            }
        }
    }
}
pub struct Reader<S> {
    source: S,
    buf: [u8; 256],
    at: usize,
    end: usize,
}
impl<S: Stream> Reader<S> {
    pub fn new(source: S) -> Self {
        Self {
            source,
            buf: [0; 256],
            at: 0,
            end: 0,
        }
    }
    pub fn record(&mut self, row: &mut Vec<u8>, d: u8) -> rt::Result<bool> {
        row.clear();
        loop {
            if self.at == self.end {
                self.end = self.source.read(&mut self.buf)?;
                self.at = 0;
                if self.end == 0 {
                    return Ok(!row.is_empty());
                }
            }
            let part = &self.buf[self.at..self.end];
            let n = part.iter().position(|&b| b == d).unwrap_or(part.len());
            row.try_reserve(n).map_err(|_| rt::NO_MEMORY)?;
            row.extend_from_slice(&part[..n]);
            self.at += n;
            if self.at != self.end {
                self.at += 1;
                return Ok(true);
            }
        }
    }
}
pub(super) fn general(s: &[u8]) -> Option<f64> {
    let s = core::str::from_utf8(s).ok()?.trim_ascii_start();
    let n = s
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || b"+-.".contains(b))
        .count();
    (1..=n).rev().find_map(|i| s[..i].parse().ok())
}
fn merge_runs(
    runs: &[Run],
    order: &Order<'_>,
    emit: impl FnMut(&[u8]) -> rt::Result<()>,
) -> rt::Result<()> {
    let mut readers = runs
        .iter()
        .map(Run::reader)
        .collect::<rt::Result<Vec<_>>>()?;
    merge(
        readers.len(),
        order,
        |i, row| readers[i].record(row, order.options.delim()),
        emit,
    )
}
fn display(port: u8, row: &[u8]) -> rt::Result<()> {
    if let Ok(text) = core::str::from_utf8(row) {
        return console::printable(port, text);
    }
    for &b in row {
        if b.is_ascii_graphic() || b == b' ' {
            console::write(port, core::str::from_utf8(&[b]).unwrap())?
        } else {
            console::write(port, &format!("\\x{b:02x}"))?
        }
    }
    Ok(())
}
fn command() -> rt::Result<u64> {
    let args = process::arguments()?;
    let o = Options::parse(&args).map_err(|_| rt::BAD_ARGUMENT)?;
    if o.help || o.version {
        let lang = nova_cli::documentation::child_language(&args);
        console::write(
            tty::OUTPUT,
            if o.help {
                lang.select(crate::EN, crate::FR)
            } else {
                crate::VERSION
            },
        )?;
        return Ok(0);
    }
    if rt::config(3) != 0 {
        return Err(rt::config(3));
    }
    if o.files.len() > 1
        || o.output.is_some()
        || o.list.is_some()
        || o.source.is_some()
        || o.has_random()
        || o.compressor.is_some()
        || o.parallel.is_some_and(|n| n != 1)
        || o.temps.iter().any(|s| s != ".")
        || o.debug
    {
        return Err(fs::UNSUPPORTED);
    }
    if o.sizes.iter().any(|s| s.ends_with('%')) {
        return Err(fs::UNSUPPORTED);
    }
    let bytes = o.buffer(0, 65536).map_err(|_| rt::BAD_ARGUMENT)?;
    let batch = o.batch.unwrap_or(4);
    if batch > 8 {
        return Err(rt::LIMIT);
    }
    let source = match o.files.first().map(|s| s.as_str()) {
        None | Some("-") if rt::config(2) != 0 => Source::Pipe(rt::config(2)),
        None | Some("-") => Source::Terminal,
        _ => {
            let mut c = Client::scoped()?;
            let size = fs::word(&c.simple(fs::STAT, 1)?, fs::PAYLOAD + 16);
            Source::File(c, 0, size)
        }
    };
    let mut reader = Reader::new(source);
    let order = Order {
        options: &o,
        collate: None,
        random_key: None,
        general,
    };
    let d = o.delim();
    let mut row = Vec::new();
    let mut previous = None;
    let mut rows = Vec::new();
    let mut runs = Vec::new();
    let mut seq = 0;
    let mut used = 0usize;
    let mut n = 0u64;
    let mut emit = |row: &[u8]| -> rt::Result<()> {
        let pipe = rt::config(1);
        if pipe != 0 {
            process::pipe_write(pipe, row)
                .and_then(|_| process::pipe_write(pipe, &[d]))
                .map_err(|(_, e)| e)?;
        } else {
            display(tty::OUTPUT, row)?;
            if d == 0 {
                console::write(tty::OUTPUT, "\\x00")?
            } else {
                console::write(tty::OUTPUT, "\n")?
            }
        }
        Ok(())
    };
    let mut last: Option<Vec<u8>> = None;
    let mut unique = |row: &[u8]| -> rt::Result<()> {
        if o.unique {
            if last.as_ref().is_some_and(|p| order.compare(p, row).is_eq()) {
                return Ok(());
            }
            last = Some(row.to_vec())
        }
        emit(row)
    };
    while reader.record(&mut row, d)? {
        n += 1;
        if let Some(quiet) = o.check {
            if previous.as_ref().is_some_and(|p: &Vec<u8>| {
                let c = order.compare(p, &row);
                c.is_gt() || o.unique && c.is_eq()
            }) {
                if !quiet {
                    console::write(
                        tty::ERROR,
                        &format!(
                            "sort: {}:{n}: disorder: ",
                            o.files.first().map_or("-", |s| s.as_str())
                        ),
                    )?;
                    display(tty::ERROR, &row)?;
                    console::write(tty::ERROR, "\n")?
                }
                return Ok(1);
            }
            previous = Some(core::mem::take(&mut row));
            continue;
        }
        if o.merge {
            unique(&row)?;
            continue;
        }
        used = used.saturating_add(row.capacity() + 2 * size_of::<Vec<u8>>());
        rows.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
        rows.push(core::mem::take(&mut row));
        if used >= bytes {
            order.sort(&mut rows);
            runs.push(Run::write(seq, |w| {
                for row in rows.drain(..) {
                    w.bytes(&row)?;
                    w.bytes(&[d])?
                }
                Ok(())
            })?);
            seq += 1;
            used = 0
        }
    }
    order.sort(&mut rows);
    if runs.is_empty() {
        for row in rows {
            unique(&row)?
        }
    } else {
        if !rows.is_empty() {
            runs.push(Run::write(seq, |w| {
                for row in rows {
                    w.bytes(&row)?;
                    w.bytes(&[d])?
                }
                Ok(())
            })?);
            seq += 1
        }
        while runs.len() > batch {
            let mut next = Vec::new();
            let mut it = runs.into_iter();
            loop {
                let mut group: Vec<_> = it.by_ref().take(batch).collect();
                if group.is_empty() {
                    break;
                }
                if group.len() == 1 {
                    next.push(group.pop().unwrap())
                } else {
                    next.push(Run::write(seq, |w| {
                        merge_runs(&group, &order, |row| {
                            w.bytes(row)?;
                            w.bytes(&[d])
                        })
                    })?);
                    seq += 1
                }
            }
            runs = next
        }
        merge_runs(&runs, &order, unique)?;
    }
    Ok(0)
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    #[cfg(feature = "qualification")]
    if rt::config(4) == 0 {
        rt::exit(if crate::fixtures::verify() { 0 } else { 1 })
    }
    let status = match command() {
        Ok(n) => n,
        Err(e) => {
            let _ = console::write(
                tty::ERROR,
                &format!("sort : {} ({e})\n", console::reason(e)),
            );
            2
        }
    };
    if rt::config(2) != 0 {
        let _ = rt::call(rt::IPC_CLOSE, rt::config(2), 0, 0);
    }
    rt::exit(status)
}
