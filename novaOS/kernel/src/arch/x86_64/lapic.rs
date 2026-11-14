//! LAPIC xAPIC en UC. La fenêtre est également celle de la route disque VT-d.
use crate::{sched, smp};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const BASE: usize = 0xffff_c900_0000_0000 + 321 * 4096;
static PERIOD: AtomicU32 = AtomicU32::new(0);
static TIMEOUT_CYCLES: AtomicU64 = AtomicU64::new(0);
static INVALIDATE: AtomicU64 = AtomicU64::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACK: [AtomicU64; smp::MAX_CPUS] = [const { AtomicU64::new(0) }; smp::MAX_CPUS];

fn read(off: usize) -> u32 {
    unsafe { ((BASE + off) as *const u32).read_volatile() }
}
fn write(off: usize, value: u32) {
    unsafe { ((BASE + off) as *mut u32).write_volatile(value) }
}

pub fn prepare() {
    let (lo, hi): (u32, u32);
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") 0x1bu32, out("eax") lo, out("edx") hi);
    }
    assert_eq!((lo & 0xc00, hi), (0x800, 0), "xAPIC attendu");
    crate::memory::map_io((lo & !0xfff) as u64, BASE as u64).expect("fenêtre xAPIC UC");
    write(0xf0, read(0xf0) | 0x1ff);
    write(0x3e0, 3);
    write(0x320, (1 << 16) | 0xe0);
    write(0x380, u32::MAX);
    let tsc = super::timer::counter();
    let start = sched::ticks();
    while sched::ticks() - start < 8 {
        core::hint::spin_loop();
    }
    let elapsed = sched::ticks() - start;
    TIMEOUT_CYCLES.store(
        (super::timer::counter() - tsc) / elapsed * u64::from(sched::HZ) * 5,
        Ordering::Release,
    );
    let period = u64::from(u32::MAX - read(0x390)) / elapsed;
    assert!(period > 0);
    PERIOD.store(period as u32, Ordering::Release);
}
pub fn start() {
    assert_eq!(read(0x20) >> 24, crate::arch::cpu_id() as u32);
    write(0xf0, read(0xf0) | 0x1ff);
    write(0x3e0, 3);
    write(0x320, (1 << 17) | 0xe0);
    write(0x380, PERIOD.load(Ordering::Acquire));
}
pub extern "C" fn tick(cs: usize) {
    write(0xb0, 0);
    sched::tick(cs & 3 == 3);
}
pub extern "C" fn ipi() {
    poll_translation();
    write(0xb0, 0);
}
pub fn poll_translation() {
    let seq = GENERATION.load(Ordering::Acquire);
    if seq == 0 {
        return;
    }
    let ack = &ACK[smp::index()];
    if ack.load(Ordering::Relaxed) != seq {
        super::paging::invalidate_local(INVALIDATE.load(Ordering::Relaxed));
        ack.store(seq, Ordering::Release);
    }
}

// L'allocateur de tables sérialise les publications. Les CPU qui attendent
// son verrou (ou celui du scheduler) traitent aussi la boîte IRQ masquées.
pub fn shootdown(va: u64) {
    let mask = smp::ONLINE.load(Ordering::Acquire);
    if mask == 1 {
        return;
    }
    INVALIDATE.store(va, Ordering::Relaxed);
    let seq = GENERATION.fetch_add(1, Ordering::Release) + 1;
    let me = smp::index();
    ACK[me].store(seq, Ordering::Release);
    let start = super::timer::counter();
    for cpu in 0..smp::count() {
        if cpu == me || mask & (1 << cpu) == 0 {
            continue;
        }
        wait_delivery(start);
        write(0x310, (smp::hardware(cpu) as u32) << 24);
        write(0x300, 0xe1);
        while ACK[cpu].load(Ordering::Acquire) != seq {
            assert!(
                super::timer::counter().wrapping_sub(start)
                    < TIMEOUT_CYCLES.load(Ordering::Relaxed),
                "invalidation TLB sans acquittement"
            );
            core::hint::spin_loop();
        }
    }
}

fn wait_delivery(start: u64) {
    while read(0x300) & (1 << 12) != 0 {
        assert!(
            super::timer::counter().wrapping_sub(start) < TIMEOUT_CYCLES.load(Ordering::Relaxed),
            "IPI non livrée"
        );
        core::hint::spin_loop();
    }
}

pub fn kick() {
    wait_delivery(super::timer::counter());
    write(0x300, 0xc0000 | 0xe2);
}
pub extern "C" fn wake() {
    write(0xb0, 0);
    sched::reschedule_idle();
}
