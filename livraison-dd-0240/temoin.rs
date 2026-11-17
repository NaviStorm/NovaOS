//! Point d’entrée de recette uniquement. cp sélectionne les objets 1 et 2 selon son contrat ordinaire.
#![no_std]
#![no_main]
extern crate alloc;
#[path = "../dd/args.rs"]
mod args;
#[path = "../dd/engine.rs"]
mod engine;
#[path = "../dd/tables.rs"]
mod tables;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use args::Action;
use engine::{Io, Stats};
use nova_cli::{console, fileapi::Client, process, protocol as w, runtime as rt, tty};
fn error(n: u64) -> String {
    console::reason(n).to_string()
}
struct Files {
    c: Client,
    dst: u64,
    len: u64,
    input: u64,
    output: u64,
}
impl Io for Files {
    fn read(&mut self, buf: &mut [u8]) -> engine::Result<usize> {
        let n = (self.len - self.input).min(buf.len() as u64) as usize;
        if n == 0 {
            return Ok(0);
        }
        let k = self.c.read(1, self.input, &mut buf[..n]).map_err(error)?;
        if k != n {
            return Err("lecture incomplète".into());
        }
        self.input += k as u64;
        Ok(k)
    }
    fn write(&mut self, data: &[u8]) -> engine::Result<()> {
        for part in data.chunks(w::MAX_WRITE) {
            self.c.write(self.dst, self.output, part).map_err(error)?;
            self.c.sync(self.dst).map_err(error)?;
            self.output += part.len() as u64;
        }
        Ok(())
    }
}
struct Memory {
    offset: usize,
    bytes: Vec<u8>,
}
const INPUT: &[u8] = include_bytes!("dd-temoin.bin");
impl Io for Memory {
    fn read(&mut self, buf: &mut [u8]) -> engine::Result<usize> {
        let n = buf.len().min(INPUT.len() - self.offset);
        buf[..n].copy_from_slice(&INPUT[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
    fn write(&mut self, data: &[u8]) -> engine::Result<()> {
        self.bytes.extend_from_slice(data);
        Ok(())
    }
}
fn vectors() -> rt::Result<()> {
    let cases: &[(&[&[u8]], Option<&[u8]>)] = include!("dd-vecteurs.rs");
    for (args, expected) in cases {
        match (args::parse(args, false), expected) {
            (Err(_), None) => (),
            (Ok(Action::Run(c)), Some(bytes)) => {
                let mut io = Memory {
                    offset: 0,
                    bytes: Vec::new(),
                };
                engine::copy(&c, &mut io, &mut Stats::default()).map_err(|_| w::IO)?;
                if &io.bytes != bytes {
                    return Err(w::IO);
                }
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
    }
    console::write(
        tty::OUTPUT,
        &alloc::format!("DD-VECTEURS {} OK\n", cases.len()),
    )
}
fn qualify() -> rt::Result<()> {
    let argv = process::arguments()?;
    let [source, dest] = argv.as_slice() else {
        return Err(rt::BAD_ARGUMENT);
    };
    let args: &[&[u8]] = match *dest {
        "copie512" => &[b"bs=512"],
        "copie4k" => &[b"bs=4K"],
        "conversion" => &[
            b"ibs=7",
            b"obs=31",
            b"count=67B",
            b"skip=2B",
            b"conv=swab,ucase",
        ],
        "matrice" => {
            vectors()?;
            &[b"count=0"]
        }
        _ => return Err(rt::BAD_ARGUMENT),
    };
    if !matches!(*source, "zeros" | "entree") {
        return Err(rt::BAD_ARGUMENT);
    }
    let Action::Run(cfg) = args::parse(args, false).map_err(|_| rt::BAD_ARGUMENT)? else {
        return Err(rt::BAD_ARGUMENT);
    };
    let mut c = Client::scoped()?;
    let len = w::word(&c.simple(w::STAT, 1)?, w::PAYLOAD + 16);
    let dst = w::word(&c.simple(w::CREATE_NEW, 2)?, 16);
    let mut io = Files {
        c,
        dst,
        len,
        input: 0,
        output: 0,
    };
    let mut stats = Stats::default();
    console::write(tty::OUTPUT, "DD-DEBUT\n")?;
    engine::copy(&cfg, &mut io, &mut stats).map_err(|_| w::IO)?;
    io.c.sync(dst)?;
    io.c.close(dst)?;
    io.c.simple(w::PUBLISH_COPY, 2)?;
    console::write(tty::OUTPUT, &alloc::format!("DD-FIN {}\n", stats.bytes))
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(qualify().err().unwrap_or(0))
}
