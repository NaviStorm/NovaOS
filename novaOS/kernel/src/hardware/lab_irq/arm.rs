//! Exclusivement QEMU virt avec GICv2, INTA du slot 6 vers SPI5, INTID 37.
use super::{io_read, io_write};
use crate::arch::timer::DIST;
const BIT: u32 = 1 << 5;
const SHIFT: u32 = 8;
const BYTE: u32 = 0xff << SHIFT;
const PRIORITY: u64 = 0x424;
const TARGET: u64 = 0x824;

pub(super) fn contains_bar(pa: u64) -> bool {
    (0x1000_0000..=0x2ff0_0000).contains(&pa)
}

fn set_byte(offset: u64, value: u32) {
    io_write(
        DIST,
        offset,
        (io_read(DIST, offset) & !BYTE) | (value & BYTE),
    );
}
pub(super) struct Route {
    priority: u32,
    target: u32,
}
impl Route {
    pub(super) fn new() -> Option<Self> {
        if cfg!(feature = "gicv3-lab") || io_read(DIST, 0xc08) & (2 << 10) != 0 {
            return None;
        }
        if (io_read(DIST, 0x104) | io_read(DIST, 0x204) | io_read(DIST, 0x304)) & BIT != 0 {
            return None;
        }
        let priority = io_read(DIST, PRIORITY) & BYTE;
        let target = io_read(DIST, TARGET) & BYTE;
        let mut bsp_targets = io_read(DIST, 0x800) & 0xff;
        if bsp_targets == 0 && crate::smp::count() == 1 {
            bsp_targets = 1;
        }
        if !bsp_targets.is_power_of_two() {
            return None;
        }
        set_byte(PRIORITY, 0x40 << SHIFT);
        set_byte(TARGET, bsp_targets << SHIFT);
        let route = Self { priority, target };
        let observed_target = io_read(DIST, TARGET) & BYTE;
        if io_read(DIST, PRIORITY) & BYTE != 0x40 << SHIFT
            || (observed_target != bsp_targets << SHIFT
                && !(crate::smp::count() == 1 && observed_target == 0))
        {
            assert!(route.restore(), "restauration initiale SPI EDU");
            return None;
        }
        Some(route)
    }
    pub(super) fn mask(&self, masked: bool) -> bool {
        io_write(DIST, if masked { 0x184 } else { 0x104 }, BIT);
        (io_read(DIST, 0x104) & BIT == 0) == masked
    }
    pub(super) fn drained(&self) -> bool {
        // Un acquittement applicatif a déjà désasserté la ligne EDU côté device.
        // N'efface pas l'état Active : seul l'EOI du CPU récepteur le retire.
        io_write(DIST, 0x284, BIT);
        (io_read(DIST, 0x104) | io_read(DIST, 0x204) | io_read(DIST, 0x304)) & BIT == 0
    }
    pub(super) fn restore(&self) -> bool {
        set_byte(PRIORITY, self.priority);
        set_byte(TARGET, self.target);
        io_read(DIST, PRIORITY) & BYTE == self.priority
            && io_read(DIST, TARGET) & BYTE == self.target
    }
}
