#![no_std]
#![no_main]
extern crate alloc;
mod common;
mod removal;
// Sûreté : entrée de cet ELF utilisateur, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(nova_cli::command::Program::Rmdir, removal::command)
}
