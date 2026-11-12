#![no_std]
#![no_main]
extern crate alloc;
mod buffer;
mod editor;
mod terminal;

use alloc::{string::String, vec::Vec};
use nova_cli::{
    command::{Operand, Program, operand},
    console, process, runtime as rt, tty, vi_wire as wire,
};

impl editor::Storage for wire::Link {
    fn publish(&mut self, name: &str, text: &[u8]) -> rt::Result<()> {
        self.ask(wire::BEGIN, &[])?;
        for part in name.as_bytes().chunks(wire::CHUNK) {
            self.ask(wire::NAME, part)?;
        }
        for part in text.chunks(wire::CHUNK) {
            self.ask(wire::TEXT, part)?;
        }
        let [a, b] = (name.len() as u16).to_le_bytes();
        let [c, d] = (text.len() as u16).to_le_bytes();
        self.ask(wire::SAVE, &[a, b, c, d])?;
        Ok(())
    }
}
fn session() -> rt::Result<()> {
    let args = process::arguments()?;
    let op = operand(Program::Vi, &args)?;
    if let Operand::Info(help) = op {
        console::write(
            tty::OUTPUT,
            if help {
                nova_cli::options::usage(Program::Vi)
            } else {
                concat!("NovaOS ", env!("CARGO_PKG_VERSION"))
            },
        )?;
        return console::write(tty::OUTPUT, "\n");
    }
    let Operand::Edit(name) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    if rt::config(3) != 0 {
        return Err(rt::config(3));
    }
    let mut link = wire::Link::new()?;
    let outcome = (|| {
        let r = link.ask(wire::INFO, &[])?;
        if r.len != 2 || r.payload[1] > 1 {
            return Err(rt::BAD_ARGUMENT);
        }
        let exists = r.payload[1] == 1;
        let mut content = Vec::new();
        loop {
            let r = link.ask(wire::READ, &(content.len() as u16).to_le_bytes())?;
            let data = &r.payload[1..r.len as usize];
            if data.is_empty() {
                break;
            }
            if content.len() + data.len() > wire::BYTES {
                return Err(rt::LIMIT);
            }
            content.try_reserve(data.len()).map_err(|_| rt::NO_MEMORY)?;
            content.extend_from_slice(data);
        }
        let text = String::from_utf8(content).map_err(|_| rt::BAD_ARGUMENT)?;
        let mut editor = editor::Editor::new(name, &text, exists).map_err(|msg| {
            let _ = console::write(tty::ERROR, &alloc::format!("{msg}\n"));
            if msg == "limite du tampon" {
                rt::LIMIT
            } else {
                rt::BAD_ARGUMENT
            }
        })?;
        drop(text);
        terminal::edit(&mut editor, &mut link)
    })();
    let finish = link.ask(wire::FIN, &[]).map(|_| ());
    outcome.and(finish)
}
// Sûreté : _start appelle ce point d'entrée après création de la pile privée.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let r = session();
    if let Err(e) = r {
        let _ = console::write(tty::ERROR, &alloc::format!("vi : {}\n", console::reason(e)));
    }
    rt::exit(r.err().unwrap_or(0))
}
