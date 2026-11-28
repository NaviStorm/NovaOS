//! Vérification hors matériel d'un dossier VT-d à partir de descriptions figées.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
use alloc::vec::Vec;

#[allow(dead_code)]
#[path = "../diagnostic-5e/topologie5i.rs"]
mod topologie5i;
use topologie5i::{Dmar, Function, Rid, validate_bridges};

mod permissions;
pub use permissions::*;

const MAX_FONCTIONS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adresse {
    pub segment: u16,
    pub rid: u16,
}

impl Adresse {
    fn pci(self) -> Rid {
        Rid {
            seg: self.segment,
            bus: (self.rid >> 8) as u8,
            dev: (self.rid as u8) >> 3,
            fun: self.rid as u8 & 7,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Fonction {
    pub adresse: Adresse,
    /// Titulaire attesté par le fournisseur de l'inventaire, y compris pour un RID aliasé.
    pub titulaire: Option<Adresse>,
    /// Code de classe PCI à 24 bits.
    pub classe: u32,
    pub pont: Option<(u8, u8)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refus {
    Limite,
    Table,
    Portee,
    Ambigu,
    Couverture,
    Alias,
    Role,
    Unite,
    Domaine,
    Plage,
    Permission,
    Reservation,
    Stockage,
    Continuite,
}

type Resultat<T> = Result<T, Refus>;

fn ajouter<T>(v: &mut Vec<T>, x: T) -> Resultat<()> {
    v.try_reserve(1).map_err(|_| Refus::Limite)?;
    v.push(x);
    Ok(())
}

pub struct Inventaire<'a> {
    fonctions: &'a [Fonction],
    unites: Vec<u64>,
    affectation: [u64; MAX_FONCTIONS],
    reservations: Vec<(u64, Plage)>,
    bits: u8,
}

fn valider_fonctions(fs: &[Fonction]) -> Resultat<()> {
    if fs.is_empty() || fs.len() > MAX_FONCTIONS {
        return Err(Refus::Limite);
    }
    let mut ponts = Vec::new();
    for (i, f) in fs.iter().enumerate() {
        let id = f.adresse.pci();
        if fs[..i].iter().any(|x| x.adresse == f.adresse)
            || f.classe > 0xffffff
            || (f.classe >> 8 == 0x0604) != f.pont.is_some()
            || f.pont.is_some_and(|(s, e)| s <= id.bus || s > e)
        {
            return Err(Refus::Portee);
        }
        ajouter(
            &mut ponts,
            Function {
                id,
                class: f.classe,
                bridge: f.pont,
                ident: 0,
                header: 0,
                command: 0,
            },
        )?;
    }
    validate_bridges(&ponts).map_err(|_| Refus::Ambigu)?;
    for f in fs {
        let bus = f.adresse.pci().bus;
        let parent = fs
            .iter()
            .filter(|p| p.adresse.segment == f.adresse.segment)
            .filter_map(|p| p.pont)
            .filter(|&(s, e)| (s..=e).contains(&bus))
            .max_by_key(|&(s, _)| s);
        if parent.is_some_and(|(s, _)| s != bus) {
            return Err(Refus::Portee);
        }
    }
    Ok(())
}

// Les bornes du chemin PCI sont déjà validées par le décodeur 5I.
fn portee(sc: &[u8], segment: u16, fs: &[Fonction], speciaux: &mut Vec<Adresse>) -> Resultat<u64> {
    let mut bus = sc[5];
    let mut chemin = sc[6..].chunks_exact(2).peekable();
    while let Some(df) = chemin.next() {
        let adresse = Adresse {
            segment,
            rid: u16::from_be_bytes([bus, df[0] << 3 | df[1]]),
        };
        let index = fs.iter().position(|f| f.adresse == adresse);
        if chemin.peek().is_none() && matches!(sc[0], 3 | 4) {
            if index.is_some() || speciaux.contains(&adresse) {
                return Err(Refus::Ambigu);
            }
            ajouter(speciaux, adresse)?;
            return Ok(0);
        }
        let i = index.ok_or(Refus::Portee)?;
        if chemin.peek().is_some() {
            bus = fs[i].pont.ok_or(Refus::Portee)?.0;
        } else {
            return match (sc[0], fs[i].pont) {
                (1, None) => Ok(1u64 << i),
                (2, Some((s, e))) => Ok(fs.iter().enumerate().fold(1u64 << i, |mask, (j, f)| {
                    if f.adresse.segment == segment && (s..=e).contains(&f.adresse.pci().bus) {
                        mask | (1u64 << j)
                    } else {
                        mask
                    }
                })),
                _ => Err(Refus::Portee),
            };
        }
    }
    Err(Refus::Portee)
}

impl<'a> Inventaire<'a> {
    /// Une table DMAR ACPI et l'inventaire PCI complet déclaré par l'appelant.
    /// Le succès décrit la couverture de ces données et ne concède aucun droit.
    pub fn decouvrir(tables: &[&[u8]], fs: &'a [Fonction]) -> Resultat<Self> {
        let inv = Self::resoudre(tables, fs)?;
        for (i, f) in fs.iter().enumerate() {
            let owner = f.titulaire.ok_or(Refus::Alias)?;
            let j = inv.index(owner).map_err(|_| Refus::Alias)?;
            if fs[j].titulaire != Some(owner) || inv.affectation[i] != inv.affectation[j] {
                return Err(Refus::Alias);
            }
        }
        Ok(inv)
    }

    fn resoudre(tables: &[&[u8]], fs: &'a [Fonction]) -> Resultat<Self> {
        let [t] = tables else {
            return Err(Refus::Table);
        };
        if !(48..=16384).contains(&t.len())
            || &t[..4] != b"DMAR"
            || u32::from_le_bytes(t[4..8].try_into().unwrap()) as usize != t.len()
            || t[8] != 1
            || !(31..=63).contains(&t[36])
            || t[37] & !7 != 0
            || t[38..48] != [0; 10]
            || t.iter().fold(0u8, |n, b| n.wrapping_add(*b)) != 0
        {
            return Err(Refus::Table);
        }
        valider_fonctions(fs)?;
        let d = Dmar::parse(t).map_err(|_| Refus::Table)?;
        if d.incomplete {
            return Err(Refus::Portee);
        }
        let mut inv = Self {
            fonctions: fs,
            affectation: [0; MAX_FONCTIONS],
            unites: Vec::new(),
            reservations: Vec::new(),
            bits: t[36] + 1,
        };
        let mut defauts = Vec::new();
        let mut speciaux = Vec::new();
        let mut offset = 48;
        for r in d.records {
            let len = u16::from_le_bytes([t[offset + 2], t[offset + 3]]) as usize;
            if (r.kind == 1 && t[offset + 4..offset + 6] != [0; 2]) || !inv.adressable(r.end) {
                return Err(Refus::Table);
            }
            offset += len;
            let mut masque = 0u64;
            let mut scopes = r.scopes;
            while !scopes.is_empty() {
                let (sc, rest) = scopes.split_at(scopes[1] as usize);
                scopes = rest;
                if (matches!(sc[0], 1 | 2) && (sc[4] != 0 || r.all))
                    || (matches!(sc[0], 3 | 4) && r.kind == 1)
                {
                    return Err(Refus::Portee);
                }
                let m = portee(sc, r.segment, fs, &mut speciaux)?;
                if masque & m != 0 {
                    return Err(Refus::Ambigu);
                }
                masque |= m;
            }
            if r.kind == 1 {
                let plage = Plage {
                    debut: r.start,
                    octets: r
                        .end
                        .checked_sub(r.start)
                        .and_then(|d| d.checked_add(1))
                        .ok_or(Refus::Plage)?,
                };
                ajouter(&mut inv.reservations, (masque, plage))?;
            } else {
                if defauts.iter().any(|&(s, _)| s == r.segment) {
                    return Err(Refus::Ambigu);
                }
                ajouter(&mut inv.unites, r.start)?;
                if r.all {
                    ajouter(&mut defauts, (r.segment, r.start))?;
                } else {
                    if r.scopes.is_empty() {
                        return Err(Refus::Portee);
                    }
                    for (i, u) in inv.affectation[..fs.len()].iter_mut().enumerate() {
                        if masque & (1u64 << i) != 0 {
                            if *u != 0 {
                                return Err(Refus::Ambigu);
                            }
                            *u = r.start;
                        }
                    }
                }
            }
        }
        for (i, f) in fs.iter().enumerate() {
            if inv.affectation[i] == 0 {
                inv.affectation[i] = defauts
                    .iter()
                    .find(|(s, _)| *s == f.adresse.segment)
                    .ok_or(Refus::Couverture)?
                    .1;
            }
        }
        Ok(inv)
    }

    fn adressable(&self, p: u64) -> bool {
        self.bits == 64 || p < 1u64 << self.bits
    }
    fn index(&self, id: Adresse) -> Resultat<usize> {
        self.fonctions
            .iter()
            .position(|f| f.adresse == id)
            .ok_or(Refus::Couverture)
    }
    pub fn unite(&self, id: Adresse) -> Resultat<u64> {
        Ok(self.affectation[self.index(id)?])
    }
    pub fn nombre_unites(&self) -> usize {
        self.unites.len()
    }
}

/// Affectations déclarées par ACPI, dans l’ordre des fonctions fournies.
/// Ce type ne peut pas être utilisé pour vérifier un bail ou accorder un droit.
pub struct CouvertureDeclaree {
    pub unites: Vec<u64>,
    pub affectations: Vec<u64>,
}

pub fn decrire(table: &[u8], fs: &[Fonction]) -> Resultat<CouvertureDeclaree> {
    let inv = Inventaire::resoudre(&[table], fs)?;
    let mut affectations = Vec::new();
    affectations
        .try_reserve_exact(fs.len())
        .map_err(|_| Refus::Limite)?;
    affectations.extend_from_slice(&inv.affectation[..fs.len()]);
    Ok(CouvertureDeclaree {
        unites: inv.unites,
        affectations,
    })
}

#[cfg(test)]
mod tests;
