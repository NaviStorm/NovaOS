use super::{
    args::{Action, native},
    engine::Engine,
};
use alloc::{format, vec::Vec};
use core::cell::RefCell;
use nova_cli::{
    console, documentation, fileapi::Client, process, protocol as fs, runtime as rt, tty,
};

fn display(bytes: &[u8]) -> rt::Result<()> {
    for chunk in bytes.utf8_chunks() {
        console::printable(tty::OUTPUT, chunk.valid())?;
        for b in chunk.invalid() {
            console::write(tty::OUTPUT, &format!("\\x{b:02x}"))?
        }
    }
    Ok(())
}

struct Destination<'a> {
    file: Option<(u64, u64)>,
    client: &'a RefCell<Option<Client>>,
    buffer: Vec<u8>,
    offset: u64,
}
impl Destination<'_> {
    fn flush(&mut self) -> rt::Result<()> {
        if !self.buffer.is_empty()
            && let Some((h, _)) = self.file
        {
            let mut borrow = self.client.borrow_mut();
            let c = borrow.as_mut().ok_or(rt::DENIED)?;
            c.write(h, self.offset, &self.buffer)?;
            c.sync(h)?;
            self.offset += self.buffer.len() as u64;
            self.buffer.clear();
        }
        Ok(())
    }
    fn write(&mut self, mut bytes: &[u8]) -> rt::Result<()> {
        if self.file.is_none() {
            return display(bytes);
        }
        while !bytes.is_empty() {
            let n = bytes.len().min(4096 - self.buffer.len());
            self.buffer.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            if self.buffer.len() == 4096 {
                self.flush()?
            }
        }
        Ok(())
    }
    fn finish(mut self) -> rt::Result<()> {
        self.flush()?;
        if let Some((h, target)) = self.file {
            let mut borrow = self.client.borrow_mut();
            let c = borrow.as_mut().ok_or(rt::DENIED)?;
            c.close(h)?;
            c.simple(fs::PUBLISH_COPY, target)?;
        }
        Ok(())
    }
}

fn run_command() -> rt::Result<()> {
    let argv = process::arguments()?;
    let args: Vec<_> = argv.iter().map(|s| s.as_bytes()).collect();
    let (o, files) = match native(&args).map_err(|e| {
        let _ = console::write(tty::ERROR, &format!("uniq: {e}\n"));
        rt::BAD_ARGUMENT
    })? {
        action @ (Action::Help | Action::Version) => {
            return console::document(
                documentation::lookup("uniq").ok_or(rt::BAD_ARGUMENT)?,
                &argv,
                matches!(action, Action::Help),
            );
        }
        Action::Run(o, f) => (o, f),
    };
    if rt::config(3) != 0 {
        return Err(rt::config(3));
    }
    let pipe = rt::config(2);
    let input = files.first().is_some_and(|&s| s != b"-");
    let output = files.get(1).is_some_and(|&s| s != b"-");
    if pipe != 0 && (input || output) {
        return Err(rt::DENIED);
    }
    let mut c = if input || output {
        Some(Client::scoped()?)
    } else {
        None
    };
    let mut remaining = if input {
        fs::word(&c.as_mut().unwrap().simple(fs::STAT, 1)?, fs::PAYLOAD + 16)
    } else {
        0
    };
    let file = if output {
        let target = if input { 2 } else { 1 };
        let h = fs::word(&c.as_mut().unwrap().simple(fs::CREATE_NEW, target)?, 16);
        Some((h, target))
    } else {
        None
    };
    let client = RefCell::new(c);
    let mut dest = Destination {
        file,
        client: &client,
        buffer: Vec::new(),
        offset: 0,
    };
    let d = o.delimiter;
    let mut engine = Engine::new(o);
    let mut buf = [0u8; 512];
    let mut row = Vec::new();
    let mut offset = 0;
    loop {
        let len = if input {
            if remaining == 0 {
                break;
            }
            let n = remaining.min(buf.len() as u64) as usize;
            let n = client
                .borrow_mut()
                .as_mut()
                .unwrap()
                .read(1, offset, &mut buf[..n])?;
            if n == 0 {
                return Err(fs::IO);
            }
            remaining -= n as u64;
            offset += n as u64;
            n
        } else if pipe != 0 {
            process::pipe_read(pipe, &mut buf)?
        } else {
            let b = rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? as u8;
            if b == 4 {
                break;
            }
            buf[0] = if b == 13 { 10 } else { b };
            1
        };
        if len == 0 {
            break;
        }
        let mut part = &buf[..len];
        while let Some(end) = part.iter().position(|&b| b == d) {
            row.extend_from_slice(&part[..end]);
            engine.record(&row, &mut |s| dest.write(s))?;
            row.clear();
            part = &part[end + 1..];
        }
        row.extend_from_slice(part);
    }
    if !row.is_empty() {
        engine.record(&row, &mut |s| dest.write(s))?
    }
    engine.finish(&mut |s| dest.write(s))?;
    dest.finish()
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let result = run_command();
    if rt::config(2) != 0 {
        let _ = rt::call(rt::IPC_CLOSE, rt::config(2), 0, 0);
    }
    if let Err(e) = result {
        let _ = console::write(
            tty::ERROR,
            &format!("uniq : {} ({e})\n", console::reason(e)),
        );
        rt::exit(e)
    }
    rt::exit(0)
}
