use crate::hal;
use core::arch::{asm, global_asm};

// Sûreté : VBAR pointe vers 16 entrées de 128 octets, alignées sur 2 Kio.
// Chaque entrée fatale établit SP avant tout accès à la pile. Les registres
// syndrome, adresse, PC et état d’origine sont capturés avant l’appel Rust.
global_asm!(r#"
.pushsection .text.exceptions,"ax"
.balign 2048
.global nova_vectors
.hidden nova_vectors
nova_vectors:
.set group,0
.rept 4
    .if group == 2
        b nova_sync_el0
    .else
        b 1f
    .endif
    .balign 128
    .if (group == 1) || (group == 2)
        b nova_irq_el1h
    .else
        b 2f
    .endif
    .balign 128
    b 2f
    .balign 128
    b 2f
    .balign 128
.set group,group+1
.endr
1:  mov x4, #1
    b 3f
2:  mov x4, #0
3:  mrs x5,tpidr_el1
    mov sp, x5
    mrs x0, esr_el1
    mrs x1, far_el1
    mrs x2, elr_el1
    mrs x3, spsr_el1
    bl {handler}
// EL1h : SP_EL1 appartient au thread. Sauver ELR/SPSR avant tout autre
// thread, qui pourra lui aussi recevoir des exceptions. Pas de FP/SIMD.
.macro save_all
    sub sp,sp,#288
    stp x0,x1,[sp,#0]
    stp x2,x3,[sp,#16]
    stp x4,x5,[sp,#32]
    stp x6,x7,[sp,#48]
    stp x8,x9,[sp,#64]
    stp x10,x11,[sp,#80]
    stp x12,x13,[sp,#96]
    stp x14,x15,[sp,#112]
    stp x16,x17,[sp,#128]
    stp x18,x19,[sp,#144]
    stp x20,x21,[sp,#160]
    stp x22,x23,[sp,#176]
    stp x24,x25,[sp,#192]
    stp x26,x27,[sp,#208]
    stp x28,x29,[sp,#224]
    mrs x0,elr_el1
    mrs x1,spsr_el1
    stp x30,x0,[sp,#240]
    str x1,[sp,#256]
    mrs x0,sp_el0
    mrs x2,tpidr_el0
    stp x0,x2,[sp,#264]
.endm
nova_sync_el0:
    save_all
    mov x0,sp
    mrs x1,esr_el1
    mrs x2,far_el1
    bl {user_handler}
    b restore_all
nova_irq_el1h:
    save_all
    mov x0,x1
    bl {irq}
restore_all:
    ldp x0,x2,[sp,#264]
    msr sp_el0,x0
    msr tpidr_el0,x2
    ldr x1,[sp,#256]
    ldp x30,x0,[sp,#240]
    msr elr_el1,x0
    msr spsr_el1,x1
    ldp x28,x29,[sp,#224]
    ldp x26,x27,[sp,#208]
    ldp x24,x25,[sp,#192]
    ldp x22,x23,[sp,#176]
    ldp x20,x21,[sp,#160]
    ldp x18,x19,[sp,#144]
    ldp x16,x17,[sp,#128]
    ldp x14,x15,[sp,#112]
    ldp x12,x13,[sp,#96]
    ldp x10,x11,[sp,#80]
    ldp x8,x9,[sp,#64]
    ldp x6,x7,[sp,#48]
    ldp x4,x5,[sp,#32]
    ldp x2,x3,[sp,#16]
    ldp x0,x1,[sp,#0]
    add sp,sp,#288
    eret
.popsection
.pushsection .bss.exception_stack,"aw",%nobits
.balign 16
.skip 16384
.global exception_stack_end
.hidden exception_stack_end
exception_stack_end:
.popsection
"#, handler = sym fault, user_handler = sym user_fault, irq = sym super::timer::irq);

pub fn install() {
    unsafe extern "C" {
        static exception_stack_end: u8;
    }
    vectors((&raw const exception_stack_end) as usize);
}

pub fn vectors(top: usize) {
    // Sûreté : la cible virt démarre à EL1, DAIF masqué, la table est permanente.
    unsafe {
        asm!("msr tpidr_el1,{}",in(reg) top, options(nostack));
        asm!("adrp x9, nova_vectors", "add x9, x9, :lo12:nova_vectors",
        "msr vbar_el1, x9", "isb", out("x9") _, options(nostack))
    };
}

extern "C" fn fault(esr: u64, address: u64, pc: u64, state: u64, sync: u64) -> ! {
    let ec = (esr >> 26) & 63;
    if sync != 0 && ec == 7 {
        hal::log(format_args!("FPU/SIMD interdit : EC=7"));
        hal::cpu::halt();
    }
    let access = match ec {
        0x20 | 0x21 => "execution",
        0x24 | 0x25 if esr & 64 != 0 => "ecriture",
        0x24 | 0x25 => "lecture",
        _ => "inconnu",
    };
    let cause = match esr & 63 {
        4..=7 => "absente",
        12..=15 => "protection",
        _ => "autre",
    };
    hal::log(format_args!(
        "ABORT adresse={address:#x} pc={pc:#x} esr={esr:#x} ec={ec:#x} spsr={state:#x} acces={access} cause={cause} synchrone={sync}"
    ));
    hal::cpu::halt()
}

#[repr(C)]
struct TrapFrame {
    x: [u64; 31],
    pc: u64,
    state: u64,
    sp: u64,
    tls: u64,
}

// Les huit derniers octets d’alignement de la pile ne sont jamais empruntés.
const _: () = assert!(size_of::<TrapFrame>() == 280 && core::mem::offset_of!(TrapFrame, pc) == 248);

extern "C" fn user_fault(f: &mut TrapFrame, esr: u64, address: u64) {
    use crate::sched::process::{Fault, Outcome, finish, syscall};
    let (pc, state) = (f.pc, f.state);
    assert_eq!(state & 31, 0, "origine EL0t");
    let ec = esr >> 26;
    if ec == 0x15 && esr & 0xffff == 0x4e {
        let r = syscall::call(f.x[8], [f.x[0], f.x[1], f.x[2]], pc, f.sp, "EL0");
        f.x[0] = r.status;
        f.x[1] = r.value;
        f.state = (state & 0xf0000000) | 0x340; // NZCV, EL0t, IRQ permises.
        return;
    }
    let (trap, access, cause, va) = match ec {
        0x20 | 0x24 => {
            let access = if ec == 0x20 {
                "execution"
            } else if esr & 64 != 0 {
                "ecriture"
            } else {
                "lecture"
            };
            let cause = match esr & 63 {
                0..=3 => "adresse",
                4..=7 => "absente",
                8..=11 => "acces",
                12..=15 => "protection",
                33 => "alignement",
                // Erreur externe synchrone ou corruption lors du parcours matériel.
                _ => fault(esr, address, pc, state, 1),
            };
            ("ABORT", access, cause, address)
        }
        0x15 => ("SVC", "instruction", "invalide", 0),
        7 => ("FP", "instruction", "fpu", 0),
        0 | 0x18 => ("UNDEFINED", "instruction", "privilege", 0),
        _ => ("exception", "instruction", "exception", 0),
    };
    finish(
        Outcome::Fault(Fault {
            trap,
            access,
            cause,
            address: va,
            syndrome: esr,
        }),
        pc,
        "EL0",
    );
}
