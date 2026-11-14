//! État de base AArch64. Le noyau est compilé pour la cible softfloat.
use core::arch::{asm, global_asm};
pub const SIZE: usize = 528;
pub fn prepare() {
    let id: u64;
    let extended: u64;
    unsafe {
        asm!("mrs {},id_aa64pfr0_el1",out(reg) id);
        asm!("mrs {},S3_0_C0_C4_2",out(reg)extended);
        assert_eq!(extended & (15 << 32), 0, "FPMR hors profil");
        let fp = (id >> 16) & 15;
        let simd = (id >> 20) & 15;
        assert!(fp <= 1 && simd <= 1, "FP/ASIMD absents");
        static PROFILE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        let profile = 1 + fp + 16 * simd;
        if let Err(previous) = PROFILE.compare_exchange(
            0,
            profile,
            core::sync::atomic::Ordering::AcqRel,
            core::sync::atomic::Ordering::Acquire,
        ) {
            assert_eq!(previous, profile, "profils FP hétérogènes");
        }
        let mut ctl: u64;
        asm!("mrs {},cpacr_el1",out(reg)ctl);
        ctl &= !((3 << 16) | (3 << 20) | (3 << 24));
        asm!("msr cpacr_el1,{}","isb",in(reg)ctl);
    }
}
pub fn initial(b: &mut [u8; SIZE]) {
    b.fill(0);
}
global_asm!(
    r#"
.section .text
.arch_extension simd
.macro vectors op
.set off,0
.irp n,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
    \op q\n,[x0,#off]
    .set off,off+16
.endr
.endm
nova_fp_save:
    vectors str
    mrs x2,fpcr
    mrs x3,fpsr
    str x2,[x0,#512]
    str x3,[x0,#520]
    ret
nova_fp_load:
    vectors ldr
    ldr x2,[x0,#512]
    ldr x3,[x0,#520]
    msr fpcr,x2
    msr fpsr,x3
    ret
"#
);
unsafe extern "C" {
    fn nova_fp_save(p: *mut u8);
    fn nova_fp_load(p: *const u8);
}
pub unsafe fn exchange(old: Option<*mut u8>, new: Option<*const u8>) {
    unsafe {
        let ctl: u64;
        asm!("mrs {},cpacr_el1",out(reg)ctl);
        asm!("msr cpacr_el1,{}","isb",in(reg)ctl | (3<<20));
        if let Some(p) = old {
            nova_fp_save(p);
        }
        if let Some(p) = new {
            nova_fp_load(p);
        } else {
            asm!("msr cpacr_el1,{}","isb",in(reg)ctl & !(3<<20));
        }
    }
}
