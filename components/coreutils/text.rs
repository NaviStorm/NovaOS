//! La fenêtre de lecture et le report UTF-8 sont indépendants de la taille du fichier.
use nova_cli::{console, fileapi::Client, protocol as w, runtime as rt, tty};
pub const CHUNK: usize = 704;
pub fn size(c: &mut Client, h: u64) -> rt::Result<u64> {
    Ok(w::word(&c.simple(w::STAT, h)?, w::PAYLOAD + 16))
}

pub fn scan(
    c: &mut Client,
    h: u64,
    mut start: u64,
    end: u64,
    visit: impl FnMut(Result<&str, u8>) -> rt::Result<bool>,
) -> rt::Result<()> {
    scan_chunks::<{ CHUNK + 3 }>(|buf| {
        let limit = (end - start).min(buf.len() as u64) as usize;
        if limit == 0 { return Ok((0, true)); }
        let n = c.read(h, start, &mut buf[..limit])?;
        if n > limit { return Err(w::IO); }
        start += n as u64;
        Ok((n, n < limit || start == end))
    }, visit)
}

// Trois octets suffisent pour garder un scalaire UTF-8 coupé entre deux lectures.
pub fn scan_chunks<const N: usize>(
    mut read: impl FnMut(&mut [u8]) -> rt::Result<(usize, bool)>,
    mut visit: impl FnMut(Result<&str, u8>) -> rt::Result<bool>,
) -> rt::Result<()> {
    let mut buf = [0u8; N];
    let mut carry = 0;
    loop {
        let (n, eof) = read(&mut buf[carry..carry + N - 3])?;
        let len = carry + n;
        let mut pos = 0;
        while pos < len {
            match core::str::from_utf8(&buf[pos..len]) {
                Ok(s) => {
                    if !visit(Ok(s))? {
                        return Ok(());
                    }
                    pos = len
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    if valid > 0
                        && !visit(Ok(core::str::from_utf8(&buf[pos..pos + valid]).unwrap()))?
                    {
                        return Ok(());
                    }
                    pos += valid;
                    if !eof && e.error_len().is_none() {
                        break;
                    }
                    if !visit(Err(buf[pos]))? {
                        return Ok(());
                    }
                    pos += 1;
                }
            }
        }
        carry = len - pos;
        buf.copy_within(pos..len, 0);
        if eof {
            break;
        }
    }
    Ok(())
}

pub fn display(c: &mut Client, h: u64, start: u64, end: u64, mut lines: u64) -> rt::Result<()> {
    if lines == 0 {
        return Ok(());
    }
    scan(c, h, start, end, |part| {
        match part {
            Err(b) => console::write(tty::OUTPUT, &alloc::format!("\\x{b:02x}"))?,
            Ok(text) => {
                for (i, _) in text.match_indices('\n') {
                    lines -= 1;
                    if lines == 0 {
                        console::printable(tty::OUTPUT, &text[..=i])?;
                        return Ok(false);
                    }
                }
                console::printable(tty::OUTPUT, text)?;
            }
        }
        Ok(true)
    })
}

pub fn tail_start(c: &mut Client, h: u64, end: u64, mut count: u64) -> rt::Result<u64> {
    if count == 0 {
        return Ok(end);
    }
    let mut at = end;
    let mut buf = [0; CHUNK];
    while at > 0 {
        let n = at.min(CHUNK as u64) as usize;
        at -= n as u64;
        if c.read(h, at, &mut buf[..n])? != n {
            return Err(rt::REFUSED);
        }
        for (i, &b) in buf[..n].iter().enumerate().rev() {
            let next = at + i as u64 + 1;
            if b == b'\n' && next < end {
                count -= 1;
                if count == 0 {
                    return Ok(next);
                }
            }
        }
    }
    Ok(0)
}
pub fn command(c: &mut Client, op: nova_cli::command::Operand) -> rt::Result<()> {
    use nova_cli::command::{Operand, Program};
    let Operand::Batch(o, program) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    for (i, path) in o.files.iter().enumerate() {
        let h = i as u64 + 1;
        if program != Program::Cat && (o.has(b'v') || o.files.len() > 1 && !o.has(b'q')) {
            if i > 0 {
                console::write(tty::OUTPUT, "\n")?
            }
            console::printable(tty::OUTPUT, &alloc::format!("==> {path} <==\n"))?;
        }
        let mut end = size(c, h)?;
        let mut start = 0;
        let mut lines = u64::MAX;
        if program == Program::Head {
            if o.bytes {
                end = end.min(o.count)
            } else {
                lines = o.count
            }
        } else if program == Program::Tail {
            start = if o.bytes {
                end.saturating_sub(o.count)
            } else {
                tail_start(c, h, end, o.count)?
            };
        }
        let pipe = rt::config(1);
        if program == Program::Cat && pipe != 0 {
            // Une lecture pleine remplit uniquement des trames entières.
            const FRAME: usize = nova_cli::process::PIPE_FRAME;
            let mut chunk = [0u8; (CHUNK / FRAME) * FRAME];
            while start < end {
                let wanted = (end - start).min(chunk.len() as u64) as usize;
                let n = c.read(h, start, &mut chunk[..wanted])?;
                if n == 0 || n > wanted { return Err(w::IO); }
                nova_cli::process::pipe_write(pipe, &chunk[..n]).map_err(|(_, code)| code)?;
                start += n as u64;
            }
        } else {
            display(c, h, start, end, lines)?;
        }
    }
    Ok(())
}
