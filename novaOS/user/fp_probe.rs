//! Image utilisateur de calcul, assemblée selon l'architecture du noyau testé.
#![no_std]
#![no_main]
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(include_str!("fp_aarch64.S"));
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(include_str!("fp_x86_64.S"));
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
