#![no_std]
#![no_main]
extern crate alloc;
mod common;
mod text;
// Sûreté : point d’entrée unique de cet ELF, appelé avec sa pile utilisateur.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(nova_cli::command::Program::Tail, text::command)
}
