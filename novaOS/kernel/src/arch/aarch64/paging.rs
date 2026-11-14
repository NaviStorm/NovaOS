//! Traduction stage 1 EL1, granule 4 Kio, VA de 48 bits.
use super::serial;
use crate::{
    hal::memory::Access,
    memory::{AddressSpace, Error, Frames, Result},
};
use core::{
    arch::{asm, global_asm},
    sync::atomic::Ordering,
};
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;
const AF: u64 = 1 << 10;
const RO: u64 = 1 << 7;

pub fn kernel_address(va: u64) -> bool {
    va >= 0xffff_0000_0000_0000
}
pub fn table(frame: u64) -> u64 {
    frame | 3
}
pub fn is_leaf(entry: u64, level: usize) -> bool {
    level == 3 || entry & 2 == 0
}
pub fn cache(entry: u64, _: usize) -> u64 {
    entry & 0x31c
} // SH, AttrIndx
pub fn leaf(frame: u64, access: Access, cache: u64) -> u64 {
    frame
        | 3
        | AF
        | UXN
        | cache
        | match access {
            Access::ReadOnly => PXN | RO,
            Access::ReadWrite => PXN,
            Access::ReadExecute => RO,
        }
}
pub fn matches(actual: u64, wanted: u64) -> bool {
    actual == wanted
}
pub fn publish() {
    // Sûreté : publication des tables en RAM normale avant le descripteur parent.
    unsafe { asm!("dsb ishst", options(nostack, preserves_flags)) };
}
pub fn invalidate(_: u64) {
    // Sûreté : dans le domaine de partage interne, invalide les caches de marche des
    // tables avant de rendre les cadres. Les autres ASID ne sont pas utilisés.
    unsafe {
        asm!(
            "dsb ishst",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags)
        )
    };
}
pub fn root() -> Result<u64> {
    let (el, ttbr, tcr, sctlr): (u64, u64, u64, u64);
    // Sûreté : Limine sur virt transfère le contrôle à un niveau privilégié.
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) el);
        if el != 4 {
            return Err(Error::Mode);
        }
        asm!("mrs {}, ttbr1_el1", out(reg) ttbr);
        asm!("mrs {}, tcr_el1", out(reg) tcr);
        asm!("mrs {}, sctlr_el1", out(reg) sctlr);
    }
    if (tcr >> 16) & 63 != 16 || (tcr >> 30) & 3 != 2 || tcr & (1 << 59) != 0 || sctlr & 1 == 0 {
        return Err(Error::Mode);
    }
    Ok(ttbr & 0x0000_ffff_ffff_f000)
}
pub fn map_devices(space: &AddressSpace, frames: &mut Frames) -> Result<()> {
    let mair: u64;
    // Sûreté : lecture des attributs installés par Limine, conservés pour éviter
    // tout changement de type mémoire pendant la transition.
    unsafe { asm!("mrs {}, mair_el1", out(reg) mair, options(nostack, preserves_flags)) };
    let index = (0..8)
        .find(|i| (mair >> (i * 8)) & 255 == 0)
        .ok_or(Error::Mode)?;
    use super::timer;
    for (va, pa) in [
        (serial::VA, serial::PA),
        (super::rtc::VA, super::rtc::PA),
        (timer::DIST, timer::DIST_PA),
    ]
    .into_iter()
    .chain(super::gic::PAGES.iter().copied())
    {
        if frames.info.entries.iter().any(|e| {
            crate::memory::mapped(e.entry_type) && e.base < pa + 4096 && e.base + e.length > pa
        }) {
            return Err(Error::Geometry);
        }
        space.insert(frames, va, leaf(pa, Access::ReadWrite, index << 2))?;
    }
    Ok(())
}

// Sûreté : thunk sans pile, sans littéral et sans adresse absolue. Le code
// s’exécute par TTBR0 pendant le break-before-make de TTBR1. x30 désigne
// l’instruction de retour en moitié haute, présente dans la nouvelle racine.
global_asm!(
    r#"
.pushsection .text.root_switch,"ax"
.balign 4096
.global nova_switch_root
.hidden nova_switch_root
nova_switch_root:
    msr ttbr1_el1, x1
    isb
    tlbi vmalle1is
    dsb ish
    isb
    msr ttbr1_el1, x0
    isb
    ret
.popsection
"#
);

pub fn activate(space: &AddressSpace, frames: &mut Frames) -> Result<()> {
    // Sûreté : symbole du thunk ci-dessus, aligné et contenu dans une page RX.
    unsafe extern "C" {
        static nova_switch_root: u8;
    }
    let high = (&raw const nova_switch_root) as u64;
    let low = 0x400000;
    let thunk = space.descriptor(frames, high)?;
    let temporary = AddressSpace::new(frames)?;
    temporary.insert(frames, low, thunk)?;
    let empty = frames.take_table()?; // Racine TTBR0 permanente, aucune page EL0.
    install_roots(space.root, temporary.root, empty, low);
    EMPTY.store(empty, Ordering::Release);
    if crate::smp::count() > 1 {
        // Le trampoline reste RX, superviseur, dans une racine basse réservée au démarrage.
        ROOT.store(space.root, Ordering::Relaxed);
        BRIDGE.store(temporary.root, Ordering::Release);
    } else {
        temporary.remove(frames, low)?;
        frames.release_table(temporary.root);
    }
    serial::READY.store(true, Ordering::Release);
    Ok(())
}

pub const RETURN_42: [u8; 8] = [0x40, 0x05, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6];

