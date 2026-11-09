mod interrupts;
mod keyboard;
pub(crate) mod paging;
mod ports;
mod serial;

use core::{arch::asm, fmt};

pub(crate) const DIAGNOSTIC: Option<fn(fmt::Arguments<'_>)> = Some(serial::line);
pub(crate) fn init() {
    serial::init();
    interrupts::install();
}

pub(crate) fn keyboard() -> Result<Option<impl FnMut() -> char>, &'static str> {
    let mut keyboard = keyboard::Keyboard::init()?;
    serial::line(format_args!("Clavier PS/2 prêt (AZERTY)."));
    Ok(Some(move || keyboard.next_char()))
}

pub(crate) fn mask_interrupts() {
    // Sûreté : le noyau est en anneau 0 sur le processeur courant.
    unsafe { asm!("cli", options(nostack)) };
}

pub(crate) fn wait() {
    // Sûreté : hlt est appelé en anneau 0. L’appelant gère le masque des IRQ.
    unsafe { asm!("hlt", options(nostack)) };
}
