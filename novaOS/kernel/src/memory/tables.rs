use super::{Error, Frames, PAGE, Result, frames::PHYS_MASK};
use crate::hal::paging;
use limine::memory_map::EntryType;

pub struct AddressSpace {
    pub(crate) root: u64,
    user: bool,
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
            root: frames.take_table()?,
            user: false,
        })
    }

    // Les CR3 utilisateur empruntent cette entrée dès leur création. La table
    // reste au noyau pour toute sa vie. Aucune feuille MMIO n'est créée ici.
    #[cfg(target_arch = "x86_64")]
    pub(crate) fn reserve_io_branch(&self, frames: &mut Frames) -> Result<()> {
        let slot = indices(super::IO_BASE)[0];
        let entry = frames.entry(self.root, slot);
        if entry == 0 {
            let table = frames.take_table()?;
            paging::publish();
            frames.put(self.root, slot, paging::table(table));
        } else if !paging::matches(entry, paging::table(entry & PHYS_MASK)) {
            return Err(Error::Verification);
        }
        Ok(())
    }

    pub(super) fn user(frames: &mut Frames, kernel: &AddressSpace) -> Result<Self> {
        let mut space = Self::new(frames)?;
        space.user = true;
        // Sur x86, emprunt des branches superviseur. Le PML4 de l’arène
        // dynamique reste ancré par les piles permanentes et la table des TCB.
        if paging::SHARED_UPPER {
            for i in 256..512 {
                let entry = frames.entry(kernel.root, i);
                if entry != 0 && !paging::matches(entry, paging::table(entry & PHYS_MASK)) {
                    frames.release_table(space.root);
                    return Err(Error::Verification);
                }
                frames.put(space.root, i, entry);
            }
        }
        Ok(space)
    }

    fn branch(&self, frame: u64) -> u64 {
        if self.user {
            paging::user_table(frame)
        } else {
            paging::table(frame)
        }
    }

    /// En cas de pénurie, toute la branche provisoire est rendue avant publication.
    pub(crate) fn insert(&self, frames: &mut Frames, va: u64, desc: u64) -> Result<()> {
        if self.user && va >= (1 << 47) {
            return Err(Error::Reserved);
        }
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
                match frames.take_table() {
                    Ok(frame) => children[i] = frame,
                    Err(e) => {
                        for &frame in &children[..i] {
                            frames.release_table(frame);
                        }
                        return Err(e);
                    }
                }
            }
            for i in 1..count {
                frames.put(children[i - 1], path[level + i], self.branch(children[i]));
            }
            frames.put(children[count - 1], path[3], desc);
            paging::publish();
            frames.put(parent, slot, self.branch(children[0]));
            return Ok(());
        }
        if frames.entry(parent, path[3]) != 0 {
            return Err(Error::Occupied);
        }
        frames.put(parent, path[3], desc);
        Ok(())
    }

    fn path(&self, frames: &Frames, va: u64) -> Result<[u64; 4]> {
        if self.user && va >= (1 << 47) {
            return Err(Error::Reserved);
        }
        let mut tables = [self.root; 4];
        for (level, slot) in indices(va)[..3].iter().enumerate() {
            let entry = frames.entry(tables[level], *slot);
            if entry == 0 {
                return Err(Error::Missing);
            }
            let child = entry & PHYS_MASK;
            if !paging::matches(entry, self.branch(child)) {
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
            frames.release_table(table);
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

impl AddressSpace {
    /// Énumération de l'espace réellement installé. Comparer à la liste privée
    /// exclut un alias non déclaré, même lorsque son adresse est inconnue du pilote.
    pub(super) fn audit_driver(
        &self,
        frames: &Frames,
        expected: impl Fn(u64) -> bool,
        pages: usize,
        dma: &[u64; 24],
    ) -> bool {
        fn descend(
            f: &Frames,
            tab: u64,
            level: usize,
            prefix: u64,
            max: usize,
            known: &impl Fn(u64) -> bool,
            deny: &[u64; 24],
            count: &mut usize,
        ) -> bool {
            for i in 0..max {
                let ent = f.entry(tab, i);
                if ent == 0 {
                    continue;
                }
                let pa = ent & PHYS_MASK;
                let va = prefix | ((i as u64) << (39 - level * 9));
                if !f.managed(pa) || deny.contains(&pa) {
                    return false;
                }
                if level == 3 {
                    if !known(va) {
                        return false;
                    }
                    *count += 1;
                } else if !paging::matches(ent, paging::user_table(pa))
                    || !descend(f, pa, level + 1, va, 512, known, deny, count)
                {
                    return false;
                }
            }
            true
        }
        let mut count = 0;
        if !descend(frames, self.root, 0, 0, 256, &expected, dma, &mut count) || count != pages {
            return false;
        }
        // Sur x86 le premier niveau U/S=0 suffit pour toutes les branches
        // hautes. Sur ARM le TTBR0 privé ne contient aucune branche haute.
        (256..512).all(|i| {
            let e = frames.entry(self.root, i);
            e == 0 || (paging::SHARED_UPPER && paging::matches(e, paging::table(e & PHYS_MASK)))
        })
    }
}
