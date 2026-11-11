#![no_std]
#![no_main]
extern crate alloc;
mod common;
// Sûreté : entrée Rust de l’ELF rm, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(nova_cli::command::Program::Rm, |c, _| {
        c.simple(nova_cli::protocol::UNLINK_AT, 0).map(|_| ())
    })
}
