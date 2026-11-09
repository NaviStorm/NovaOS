pub mod framebuffer;
mod interrupts;
pub mod keyboard;
mod ports;
pub mod serial;

pub fn halt() -> ! {
    loop {
        // Sûreté : le noyau s’exécute au niveau 0. Les IRQ restent masquées
        // pendant l’arrêt définitif, y compris après une erreur matérielle.
        unsafe { core::arch::asm!("cli", "hlt", options(nomem, nostack)) };
    }
}
