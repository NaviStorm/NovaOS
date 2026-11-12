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
mod script;
mod vi;

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
    let mut frame = script::Frame::default();
    let mut history = history::History::configured()?;
    let mut input = input::Input::default();
    let exit = loop {
        if test {
            console::edit(2)?;
        }
        let input_line = line_editor::line(&mut input, &history, "nsh> ");
        let result = (|| {
            let mut source = input_line?;
            history.remember(&source);
            let program = loop {
                match parser::parse(&source) {
                    Ok(p) => break p,
                    Err(parser::Error::Incomplete) => {
                        let more = line_editor::line(&mut input, &history, "...> ")?;
                        if source.len() + 1 + more.len() > lexer::SOURCE_MAX {
                            return Err(rt::LIMIT);
                        }
                        source.push('\n');
                        source.push_str(&more);
                    }
                    Err(e) => return Err(e.status()),
                }
            };
            let result = script::Engine::new(&mut fs, test)?.list(&mut frame, &program, 0, 0);
            if let Err(code) = result {
                frame.status = code;
                console::write(
                    tty::ERROR,
                    &alloc::format!("erreur : {}\n", console::reason(code)),
                )?;
                return Ok(script::Flow::Status(code));
            }
            result
        })();
        match result {
            Ok(script::Flow::Exit(n)) => break n,
            Ok(_) => (),
            Err(e) => {
                frame.status = e;
                console::write(
                    tty::ERROR,
                    &alloc::format!("saisie refusée : {}\n", console::reason(e)),
                )?;
            }
        }
    };
    fs.close(0)?;
    if exit != 0 {
        return Err(exit);
    }
    Ok(())
}
// Sûreté : appelé exclusivement par le point d'entrée de cet ELF avec sa pile privée.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(session().err().unwrap_or(0))
}
