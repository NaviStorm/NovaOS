//! Timer physique non sécurisé d’EL1 de la machine virt.
use super::gic;
pub(crate) use super::gic::disk_line;
#[cfg(feature = "gicv3-lab")]
pub(crate) fn select_disk(id: u32) -> crate::hardware::block::Result<()> {
    gic::select_disk(id)
}
#[cfg(not(feature = "gicv3-lab"))]
pub(crate) fn select_disk(id: u32) -> crate::hardware::block::Result<()> {
    if id == 37 {
        Ok(())
    } else {
        Err(crate::hardware::block::Error::Route)
    }
}
fn disk_id() -> u32 {
    #[cfg(feature = "gicv3-lab")]
    {
        gic::disk_id()
    }
    #[cfg(not(feature = "gicv3-lab"))]
    {
        37
    }
}
use crate::sched;
use core::{
    arch::asm,
    sync::atomic::{AtomicU32, Ordering},
};
static PERIOD: AtomicU32 = AtomicU32::new(0);
pub const DIST_PA: u64 = 0x0800_0000;
pub const DIST: u64 = super::serial::VA + 0x1000;
pub(super) const PPI: u32 = 30;

pub(super) fn read(base: u64, offset: u64) -> u32 {
    // Sûreté : fenêtres Device GICD/GICC mappées, registre 32 bits aligné.
    unsafe { ((base + offset) as *const u32).read_volatile() }
}
pub(super) fn write(base: u64, offset: u64, value: u32) {
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
    #[cfg(feature = "gicv3-lab")]
    gic::map_redistributors();
    gic::init();
    arm();
    // Sûreté : active uniquement la source physique configurée, avant démasquage CPU.
    unsafe {
        asm!("dsb sy","msr cntp_ctl_el0,{}","isb",in(reg) 1u64,options(nostack));
    }
}
pub(super) extern "C" fn irq(state: usize) {
    let (id, ack) = gic::acknowledge();
    if (1020..=1023).contains(&id) {
        return;
    }
    if id == PPI {
        arm();
    } else if id == disk_id() {
        if !crate::hardware::block::interrupt() {
            disk_line(false)
        }
    } else if (32..1020).contains(&id) {
        crate::hardware::block::stray();
        write(DIST, 0x180 + 4 * u64::from(id / 32), 1 << (id % 32));
    }
    gic::end(ack);
    if id == PPI {
        sched::tick(state & 15 == 0)
    } else if id == 1 {
        crate::sched::reschedule_idle();
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

pub(crate) fn purge_disk() -> crate::hardware::block::Result<()> {
    let bit = 1 << (disk_id() - 32);
    write(DIST, 0x284, bit);
    if (read(DIST, 0x204) | read(DIST, 0x304) | read(DIST, 0x104)) & bit == 0 {
        Ok(())
    } else {
        Err(crate::hardware::block::Error::Quarantine)
    }
}
pub(crate) fn disk_pending() -> bool {
    read(DIST, 0x204) & (1 << (disk_id() - 32)) != 0
}

pub fn start_secondary() {
    gic::secondary();
    arm();
    unsafe {
        asm!("dsb sy", "msr cntp_ctl_el0,{}", "isb",in(reg) 1u64,options(nostack));
    }
}
