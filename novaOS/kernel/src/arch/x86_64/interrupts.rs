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

extern "C" fn acknowledge_master() {
    Port::PicCommand.write(0x20);
}
extern "C" fn ignore_spurious() {}

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
static mut GDT: [u64; 11] = [0; 11];

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
        limit: 87,
        address: gdt as usize,
    };
    // Sûreté : installation unique sur le BSP. On conserve les sept descripteurs
    // Limine dont CS/SS ; le TSS permanent porte deux IST distinctes pour #PF/#DF.
    unsafe {
        (old.address as *const u8).copy_to_nonoverlapping(gdt.cast(), 56);
        for i in 1..7 {
            let segment = gdt.add(i).read();
            assert_eq!((segment >> 44) & 7, 1, "segment Limine non superviseur");
        }
        gdt.add(7)
            .write(103 | ((base & 0xffffff) << 16) | (0x89 << 40) | ((base & 0xff000000) << 32));
        gdt.add(8).write(base >> 32);
        gdt.add(10).write(0x00af_fb00_0000_ffff); // code 64 bits CPL3
        gdt.add(9).write(0x00cf_f300_0000_ffff); // données CPL3

        tss.add(36)
            .cast::<usize>()
            .write_unaligned((&raw const PAGE_STACK.0) as usize + 16384);
        tss.add(44)
            .cast::<usize>()
            .write_unaligned((&raw const DOUBLE_STACK.0) as usize + 16384);
        tss.add(102).cast::<u16>().write_unaligned(104);
        asm!("lgdt [{}]","ltr ax","xor eax,eax","lldt ax",in(reg) &descriptor,inout("ax") 0x38u16 => _,options(nostack));
    }
}
pub(super) fn set_rsp0(top: usize) {
    // Sûreté : BSP sous masque IRQ, TSS permanent de 104 octets ; RSP0 est
    // un champ non aligné à l’offset 4, lu au prochain changement de CPL.
    unsafe {
        (&raw mut TSS)
            .cast::<u8>()
            .add(4)
            .cast::<usize>()
            .write_unaligned(top)
    };
}
pub(super) fn install() {
    let cs: u16;
    hal::interrupts::mask();
    // Sûreté : seul le processeur d’amorçage exécute l’initialisation. La GDT
    // du noyau reprend le sélecteur de code chargé par Limine.
    unsafe { asm!("mov {cs:x}, cs", cs = out(reg) cs, options(nostack, preserves_flags)) };
    install_emergency_stacks();
    let mut table = Idt([Gate::new(fault_entry as *const () as usize, cs); 256]);
    let master_eoi = Gate::new(spurious_slave as *const () as usize, cs);
    table.0[14] = Gate::new(page_fault_entry as *const () as usize, cs);
    // Sûreté : liste fixe de 32 pointeurs émis par exceptions.S.
    unsafe extern "C" {
        static nova_exception_entries: [usize; 32];
    }
    for vector in [0, 1, 3, 4, 5, 6, 7, 10, 11, 12, 13, 16, 17, 19, 20, 21] {
        // Sûreté : vecteur constant borné ; code et table sont permanents.
        table.0[vector] = Gate::new(unsafe { nova_exception_entries[vector] }, cs);
    }
    table.0[3].attributes |= 0x60; // INT3 utilisateur
    table.0[14].ist = 1;
    table.0[8].ist = 2;
    // NMI et #MC restent terminales, y compris dans la fenêtre de changement de RSP.
    table.0[2].ist = 2;
    table.0[18].ist = 2;
    table.0[0x20] = Gate::new(timer_entry as *const () as usize, cs);
    table.0[0x21] = Gate::new(keyboard_entry as *const () as usize, cs);
    // Vecteurs externes non attribués : aucune livraison aux processus.
    table.0[0x30..0xff].fill(Gate::new(orphan_entry as *const () as usize, cs));
    table.0[0x51] = Gate::new(disk_entry as *const () as usize, cs);
    table.0[0xff] = Gate::new(spurious_master as *const () as usize, cs);
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
// Un défaut CPL3 quitte l’IST pour RSP0 avant de terminer le thread ;
// un défaut du noyau garde la pile d’urgence. Aucun de ces chemins ne retourne.
#[unsafe(naked)]
extern "C" fn page_fault_entry() {
    naked_asm!("mov rdi, cr2", "mov rsi, [rsp]", "mov rdx, [rsp+8]", "mov rcx, [rsp+16]",
        "test cl,3","jz 2f","mov rsp,[rip+{tss}+4]","2:",
        "cld", "and rsp, -16", "call {pf}", tss=sym TSS,pf=sym page_fault);
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
    if cs & 3 == 3 && error & 4 != 0 && error & 8 == 0 {
        crate::sched::process::finish(
            Outcome::Fault(Fault {
                trap: "#PF",
                access,
                cause,
                address,
                syndrome: error,
            }),
            pc,
            "Ring3",
        );
    }
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
                "mov rdi,[rsp+128]", // CS matériel après 15 GPR et RIP
                "mov ax,ds","push rax","mov ax,es","push rax","mov ax,fs","push rax","mov ax,gs","push rax",
                "mov r12,rsp","and rsp,-16","cld","pushfq","and qword ptr [rsp],-262145","popfq","call {irq}","mov rsp,r12",
                "pop rax","mov gs,ax","pop rax","mov fs,ax","pop rax","mov es,ax","pop rax","mov ds,ax",
                "test byte ptr [rsp+128],3","jz 2f",
                "mov rax,[rsp+144]","shl rax,16","sar rax,16","cmp rax,[rsp+144]","je 2f",
                "mov rdi,[rsp+120]","mov rsi,[rsp+144]","and rsp,-16","call {bad_sp}","2:",
                "pop r15","pop r14","pop r13","pop r12","pop r11","pop r10","pop r9","pop r8",
                "pop rbp","pop rdi","pop rsi","pop rdx","pop rcx","pop rbx","pop rax","iretq",irq=sym $handler,bad_sp=sym invalid_stack);
        }
    };
}
extern "C" fn disk_irq() {
    crate::hardware::block::interrupt();
    crate::hardware::block::eoi();
}
extern "C" fn orphan_irq() {
    crate::hardware::block::stray();
    crate::hardware::block::eoi()
}
irq_entry!(orphan_entry, orphan_irq);
irq_entry!(disk_entry, disk_irq);
irq_entry!(timer_entry, super::timer::tick);
irq_entry!(keyboard_entry, super::keyboard::irq);
irq_entry!(spurious_master, ignore_spurious);
irq_entry!(spurious_slave, acknowledge_master);

