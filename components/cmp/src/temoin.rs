//! Adaptateur mémoire exclusivement lié dans le témoin explicite du SDK.
use super::*;
use alloc::{vec, vec::Vec};
use nova_cli::runtime as rt;

struct Bytes {
    bytes: &'static [u8],
    pos: usize,
    pipe: bool,
}
impl Input for Bytes {
    fn remaining(&self) -> Option<u64> {
        (!self.pipe).then_some((self.bytes.len() - self.pos) as u64)
    }
    fn identity(&self) -> Option<(u64, u64, u64)> {
        None
    }
    fn skip(&mut self, n: u64) -> Result<(), String> {
        if self.pipe {
            return engine::discard(self, n).map_err(|e| format!("-: {e}"));
        }
        self.pos = (n as usize).min(self.bytes.len());
        Ok(())
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        let n = (self.bytes.len() - self.pos).min(buf.len()).min(79);
        buf[..n].copy_from_slice(&self.bytes[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}
struct Memory {
    files: Vec<(&'static [u8], &'static [u8])>,
    stdin: &'static [u8],
    output: [Vec<u8>; 2],
    opens: usize,
}
impl Output for Memory {
    fn emit(&mut self, err: bool, b: &[u8]) -> Result<(), String> {
        self.output[err as usize].extend(b);
        Ok(())
    }
}
impl Platform for Memory {
    type Source = Bytes;
    fn open(&mut self, name: &[u8], _: usize) -> Result<Bytes, String> {
        self.opens += 1;
        let pipe = name == b"-";
        let bytes = if pipe {
            self.stdin
        } else {
            self.files
                .iter()
                .find(|(n, _)| n == &name)
                .map(|(_, v)| *v)
                .ok_or_else(|| {
                    format!(
                        "{}: No such file or directory",
                        String::from_utf8_lossy(name)
                    )
                })?
        };
        Ok(Bytes {
            bytes,
            pos: 0,
            pipe,
        })
    }
    fn block(&self) -> usize {
        256
    }
}
fn number(input: &mut &'static [u8]) -> usize {
    let (a, b) = input.split_at(4);
    *input = b;
    u32::from_le_bytes(a.try_into().unwrap()) as usize
}
fn debug(s: &str) {
    rt::call(rt::DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mut data: &[u8] = include_bytes!(env!("CMP_VECTEURS"));
    let pool: Vec<_> = (0..number(&mut data))
        .map(|_| {
            let n = number(&mut data);
            let (a, b) = data.split_at(n);
            data = b;
            a
        })
        .collect();
    let count = number(&mut data);
    let mut errors = 0;
    for index in 0..count {
        let args: Vec<_> = (0..number(&mut data))
            .map(|_| pool[number(&mut data)])
            .collect();
        let stdin = pool[number(&mut data)];
        let files = (0..number(&mut data))
            .map(|_| (pool[number(&mut data)], pool[number(&mut data)]))
            .collect();
        let lang = pool[number(&mut data)];
        let expected = number(&mut data);
        let stdout = pool[number(&mut data)];
        let stderr = pool[number(&mut data)];
        let mut mem = Memory {
            files,
            stdin,
            output: [vec![], vec![]],
            opens: 0,
        };
        let fr =
            locale::Language::from_tag(core::str::from_utf8(lang).ok()) == locale::Language::French;
        let rc = execute(&mut mem, &args, b"cmp", fr, false).unwrap_or_else(|e| {
            mem.emit(true, format!("cmp: {e}\n").as_bytes()).unwrap();
            2
        });
        let doc = matches!(
            options::parse(&args, b"cmp", false),
            Ok(Action::Help | Action::Version) | Err(_)
        );
        if rc as usize != expected
            || mem.output[0] != stdout
            || mem.output[1] != stderr
            || doc && mem.opens != 0
        {
            errors += 1;
            debug(&format!(
                "CMP cas={index} rc={rc} attendu={expected} err={} out={}",
                mem.output[1] == stderr,
                mem.output[0] == stdout
            ));
        }
    }
    debug(&format!("CMP vecteurs={count} ecarts={errors}"));
    rt::exit(u64::from(errors != 0))
}
