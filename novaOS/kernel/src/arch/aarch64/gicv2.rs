//! Interface mémoire GICv2 de la machine virt historique.
use super::timer::{DIST, PPI, read, write};
use core::arch::asm;

pub(super) const CPU: u64 = DIST + 0x1000;
pub(super) const PAGES: &[(u64, u64)] = &[(CPU, 0x0801_0000)];

pub(super) fn init() {
    write(CPU, 0, 0);
    write(DIST, 0, 0);
    let banks = (read(DIST, 4) & 31) + 1;
    for i in 0..banks {
        write(DIST, 0x180 + 4 * u64::from(i), u32::MAX);
    }
    write(DIST, 0, 1);
    secondary();
}
pub(super) fn secondary() {
    write(DIST, 0x280, 1 << PPI);
    let priority = 0x400 + u64::from(PPI / 4) * 4;
    let shift = (PPI % 4) * 8;
    write(
        DIST,
        priority,
        (read(DIST, priority) & !(255 << shift)) | (0x80 << shift),
    );
    write(DIST, 0x100, (1 << PPI) | 2);
    write(CPU, 4, 0xff);
    write(CPU, 8, 0);
    write(CPU, 0, 1);
}

pub(super) fn acknowledge() -> (u32, u32) {
    let ack = read(CPU, 0x0c);
    (ack & 0x3ff, ack)
}

pub(super) fn end(ack: u32) {
    write(CPU, 0x10, ack);
    // Sûreté : termine l’écriture Device avant la prochaine livraison d’IRQ.
    unsafe { asm!("dsb sy", options(nostack)) };
}

pub(crate) fn disk_line(enable: bool) {
    const BIT: u32 = 1 << 5; // INTID 37, SPI5 du disque 00:06.0
    if enable {
        let pri = 0x400 + (37 / 4) * 4;
        write(DIST, pri, (read(DIST, pri) & !0xff00) | 0x4000);
        let target = 0x800 + (37 / 4) * 4;
        write(DIST, target, (read(DIST, target) & !0xff00) | 0x100);
        write(DIST, 0x104, BIT);
    } else {
        write(DIST, 0x184, BIT)
    }
    // Sûreté : terminer la configuration GIC avant publication ou retrait du droit.
    unsafe { asm!("dsb sy", options(nostack, preserves_flags)) }
}

pub fn kick() {
    unsafe {
        asm!("dsb ishst", options(nostack));
    }
    write(DIST, 0xf00, (1 << 24) | 1);
}
