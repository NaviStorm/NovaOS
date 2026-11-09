#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod arch;
mod boot;
mod console;
pub mod hal;
mod hardware;
mod heap;
mod input;
mod memory;

use core::panic::PanicInfo;
use hal::log;
use hardware::framebuffer;

const BANNER: &str = concat!("NovaOS v", env!("CARGO_PKG_VERSION"));

// Sûreté : seul symbole d’entrée, désigné par ENTRY dans le script de liaison.
#[unsafe(no_mangle)]
pub extern "C" fn nova_start() -> ! {
    hal::init();
    let info = boot::memory().unwrap_or_else(|reason| panic!("{reason}"));
    // Sûreté : unique appel au point d’entrée Limine, sans AP ni allocateur antérieur.
    let mut memory = unsafe { memory::Memory::init(info) }
        .unwrap_or_else(|error| panic!("Initialisation mémoire : {error:?}"));
    log(format_args!("{BANNER}"));
    let scenario = boot::scenario();
    if !scenario.starts_with(b"heap-") {
        memory
            .test(scenario)
            .unwrap_or_else(|error| panic!("Autotest mémoire : {error:?}"));
    }
    heap::install(memory);
    if scenario.starts_with(b"heap-") {
        heap::test(scenario);
    }
    let Err(reason) = interactive();
    log(format_args!("{reason}"));
    hal::cpu::halt()
}

fn interactive() -> Result<core::convert::Infallible, &'static str> {
    let (screen, bottom) = framebuffer::show_banner(&boot::framebuffer()?, BANNER)?;
    log(format_args!("Affichage prêt."));
    let mut console = console::Console::new(screen, bottom)?;
    let Some(mut next_char) = hal::keyboard()? else {
        hal::cpu::halt();
    };
    loop {
        let _line = input::read_line(&mut next_char, &mut console)?;
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    log(format_args!("Panique du noyau : {info}"));
    hal::cpu::halt()
}
