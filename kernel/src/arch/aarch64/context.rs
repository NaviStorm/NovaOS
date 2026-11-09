use core::arch::{asm, naked_asm};
pub const fn irq_enabled(flags: usize) -> bool {
    flags & (1 << 7) == 0
}
pub fn enable() {
    // Sûreté : EL1, vecteurs et GIC configurés ; seule la classe IRQ est démasquée.
    unsafe { asm!("msr daifclr,#2", "isb", options(nostack, preserves_flags)) };
}
pub fn prepare() {
    // Sûreté : migration de la pile Limine en EL1h avant activation des IRQ.
    // SP garde exactement sa valeur. FPEN=0 interdit FP/Advanced SIMD à EL1.
    unsafe {
        asm!("mov x9,sp","msr spsel,#1","mov sp,x9","mrs x9,cpacr_el1",
        "bic x9,x9,#(3<<20)","msr cpacr_el1,x9","isb",out("x9") _);
    }
}
pub fn sp() -> usize {
    let value;
    // Sûreté : lecture du SP courant.
    unsafe { asm!("mov {},sp",out(reg) value,options(nomem,nostack,preserves_flags)) };
    value
}
pub unsafe fn initial(top: usize, entry: extern "C" fn() -> !) -> usize {
    // Sûreté : cadre AAPCS64 de x19..x30 dans la pile exclusive déjà remise à zéro.
    unsafe {
        ((top - 8) as *mut usize).write(entry as usize);
    }
    top - 96
}
// Sûreté : EL1h, IRQ masquées, cadres alignés sur 16 octets. Un contexte IRQ
// contient en plus tous les registres volatils, ELR et SPSR dans le stub de vecteur.
#[unsafe(naked)]
pub unsafe extern "C" fn switch(_: *mut usize, _: usize) {
    naked_asm!(
        "stp x19,x20,[sp,#-96]!",
        "stp x21,x22,[sp,#16]",
        "stp x23,x24,[sp,#32]",
        "stp x25,x26,[sp,#48]",
        "stp x27,x28,[sp,#64]",
        "stp x29,x30,[sp,#80]",
        "mov x2,sp",
        "str x2,[x0]",
        "mov sp,x1",
        "ldp x21,x22,[sp,#16]",
        "ldp x23,x24,[sp,#32]",
        "ldp x25,x26,[sp,#48]",
        "ldp x27,x28,[sp,#64]",
        "ldp x29,x30,[sp,#80]",
        "ldp x19,x20,[sp],#96",
        "ret"
    );
}

// Sûreté : fonctions de test AAPCS64, pile et registres non volatils restaurés.
core::arch::global_asm!(include_str!("register_probe.S"));

pub unsafe fn overflow(low: usize) -> ! {
    // Sûreté : sonde terminale. La branche synchrone de VBAR prend sa pile
    // d’urgence avant toute sauvegarde, même avec SP dans la garde.
    unsafe {
        asm!("mov sp,{}","str xzr,[sp,#-8]!","brk #0",in(reg) low,options(noreturn));
    }
}
pub unsafe fn forbidden_fp() {
    // Sûreté : sonde terminale de MOVI V0.16B,#0 alors que FPEN=0.
    unsafe {
        asm!(".inst 0x4f00e400", options(nostack));
    }
}

pub fn idle_wait() {
    // WFI peut se réveiller pour une IRQ GIC pendante même avec DAIF.I=1.
    // La garde appelante restaure le masque et permet sa livraison au retour.
    super::wait();
}
