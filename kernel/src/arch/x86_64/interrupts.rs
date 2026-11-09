use super::{ports::Port, serial};
use crate::hal;
use core::{
    arch::{asm, naked_asm},
    mem::size_of,
};

#[derive(Clone, Copy)]
#[repr(C)]
struct Gate {
    low: u16,
    cs: u16,
    ist: u8,
    attributes: u8,
    middle: u16,
    high: u32,
    reserved: u32,
}

impl Gate {
    const fn new(address: usize, cs: u16) -> Self {
        Self {
            low: address as u16,
            cs,
            ist: 0,
            attributes: 0x8e,
            middle: (address >> 16) as u16,
            high: (address >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, align(16))]
struct Idt([Gate; 256]);

#[repr(C, packed)]
struct Descriptor {
    limit: u16,
    address: *const Idt,
}

static mut IDT: Idt = Idt([Gate::new(0, 0); 256]);

// Sûreté : entrée exclusive par une porte d’interruption x86_64. IRQ1 et les
// IRQ15 parasites attendent un EOI sur le PIC maître. Aucun registre ne change
// au retour et iretq restaure le cadre matériel, y compris RFLAGS.
#[unsafe(naked)]
extern "C" fn acknowledge_master() {
    naked_asm!(
        "push rax",
        "mov al, 0x20",
        "out 0x20, al",
        "pop rax",
        "iretq"
    );
}

// Sûreté : le PIC n’a pas acquitté l’IRQ7 parasite. Le cadre matériel est intact.
#[unsafe(naked)]
extern "C" fn spurious_master() {
    naked_asm!("iretq");
}

// Sûreté : cette porte fatale ne retourne jamais. Elle peut donc abandonner
// le cadre matériel (avec ou sans code d’erreur), aligner la pile et appeler Rust.
#[unsafe(naked)]
extern "C" fn fault_entry() {
    naked_asm!("cld", "and rsp, -16", "call {}", sym fatal);
}

extern "C" fn fatal() -> ! {
    serial::line(format_args!(
        "Erreur : interruption inattendue, arrêt du noyau."
    ));
    hal::cpu::halt()
}

pub(super) fn install() {
    let cs: u16;
    hal::interrupts::mask();
    // Sûreté : seul le processeur d’amorçage exécute l’initialisation. La GDT
    // de Limine reste mappée et son sélecteur de code reste valide.
    unsafe { asm!("mov {cs:x}, cs", cs = out(reg) cs, options(nostack, preserves_flags)) };
    let mut table = Idt([Gate::new(fault_entry as *const () as usize, cs); 256]);
    let master_eoi = Gate::new(acknowledge_master as *const () as usize, cs);
    table.0[14] = Gate::new(page_fault_entry as *const () as usize, cs);
    table.0[0x21] = master_eoi;
    table.0[0x27] = Gate::new(spurious_master as *const () as usize, cs);
    table.0[0x2f] = master_eoi;
    let descriptor = Descriptor {
        limit: (size_of::<Idt>() - 1) as u16,
        address: &raw const IDT,
    };
    // Sûreté : unique installation sur le BSP, IRQ masquées. Les 256 portes
    // sont écrites avant lidt. La table reste allouée et immuable ensuite.
    unsafe {
        (&raw mut IDT).write(table);
        asm!("lidt [{}]", in(reg) &descriptor, options(readonly, nostack, preserves_flags));
    }

    // ICW1..4 : deux PIC en cascade, vecteurs 0x20 et 0x28, mode 8086.
    use Port::*;
    PicCommand.write(0x11);
    SlaveCommand.write(0x11);
    PicMask.write(0x20);
    SlaveMask.write(0x28);
    PicMask.write(1 << 2);
    SlaveMask.write(2);
    PicMask.write(1);
    SlaveMask.write(1);
    PicMask.write(0xff);
    SlaveMask.write(0xff);
}

pub(super) fn allow_keyboard() {
    Port::PicMask.write(!(1 << 1));
}

pub(super) fn sleep_until_irq() {
    // Sûreté : l’IDT est installée, seule IRQ1 est démasquée. IF est nul
    // pendant la vérification du tampon. L’ombre d’interruption de sti garantit
    // que hlt s’exécute avant livraison d’une IRQ déjà pendante.
    unsafe { asm!("sti", "hlt", "cli") };
}

// Sûreté : la porte 14 fournit un code d’erreur, RIP et CS sur la pile.
// Le traitement fatal aligne la pile SysV et ne retourne jamais via ce cadre.
#[unsafe(naked)]
extern "C" fn page_fault_entry() {
    naked_asm!("mov rdi, cr2", "mov rsi, [rsp]", "mov rdx, [rsp+8]", "mov rcx, [rsp+16]",
        "cld", "and rsp, -16", "call {}", sym page_fault);
}

extern "C" fn page_fault(address: u64, error: u64, pc: u64, cs: u64) -> ! {
    let access = if error & 16 != 0 {
        "execution"
    } else if error & 2 != 0 {
        "ecriture"
    } else {
        "lecture"
    };
    let cause = if error & 8 != 0 {
        "reserve"
    } else if error & 1 == 0 {
        "absente"
    } else {
        "protection"
    };
    serial::line(format_args!(
        "#PF adresse={address:#x} pc={pc:#x} code={error:#x} cs={cs:#x} acces={access} cause={cause} utilisateur={}",
        error & 4 != 0
    ));
    hal::cpu::halt()
}
