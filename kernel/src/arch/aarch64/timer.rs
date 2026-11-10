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
    }
    write(CPU, 0x10, ack); // EOI avant de pouvoir suspendre cette pile.
    // Sûreté : termine l’écriture Device avant la prochaine livraison d’IRQ.
    unsafe {
        asm!("dsb sy", options(nostack));
    }
    assert_eq!(id, PPI, "IRQ GIC inattendue");
    sched::tick(state & 15 == 0);
}

pub fn pending() -> bool {
    let ctl: u64;
    // Sûreté : lecture privilégiée de ISTATUS ; aucun acquittement du timer.
    unsafe {
        asm!("mrs {},cntp_ctl_el0",out(reg) ctl,options(nostack));
    }
    ctl & 7 == 5
}
