//! Diagnostic local du profil Live. Le journal reste lisible même sans surface.
use crate::{boot, hardware::framebuffer::Screen};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use font8x8::legacy::BASIC_LEGACY;

pub const ENTREE: u64 = 1;
pub const MEMOIRE: u64 = 2;
pub const ORDONNANCEUR: u64 = 4;
pub const SMP: u64 = 8;
const TERMINE: u64 = 16;
const SANS_ECRAN: u64 = 256;
const LARGEUR: usize = 256;
const HAUTEUR: usize = 88;

// Bits 0..4 : jalons, bit 8 : sans vidéo, bits 16..23 : premier refus.
#[unsafe(no_mangle)]
static NOVA6A_JOURNAL: AtomicU64 = AtomicU64::new(0);
// Historique borné append-only. Un enregistrement reste disponible après l'arrêt.
#[unsafe(no_mangle)]
static NOVA6A_EVENEMENTS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
fn consigner(code: u64) {
    for cellule in &NOVA6A_EVENEMENTS {
        if cellule
            .compare_exchange(0, code, Ordering::Release, Ordering::Relaxed)
            .is_ok()
        {
            break;
        }
    }
}

struct Affichage(UnsafeCell<Option<Screen>>);
// Sûreté : chaque accès à la surface est exclusif, y compris en cas de panique.
unsafe impl Sync for Affichage {}
static ECRAN: Affichage = Affichage(UnsafeCell::new(None));
static OCCUPE: AtomicBool = AtomicBool::new(false);

fn surface(f: impl FnOnce(&mut Option<Screen>)) {
    let _irq = crate::hal::interrupts::Guard::new();
    if OCCUPE
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_ok()
    {
        // Sûreté : le verrou est détenu et aucun callback ne conserve cette référence.
        f(unsafe { &mut *ECRAN.0.get() });
        OCCUPE.store(false, Ordering::Release);
    }
}

fn ligne(ecran: &mut Screen, index: usize, texte: &[u8]) {
    let z = 3.min(ecran.width / LARGEUR).min(ecran.height / HAUTEUR);
    let fond = ecran.rgb(16, 24, 39);
    let encre = ecran.rgb(240, 244, 248);
    if index == 0 {
        ecran.fill(0, 0, LARGEUR * z, HAUTEUR * z, fond);
    }
    let haut = 4 + 13 * index;
    ecran.fill(0, haut * z, LARGEUR * z, 10 * z, fond);
    for (i, &c) in texte.iter().enumerate() {
        for (y, bits) in BASIC_LEGACY[c as usize].iter().enumerate() {
            for x in 0..8 {
                if bits & (1 << x) != 0 {
                    ecran.fill((4 + i * 8 + x) * z, (haut + y) * z, z, z, encre);
                }
            }
        }
    }
}

pub fn debut() {
    let video = boot::surface_live(true).filter(|s| s.width >= LARGEUR && s.height >= HAUTEUR);
    if video.is_none() {
        consigner(SANS_ECRAN);
        NOVA6A_JOURNAL.fetch_or(SANS_ECRAN, Ordering::Release);
    }
    surface(|slot| {
        *slot = video;
        if let Some(s) = slot {
            ligne(s, 0, b"NovaOS - Diagnostic noyau");
        }
    });
    jalon(ENTREE);
}

pub fn jalon(bit: u64) {
    let mut smp = *b"4  SMP verifie - 00 CPU";
    let mut evenement = bit;
    let (i, texte): (_, &[u8]) = match bit {
        ENTREE => (1, b"1  Entree noyau"),
        MEMOIRE => (2, b"2  Memoire initialisee"),
        ORDONNANCEUR => (3, b"3  Ordonnanceur actif"),
        SMP => {
            let n = crate::smp::count() as u8;
            let p = b"4  SMP verifie - ".len();
            smp[p] = b'0' + n / 10;
            smp[p + 1] = b'0' + n % 10;
            evenement |= u64::from(n) << 16;
            (4, &smp)
        }
        _ => return,
    };
    consigner(evenement);
    NOVA6A_JOURNAL.fetch_or(bit, Ordering::Release);
    surface(|slot| {
        if let Some(s) = slot {
            ligne(s, i, texte);
        }
    });
}

pub fn refus(code: u8) {
    let premier = NOVA6A_JOURNAL.fetch_update(Ordering::AcqRel, Ordering::Acquire, |etat| {
        (etat >> 16 == 0).then_some(etat | (u64::from(code) << 16))
    });
    if premier.is_ok() {
        consigner(0x200 | u64::from(code));
    }
    let code = (NOVA6A_JOURNAL.load(Ordering::Acquire) >> 16) as u8;
    let motif: &[u8] = match code {
        1 => b"REFUS R01 - relais memoire",
        2 => b"REFUS R02 - memoire noyau",
        3 => b"REFUS R03 - profil de boot",
        4 => b"REFUS R04 - avant ordonnanceur",
        5 => b"REFUS R05 - execution ou SMP",
        _ => b"REFUS R06 - ecran indisponible",
    };
    surface(|slot| {
        if let Some(s) = slot {
            ligne(s, 5, motif);
        }
    });
}

pub fn panique() {
    let etat = NOVA6A_JOURNAL.load(Ordering::Acquire);
    if etat >> 16 == 0 {
        refus(if etat & ORDONNANCEUR == 0 { 4 } else { 5 });
    }
}

pub fn terminer(normal: bool) {
    let j = NOVA6A_JOURNAL.load(Ordering::Acquire);
    if j >> 16 == 0 {
        if !normal || j & 15 != 15 {
            refus(if j & ORDONNANCEUR == 0 { 4 } else { 5 });
        } else if j & SANS_ECRAN != 0 {
            refus(6);
        } else {
            surface(|slot| {
                if let Some(s) = slot {
                    ligne(s, 5, b"VERDICT OK - arret local");
                }
            });
        }
    }
    consigner(TERMINE);
    let fin = NOVA6A_JOURNAL.fetch_or(TERMINE, Ordering::AcqRel) | TERMINE;
    if let Some(ecrire) = crate::arch::DIAGNOSTIC {
        ecrire(format_args!(
            "NOVA6A journal={fin:#x} refus=R{:02}",
            fin >> 16
        ));
    }
}
