//! Format standard XSAVE (x87/SSE/YMM) ou image FXSAVE64. Les autres
//! composantes XCR0 et les extensions superviseur ne sont jamais demandées.
use core::{
    arch::{
        asm,
        x86_64::{__cpuid, __cpuid_count},
    },
    sync::atomic::{AtomicUsize, Ordering},
};
pub const SIZE: usize = 832;
static MASK: AtomicUsize = AtomicUsize::new(0);

pub fn prepare() {
    let vendor = __cpuid(0);
    let intel = (vendor.ebx, vendor.edx, vendor.ecx) == (0x756e6547, 0x49656e69, 0x6c65746e);
    let amd = (vendor.ebx, vendor.edx, vendor.ecx) == (0x68747541, 0x69746e65, 0x444d4163);
    assert!(
        intel || (amd && __cpuid(0x80000000).eax >= 0x80000008 && __cpuid(0x80000008).ebx & 4 != 0),
        "restauration des pointeurs x87 non qualifiée"
    );
    let features = __cpuid(1);
    let required = 1 | (1 << 24) | (1 << 25) | (1 << 26);
    assert_eq!(features.edx & required, required, "x87/FXSR/SSE2 requis");
    let avx = features.ecx & (5 << 26) == 5 << 26
        && __cpuid(0).eax >= 0xd
        && __cpuid_count(0xd, 0).eax & 7 == 7;
    let mask = if avx { 7 } else { 3 };
    unsafe {
        if amd && __cpuid(0x80000001).edx & (1 << 25) != 0 {
            let lo: u32;
            let hi: u32;
            asm!("rdmsr", in("ecx")0xc0000080u32,out("eax")lo,out("edx")hi);
            asm!("wrmsr", in("ecx")0xc0000080u32,in("eax")lo & !(1<<14),in("edx")hi);
        }
        let mut cr0: usize;
        let mut cr4: usize;
        asm!("mov {}, cr0", out(reg) cr0);
        asm!("mov cr0, {}", in(reg) (cr0 & !4) | 0x2a);
        asm!("mov {}, cr4", out(reg) cr4);
        cr4 |= 3 << 9;
        if avx {
            asm!("mov cr4, {}", in(reg) cr4 | (1 << 18));
            asm!("xsetbv", in("ecx") 0u32, in("eax") 7u32, in("edx") 0u32);
            let ymm = __cpuid_count(0xd, 2);
            assert_eq!((ymm.ebx, ymm.eax), (576, 256), "format YMM inconnu");
            assert_eq!(__cpuid_count(0xd, 0).ebx as usize, SIZE);
        } else {
            asm!("mov cr4, {}", in(reg) cr4 & !(1 << 18));
        }
    }
    // Les bits MXCSR admis doivent rester restaurables après migration.
    #[repr(align(16))]
    struct Legacy([u8; 512]);
    let mut legacy = Legacy([0; 512]);
    unsafe {
        asm!("clts","fxsave64 [{}]",in(reg)legacy.0.as_mut_ptr(),options(nostack));
        let cr0: usize;
        asm!("mov {},cr0",out(reg)cr0);
        asm!("mov cr0,{}",in(reg)cr0|8);
    }
    let reported = u32::from_le_bytes(legacy.0[28..32].try_into().unwrap());
    let writable = if reported == 0 { 0xffbf } else { reported };
    static MXCSR_MASK: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    if let Err(previous) =
        MXCSR_MASK.compare_exchange(0, writable, Ordering::AcqRel, Ordering::Acquire)
    {
        assert_eq!(previous, writable, "masques MXCSR hétérogènes");
    }
    if crate::smp::index() == 0 {
        crate::hal::log(format_args!(
            "SMP FPU x86 sauvegarde={} xcr0_demande={}",
            if avx { "xsave64" } else { "fxsave64" },
            if avx { 7 } else { 0 }
        ));
    }
    if let Err(previous) = MASK.compare_exchange(0, mask, Ordering::AcqRel, Ordering::Acquire) {
        assert_eq!(previous, mask, "profils FPU hétérogènes");
    }
}
pub fn initial(b: &mut [u8; SIZE]) {
    b.fill(0);
    b[..2].copy_from_slice(&0x037fu16.to_le_bytes());
    b[24..28].copy_from_slice(&0x1f80u32.to_le_bytes());
    b[512] = MASK.load(Ordering::Acquire) as u8;
}
pub unsafe fn exchange(old: Option<*mut u8>, new: Option<*const u8>) {
    let avx = MASK.load(Ordering::Acquire) == 7;
    unsafe {
        asm!("clts", options(nostack));
        if let Some(p) = old {
            if avx {
                asm!("xsave64 [{}]",in(reg)p,in("eax")7u32,in("edx")0u32,options(nostack));
            } else {
                asm!("fxsave64 [{}]",in(reg)p,options(nostack));
            }
        }
        if let Some(p) = new {
            if avx {
                asm!("xrstor64 [{}]",in(reg)p,in("eax")7u32,in("edx")0u32,options(nostack));
            } else {
                asm!("fxrstor64 [{}]",in(reg)p,options(nostack));
            }
        } else {
            let cr0: usize;
            asm!("mov {},cr0",out(reg)cr0);
            asm!("mov cr0,{}",in(reg)cr0|8);
        }
    }
}
