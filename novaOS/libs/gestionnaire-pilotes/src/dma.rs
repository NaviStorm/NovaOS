//! Registre logique du simulateur. Aucun identifiant de cadre physique, aucune IOVA.
use crate::{Error, Result};
use alloc::{string::String, vec::Vec};

pub const MAX_FRAMES: usize = 259;
const WORDS: usize = MAX_FRAMES.div_ceil(64);

/// Ensemble transitoire porté par l'oracle CPU, absent des groupes résidents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageSet([u64; WORDS]);
impl PageSet {
    pub fn range(first: usize, count: usize) -> Result<Self> {
        let end = first
            .checked_add(count)
            .filter(|n| *n <= MAX_FRAMES)
            .ok_or(Error::Format)?;
        let mut set = Self::default();
        for i in first..end {
            set.0[i / 64] |= 1 << (i % 64);
        }
        Ok(set)
    }
    /// Ajoute une preuve individuelle, y compris dans un reçu non contigu.
    pub fn insert(&mut self, page: usize) -> Result<()> {
        if page >= MAX_FRAMES {
            return Err(Error::Format);
        }
        self.0[page / 64] |= 1u64 << (page % 64);
        Ok(())
    }
    pub fn contains(&self, i: usize) -> bool {
        i < MAX_FRAMES && self.0[i / 64] & (1 << (i % 64)) != 0
    }
    fn fits(&self, len: usize) -> bool {
        self.0.iter().enumerate().all(|(i, word)| {
            let bits = len.saturating_sub(i * 64).min(64);
            bits == 64 || word >> bits == 0
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Direction {
    Unspecified,
    Read,
    Write,
    Both,
}
impl Direction {
    fn parse(s: &str) -> Result<Self> {
        match s.rsplit_once(':').map(|(_, m)| m) {
            Some("r") => Ok(Self::Read),
            Some("w") => Ok(Self::Write),
            Some("rw") => Ok(Self::Both),
            _ => Err(Error::Format),
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Region {
    pub name: &'static str,
    pub frames: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageInfo {
    /// Index dans le quota global du simulateur. Il ne désigne aucune adresse.
    pub frame: u16,
    pub direction: Direction,
    pub exposed: bool,
    pub source_stopped: bool,
    pub translation_removed: bool,
    pub ownership_returned: bool,
    pub released: bool,
    pub quarantined: bool,
}
const EXPOSED: u8 = 4;
const STOPPED: u8 = 8;
const UNMAPPED: u8 = 16;
const OWNED: u8 = 32;
const RELEASE: u8 = STOPPED | UNMAPPED | OWNED;

// Trois octets par page, sans alignement implicite entre entrées.
#[derive(Clone, Copy, Default)]
struct Entry {
    number: [u8; 2],
    flags: u8,
}
#[derive(Default)]
pub(crate) struct Ledger(Vec<Entry>);
impl Ledger {
    pub fn reserve(count: usize, signed: &[String], layout: Option<&[Region]>) -> Result<Self> {
        if count > MAX_FRAMES {
            return Err(Error::Full);
        }
        if let Some(parts) = layout {
            if parts
                .iter()
                .try_fold(0usize, |n, r| n.checked_add(r.frames))
                != Some(count)
            {
                return Err(Error::Format);
            }
            if parts
                .iter()
                .any(|r| r.frames == 0 || !signed.iter().any(|s| s == r.name))
                || signed.iter().any(|s| !parts.iter().any(|r| r.name == s))
            {
                return Err(Error::Denied);
            }
        } else if count != 0 && signed.len() != 1 {
            // Pas de capacité attribuable sans correspondance rôle/page authentifiée.
            return Err(Error::Full);
        }
        let mut pages = Vec::new();
        pages.try_reserve_exact(count).map_err(|_| Error::Memory)?;
        if let Some(parts) = layout {
            for r in parts {
                pages.resize(
                    pages.len() + r.frames,
                    Entry {
                        flags: Direction::parse(r.name)? as u8,
                        ..Entry::default()
                    },
                );
            }
        } else {
            let dir = match signed {
                [only] => Direction::parse(only)?,
                _ => Direction::Unspecified,
            };
            pages.resize(
                count,
                Entry {
                    flags: dir as u8,
                    ..Entry::default()
                },
            );
        }
        Ok(Self(pages))
    }
    pub fn bytes(&self) -> usize {
        self.0.capacity() * size_of::<Entry>()
    }
    /// Attribution atomique de numéros fictifs libres. La table transitoire couvre le quota
    /// global 4C. Les numéros du stock initial et ceux encore retenus restent indisponibles.
    pub fn allocate<'a>(
        &mut self,
        baseline: u32,
        limit: u32,
        others: impl Iterator<Item = &'a Self>,
    ) -> Result<()> {
        if self.0.is_empty() {
            return Ok(());
        }
        const LIMIT: usize = crate::resources::Use::LAB.dma as usize;
        if limit as usize > LIMIT || baseline > limit {
            return Err(Error::Format);
        }
        let mut used = [0u64; LIMIT.div_ceil(64)];
        for ledger in others {
            for e in &ledger.0 {
                if e.flags & RELEASE != RELEASE {
                    let n = usize::from(u16::from_le_bytes(e.number));
                    used[n / 64] |= 1u64 << (n % 64);
                }
            }
        }
        let mut free = (baseline..limit).filter(|n| {
            let n = *n as usize;
            used[n / 64] & (1u64 << (n % 64)) == 0
        });
        for e in &mut self.0 {
            e.number = (free.next().ok_or(Error::Full)? as u16).to_le_bytes()
        }
        Ok(())
    }
    pub fn page(&self, i: usize, quarantine: bool) -> Result<PageInfo> {
        let entry = self.0.get(i).ok_or(Error::Format)?;
        let b = entry.flags;
        let released = b & RELEASE == RELEASE;
        Ok(PageInfo {
            frame: u16::from_le_bytes(entry.number),
            direction: [
                Direction::Unspecified,
                Direction::Read,
                Direction::Write,
                Direction::Both,
            ][usize::from(b & 3)],
            exposed: b & EXPOSED != 0,
            source_stopped: b & STOPPED != 0,
            translation_removed: b & UNMAPPED != 0,
            ownership_returned: b & OWNED != 0,
            released,
            quarantined: quarantine && !released,
        })
    }
    pub fn expose_all(&mut self) {
        for e in &mut self.0 {
            e.flags |= EXPOSED;
        }
    }
    pub fn evidence(
        &mut self,
        stopped: bool,
        local_stop: &PageSet,
        unmapped: &PageSet,
        owned: &PageSet,
    ) -> Result<(bool, u32)> {
        if ![local_stop, unmapped, owned]
            .iter()
            .all(|s| s.fits(self.0.len()))
        {
            return Err(Error::Format);
        }
        let mut changed = false;
        let mut held = 0;
        for (i, e) in self.0.iter_mut().enumerate() {
            let b = &mut e.flags;
            let old = *b;
            if stopped || local_stop.contains(i) {
                *b |= STOPPED
            }
            if unmapped.contains(i) {
                *b |= UNMAPPED
            }
            if owned.contains(i) {
                *b |= OWNED
            }
            changed |= old != *b;
            held += u32::from(*b & RELEASE != RELEASE);
        }
        Ok((changed, held))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    #[test]
    fn bornes_directions_et_restitution_par_page() {
        let signed = vec![String::from("rx:w")];
        for n in [0, 1, 22, 128, 129, 258, 259] {
            let mut book = Ledger::reserve(n, &signed, None).unwrap();
            assert_eq!(book.bytes(), 3 * n);
            book.expose_all();
            let full = PageSet::range(0, n).unwrap();
            let none = PageSet::default();
            assert_eq!(
                book.evidence(false, &none, &full, &full).unwrap().1,
                n as u32
            );
            for i in (0..n).rev() {
                assert!(book.page(i, true).unwrap().quarantined);
                let one = PageSet::range(i, 1).unwrap();
                let (_, held) = book.evidence(false, &one, &none, &none).unwrap();
                assert_eq!(held, i as u32);
                let p = book.page(i, true).unwrap();
                assert!(
                    p.exposed
                        && p.source_stopped
                        && p.ownership_returned
                        && p.translation_removed
                        && p.released
                        && !p.quarantined
                );
                assert_eq!(p.direction, Direction::Write);
            }
            assert!(book.page(n, false).is_err());
        }
        let mut sparse = PageSet::default();
        for i in [0, 128, 258, 258] {
            sparse.insert(i).unwrap();
        }
        let mut book = Ledger::reserve(259, &signed, None).unwrap();
        assert_eq!(
            book.evidence(false, &sparse, &sparse, &sparse).unwrap().1,
            256
        );
        assert!(book.page(258, true).unwrap().released);
        assert!(book.page(257, true).unwrap().quarantined);
        let before = sparse;
        assert_eq!(sparse.insert(259), Err(Error::Format));
        assert_eq!(sparse, before);
        for n in [260, usize::MAX] {
            assert_eq!(Ledger::reserve(n, &signed, None).err(), Some(Error::Full));
            assert_eq!(PageSet::range(n, 1), Err(Error::Format));
        }
    }
    #[test]
    fn observation_hors_bail_atomique_et_profil_partiel_refuse() {
        let signed = vec![String::from("a:r"), String::from("b:w")];
        let layout = [
            Region {
                name: "a:r",
                frames: 128,
            },
            Region {
                name: "b:w",
                frames: 1,
            },
        ];
        let mut book = Ledger::reserve(129, &signed, Some(&layout)).unwrap();
        assert_eq!(book.page(127, false).unwrap().direction, Direction::Read);
        assert_eq!(book.page(128, false).unwrap().direction, Direction::Write);
        assert_eq!(
            Ledger::reserve(130, &signed, Some(&layout)).err(),
            Some(Error::Format)
        );
        let dup = [
            Region {
                name: "a:r",
                frames: 128,
            },
            Region {
                name: "a:r",
                frames: 1,
            },
        ];
        assert_eq!(
            Ledger::reserve(129, &signed, Some(&dup)).err(),
            Some(Error::Denied)
        );
        let forged = [
            Region {
                name: "a:rw",
                frames: 128,
            },
            Region {
                name: "b:w",
                frames: 1,
            },
        ];
        assert_eq!(
            Ledger::reserve(129, &signed, Some(&forged)).err(),
            Some(Error::Denied)
        );
        let outside = PageSet::range(128, 2).unwrap();
        let inside = PageSet::range(0, 129).unwrap();
        assert_eq!(
            book.evidence(true, &inside, &inside, &outside),
            Err(Error::Format)
        );
        for i in 0..129 {
            let p = book.page(i, false).unwrap();
            assert!(!p.source_stopped && !p.translation_removed && !p.ownership_returned);
        }
    }
    #[test]
    fn allocateur_reutilise_seulement_les_numeros_reconcilies() {
        let signed = vec![String::from("rx:w")];
        let mut old = Ledger::reserve(259, &signed, None).unwrap();
        old.allocate(2, 261, core::iter::empty()).unwrap();
        old.expose_all();
        let last = PageSet::range(258, 1).unwrap();
        old.evidence(false, &last, &last, &last).unwrap();
        let mut new = Ledger::reserve(1, &signed, None).unwrap();
        new.allocate(2, 261, [&old].into_iter()).unwrap();
        assert_eq!(new.page(0, false).unwrap().frame, 260);
        assert_eq!(old.page(258, true).unwrap().frame, 260);
        assert!(old.page(258, true).unwrap().released);
        let mut another = Ledger::reserve(1, &signed, None).unwrap();
        assert_eq!(
            another.allocate(2, 261, [&old, &new].into_iter()),
            Err(Error::Full)
        );
        let more = PageSet::range(128, 1).unwrap();
        old.evidence(false, &more, &more, &more).unwrap();
        another.allocate(2, 261, [&old, &new].into_iter()).unwrap();
        assert_eq!(another.page(0, false).unwrap().frame, 130);
        assert!(!new.page(0, false).unwrap().released);
    }
}
