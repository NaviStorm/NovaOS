#![no_std]
#![no_main]
extern crate alloc;
mod common;
use nova_cli::{command::Program, console, fileapi::Client, runtime as rt, tty};
fn pwd(c: &mut Client) -> rt::Result<()> {
    console::printable(tty::OUTPUT, &nova_cli::directory::cwd(c)?)?;
    console::write(tty::OUTPUT, "\n")
}
// Sûreté : entrée Rust de l'ELF pwd, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(Program::Pwd, |c, _| pwd(c))
}