pub fn sync_code(written: u64, execute: u64) {
    let ctr: u64;
    // Sûreté : le cadre de code est accessible par ces deux alias du noyau.
    // Nettoie au point d’unification avant d’invalider le cache d’instructions.
    unsafe {
        asm!("mrs {}, ctr_el0", out(reg) ctr, options(nostack));
        let dline = 4 << ((ctr >> 16) & 15);
        let iline = 4 << (ctr & 15);
        for i in (0..4096).step_by(dline) {
            asm!("dc cvau, {}", in(reg) written + i as u64);
        }
        asm!("dsb ish");
        for i in (0..4096).step_by(iline) {
            asm!("ic ivau, {}", in(reg) execute + i as u64);
        }
        asm!("dsb ish", "isb");
    }
}
pub unsafe fn probe(address: u64, op: crate::hal::memory::Probe) {
    use crate::hal::memory::Probe::*;
    // Sûreté : uniquement sur adresse de test ; la cible de BLR est alignée et
    // contient un retour valide si PXN venait à manquer.
    unsafe {
        match op {
            Write => {
                asm!("ldrb w9, [{addr}]", "strb w9, [{addr}]", addr=in(reg) address, out("x9") _)
            }
            Read => asm!("ldrb wzr, [{}]",in(reg) address),
            Execute => asm!("blr {}",in(reg) address, clobber_abi("C")),
        }
    }
}

pub fn lower_empty(_: &AddressSpace, frames: &Frames) -> bool {
    let root: u64;
    // Sûreté : lecture de la racine basse installée par activate sur le BSP à EL1.
    unsafe { asm!("mrs {}, ttbr0_el1",out(reg) root, options(nostack)) };
    frames.covers(root, limine::memory_map::EntryType::USABLE) && frames.empty_prefix(root, 512)
}

pub const SHARED_UPPER: bool = false;
pub fn kernel_thread_root(_: u64) -> u64 {
    let value: u64;
    // Sûreté : appelé à EL1 au démarrage, TTBR0 désigne la racine basse vide.
    unsafe { asm!("mrs {},ttbr0_el1",out(reg) value,options(nostack)) };
    value
}
pub fn user_table(frame: u64) -> u64 {
    table(frame)
}
pub fn user_leaf(frame: u64, access: Access, cache: u64) -> u64 {
    let el0 = (1 << 6) | (1 << 11); // AP[1], nG
    let mut desc = leaf(frame, access, cache) | el0 | PXN;
    if access == Access::ReadExecute {
        desc &= !UXN;
    }
    desc
}
pub unsafe fn switch_thread_root(root: u64) {
    // Un ASID distinct par PE empêche la confusion de deux racines actives.
    // Avant de réutiliser son ASID, le PE sélectionne une racine vide (ASID 0)
    // puis termine les invalidations. Aucune exécution EL0 pendant ce passage.
    let asid = ((crate::smp::index() + 1) as u64) << 48;
    let empty = EMPTY.load(Ordering::Acquire);
    unsafe {
        asm!("dsb ishst", "msr ttbr0_el1,{empty}", "isb", "tlbi aside1is,{asid}",
            "dsb ish", "isb", "msr ttbr0_el1,{tagged}", "isb",
            empty=in(reg) empty, asid=in(reg) asid, tagged=in(reg) (root | asid),
            options(nostack, preserves_flags))
    };
}
pub fn sync_user_code(alias: u64) {
    sync_code(alias, alias);
    // Sûreté : le code a été nettoyé au PoU par son alias HHDM. La VA
    // utilisateur est encore inactive ; invalider tous les alias I du domaine avant publication.
    unsafe {
        asm!(
            "ic ialluis",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags)
        )
    };
}

pub fn device_cache() -> Result<u64> {
    let mair: u64;
    // Sûreté : lecture EL1 du MAIR actif, recherche de Device-nGnRnE.
    unsafe { asm!("mrs {},mair_el1",out(reg) mair,options(nostack)) };
    (0..8u64)
        .find(|i| (mair >> (8 * i)) & 255 == 0)
        .map(|i| i << 2)
        .ok_or(Error::Mode)
}

use core::sync::atomic::AtomicU64;
static ROOT: AtomicU64 = AtomicU64::new(0);
static BRIDGE: AtomicU64 = AtomicU64::new(0);
static EMPTY: AtomicU64 = AtomicU64::new(0);

pub fn secondary() {
    let temporary = BRIDGE.load(Ordering::Acquire);
    assert_ne!(temporary, 0);
    install_roots(
        ROOT.load(Ordering::Relaxed),
        temporary,
        EMPTY.load(Ordering::Relaxed),
        0x400000,
    );
}

fn install_roots(root: u64, temporary: u64, empty: u64, low: u64) {
    let tcr: u64;
    // Sûreté : seul le régime TTBR0, inutilisé par Limine rév. 3, est reconfiguré.
    // Le code, la pile et VBAR ont des VA hautes couvertes par les deux espaces.
    unsafe { asm!("mrs {}, tcr_el1", out(reg) tcr, options(nostack)) };
    let lower = (tcr & !0xffff & !(1 << 37) & !(1 << 22)) | 16 | (1 << 8) | (1 << 10) | (3 << 12);
    // Sûreté : la branche basse garde la même traduction pendant tout le
    // remplacement de TTBR1. Les barrières séparent invalidation et publication.
    unsafe {
        asm!("dsb ishst", "msr tcr_el1, x5", "isb", "tlbi vmalle1is", "dsb ish", "isb",
            "msr ttbr0_el1, x3", "msr tcr_el1, x4", "isb", "blr x2",
            "msr ttbr0_el1, x1", "isb", "tlbi vmalle1is", "dsb ish", "isb",
            in("x0") root, in("x1") empty, in("x2") low, in("x3") temporary,
            in("x4") lower, in("x5") tcr | (1<<7), out("x30") _, options(nostack));
    }
}
