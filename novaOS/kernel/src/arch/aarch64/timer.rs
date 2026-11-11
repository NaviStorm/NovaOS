//! Timer physique non sécurisé d’EL1 et GICv2 de la machine virt.
use crate::sched;
use core::{
    arch::asm,
    sync::atomic::{AtomicU32, Ordering},
};
static PERIOD: AtomicU32 = AtomicU32::new(0);
pub const DIST_PA: u64 = 0x0800_0000;
pub const CPU_PA: u64 = 0x0801_0000;
pub const DIST: u64 = super::serial::VA + 0x1000;
pub const CPU: u64 = DIST + 0x1000;
const PPI: u32 = 30;

fn read(base: u64, offset: u64) -> u32 {
    // Sûreté : fenêtres Device GICD/GICC mappées, registre 32 bits aligné.
    unsafe { ((base + offset) as *const u32).read_volatile() }
}
fn write(base: u64, offset: u64, value: u32) {
    // Sûreté : registres réservés au pilote GIC sur le BSP, IRQ masquées.
    unsafe {
        ((base + offset) as *mut u32).write_volatile(value);
    }
}
fn arm() {
    // Sûreté : recharge en ticks du compteur générique sur le BSP à EL1.
    unsafe {
        asm!("msr cntp_tval_el0,{}","isb",in(reg) u64::from(PERIOD.load(Ordering::Relaxed)),options(nostack));
    }
}
pub fn start(hz: u32) {
    let freq: u64;
    // Sûreté : CNTFRQ_EL0 est lisible au niveau de démarrage EL1.
    unsafe {
        asm!("mrs {},cntfrq_el0",out(reg) freq,options(nostack));
    }
    let period = freq / u64::from(hz);
    assert!((1..=i32::MAX as u64).contains(&period));
    PERIOD.store(period as u32, Ordering::Relaxed);
    write(CPU, 0, 0);
    write(DIST, 0, 0);
    let banks = (read(DIST, 4) & 31) + 1;
    for i in 0..banks {
        write(DIST, 0x180 + 4 * u64::from(i), u32::MAX);
    }
    write(DIST, 0x280, 1 << PPI);
    let priority = 0x400 + u64::from(PPI / 4) * 4;
    let shift = (PPI % 4) * 8;
    write(
        DIST,
        priority,
        (read(DIST, priority) & !(255 << shift)) | (0x80 << shift),
    );
    write(DIST, 0x100, 1 << PPI);
    write(CPU, 4, 0xff);
    write(CPU, 8, 0);
    write(DIST, 0, 1);
    write(CPU, 0, 1);
    arm();
    // Sûreté : active uniquement la source physique configurée, avant démasquage CPU.
    unsafe {
        asm!("dsb sy","msr cntp_ctl_el0,{}","isb",in(reg) 1u64,options(nostack));
    }
}
pub(super) extern "C" fn irq(state: usize) {
    let ack = read(CPU, 0x0c);
    let id = ack & 0x3ff;
    if id >= 1020 {
        return;
    }
    if id == PPI {
        arm();
    } else if id == 37 {
        if !crate::hardware::block::interrupt() {
            disk_line(false)
        }
    } else if id >= 32 {
        crate::hardware::block::stray();
        write(DIST, 0x180 + 4 * u64::from(id / 32), 1 << (id % 32));
    }
    write(CPU, 0x10, ack); // EOI avant de pouvoir suspendre cette pile.
    // Sûreté : termine l’écriture Device avant la prochaine livraison d’IRQ.
    unsafe {
        asm!("dsb sy", options(nostack));
    }
    if id == PPI {
        sched::tick(state & 15 == 0)
    }
}

pub fn pending() -> bool {
    let ctl: u64;
    // Sûreté : lecture privilégiée de ISTATUS ; aucun acquittement du timer.
    unsafe {
        asm!("mrs {},cntp_ctl_el0",out(reg) ctl,options(nostack));
    }
    ctl & 7 == 5
}

pub fn counter() -> u64 {
    let v;
    // Sûreté : lecture ordonnée du compteur physique au niveau EL1.
    unsafe { asm!("isb","mrs {},cntpct_el0",out(reg)v,options(nostack)) };
    v
}
pub fn counter_hz() -> u64 {
    let v;
    // Sûreté : fréquence architecturale du timer générique à EL1.
    unsafe { asm!("mrs {},cntfrq_el0",out(reg)v,options(nostack)) };
    v
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
pub(crate) fn purge_disk() -> crate::hardware::block::Result<()> {
    write(DIST, 0x284, 1 << 5);
    if (read(DIST, 0x204) | read(DIST, 0x304) | read(DIST, 0x104)) & (1 << 5) == 0 {
        Ok(())
    } else {
        Err(crate::hardware::block::Error::Quarantine)
    }
}
pub(crate) fn disk_pending() -> bool {
    read(DIST, 0x204) & (1 << 5) != 0
}
