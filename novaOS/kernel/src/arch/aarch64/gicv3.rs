//! GICv3 du banc virt-10.0, sans ITS. Les quatre INTx PCI sont les SPI 3 à 6.
use super::timer::{DIST, PPI, counter, counter_hz, read, write};
use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering};

const REDIST: u64 = DIST + 0x3000;
const SGI: u64 = DIST + 0x4000;
const ROUTES: u64 = DIST + 0x5000;
const IDS: u64 = DIST + 0x6000;
static DISK: AtomicU32 = AtomicU32::new(37);

pub(super) fn disk_id() -> u32 {
    DISK.load(Ordering::Acquire)
}

pub(super) const PAGES: &[(u64, u64)] = &[
    (REDIST, 0x080a_0000),
    (SGI, 0x080b_0000),
    (ROUTES, 0x0800_6000),
    (IDS, 0x0800_f000),
];

fn wait_clear(base: u64, off: u64, mask: u32) {
    let start = counter();
    while read(base, off) & mask != 0 {
        assert!(
            counter().wrapping_sub(start) < counter_hz(),
            "GICv3 sans acquittement"
        );
        core::hint::spin_loop();
    }
}

fn priority(base: u64, id: u32, value: u8) {
    // Sûreté : les priorités GICD/GICR acceptent des écritures d’un octet.
    unsafe {
        let register = (base + 0x400 + u64::from(id)) as *mut u8;
        register.write_volatile(value);
        assert_eq!(register.read_volatile(), value);
    }
}

pub(super) fn init() {
    assert_eq!(read(IDS, 0xfe8) >> 4 & 15, 3, "GICv3 attendu");
    let disabled = read(DIST, 0) & !7;
    write(DIST, 0, disabled);
    wait_clear(DIST, 0, 1 << 31);
    let banks = (read(DIST, 4) & 31) + 1;
    assert!(banks > 1);
    for bank in 1..u64::from(banks) {
        write(DIST, 0x180 + bank * 4, u32::MAX);
    }
    wait_clear(DIST, 0, 1 << 31);
    let routed = disabled | (1 << 4);
    write(DIST, 0, routed);
    wait_clear(DIST, 0, 1 << 31);
    // Sûreté : le cadre IROUTER est mappé Device. Le BSP utilise les registres
    // système EL1, groupe 1, EOI combiné à la désactivation.
    configure_disk(disk_id());
    write(DIST, 0, routed | 2);
    wait_clear(DIST, 0, 1 << 31);
    secondary();
}
pub(super) fn secondary() {
    let affinity = super::cpu_id() as u64;
    let packed = ((affinity >> 8) & 0xff00_0000) | (affinity & 0xff_ffff);
    let (redist, sgi) = (0..crate::smp::count())
        .map(redistributor)
        .find(|(rd, _)| u64::from(read(*rd, 0x0c)) == packed)
        .expect("redistributeur du PE absent");
    assert_eq!(read(redist, 0) & 1, 0, "LPIs actifs");
    write(redist, 0x14, read(redist, 0x14) & !2);
    wait_clear(redist, 0x14, 4);
    write(sgi, 0x180, u32::MAX);
    wait_clear(redist, 0, 1 << 3);
    write(sgi, 0x280, (1 << PPI) | 2);
    write(sgi, 0x80, read(sgi, 0x80) | ((1 << PPI) | 2));
    priority(sgi, PPI, 0x80);
    write(sgi, 0x100, (1 << PPI) | 2);
    priority(sgi, 1, 0x80);
    unsafe {
        let sre: u64;
        asm!("mrs {sre}, ICC_SRE_EL1", "orr {sre}, {sre}, #1", "msr ICC_SRE_EL1, {sre}",
             "isb", "mrs {sre}, ICC_SRE_EL1", sre = out(reg) sre, options(nostack));
        assert_ne!(sre & 1, 0);
        asm!("mrs {ctl}, ICC_CTLR_EL1", "bic {ctl}, {ctl}, #2", "msr ICC_CTLR_EL1, {ctl}",
             "msr ICC_BPR1_EL1, xzr",
             "msr ICC_PMR_EL1, {pmr}", "msr ICC_IGRPEN1_EL1, {one}", "dsb sy", "isb",
             ctl = out(reg) _, pmr = in(reg) 255u64, one = in(reg) 1u64, options(nostack));
        let (mask, group, control): (u64, u64, u64);
        asm!("mrs {}, ICC_PMR_EL1", "mrs {}, ICC_IGRPEN1_EL1", "mrs {}, ICC_CTLR_EL1",
             out(reg) mask, out(reg) group, out(reg) control, options(nostack));
        assert!(mask > 0x80 && group & 1 != 0 && control & 2 == 0);
        crate::hal::log(format_args!(
            "GICv3 groupe=1 PMR={mask:#x} PPI30=0x80 SPI37=0x40"
        ));
    }
}

