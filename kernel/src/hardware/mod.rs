pub mod framebuffer;
pub mod serial;

pub fn halt() -> ! {
    loop {
        // Sûreté : le noyau s’exécute au niveau 0. Les IRQ restent masquées
        // en l’absence d’IDT.
        unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)) };
    }
}
