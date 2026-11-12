#![no_std]
#![no_main]
extern crate alloc;
use nova_cli::{console, fileapi::Client, runtime as rt, tty};
mod aliases;
mod builtins;
mod capabilities;
mod executor;
mod glob;
mod history;
mod input;
mod lexer;
mod line_editor;
mod parser;
mod paths;

fn session() -> rt::Result<()> {
    let mut fs = Client::new()?;
    let test = rt::config(1) == 42;
    let mark = |text: &[u8]| {
        if test {
            rt::call(rt::DEBUG_WRITE, text.as_ptr() as u64, text.len() as u64, 0);
        }
    };
    mark(b"ech debut");
    fs.mount()?;
    mark(b"ech montage-cli");
    if test && rt::call(tty::SPAWN, u64::MAX, 0, 0).status != rt::BAD_ARGUMENT {
        return Err(rt::REFUSED);
    }
    let mut aliases = aliases::Aliases::default();
    let mut history = history::History::configured()?;
    let mut input = input::Input::default();
    loop {
        if test {
            console::edit(2)?;
        }
        let command = line_editor::line(&mut input, &history)
            .and_then(|s| {
                history.remember(&s);
                lexer::lex(&s)
            })
            .map(parser::parse);
        match command {
            Ok(Some(cmd)) => match executor::execute(&mut fs, cmd, test, &mut aliases) {
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
