//! Ces profils ne sont attribués que par la sonde ELF du lanceur.
use crate::runtime::*;
use alloc::{boxed::Box, vec::Vec};
use core::hint::black_box;

static mut BSS: [u8; 8193] = [0; 8193];
static READ_ONLY: [u8; 8] = *b"NovaRO!!";
static mut DATA: u64 = 0x3a119205;
#[inline(never)]
fn descend(depth: usize) -> usize {
    let mut frame = [0u8; 512];
    black_box(&mut frame)[depth % 512] = 1;
    let n = if depth == 0 {
        0
    } else {
        descend(black_box(depth - 1))
    };
    n + usize::from(black_box(frame)[depth % 512])
}
fn resize(old: u64, new: u64) -> Reply {
    call(HEAP_RESIZE, old, new, 0)
}
// Lecture effectuée par une instruction utilisateur dans une VM de qualification.
pub(super) fn load(address: u64) -> u64 {
    let n: u64;
    // Sûreté : la sonde de faute s'exécute dans le processus jetable.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!("mov {}, [{}]",out(reg)n,in(reg)address,options(nostack))
    }
    // Sûreté : même sonde confinée, instruction AArch64.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("ldr {}, [{}]",out(reg)n,in(reg)address,options(nostack))
    }
    n
}
pub(super) fn run(mode: u64) -> Result<()> {
    let base = value(resize(0, 0))?;
    let end = base + 8192;
    let stamp = |s: &[u8]| value(call(DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0));
    match mode {
        101 => {
            // Sûreté : la BSS est privée au seul thread du service et vient du chargeur.
            unsafe {
                let p = (&raw mut BSS).cast::<u8>();
                if p.read_volatile() != 0 || p.add(8192).read_volatile() != 0 {
                    return Err(REFUSED);
                }
                p.add(8192).write_volatile(41);
                if p.add(8192).read_volatile() != 41 {
                    return Err(REFUSED);
                }
            }
            // Sûreté : objet aligné appartenant au segment RW du processus neuf.
            unsafe {
                let p = &raw mut DATA;
                if p.read_volatile() != 0x3a119205 {
                    return Err(REFUSED);
                }
                p.write_volatile(0x42);
            }
            if descend(24) != 25 {
                return Err(REFUSED);
            }
            stamp(b"chrono debut")?;
            stamp(b"vide")?;
            if resize(base, base + 1).status != BAD_ARGUMENT
                || resize(base, 1 << 47).status != NO_MEMORY
            {
                return Err(REFUSED);
            }
            stamp(b"chrono debut")?;
            value(resize(base, end))?;
            stamp(b"extension_2")?;
            if load(base) != 0 || load(end - 4096) != 0 {
                return Err(REFUSED);
            }
            stamp(b"chrono debut")?;
            value(resize(end, base))?;
            stamp(b"retrait_2")?;
            if resize(end, base).status != BAD_ARGUMENT {
                return Err(REFUSED);
            }
            let mut bytes = Vec::new();
            if bytes.try_reserve_exact(1usize << 40).is_ok() || value(resize(0, 0))? != base {
                return Err(REFUSED);
            }
            bytes.try_reserve_exact(128 * 1024).map_err(|_| NO_MEMORY)?;
            bytes.resize(128 * 1024, 0x57u8);
            if black_box(&bytes)[127 * 1024] != 0x57 {
                return Err(REFUSED);
            }
            drop(bytes);
            stamp(b"chrono debut")?;
            for i in 0..4096 {
                black_box(Box::new(black_box(i)));
            }
            stamp(b"alloc_free_4096")?;
            let mut objects = Vec::new();
            objects.try_reserve_exact(512).map_err(|_| NO_MEMORY)?;
            stamp(b"chrono debut")?;
            for i in 0..512 {
                objects.push(Box::new(black_box(i)));
            }
            stamp(b"alloc_512")?;
            stamp(b"chrono debut")?;
            objects.clear();
            stamp(b"free_512")?;
            Ok(())
        }
        102 => {
            value(resize(base, end))?;
            value(resize(end, base))?;
            black_box(load(base));
            Err(REFUSED)
        }
        103 => {
            black_box(descend(16384));
            Err(REFUSED)
        }
        104 => {
            black_box(load(0x20000));
            Err(REFUSED)
        }
        105 => {
            // La sonde du fournisseur de pages doit interrompre cette allocation.
            let b = alloc::vec![0x72u8;128*1024];
            black_box(&b);
            Err(REFUSED)
        }
        106 | 107 => {
            let p = value(resize(base, end))? - 8192;
            // Sûreté : p désigne le début d'une page appartenant au processus.
            unsafe { (p as *mut u64).write_volatile(mode) }
            for _ in 0..8 {
                value(call(THREAD_YIELD, 0, 0, 0))?;
            }
            if load(p) != mode {
                return Err(REFUSED);
            }
            Ok(())
        }
        108 | 111 => {
            let addr = if mode == 108 {
                descend as *const () as usize
            } else {
                READ_ONLY.as_ptr() as usize
            };
            // Sûreté : sonde confinée d'écriture sur le texte RX ou les constantes R de ce processus.
            #[cfg(target_arch = "x86_64")]
            unsafe {
                core::arch::asm!("mov byte ptr [{}],0",in(reg)addr,options(nostack))
            }
            // Sûreté : même sonde confinée, instruction AArch64.
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("strb wzr,[{}]",in(reg)addr,options(nostack))
            }
            Err(REFUSED)
        }
        109 => {
            let addr = &raw mut DATA;
            // Sûreté : branchement de test dans la page NX privée du processus jetable.
            #[cfg(target_arch = "x86_64")]
            unsafe {
                core::arch::asm!("jmp {}",in(reg)addr,options(noreturn))
            }
            // Sûreté : même sonde confinée, instruction AArch64.
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("br {}",in(reg)addr,options(noreturn))
            }
        }
        110 => {
            black_box(load((1 << 46) - 8192));
            Err(REFUSED)
        }
        _ => Err(BAD_ARGUMENT),
    }
}
