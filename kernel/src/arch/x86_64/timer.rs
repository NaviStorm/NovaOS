use super::ports::Port;
use crate::sched;

pub fn start(hz: u32) {
    let divisor = (1_193_182u32 + hz / 2) / hz;
    assert!((1..=65535).contains(&divisor));
    Port::PitMode.write(0x34); // Canal 0, octet bas puis haut, générateur périodique.
    Port::Pit0.write(divisor as u8);
    Port::Pit0.write((divisor >> 8) as u8);
    Port::PicMask.write(Port::PicMask.read() & !1);
}
pub(super) extern "C" fn tick(cs: usize) {
    Port::PicCommand.write(0x20);
    sched::tick(cs & 3 == 3);
}

/// Sonde matérielle appelée IRQ locales masquées.
pub fn pending() -> bool {
    Port::PicCommand.write(0x0a); // OCW3 : lire IRR (requêtes en attente).
    Port::PicCommand.read() & 1 != 0
}

pub fn counter() -> u64 {
    let (lo, hi): (u32, u32);
    // Sûreté : LFENCE/RDTSC en anneau zéro, compteur local du seul BSP.
    unsafe {
        core::arch::asm!("lfence","rdtsc","lfence",out("eax")lo,out("edx")hi,options(nostack))
    };
    (u64::from(hi) << 32) | u64::from(lo)
}
pub fn counter_hz() -> u64 {
    let start = crate::sched::ticks();
    while crate::sched::ticks() == start {
        core::hint::spin_loop();
    }
    let first = crate::sched::ticks();
    let a = counter();
    while crate::sched::ticks() - first < 64 {
        core::hint::spin_loop();
    }
    (counter() - a) * u64::from(crate::sched::HZ) / (crate::sched::ticks() - first)
}
