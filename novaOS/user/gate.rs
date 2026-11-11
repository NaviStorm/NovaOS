use super::abi::Reply;
use core::arch::asm;
pub fn call(id: u64, a: u64, b: u64, c: u64) -> Reply {
    let (status, value);
    // Sûreté : conventions NovaOS v1. Les registres de retour et les
    // destructions architecturales sont déclarés ; la porte peut écrire en mémoire.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("syscall",inlateout("rax")id=>status,in("rdi")a,in("rsi")b,
        inlateout("rdx")c=>value,lateout("rcx")_,lateout("r11")_,options(nostack))
    };
    // Sûreté : SVC v1 préserve tous les registres hors x0/x1 et restaure SP.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!("svc #0x4e",in("x8")id,inlateout("x0")a=>status,inlateout("x1")b=>value,
        in("x2")c,options(nostack))
    };
    Reply { status, value }
}
