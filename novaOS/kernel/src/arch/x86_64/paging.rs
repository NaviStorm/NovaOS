//! Descripteurs IA-32e de 4 Kio, contrôle de CR3 et invalidation locale.
use crate::{
    hal::memory::Access,
    memory::{Error, Result},
};
use core::arch::asm;
const NX: u64 = 1 << 63;

pub fn kernel_address(va: u64) -> bool {
    va >= 0xffff_8000_0000_0000
}
pub fn table(frame: u64) -> u64 {
    frame | 3
}
pub fn is_leaf(entry: u64, level: usize) -> bool {
    level == 3 || (level != 0 && entry & 128 != 0)
}
pub fn cache(entry: u64, level: usize) -> u64 {
    let pat = if level == 3 {
        entry & 128
    } else {
        (entry >> 5) & 128
    };
    entry & 0x18 | pat
}
pub fn leaf(frame: u64, access: Access, cache: u64) -> u64 {
    frame
        | 1
        | cache
        | match access {
            Access::ReadOnly => NX,
            Access::ReadWrite => NX | 2,
            Access::ReadExecute => 0,
        }
}
pub fn root() -> Result<u64> {
    let (cr3, cr4, mut cr0): (u64, u64, u64);
    let efer: u32;
    // Sûreté : le BSP est en mode long, anneau 0. EFER existe en mode long.
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nostack));
        asm!("mov {}, cr4", out(reg) cr4, options(nostack));
        asm!("mov {}, cr0", out(reg) cr0, options(nostack));
        asm!("rdmsr", in("ecx") 0xc0000080u32, out("eax") efer, out("edx") _, options(nostack));
    }
    if cr4 & ((1 << 12) | (1 << 17)) != 0 || efer & (1 << 11) == 0 {
        return Err(Error::Mode);
    }
    cr0 |= 1 << 16;
    // Sûreté : préserve le mode courant et impose WP pour les écritures superviseur.
    unsafe { asm!("mov cr0, {}", in(reg) cr0, options(nostack)) };
    Ok(cr3 & !0xfff)
}
pub fn invalidate(va: u64) {
    // Sûreté : adresse canonique, un seul processeur actif, instruction de niveau 0.
    unsafe { asm!("invlpg [{}]", in(reg) va, options(nostack, preserves_flags)) };
}

pub fn activate(space: &crate::memory::AddressSpace, _: &mut crate::memory::Frames) -> Result<()> {
    let cr4: u64;
    // Sûreté : la nouvelle racine couvre le code, la pile, la GDT et l’IDT.
    // La désactivation temporaire de PGE évacue aussi les traductions globales.
    unsafe {
        asm!("mov {}, cr4", out(reg) cr4, options(nostack));
        asm!("mov cr4, {}", "mov cr3, {}", "mov cr4, {}", in(reg) cr4 & !(1<<7), in(reg) space.root, in(reg) cr4, options(nostack));
    }
    Ok(())
}
pub fn map_devices(_: &crate::memory::AddressSpace, _: &mut crate::memory::Frames) -> Result<()> {
    Ok(())
}

pub fn matches(actual: u64, wanted: u64) -> bool {
    actual & !0x60 == wanted
} // A/D matériels
pub fn publish() {
    core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
}

pub const RETURN_42: [u8; 8] = [0xb8, 42, 0, 0, 0, 0xc3, 0, 0];
pub fn sync_code(_: u64, _: u64) {
    // Sérialise le flot d’instructions après publication de code sur ce BSP.
    core::arch::x86_64::__cpuid(0);
}
pub unsafe fn probe(address: u64, op: crate::hal::memory::Probe) {
    use crate::hal::memory::Probe::*;
    // Sûreté : scénario destructif isolé ; la sonde d’écriture réécrit le même
    // octet, et la cible d’appel contient une routine de test qui peut retourner.
    unsafe {
        match op {
            Write => {
                asm!("mov al, [{}]", "mov [{}], al", in(reg) address, in(reg) address, out("al") _)
            }
            Read => asm!("mov al, [{}]", in(reg) address, out("al") _),
            Execute => asm!("call {}", in(reg) address, clobber_abi("C")),
        }
    }
}

pub fn lower_empty(space: &crate::memory::AddressSpace, frames: &crate::memory::Frames) -> bool {
    frames.empty_prefix(space.root, 256)
}

pub const SHARED_UPPER: bool = true;
pub fn kernel_thread_root(kernel: u64) -> u64 {
    kernel
}
pub fn user_table(frame: u64) -> u64 {
    table(frame) | 4
}
pub fn user_leaf(frame: u64, access: Access, cache: u64) -> u64 {
    leaf(frame, access, cache) | 4
}
pub unsafe fn switch_thread_root(root: u64) {
    // Sûreté : racine validée par le gestionnaire ; PCID désactivé, aucune
    // feuille NovaOS globale. CR3 invalide toutes les traductions non globales.
    unsafe { asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags)) };
}
pub fn sync_user_code(alias: u64) {
    sync_code(alias, alias);
}
