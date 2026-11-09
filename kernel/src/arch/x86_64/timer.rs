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
pub(super) extern "C" fn tick() {
    Port::PicCommand.write(0x20);
    sched::tick();
}

/// Sonde matérielle appelée IRQ locales masquées.
pub fn pending() -> bool {
    Port::PicCommand.write(0x0a); // OCW3 : lire IRR (requêtes en attente).
    Port::PicCommand.read() & 1 != 0
}
