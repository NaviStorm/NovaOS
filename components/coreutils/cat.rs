#![no_std]
#![no_main]
extern crate alloc;
mod common;
mod text;
// Sûreté : entrée Rust de l’ELF cat, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(nova_cli::command::Program::Cat, |c, op| {
        if let nova_cli::command::Operand::Read(_, lines) = op {
            text::display(c, lines)
        } else {
            Err(nova_cli::runtime::BAD_ARGUMENT)
        }
    })
}
