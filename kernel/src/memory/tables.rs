use super::{Error, Frames, PAGE, Result, frames::PHYS_MASK};
use crate::hal::paging;
use limine::memory_map::EntryType;

pub struct AddressSpace {
    pub(crate) root: u64,
}

fn indices(va: u64) -> [usize; 4] {
    [39, 30, 21, 12].map(|shift| ((va >> shift) & 511) as usize)
}

impl Frames {
    fn entry(&self, table: u64, slot: usize) -> u64 {
        // Sûreté : table est un cadre de table validé ou alloué par le BSP,
        // indexé sur 9 bits. Aucune référence Rust à la table n’est conservée.
        unsafe { self.direct(table).wrapping_add(slot).read_volatile() }
    }

    fn put(&mut self, table: u64, slot: usize, value: u64) {
        // Sûreté : seules les tables créées par NovaOS sont modifiées, par le
        // BSP avec IRQ masquées. Une entrée alignée est publiée en une écriture.
        unsafe { self.direct(table).wrapping_add(slot).write_volatile(value) }
    }

    /// Lit les attributs matériels hérités, y compris ceux des grands blocs.
    pub(super) fn inherited(&self, root: u64, va: u64) -> Result<(u64, u64)> {
        let mut table = root;
        for (level, slot) in indices(va).into_iter().enumerate() {
            if !self.covers(table, EntryType::BOOTLOADER_RECLAIMABLE) {
                return Err(Error::BootTable);
            }
            let entry = self.entry(table, slot);
            if entry & 1 == 0 {
                return Err(Error::Missing);
            }
            if paging::is_leaf(entry, level) {
                let mask = (1 << (39 - level * 9)) - 1;
                return Ok((
                    (entry & PHYS_MASK & !mask) | (va & mask),
                    paging::cache(entry, level),
                ));
            }
            table = entry & PHYS_MASK;
        }
        Err(Error::BootTable)
    }
}

impl AddressSpace {
    pub(crate) fn new(frames: &mut Frames) -> Result<Self> {
        Ok(Self {
            root: frames.take()?,
        })
    }

    /// En cas de pénurie, toute la branche provisoire est rendue avant publication.
    pub(crate) fn insert(&self, frames: &mut Frames, va: u64, desc: u64) -> Result<()> {
        let path = indices(va);
        let mut parent = self.root;
        for level in 0..3 {
            let slot = path[level];
            let old = frames.entry(parent, slot);
            if old != 0 {
                parent = old & PHYS_MASK;
                continue;
            }
            let mut children = [0; 3];
            let count = 3 - level;
            for i in 0..count {
                match frames.take() {
                    Ok(frame) => children[i] = frame,
                    Err(e) => {
                        for &frame in &children[..i] {
                            frames.release(frame);
                        }
                        return Err(e);
                    }
                }
            }
            for i in 1..count {
                frames.put(children[i - 1], path[level + i], paging::table(children[i]));
            }
            frames.put(children[count - 1], path[3], desc);
            paging::publish();
            frames.put(parent, slot, paging::table(children[0]));
            return Ok(());
        }
        if frames.entry(parent, path[3]) != 0 {
            return Err(Error::Occupied);
        }
        frames.put(parent, path[3], desc);
        Ok(())
    }

    fn path(&self, frames: &Frames, va: u64) -> Result<[u64; 4]> {
        let mut tables = [self.root; 4];
        for (level, slot) in indices(va)[..3].iter().enumerate() {
            let entry = frames.entry(tables[level], *slot);
            if entry == 0 {
                return Err(Error::Missing);
            }
            let child = entry & PHYS_MASK;
            if !paging::matches(entry, paging::table(child)) {
                return Err(Error::Geometry);
            }
            tables[level + 1] = child;
        }
        Ok(tables)
    }

    pub(crate) fn descriptor(&self, frames: &Frames, va: u64) -> Result<u64> {
        let tables = self.path(frames, va)?;
        let desc = frames.entry(tables[3], indices(va)[3]);
        if desc == 0 {
            Err(Error::Missing)
        } else {
            Ok(desc)
        }
    }

    pub(super) fn direct_alias(&self, frames: &Frames, pa: u64) -> Result<DirectAlias> {
        let va = frames.info.offset + pa;
        let table = self.path(frames, va)?[3];
        let entry = frames.entry(table, indices(va)[3]);
        if entry & 1 == 0 || entry & PHYS_MASK != pa {
            return Err(Error::Geometry);
        }
        Ok(DirectAlias {
            va,
            table,
            pa,
            cache: paging::cache(entry, 3),
        })
    }

    pub(crate) fn remove(&self, frames: &mut Frames, va: u64) -> Result<u64> {
        let tables = self.path(frames, va)?;
        let path = indices(va);
        let old = frames.entry(tables[3], path[3]);
        if old == 0 {
            return Err(Error::Missing);
        }
        frames.put(tables[3], path[3], 0);
        let mut first_free = 4;
        for level in (1..=3).rev() {
            if (0..512).any(|i| frames.entry(tables[level], i) != 0) {
                break;
            }
            frames.put(tables[level - 1], path[level - 1], 0);
            first_free = level;
        }
        paging::invalidate(va);
        for &table in &tables[first_free..] {
            frames.release(table);
        }
        Ok(old & PHYS_MASK)
    }
}

pub const ARENA: u64 = 0xffff_f000_0000_0000;
pub(super) fn check_page(va: u64) -> Result<()> {
    if va % PAGE != 0 {
        return Err(Error::Alignment);
    }
    if !(ARENA..ARENA + (1 << 39)).contains(&va) {
        return Err(Error::Reserved);
    }
    Ok(())
}

pub(super) struct DirectAlias {
    va: u64,
    table: u64,
    pa: u64,
    pub cache: u64,
}

impl DirectAlias {
    // Seules les permissions changent. Cadre, taille, cache et partage restent identiques.
    pub fn protect(&self, frames: &mut Frames, access: super::Access) {
        frames.put(
            self.table,
            indices(self.va)[3],
            paging::leaf(self.pa, access, self.cache),
        );
        paging::invalidate(self.va);
    }
}

impl Frames {
    pub(crate) fn empty_prefix(&self, root: u64, entries: usize) -> bool {
        (0..entries).all(|i| self.entry(root, i) == 0)
    }
}
