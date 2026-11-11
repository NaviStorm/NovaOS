#![no_std]
#![no_main]
extern crate alloc;
mod common;
use nova_cli::{command::Program, console, fileapi::Client, protocol as w, runtime as rt, tty};
fn pwd(c: &mut Client) -> rt::Result<()> {
    let mut buf = alloc::vec::Vec::new();
    let mut generation = 0;
    loop {
        let a = c.cwd_fragment(buf.len() as u64, generation)?;
        generation = w::word(&a, 24);
        let n = w::length(&a);
        let total = w::word(&a, 32);
        if total > 8192 || n > w::FRAGMENT || buf.len() as u64 + n as u64 > total {
            return Err(rt::REFUSED);
        }
        buf.extend_from_slice(&a[w::PAYLOAD..w::PAYLOAD + n]);
        if buf.len() as u64 == total {
            break;
        }
        if n == 0 {
            return Err(rt::REFUSED);
        }
    }
    console::printable(
        tty::OUTPUT,
        core::str::from_utf8(&buf).map_err(|_| rt::BAD_ARGUMENT)?,
    )?;
    console::write(tty::OUTPUT, "\n")
}
// Sûreté : entrée Rust de l'ELF pwd, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(Program::Pwd, |c, _| pwd(c))
}
