#[allow(dead_code)]
mod commun {
    include!("../diagnostic-5o/ecran.rs");
    pub(super) fn geometrie(st: &efi::SystemTable, map: &firmware::Map) -> Result<[u64; 6], u64> {
        let s = console(st, map)?;
        crate::check(s.admise(), 44)?;
        Ok([
            s.base,
            s.taille,
            s.largeur.into(),
            s.hauteur.into(),
            s.pas.into(),
            s.format.into(),
        ])
    }
    pub(super) fn lettre(c: u8) -> [u8; 7] {
        match c {
            b'1' => [4, 12, 4, 4, 4, 4, 14],
            b'2' => [14, 17, 1, 2, 4, 8, 31],
            b'3' => [30, 1, 1, 14, 1, 1, 30],
            b'5' => [31, 16, 16, 30, 1, 1, 30],
            b'6' => [14, 16, 16, 30, 17, 17, 14],
            b'7' => [31, 1, 2, 4, 8, 8, 8],
            b'8' => [14, 17, 17, 14, 17, 17, 14],
            b'9' => [14, 17, 17, 15, 1, 1, 14],
            _ => glyphe(c),
        }
    }
}
use crate::{Resultat, check, firmware, signature};
use alloc::format;
use r_efi::efi;
pub struct Admission([u64; 6]);
pub fn autoriser(
    st: &efi::SystemTable,
    map: &firmware::Map,
    r: &Resultat,
    old: &[u8],
    lease: &[u8],
) -> Result<Admission, u64> {
    let a = signature(old, 42)?;
    let b = signature(lease, 42)?;
    check(
        a.len() == 104
            && b.len() == 168
            && &a[..16] == b"NOVA5O-DISPLAY1!"
            && &b[..16] == b"NOVA5R-DISPLAY1!"
            && a[16..32] == r.epoque
            && a[32..64] == r.image
            && b[16..64] == a[16..64]
            && b[64..96] == a[64..96]
            && a[96..104] == [r.confiance as u8, 1, 1, 40, 0, 0, 0, 0]
            && b[96..104] == [r.confiance as u8, 1, 1, 0, 0, 0, 0, 0],
        44,
    )?;
    let f = commun::geometrie(st, map)?;
    check(
        a[64..72] == f[0].to_le_bytes()
            && a[72..80] == f[1].to_le_bytes()
            && (0..4).all(|i| a[80 + 4 * i..84 + 4 * i] == (f[2 + i] as u32).to_le_bytes()),
        44,
    )?;
    Ok(Admission(f))
}
impl Admission {
    pub fn faits(&self) -> [u64; 6] {
        self.0
    }
    pub fn afficher(&self, r: &Resultat, status: u64) {
        let raison = match status {
            0 => "USB AUTHENTIFIE",
            50 => "CONTRAT ABSENT OU FORMAT",
            51 => "SIGNATURE USB INVALIDE",
            52 => "EPOQUE OU INCARNATION",
            53 => "IDENTITE PE INVALIDE",
            54 => "ORIGINE USB INCOHERENTE",
            55 => "MAPPINGS DMA INCOHERENTS",
            56 => "TRANSACTION EN VOL",
            57 => "SUBSTITUTION DU CONTRAT",
            6 => "MANIFESTE INVALIDE",
            18 => "PAQUET ELF INVALIDE",
            _ => "INVENTAIRE INCOMPLET",
        };
        let titre = if status == 0 {
            format!("NOVAOS  DIAGNOSTIC USB VALIDE")
        } else {
            format!("NOVAOS  REFUS {status}")
        };
        let inventaire = format!(
            "PCI {}   IOAPIC {}   HPET {}   DRHD {}",
            r.pci, r.ioapic, r.hpet, r.drhd
        );
        let ligne = if status == 0 && r.compatibles > 0 {
            "PILOTE COMPATIBLE  IOAPIC"
        } else {
            "AUCUNE SELECTION PUBLIEE"
        };
        let hpet = if status == 0 && r.conflits > 0 {
            "HPET A ET B  CONFLIT"
        } else {
            "HPET  AUCUN PLAN PUBLIE"
        };
        let lines = [
            titre.as_str(),
            raison,
            if r.confiance == 1 {
                "SECURE BOOT ACTIF"
            } else {
                "SECURE BOOT INACTIF"
            },
            if r.usb == 1 {
                "DMA PROTEGEE  BANC Q35"
            } else {
                "DMA USB NON VALIDEE"
            },
            inventaire.as_str(),
            ligne,
            hpet,
            "ACTIVATION MATERIELLE  AUCUNE",
        ];
        for y in 0..120 {
            for x in 0..544 {
                let fg = if x >= 8 && y >= 3 {
                    let dy = (y - 3) % 14;
                    let dx = (x - 8) % 12;
                    lines
                        .get((y - 3) / 14)
                        .and_then(|s| s.as_bytes().get((x - 8) / 12))
                        .is_some_and(|c| {
                            dy < 14 && dx < 10 && commun::lettre(*c)[dy / 2] & (16 >> (dx / 2)) != 0
                        })
                } else {
                    false
                };
                unsafe {
                    core::ptr::write_volatile(
                        (self.0[0] as *mut u32).add(y * self.0[4] as usize + x),
                        if fg { 0xffffff } else { 0x161616 },
                    )
                }
            }
        }
    }
}
