#![no_std]
#![no_main]
extern crate alloc;
use nova_cli::{console, fileapi::Client, runtime as rt, tty};
mod builtins;
mod capabilities;
mod executor;
mod input;
mod lexer;
mod line_editor;
mod parser;

fn session() -> rt::Result<()> {
    let mut fs = Client::new()?;
    fs.mount()?;
    let test = rt::config(1) == 42;
    if test && rt::call(tty::SPAWN, u64::MAX, 0, 0).status != rt::BAD_ARGUMENT {
        return Err(rt::REFUSED);
    }
    let mut input = input::Input::default();
    loop {
        if test {
            console::edit(2)?;
        }
        let command = line_editor::line(&mut input)
            .and_then(|s| lexer::lex(&s))
            .map(parser::parse);
        match command {
            Ok(Some(cmd)) => match executor::execute(&mut fs, cmd, test) {
                Ok(false) => break,
                Ok(true) => {}
                Err(e) => console::write(
                    tty::ERROR,
                    &alloc::format!("erreur : {}\n", console::reason(e)),
                )?,
            },
            Ok(None) => {}
            Err(e) => console::write(
                tty::ERROR,
                &alloc::format!("saisie refusée : {}\n", console::reason(e)),
            )?,
        }
    }
    fs.close(0)
}
// Sûreté : appelé exclusivement par le point d'entrée de cet ELF avec sa pile privée.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(session().err().unwrap_or(0))
}
