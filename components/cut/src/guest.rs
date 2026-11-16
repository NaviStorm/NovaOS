use super::{
    args::{Action, parse},
    engine::{Config, Stream},
};
use alloc::{format, vec::Vec};
use nova_cli::{
    console, documentation, fileapi::Client, process, protocol as fs, runtime as rt, tty,
};

fn display(bytes: &[u8]) -> rt::Result<()> {
    for part in bytes.utf8_chunks() {
        console::printable(tty::OUTPUT, part.valid())?;
        for byte in part.invalid() {
            console::write(tty::OUTPUT, &format!("\\x{byte:02x}"))?
        }
    }
    Ok(())
}
fn consume(cfg: &Config, mut read: impl FnMut(&mut [u8]) -> rt::Result<usize>) -> rt::Result<()> {
    let mut buf = [0; 1024];
    let mut filtered = Vec::new();
    let mut display_buf = Vec::new();
    let mut stream = Stream::new(cfg);
    loop {
        let n = read(&mut buf)?;
        filtered.clear();
        if n == 0 {
            stream.finish(&mut filtered)
        } else {
            stream.feed(&buf[..n], &mut filtered)
        }
        // A Unicode scalar can span two writes even when cut is selecting bytes.
        display_buf.extend_from_slice(&filtered);
        let mut end = display_buf.len();
        if n != 0 {
            let start = end.saturating_sub(3);
            for i in start..end {
                if display_buf[i] >= 0xc2
                    && core::str::from_utf8(&display_buf[i..])
                        .is_err_and(|e| e.error_len().is_none() && e.valid_up_to() == 0)
                {
                    end = i;
                    break;
                }
            }
        }
        display(&display_buf[..end])?;
        display_buf.drain(..end);
        if n == 0 {
            return Ok(());
        }
    }
}
fn command() -> rt::Result<()> {
    let argv = process::arguments()?;
    let mut args: Vec<_> = argv.iter().map(|s| s.as_bytes()).collect();
    let mut utf8 = false;
    let mut posix = false;
    if let Some(mode) = args.first().and_then(|a| a.strip_prefix(b"\0cut:")) {
        match mode {
            b"0" => {}
            b"1" => utf8 = true,
            b"2" => posix = true,
            b"3" => {
                utf8 = true;
                posix = true
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        args.remove(0);
    }
    let (cfg, files) = match parse(&args, utf8, posix).map_err(|message| {
        let _ = console::write(tty::ERROR, &format!("cut: {message}\n"));
        rt::BAD_ARGUMENT
    })? {
        Action::Run(c, f) => (c, f),
        doc => {
            return console::document(
                documentation::lookup("cut").ok_or(rt::BAD_ARGUMENT)?,
                &argv,
                matches!(doc, Action::Help),
            );
        }
    };
    if rt::config(3) != 0 {
        return Err(rt::config(3));
    }
    let mut client = None;
    let mut handle = 0;
    let pipe = rt::config(2);
    let mut eof = false;
    for f in files {
        if f == b"-" {
            consume(&cfg, |buf| {
                if eof {
                    return Ok(0);
                }
                let n = if pipe != 0 {
                    process::pipe_read(pipe, buf)?
                } else {
                    let b = rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? as u8;
                    if b == 4 {
                        0
                    } else {
                        buf[0] = if b == 13 { 10 } else { b };
                        1
                    }
                };
                eof = n == 0;
                Ok(n)
            })?
        } else {
            handle += 1;
            if client.is_none() {
                client = Some(Client::scoped()?)
            }
            let c = client.as_mut().unwrap();
            let size = fs::word(&c.simple(fs::STAT, handle)?, fs::PAYLOAD + 16);
            let mut offset = 0;
            consume(&cfg, |buf| {
                if offset == size {
                    return Ok(0);
                }
                let wanted = (size - offset).min(buf.len() as u64) as usize;
                let n = c.read(handle, offset, &mut buf[..wanted])?;
                if n == 0 || n > wanted {
                    return Err(fs::IO);
                }
                offset += n as u64;
                Ok(n)
            })?
        }
    }
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let result = command();
    let pipe = rt::config(2);
    if pipe != 0 {
        let _ = rt::call(rt::IPC_CLOSE, pipe, 0, 0);
    }
    if let Err(e) = result {
        if e != rt::BAD_ARGUMENT {
            let _ = console::write(tty::ERROR, &format!("cut: {}\n", console::reason(e)));
        }
        rt::exit(e)
    }
    rt::exit(0)
}