pub(super) fn acknowledge() -> (u32, u32) {
    let id: u64;
    // Sûreté : IAR1 acquitte l’IRQ du groupe 1 du PE courant.
    unsafe { asm!("mrs {}, ICC_IAR1_EL1", out(reg) id, options(nostack)) };
    (id as u32 & 0xff_ffff, id as u32)
}

pub(super) fn end(ack: u32) {
    // Sûreté : l’ISR VirtIO ou le rechargement du timer précède l’EOI.
    unsafe {
        asm!("dsb sy", "msr ICC_EOIR1_EL1, {}", "isb", in(reg) u64::from(ack), options(nostack))
    };
}

pub(crate) fn disk_line(enable: bool) {
    let bit = 1 << (disk_id() - 32);
    write(DIST, if enable { 0x104 } else { 0x184 }, bit);
    wait_clear(DIST, 0, 1 << 31);
    assert_eq!(read(DIST, 0x104) & bit != 0, enable);
}

fn configure_disk(id: u32) {
    let bit = 1 << (id - 32);
    write(DIST, 0x84, read(DIST, 0x84) | bit);
    write(DIST, 0xc08, read(DIST, 0xc08) & !(2 << ((id % 16) * 2)));
    priority(DIST, id, 0x40);
    write(DIST, 0x284, bit);
    let affinity = crate::smp::hardware(0) as u64;
    // Sûreté : IROUTER est mappé Device, la ligne sélectionnée est masquée.
    unsafe {
        let reg = (ROUTES + u64::from(id) * 8) as *mut u64;
        reg.write_volatile(affinity);
        assert_eq!(reg.read_volatile(), affinity);
    }
}

pub(super) fn select_disk(id: u32) -> crate::hardware::block::Result<()> {
    let _guard = crate::hal::interrupts::Guard::new();
    if !(35..=38).contains(&id) {
        return Err(crate::hardware::block::Error::Route);
    }
    let bits = (1 << (id - 32)) | (1 << (disk_id() - 32));
    if (read(DIST, 0x104) | read(DIST, 0x204) | read(DIST, 0x304)) & bits != 0 {
        return Err(crate::hardware::block::Error::Route);
    }
    configure_disk(id);
    DISK.store(id, Ordering::Release);
    Ok(())
}

fn redistributor(frame: usize) -> (u64, u64) {
    if frame == 0 {
        return (REDIST, SGI);
    }
    let rd = 0xffff_c900_0020_0000 + 8192 * frame as u64;
    (rd, rd + 4096)
}

pub fn map_redistributors() {
    for frame in 1..crate::smp::count() {
        let (rd, sgi) = redistributor(frame);
        let pa = 0x080a_0000 + frame as u64 * 0x20000;
        crate::memory::map_io(pa, rd).expect("GICR RD");
        crate::memory::map_io(pa + 0x10000, sgi).expect("GICR SGI");
    }
}
pub fn kick() {
    unsafe {
        asm!("dsb ishst","msr ICC_SGI1R_EL1,{}","isb",in(reg) (1u64<<40)|(1<<24),options(nostack));
    }
}
