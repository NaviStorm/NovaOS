#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod boot;
mod hardware;

use core::panic::PanicInfo;
use hardware::{framebuffer, halt, serial};

const BANNER: &str = concat!("NovaOS v", env!("CARGO_PKG_VERSION"));

// Sûreté : seul symbole d’entrée, désigné par ENTRY dans le script de liaison.
#[unsafe(no_mangle)]
pub extern "C" fn nova_start() -> ! {
    serial::init();
    serial::line(BANNER);
    match boot::framebuffer().and_then(|fb| framebuffer::show_banner(&fb, BANNER)) {
        Ok(()) => serial::line("Affichage prêt."),
        Err(reason) => serial::line(reason),
    }
    halt()
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    use core::fmt::Write;
    let _ = writeln!(serial::Com1, "Panique du noyau : {info}\r");
    halt()
}
