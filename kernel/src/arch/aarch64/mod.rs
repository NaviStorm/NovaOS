//! Amorçage de la machine QEMU virt sous UEFI/Limine.
mod exceptions;
pub(crate) mod paging;
mod serial;
use core::{arch::asm, fmt};

pub(crate) const DIAGNOSTIC: Option<fn(fmt::Arguments<'_>)> = Some(serial::line);
pub(crate) fn init() {
    mask_interrupts();
    exceptions::install();
}

pub(crate) fn keyboard() -> Result<Option<fn() -> char>, &'static str> {
    Ok(None)
}

pub(crate) fn mask_interrupts() {
    // Sûreté : Limine transfère le contrôle à un niveau d’exception privilégié.
    // DAIFSet masque les exceptions asynchrones avant toute installation de vecteurs.
    unsafe { asm!("msr DAIFSet, #0xf", options(nostack, preserves_flags)) };
}

pub(crate) fn wait() {
    // Sûreté : WFI est exécuté par le noyau au niveau privilégié de démarrage.
    unsafe { asm!("wfi", options(nostack, preserves_flags)) };
}
