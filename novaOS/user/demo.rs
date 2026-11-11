#![no_std]
#![no_main]

#[path = "../abi/native.rs"]
mod abi;
mod calls;
mod gate;

// Sûreté : point d’entrée autonome, sans référence au noyau. Les instructions
// de sonde sont exécutées uniquement dans l’espace utilisateur de la VM.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(include_str!("x86_64.S"));
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(include_str!("aarch64.S"));

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
