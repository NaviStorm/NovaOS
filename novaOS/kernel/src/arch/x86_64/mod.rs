pub(crate) mod context;
mod interrupts;
mod keyboard;
mod lapic;
pub(crate) mod paging;
mod ports;
pub(crate) mod rtc;
pub(crate) mod serial;
pub(crate) mod timer;

use core::{arch::asm, fmt};

/// La VGA standard de Q35 possède sa propre mémoire vidéo.
pub(crate) const VIDEO_SYSTEM_RAM: bool = false;

pub(crate) const DIAGNOSTIC: Option<fn(fmt::Arguments<'_>)> = Some(serial::line);
pub(crate) fn init() {
    serial::init();
    interrupts::install();
}

pub(crate) fn keyboard() -> Result<Option<impl FnMut() -> char>, &'static str> {
    let mut keyboard = keyboard::Keyboard::init()?;
    crate::hal::log(format_args!("Clavier PS/2 prêt (AZERTY)."));
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

pub(crate) fn save_and_mask_interrupts() -> usize {
    let flags: usize;
    // Sûreté : CPL0, pile noyau valide ; pushfq/pop sont appariés.
    unsafe { asm!("pushfq", "pop {}", "cli", out(reg) flags) };
    flags
}

pub(crate) fn restore_interrupts(flags: usize) {
    if flags & (1 << 9) != 0 {
        // Sûreté : le verrou est libéré et IF était actif avant la section.
        unsafe { asm!("sti", options(nostack)) };
    }
}

/// Identifiant APIC initial sur la machine Q35 configurée par NovaOS.
pub(crate) fn cpu_id() -> usize {
    use core::arch::x86_64::{__cpuid, __cpuid_count};
    use core::sync::atomic::{AtomicU32, Ordering};
    static LEAF: AtomicU32 = AtomicU32::new(0);
    let mut leaf = LEAF.load(Ordering::Relaxed);
    if leaf == 0 {
        let max = __cpuid(0).eax;
        leaf = [0x1f, 0xb]
            .into_iter()
            .find(|&v| v <= max && __cpuid_count(v, 0).ebx & 0xffff != 0)
            .unwrap_or(1);
        LEAF.store(leaf, Ordering::Relaxed);
    }
    let info = __cpuid_count(leaf, 0);
    if leaf == 1 {
        (info.ebx >> 24) as usize
    } else {
        info.edx as usize
    }
}

pub(crate) fn init_secondary(cpu: usize) {
    interrupts::secondary(cpu);
    context::prepare();
}

pub(crate) fn prepare_secondaries() {
    interrupts::prepare_secondaries();
    lapic::prepare();
}
pub(crate) use lapic::{kick, kick_one, poll_translation, stop_budget, stop_ipi};

#[cfg(feature = "user-fpu")]
pub(crate) mod fpu;
