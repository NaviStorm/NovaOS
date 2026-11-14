//! Accès sérialisé au mécanisme de configuration PCI n° 1 du segment zéro.
pub(super) fn port(bdf: u16, off: usize, bytes: usize, v: Option<u32>) -> u32 {
    use core::arch::asm;
    assert!(off < 256 && matches!(bytes, 1 | 2 | 4) && off % bytes == 0);
    let _guard = crate::sched::Guard::new();
    let address = 0x8000_0000 | ((bdf as u32) << 8) | (off as u32 & !3);
    let port = 0xcfcu16 + (off as u16 & 3);
    // Sûreté : CPL0 uniquement. L'adresse et la donnée du mécanisme PCI #1
    // sont sérialisées entre processeurs. Les écritures sont réservées au transport.
    unsafe {
        asm!("out dx,eax",in("dx")0xcf8u16,in("eax")address,options(nostack,preserves_flags));
        match (v, bytes) {
            (Some(x), 1) => {
                asm!("out dx,al",in("dx")port,in("al")x as u8,options(nostack,preserves_flags))
            }
            (Some(x), 2) => {
                asm!("out dx,ax",in("dx")port,in("ax")x as u16,options(nostack,preserves_flags))
            }
            (Some(x), 4) => {
                asm!("out dx,eax",in("dx")port,in("eax")x,options(nostack,preserves_flags))
            }
            (None, 1) => {
                let x: u8;
                asm!("in al,dx",in("dx")port,out("al")x,options(nostack,preserves_flags));
                return x.into();
            }
            (None, 2) => {
                let x: u16;
                asm!("in ax,dx",in("dx")port,out("ax")x,options(nostack,preserves_flags));
                return x.into();
            }
            (None, 4) => {
                let x: u32;
                asm!("in eax,dx",in("dx")port,out("eax")x,options(nostack,preserves_flags));
                return x;
            }
            _ => unreachable!(),
        }
    }
    0
}
