use core::arch::{asm, naked_asm};

pub const fn irq_enabled(flags: usize) -> bool {
    flags & 0x200 != 0
}
pub fn enable() {
    // Sûreté : IDT, PML4 et contrôleurs en place avant le premier thread.
    unsafe { asm!("sti", options(nostack)) };
}
pub fn prepare() {
    let cr0: usize;
    // Sûreté : anneau 0. TS interdit x87/MMX/SSE aux threads soft-float (#NM).
    unsafe {
        asm!("mov {}, cr0",out(reg) cr0);
        asm!("mov cr0, {}",in(reg) cr0 | 8);
    }
}
pub fn sp() -> usize {
    let value;
    // Sûreté : lecture du pointeur de pile local, sans le modifier.
    unsafe { asm!("mov {}, rsp",out(reg) value,options(nomem,nostack,preserves_flags)) };
    value
}
pub unsafe fn initial(top: usize, entry: extern "C" fn() -> !) -> usize {
    let base = top - 64;
    // Sûreté : tranche privée de 64 Kio, alignée et initialement nulle.
    // Six registres, adresse de retour puis emplacement factice : RSP%16=8 à l’entrée.
    unsafe {
        ((top - 16) as *mut usize).write(entry as usize);
    }
    base
}
// Sûreté : ABI SysV, IRQ masquées. Les registres volatils du code interrompu
// sont déjà dans le cadre IRQ. Le slot de sortie est stable jusqu’au changement de RSP.
#[unsafe(naked)]
pub unsafe extern "C" fn switch(_: *mut usize, _: usize) {
    naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov [rdi],rsp",
        "mov rsp,rsi",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret"
    );
}

// Sûreté : fonctions de test ABI C, pile et registres non volatils restaurés.
core::arch::global_asm!(include_str!("register_probe.S"));

pub unsafe fn overflow(low: usize) -> ! {
    // Sûreté : sonde terminale d’une VM dédiée ; #PF dispose de sa propre IST.
    unsafe {
        asm!("mov rsp,{}","push rax","ud2",in(reg) low,options(noreturn));
    }
}
pub unsafe fn forbidden_fp() {
    // Sûreté : sonde terminale destinée à déclencher #NM avec CR0.TS=1.
    unsafe {
        asm!("fld1", options(nostack));
    }
}

pub fn idle_wait() {
    // Sûreté : IRQ masquées et file vide. L’ombre de STI rend le contrôle
    // du prédicat et la veille atomiques face à une IRQ déjà pendante.
    unsafe {
        asm!("sti", "hlt", "cli", options(nostack));
    }
}
