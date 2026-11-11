//! Horloge civile PL031 de la machine QEMU virt, résolue à la seconde.
pub(super) const PA: u64 = 0x0901_0000;
pub(super) const VA: u64 = super::serial::VA + 0x3000;

fn read(offset: u64) -> u32 {
    // Sûreté : map_devices réserve cette page Device au noyau. Les offsets
    // internes désignent uniquement des registres PL031 alignés en lecture.
    unsafe { ((VA + offset) as *const u32).read_volatile() }
}

pub(crate) fn seconds() -> Option<u64> {
    let id = [0x31, 0x10, 0x14, 0, 0x0d, 0xf0, 0x05, 0xb1];
    for (n, expected) in id.into_iter().enumerate() {
        if read(0xfe0 + 4 * n as u64) != expected {
            return None;
        }
    }
    Some(u64::from(read(0)))
}
