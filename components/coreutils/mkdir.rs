#![no_std]
#![no_main]
extern crate alloc;
mod common;
// Sûreté : entrée Rust de l’ELF mkdir, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(nova_cli::command::Program::Mkdir, |c, _| {
        c.simple(nova_cli::protocol::MKDIR_AT, 0).map(|_| ())
    })
}
