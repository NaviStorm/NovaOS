use super::*;
use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;
use nova_cli::{console, fileapi::Client, process, protocol as w, runtime as rt, tty};

type Session = Rc<RefCell<Client>>;
enum Source {
    File {
        fs: Session,
        key: u64,
        id: u64,
        size: u64,
        pos: u64,
    },
    Pipe {
        key: u64,
        frame: [u8; process::PIPE_FRAME],
        start: usize,
        end: usize,
    },
}
fn message(code: u64) -> String {
    format!("{} ({code})", console::reason(code))
}
impl Input for Source {
    fn remaining(&self) -> Option<u64> {
        match self {
            Self::File { size, pos, .. } => Some(size.saturating_sub(*pos)),
            _ => None,
        }
    }
    fn identity(&self) -> Option<(u64, u64, u64)> {
        match self {
            Self::File { id, pos, .. } => Some((0, *id, *pos)),
            _ => None,
        }
    }
    fn skip(&mut self, count: u64) -> Result<(), String> {
        match self {
            Self::File { pos, .. } => {
                *pos = count;
                Ok(())
            }
            Self::Pipe { .. } => engine::discard(self, count),
        }
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        if buf.is_empty() {
            return Ok(0);
        }
        match self {
            Self::Pipe {
                key,
                frame,
                start,
                end,
            } => {
                if *start == *end {
                    *end = process::pipe_read(*key, frame).map_err(message)?;
                    *start = 0;
                }
                let n = buf.len().min(*end - *start);
                buf[..n].copy_from_slice(&frame[*start..*start + n]);
                *start += n;
                Ok(n)
            }
            Self::File {
                fs, key, size, pos, ..
            } => {
                let n = size.saturating_sub(*pos).min(buf.len() as u64) as usize;
                if n == 0 {
                    return Ok(0);
                }
                let got = fs
                    .borrow_mut()
                    .read(*key, *pos, &mut buf[..n])
                    .map_err(message)?;
                if got != n {
                    return Err(message(rt::REVOKED));
                }
                *pos += got as u64;
                Ok(got)
            }
        }
    }
}
struct Guest {
    fs: Option<Session>,
    next_file: u64,
}
impl Output for Guest {
    fn emit(&mut self, err: bool, bytes: &[u8]) -> Result<(), String> {
        let s = core::str::from_utf8(bytes).map_err(|_| message(rt::BAD_ARGUMENT))?;
        console::write(if err { tty::ERROR } else { tty::OUTPUT }, s).map_err(message)
    }
}
impl Platform for Guest {
    type Source = Source;
    fn open(&mut self, name: &[u8], _: usize) -> Result<Source, String> {
        if rt::config(3) != 0 {
            return Err(message(rt::config(3)));
        }
        if name == b"-" {
            let key = rt::config(2);
            if key == 0 {
                return Err(message(w::UNSUPPORTED));
            }
            return Ok(Source::Pipe {
                key,
                frame: [0; process::PIPE_FRAME],
                start: 0,
                end: 0,
            });
        }
        if self.fs.is_none() {
            self.fs = Some(Rc::new(RefCell::new(Client::scoped().map_err(message)?)))
        }
        let fs = self.fs.as_ref().unwrap().clone();
        let key = self.next_file;
        self.next_file += 1;
        let stat = fs.borrow_mut().simple(w::STAT, key).map_err(message)?;
        Ok(Source::File {
            fs,
            key,
            id: w::word(&stat, w::PAYLOAD),
            size: w::word(&stat, w::PAYLOAD + 16),
            pos: 0,
        })
    }
    fn block(&self) -> usize {
        4096
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mut guest = Guest {
        fs: None,
        next_file: 1,
    };
    let result = process::arguments().map_err(message).and_then(|args| {
        let fr = nova_cli::documentation::child_language(&args)
            == nova_cli::documentation::Language::French;
        let visible = if fr { &args[..1] } else { &args[..] };
        let bytes: Vec<_> = visible.iter().map(|s| s.as_bytes()).collect();
        execute(&mut guest, &bytes, b"cmp", fr, false)
    });
    let status = result.unwrap_or_else(|e| {
        let _ = guest.emit(true, format!("cmp: {e}\n").as_bytes());
        2
    });
    if rt::config(2) != 0 {
        rt::call(rt::IPC_CLOSE, rt::config(2), 0, 0);
    }
    rt::exit(status.into())
}
