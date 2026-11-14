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
        "bic x9,x9,#(3<<20)","msr cpacr_el1,x9","msr cntkctl_el1,xzr","mrs x9,sctlr_el1",
        "bic x9,x9,#(1<<9)","bic x9,x9,#(1<<26)","msr sctlr_el1,x9","isb",out("x9") _);
    }
    #[cfg(feature = "user-fpu")]
    super::fpu::prepare();
    if pan_supported() {
        // Sûreté : FEAT_PAN vérifiée, SPAN=0 force PAN à toute entrée à EL1.
        unsafe {
            asm!("mrs x9,sctlr_el1","bic x9,x9,#(1<<23)","msr sctlr_el1,x9","isb",
            ".arch_extension pan","msr pan,#1",out("x9") _,options(nostack))
        };
    }
}
pub fn pan_supported() -> bool {
    let features: u64;
    // Sûreté : registre d’identification lisible à EL1 sur la cible ARMv8-A.
    unsafe { asm!("mrs {},id_aa64mmfr1_el1",out(reg) features,options(nostack)) };
    (features >> 20) & 15 != 0
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
pub unsafe extern "C" fn switch(
    _: *mut usize,
    _: usize,
    _: *const core::sync::atomic::AtomicUsize,
) {
    naked_asm!(
        "stp x19,x20,[sp,#-96]!",
        "stp x21,x22,[sp,#16]",
        "stp x23,x24,[sp,#32]",
        "stp x25,x26,[sp,#48]",
        "stp x27,x28,[sp,#64]",
        "stp x29,x30,[sp,#80]",
        "mov x3,sp",
        "str x3,[x0]",
        "mov sp,x1",
        "cbz x2,2f",
        "stlr xzr,[x2]",
        "2:",
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

// La commutation suivante restaure SP_EL1 depuis le TCB de destination.
pub fn kernel_stack(_: usize) {}

// Sûreté : EL1h, racine TTBR0 privée active et interruptions masquées. ERET
// reçoit un SPSR EL0t fixe avec IRQ permises. Aucun registre général ni TLS
// du contexte noyau n’est observable à la première instruction.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_user(_: usize, _: usize, _: usize) -> ! {
    naked_asm!(
        "mov sp,x0",
        "msr elr_el1,x1",
        "msr sp_el0,x2",
        "mov x0,#0x340",
        "msr spsr_el1,x0",
        "msr tpidr_el0,xzr",
        "msr tpidrro_el0,xzr",
        ".irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30",
        "mov x\\n,xzr",
        ".endr",
        "eret"
    );
}

pub fn report_syscalls() {
    let mut active = 0u64;
    if pan_supported() {
        // Sûreté : FEAT_PAN vérifiée avant lecture du PSTATE privilégié.
        unsafe { asm!(".arch_extension pan","mrs {},pan",out(reg) active,options(nostack)) };
    }
    crate::hal::log(format_args!("ABI ERET PAN={}", (active >> 22) & 1));
}
