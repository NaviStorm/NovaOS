#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod boot;
mod console;
mod hardware;
mod input;

use core::panic::PanicInfo;
use hardware::{framebuffer, halt, keyboard::Keyboard, serial};

const BANNER: &str = concat!("NovaOS v", env!("CARGO_PKG_VERSION"));

// Sûreté : seul symbole d’entrée, désigné par ENTRY dans le script de liaison.
#[unsafe(no_mangle)]
pub extern "C" fn nova_start() -> ! {
    serial::init();
    serial::line(BANNER);
    let Err(reason) = interactive();
    serial::line(reason);
    halt()
}

fn interactive() -> Result<core::convert::Infallible, &'static str> {
    let (screen, bottom) = framebuffer::show_banner(&boot::framebuffer()?, BANNER)?;
    serial::line("Affichage prêt.");
    let mut console = console::Console::new(screen, bottom)?;
    let mut keyboard = Keyboard::init()?;
    serial::line("Clavier PS/2 prêt (AZERTY).");
    loop {
        let _line = input::read_line(&mut keyboard, &mut console);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    use core::fmt::Write;
    let _ = writeln!(serial::Com1, "Panique du noyau : {info}\r");
    halt()
}
