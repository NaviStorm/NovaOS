//! Amorçage de la machine QEMU virt sous UEFI/Limine.
pub(crate) mod context;
mod exceptions;
pub(crate) mod paging;
pub(crate) mod serial;
pub(crate) mod timer;
use core::{arch::asm, fmt};

/// Le ramfb de virt affiche des pixels stockés en RAM système.
pub(crate) const VIDEO_SYSTEM_RAM: bool = true;

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

pub(crate) fn save_and_mask_interrupts() -> usize {
    let flags: usize;
    // Sûreté : opérations locales privilégiées sur DAIF à EL1.
    unsafe { asm!("mrs {}, daif", "msr daifset, #0xf", out(reg) flags, options(nostack)) };
    flags
}

pub(crate) fn restore_interrupts(flags: usize) {
    // Sûreté : image DAIF capturée sur ce CPU avant l’acquisition du verrou.
    unsafe { asm!("msr daif, {}", in(reg) flags, options(nostack)) };
}

pub(crate) fn cpu_id() -> usize {
    let mpidr: usize;
    // Sûreté : lecture seule de l’affinité du processeur courant à EL1.
    unsafe { asm!("mrs {}, mpidr_el1", out(reg) mpidr, options(nomem, nostack, preserves_flags)) };
    mpidr & 0xff_00ff_ffff
}
