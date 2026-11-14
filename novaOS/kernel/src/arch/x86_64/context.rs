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
    let cr4: usize;
    let features = core::arch::x86_64::__cpuid_count(7, 0);
    // Sûreté : anneau 0. TS interdit x87/MMX/SSE aux threads soft-float (#NM).
    unsafe {
        asm!("mov {}, cr0",out(reg) cr0);
        asm!("mov cr0, {}",in(reg) cr0 | 8);
        asm!("wrmsr", in("ecx") 0x174u32, in("eax") 0u32, in("edx") 0u32);
        asm!("mov {},cr4",out(reg) cr4);
        let mut flags = cr4 & !((1 << 16) | (1 << 22) | (1 << 8)); // FSGSBASE, PKE, PCE
        if features.ebx & (1 << 7) != 0 {
            flags |= 1 << 20;
        } // SMEP
        if features.ebx & (1 << 20) != 0 {
            flags |= 1 << 21;
        } // SMAP
        if features.ecx & 4 != 0 {
            flags |= 1 << 11;
        } // UMIP
        asm!("mov cr4,{}",in(reg) flags);
    }
    super::interrupts::enable_syscall();
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
pub unsafe extern "C" fn switch(
    _: *mut usize,
    _: usize,
    _: *const core::sync::atomic::AtomicUsize,
) {
    naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov [rdi],rsp",
        "mov rsp,rsi",
        "test rdx,rdx",
        "jz 2f",
        "mov qword ptr [rdx],0",
        "2:",
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

pub fn kernel_stack(top: usize) {
    super::interrupts::set_rsp0(top);
}

// Sûreté : entrée sans retour sous masque IRQ. Le TSS désigne cette même pile
// superviseur. Le cadre iretq fixe CPL3, IF=1 et IOPL=0. Aucun GPR du noyau
// n’est transmis. Les sélecteurs utilisateur désignent des segments de base zéro.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_user(_: usize, _: usize, _: usize) -> ! {
    naked_asm!(
        "mov rsp,rdi",
        "push 0x4b",
        "push rdx",
        "push 0x202",
        "push 0x53",
        "push rsi",
        "mov ax,0x4b",
        "mov ds,ax",
        "mov es,ax",
        "mov fs,ax",
        "mov gs,ax",
        ".irp r,rax,rbx,rcx,rdx,rdi,rsi,rbp,r8,r9,r10,r11,r12,r13,r14,r15",
        "xor \\r,\\r",
        ".endr",
        "iretq"
    );
}

pub fn report_syscalls() {
    let (fast, full) = super::interrupts::syscall_paths();
    assert!(fast > 0 && full > 0);
    let cr4: u64;
    // Sûreté : lecture d’état CR4 à CPL0 pour le diagnostic.
    unsafe { asm!("mov {},cr4",out(reg) cr4,options(nostack)) };
    crate::hal::log(format_args!(
        "ABI SYSRET={fast} IRETQ={full} SMAP={}",
        (cr4 >> 21) & 1
    ));
}
