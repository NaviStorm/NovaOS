#![no_std]
#![no_main]
extern crate alloc;
mod buffer;
mod editor;
mod gap;
mod history;
mod screen;
mod terminal;

use alloc::{string::String, vec::Vec};
use nova_cli::{
    command::{Operand, Program, operand},
    console, process, runtime as rt, tty, vi_wire as wire,
};

impl editor::Storage for wire::Link {
    fn publish(&mut self, name: &str, text: &[u8]) -> rt::Result<()> {
        let mut sizes = [0; 10];
        sizes[..2].copy_from_slice(
            &u16::try_from(name.len())
                .map_err(|_| rt::LIMIT)?
                .to_le_bytes(),
        );
        sizes[2..].copy_from_slice(&(text.len() as u64).to_le_bytes());
        self.ask(wire::BEGIN, &sizes)?;
        for part in name.as_bytes().chunks(wire::CHUNK) {
            self.ask(wire::NAME, part)?;
        }
        for part in text.chunks(wire::CHUNK) {
            self.ask(wire::TEXT, part)?;
        }
        self.ask(wire::SAVE, &[])?;
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
        if r.len != 10 || r.payload[1] > 1 {
            return Err(rt::BAD_ARGUMENT);
        }
        let exists = r.payload[1] == 1;
        let len = wire::length(&r.payload[2..10])?;
        let mut content = Vec::new();
        content.try_reserve_exact(len).map_err(|_| rt::NO_MEMORY)?;
        while content.len() < len {
            let r = link.ask(wire::READ, &(content.len() as u64).to_le_bytes())?;
            let data = &r.payload[1..r.len as usize];
            if data.is_empty() || data.len() > len - content.len() {
                return Err(rt::BAD_ARGUMENT);
            }
            content.extend_from_slice(data);
        }
        let text = String::from_utf8(content).map_err(|_| rt::BAD_ARGUMENT)?;
        let mut editor = editor::Editor::new(name, text, exists).map_err(|msg| {
            let _ = console::write(tty::ERROR, msg);
            let _ = console::write(tty::ERROR, "\n");
            if msg == "mémoire insuffisante" {
                rt::NO_MEMORY
            } else {
                rt::BAD_ARGUMENT
            }
        })?;
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
        let _ = console::write(tty::ERROR, "vi : ");
        let _ = console::write(tty::ERROR, console::reason(e));
        let _ = console::write(tty::ERROR, "\n");
    }
    rt::exit(r.err().unwrap_or(0))
}
