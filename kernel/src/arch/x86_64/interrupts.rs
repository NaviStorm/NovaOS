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
    address: usize,
}

static mut IDT: Idt = Idt([Gate::new(0, 0); 256]);

// Sûreté : entrée par la porte IRQ15 parasite, EOI sur le PIC maître. Aucun registre ne change
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

#[repr(align(16))]
struct EmergencyStack([u8; 16384]);
static mut PAGE_STACK: EmergencyStack = EmergencyStack([0; 16384]);
static mut DOUBLE_STACK: EmergencyStack = EmergencyStack([0; 16384]);
static mut TSS: [u8; 104] = [0; 104];
static mut GDT: [u64; 9] = [0; 9];

fn install_emergency_stacks() {
    let mut old = Descriptor {
        limit: 0,
        address: 0,
    };
    // Sûreté : IRQ masquées au démarrage, l’ancienne GDT Limine est encore mappée.
    unsafe {
        asm!("sgdt [{}]",in(reg) &mut old, options(nostack,preserves_flags));
    }
    assert!(old.limit >= 55, "GDT Limine trop courte");
    let tss = (&raw mut TSS).cast::<u8>();
    let base = tss as u64;
    let gdt = (&raw mut GDT).cast::<u64>();
    let descriptor = Descriptor {
        limit: 71,
        address: gdt as usize,
    };
    // Sûreté : installation unique sur le BSP. On conserve les sept descripteurs
    // Limine dont CS/SS ; le TSS permanent porte deux IST distinctes pour #PF/#DF.
    unsafe {
        (old.address as *const u8).copy_to_nonoverlapping(gdt.cast(), 56);
        gdt.add(7)
            .write(103 | ((base & 0xffffff) << 16) | (0x89 << 40) | ((base & 0xff000000) << 32));
        gdt.add(8).write(base >> 32);
        tss.add(36)
            .cast::<usize>()
            .write_unaligned((&raw const PAGE_STACK.0) as usize + 16384);
        tss.add(44)
            .cast::<usize>()
            .write_unaligned((&raw const DOUBLE_STACK.0) as usize + 16384);
        tss.add(102).cast::<u16>().write_unaligned(104);
        asm!("lgdt [{}]","ltr ax",in(reg) &descriptor,in("ax") 0x38u16,options(nostack,preserves_flags));
    }
}
pub(super) fn install() {
    let cs: u16;
    hal::interrupts::mask();
    // Sûreté : seul le processeur d’amorçage exécute l’initialisation. La GDT
    // du noyau reprend le sélecteur de code chargé par Limine.
    unsafe { asm!("mov {cs:x}, cs", cs = out(reg) cs, options(nostack, preserves_flags)) };
    install_emergency_stacks();
    let mut table = Idt([Gate::new(fault_entry as *const () as usize, cs); 256]);
    let master_eoi = Gate::new(acknowledge_master as *const () as usize, cs);
    table.0[14] = Gate::new(page_fault_entry as *const () as usize, cs);
    table.0[7] = Gate::new(fpu_entry as *const () as usize, cs);
    table.0[14].ist = 1;
    table.0[8].ist = 2;
    table.0[0x20] = Gate::new(timer_entry as *const () as usize, cs);
    table.0[0x21] = Gate::new(keyboard_entry as *const () as usize, cs);
    table.0[0x27] = Gate::new(spurious_master as *const () as usize, cs);
    table.0[0x2f] = master_eoi;
    let descriptor = Descriptor {
        limit: (size_of::<Idt>() - 1) as u16,
        address: (&raw const IDT) as usize,
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
    Port::PicMask.write(Port::PicMask.read() & !(1 << 1));
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

// Sûreté : portes d’interruption DPL0, sans code d’erreur. Préserve tous les
// GPR avant Rust, même en plein calcul ; iretq restitue notamment RIP, RSP et
// RFLAGS. r12 garde l’adresse du cadre pendant l’alignement SysV et la commutation.
macro_rules! irq_entry {
    ($name:ident, $handler:path) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            naked_asm!("push rax","push rbx","push rcx","push rdx","push rsi","push rdi",
                "push rbp","push r8","push r9","push r10","push r11","push r12","push r13","push r14","push r15",
                "mov r12,rsp","and rsp,-16","cld","call {}","mov rsp,r12",
                "pop r15","pop r14","pop r13","pop r12","pop r11","pop r10","pop r9","pop r8",
                "pop rbp","pop rdi","pop rsi","pop rdx","pop rcx","pop rbx","pop rax","iretq",sym $handler);
        }
    };
}
irq_entry!(timer_entry, super::timer::tick);
irq_entry!(keyboard_entry, super::keyboard::irq);

// Sûreté : #NM ne pousse pas de code d’erreur, la branche est terminale.
#[unsafe(naked)]
extern "C" fn fpu_entry() {
    naked_asm!("cld","and rsp,-16","call {}",sym forbidden);
}
extern "C" fn forbidden() -> ! {
    hal::log(format_args!("FPU/SIMD interdit : #NM"));
    hal::cpu::halt()
}
