//! Contrôle du binaire de distribution avec le parseur du noyau de la même ISA.
use std::{env, fs};

#[derive(Clone, Copy, Debug)]
enum Access {
    ReadExecute,
    ReadWrite,
    ReadOnly,
}

#[derive(Debug)]
enum Error {
    Geometry,
    Occupied,
    Reserved,
}

type Result<T> = std::result::Result<T, Error>;
const PAGE: u64 = 4096;
const DATA: u64 = 0x401000;

#[path = "../novaOS/kernel/src/memory/user/elf.rs"]
mod elf;

fn main() {
    let path = env::args().nth(1).expect("ELF à contrôler");
    let image = fs::read(path).expect("lecture ELF");
    let parsed = elf::Elf::parse(&image).expect("admission du noyau");
    println!("entrée={:#x} pile={:#x}", parsed.entry, parsed.stack);
    for seg in parsed.segments() {
        println!("{:?} {:#x}..{:#x} offset={} octets={} pages={}", seg.access, seg.start,
                 seg.end, seg.offset, seg.filesz, seg.pages().count());
    }
}