use crate::sched::process::{Fault, Outcome, finish};

// Sûreté : stubs IDT terminales ; normalisent le code d’erreur puis utilisent
// uniquement les arguments du cadre matériel, avec pile SysV alignée.
core::arch::global_asm!(include_str!("exceptions.S"), handler=sym exception);

extern "C" fn exception(vector: u64, code: u64, pc: u64, cs: u64) -> ! {
    if cs & 3 == 3 {
        let (trap, cause) = match vector {
            13 => ("#GP", "privilege"),
            7 => ("#NM", "fpu"),
            6 => ("#UD", "invalide"),
            _ => ("exception", "exception"),
        };
        hal::log(format_args!("EXCEPTION vecteur={vector} cs={cs:#x}"));
        finish(
            Outcome::Fault(Fault {
                trap,
                cause,
                access: "instruction",
                address: 0,
                syndrome: code,
            }),
            pc,
            "Ring3",
        );
    }
    if vector == 7 {
        hal::log(format_args!("FPU/SIMD interdit : #NM"));
    } else {
        hal::log(format_args!(
            "Exception noyau vecteur={vector} pc={pc:#x} code={code:#x}"
        ));
    }
    hal::cpu::halt()
}

extern "C" fn invalid_stack(pc: u64, sp: u64) -> ! {
    finish(
        Outcome::Fault(Fault {
            trap: "#SS",
            cause: "pile",
            access: "retour",
            address: sp,
            syndrome: 0,
        }),
        pc,
        "Ring3",
    );
}

// Sûreté : la porte assembleur lit RSP0 dans le TSS avant toute utilisation
// de la pile. Le scratch est privé au BSP et n’est vivant que sous masque IRQ.
core::arch::global_asm!(include_str!("syscall.S"), tss=sym TSS, dispatch=sym syscall_dispatch);

#[repr(C)]
struct CallFrame {
    ax: u64,
    bx: u64,
    cx: u64,
    dx: u64,
    si: u64,
    di: u64,
    bp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    pc: u64,
    cs: u64,
    flags: u64,
    sp: u64,
    ss: u64,
}

const _: () = assert!(
    size_of::<CallFrame>() == 160
        && core::mem::offset_of!(CallFrame, pc) == 120
        && core::mem::offset_of!(CallFrame, sp) == 144
);

extern "C" fn syscall_dispatch(f: &mut CallFrame) {
    // IF est imposé. IOPL, NT, VM, RF et les bits réservés ne sont pas importés.
    f.flags = (f.flags & 0x240dd5) | 0x202;
    f.cx = f.pc;
    f.r11 = f.flags;
    let r = crate::sched::process::syscall::call(f.ax, [f.di, f.si, f.dx], f.pc, f.sp, "Ring3");
    f.ax = r.status;
    f.dx = r.value;
}

pub(super) fn enable_syscall() {
    // Sûreté : adresse d’entrée définie par syscall.S, dans .text superviseur RX.
    unsafe extern "C" {
        fn nova_syscall();
    }
    fn write_msr(msr: u32, value: u64) {
        // Sûreté : MSR SYSCALL documentés, initialisation à CPL0 sur le BSP.
        unsafe {
            asm!("wrmsr",in("ecx") msr,in("eax") value as u32,in("edx") (value>>32) as u32,options(nostack))
        };
    }
    let cs: u16;
    let ss: u16;
    // Sûreté : lecture des sélecteurs actifs avant installation de STAR.
    unsafe { asm!("mov {0:x},cs","mov {1:x},ss",out(reg) cs,out(reg) ss,options(nostack)) };
    assert_eq!((cs, ss), (0x28, 0x30));
    write_msr(0xc0000081, (0x40u64 << 48) | (0x28u64 << 32));
    write_msr(0xc0000082, nova_syscall as *const () as u64);
    write_msr(0xc0000084, 0x47700);
    // Sûreté : EFER.SCE est activé en dernier, avec une porte et une GDT valides.
    unsafe {
        asm!("rdmsr","or eax,1","wrmsr",in("ecx") 0xc0000080u32,out("eax") _,out("edx") _,options(nostack))
    };
}

pub(super) fn syscall_paths() -> (u64, u64) {
    // Sûreté : deux compteurs 64 bits alignés, modifiés uniquement en
    // assembleur sur CPU0, IRQ masquées aux points de lecture et d’écriture.
    unsafe extern "C" {
        static nova_fast: u64;
        static nova_iret: u64;
    }
    let _irq = hal::interrupts::Guard::new();
    unsafe {
        (
            (&raw const nova_fast).read_volatile(),
            (&raw const nova_iret).read_volatile(),
        )
    }
}
