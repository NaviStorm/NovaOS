//! Exclusivement Q35, INTA de 00:06.0 → PIRQG → GSI 22, profil sans partageur.
//! La fenêtre LAPIC existe aussi quand Limine n'annonce qu'un seul processeur.
use super::{io_read, io_write};
use crate::memory::{self, IO_BASE, PAGE};
const IOAPIC: u64 = IO_BASE + 324 * PAGE;
const LAPIC: u64 = IO_BASE + 325 * PAGE;
const LAPIC_PA: u64 = 0xfee0_0000;
const IOAPIC_PA: u64 = 0xfec0_0000;
const VECTOR: u32 = 0x52;
const GSI: u32 = 22;
const LOW: u32 = 0x10 + 2 * GSI;
const MASK: u32 = 1 << 16;
const ROUTE: u32 = VECTOR | (1 << 15) | (1 << 13); // niveau, actif bas
const BUSY: u32 = (1 << 14) | (1 << 12); // remote IRR ou delivery status

pub(super) fn contains_bar(pa: u64) -> bool {
    (0x8000_0000..=0xaff0_0000).contains(&pa)
}

fn read(reg: u32) -> u32 {
    io_write(IOAPIC, 0, reg);
    io_read(IOAPIC, 0x10)
}
fn write(reg: u32, value: u32) {
    io_write(IOAPIC, 0, reg);
    io_write(IOAPIC, 0x10, value);
}

pub(super) struct Route {
    previous: (u32, u32),
    spurious: u32,
}
impl Route {
    pub(super) fn new() -> Option<Self> {
        let lo: u32;
        let hi: u32;
        // Sûreté : vérifier l'adresse et le mode du LAPIC local au BSP.
        unsafe {
            core::arch::asm!("rdmsr", in("ecx") 0x1bu32, out("eax") lo, out("edx") hi, options(nostack));
        }
        if hi != 0 || lo & !0xfff != LAPIC_PA as u32 || lo & 0xc00 != 0x800 {
            return None;
        }
        memory::map_io(IOAPIC_PA, IOAPIC).ok()?;
        if memory::map_io(LAPIC_PA, LAPIC).is_err() {
            memory::unmap_lab_io(IOAPIC_PA, IOAPIC).expect("retrait fenêtre IOAPIC");
            return None;
        }
        let old = (read(LOW), read(LOW + 1));
        let destination = io_read(LAPIC, 0x20);
        let spurious = io_read(LAPIC, 0xf0);
        let route = Self {
            previous: old,
            spurious,
        };
        if (read(1) >> 16) & 0xff < GSI
            || old.0 & (MASK | BUSY) != MASK
            || destination >> 24 != crate::arch::cpu_id() as u32
            || spurious & (1 << 12) != 0
            || !route.drained()
        {
            release_windows();
            return None;
        }
        io_write(LAPIC, 0xf0, spurious | 0x1ff);
        if io_read(LAPIC, 0xf0) != spurious | 0x1ff {
            assert!(route.restore(), "restauration initiale IOAPIC/LAPIC EDU");
            return None;
        }
        write(LOW + 1, destination & 0xff00_0000);
        write(LOW, ROUTE | MASK);
        if read(LOW) != ROUTE | MASK || read(LOW + 1) != destination & 0xff00_0000 {
            assert!(route.restore(), "restauration initiale IOAPIC/LAPIC EDU");
            return None;
        }
        Some(route)
    }

    pub(super) fn mask(&self, masked: bool) -> bool {
        let value = ROUTE | if masked { MASK } else { 0 };
        write(LOW, value);
        read(LOW) & !BUSY == value
    }
    pub(super) fn drained(&self) -> bool {
        // Remote IRR couvre le message de niveau jusqu'à l'EOI du BSP, même
        // si la retraite s'exécute sur un AP. Le BSP vérifie aussi son IRR/ISR.
        let entry = read(LOW);
        let bit = 1 << (VECTOR % 32);
        let bank = u64::from(VECTOR / 32) * 0x10;
        entry & (MASK | BUSY) == MASK
            && (crate::smp::index() != 0
                || (io_read(LAPIC, 0x100 + bank) | io_read(LAPIC, 0x200 + bank)) & bit == 0)
    }
    pub(super) fn restore(&self) -> bool {
        write(LOW + 1, self.previous.1);
        write(LOW, self.previous.0);
        io_write(LAPIC, 0xf0, self.spurious);
        if (read(LOW), read(LOW + 1)) != self.previous || io_read(LAPIC, 0xf0) != self.spurious {
            return false;
        }
        release_windows();
        true
    }
}

fn release_windows() {
    memory::unmap_lab_io(LAPIC_PA, LAPIC).expect("retrait fenêtre LAPIC");
    memory::unmap_lab_io(IOAPIC_PA, IOAPIC).expect("retrait fenêtre IOAPIC");
}

pub(super) fn eoi() {
    io_write(LAPIC, 0xb0, 0);
    let _ = io_read(LAPIC, 0x20);
}
