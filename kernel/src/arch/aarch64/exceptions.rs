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
.rept 4
    b 1f
    .balign 128
    b 2f
    .balign 128
    b 2f
    .balign 128
    b 2f
    .balign 128
.endr
1:  mov x4, #1
    b 3f
2:  mov x4, #0
3:  adrp x5, exception_stack_end
    add x5, x5, :lo12:exception_stack_end
    mov sp, x5
    mrs x0, esr_el1
    mrs x1, far_el1
    mrs x2, elr_el1
    mrs x3, spsr_el1
    bl {handler}
.popsection
.pushsection .bss.exception_stack,"aw",%nobits
.balign 16
.skip 16384
exception_stack_end:
.popsection
"#, handler = sym fault);

pub fn install() {
    // Sûreté : la cible virt démarre à EL1, DAIF masqué, la table est permanente.
    unsafe {
        asm!("adrp x9, nova_vectors", "add x9, x9, :lo12:nova_vectors",
        "msr vbar_el1, x9", "isb", out("x9") _, options(nostack))
    };
}

extern "C" fn fault(esr: u64, address: u64, pc: u64, state: u64, sync: u64) -> ! {
    let ec = (esr >> 26) & 63;
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
