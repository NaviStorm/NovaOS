#![no_std]
#![no_main]
extern crate alloc;
mod common;
use nova_cli::{
    command::{Operand, Program},
    console,
    fileapi::Client,
    protocol as w, runtime as rt, tty,
};
fn list(c: &mut Client, detail: bool) -> rt::Result<()> {
    let mut previous = alloc::vec::Vec::new();
    let mut generation = 0;
    loop {
        let mut name = alloc::vec::Vec::new();
        loop {
            let a = c.readdir_fragment(0, &previous, name.len() as u64, generation)?;
            generation = w::word(&a, 24);
            let n = w::length(&a);
            let len = w::word(&a, 32);
            if len == 0 && name.is_empty() {
                return Ok(());
            }
            if n == 0 || n > w::FRAGMENT || len > 255 || name.len() as u64 + n as u64 > len {
                return Err(rt::REFUSED);
            }
            name.extend_from_slice(&a[w::PAYLOAD..w::PAYLOAD + n]);
            if name.len() as u64 == len {
                break;
            }
        }
        let stat = c.at(w::LOOKUP_AT, 0, &name, 0)?;
        let kind = if stat[w::PAYLOAD + 24] == 2 {
            "répertoire"
        } else {
            "fichier"
        };
        console::write(tty::OUTPUT, kind)?;
        console::write(tty::OUTPUT, " ")?;
        console::printable(
            tty::OUTPUT,
            core::str::from_utf8(&name).map_err(|_| rt::BAD_ARGUMENT)?,
        )?;
        if detail {
            console::write(
                tty::OUTPUT,
                &alloc::format!(
                    "  id={} taille={}",
                    w::word(&stat, w::PAYLOAD),
                    w::word(&stat, w::PAYLOAD + 16)
                ),
            )?;
        }
        console::write(tty::OUTPUT, "\n")?;
        previous = name;
    }
}
// Sûreté : entrée Rust de l'ELF ls, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(Program::Ls, |c, op| {
        list(c, matches!(op, Operand::Directory(_, true)))
    })
}
