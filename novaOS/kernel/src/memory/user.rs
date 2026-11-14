//! Propriété des mappings privés. Toutes les copies passent par le HHDM du cadre validé.
use super::*;
mod elf;
pub(crate) use elf::STACK_TOP as ELF_TOP;
use elf::{Elf, HEAP};

pub const CODE: u64 = 0x400000;
pub const DATA: u64 = 0x401000;
pub const PRIVATE: u64 = 0x600000;
pub const STACK: u64 = 0x800000;
pub const TOP: u64 = STACK + PAGE;

pub(crate) struct UserSpace {
    tables: AddressSpace,
    mapped: [u64; 4],
    len: usize,
    elf: Option<Elf<'static>>,
    loaded: usize,
    stack_extra: usize,
    heap_pages: usize,
}

impl UserSpace {
    pub(crate) fn audit_driver(&self, m: &Memory, dma: &[u64; 24]) -> bool {
        self.tables.audit_driver(
            &m.frames,
            |va| self.owns(va),
            self.page_counts().0 + self.page_counts().1,
            dma,
        )
    }

    pub fn id(&self) -> SpaceId {
        SpaceId(self.tables.root)
    }

    fn add(&mut self, m: &mut Memory, va: u64, perm: Access, bytes: &[u8]) -> Result<()> {
        if self.len == self.mapped.len() {
            return Err(Error::Geometry);
        }
        self.map(m, va, perm, 0, bytes)?;
        self.mapped[self.len] = va;
        self.len += 1;
        Ok(())
    }
    fn map(
        &self,
        m: &mut Memory,
        va: u64,
        perm: Access,
        offset: usize,
        bytes: &[u8],
    ) -> Result<()> {
        if bytes.len() > PAGE as usize - offset {
            return Err(Error::Geometry);
        }
        let frame = m.frames.take()?;
        let alias = m
            .space
            .direct_alias(&m.frames, frame)
            .expect("HHDM du cadre privé");
        let dst = m.frames.direct(frame).cast::<u8>();
        // Sûreté : cadre neuf, exclusif et zéro ; la copie bornée précède toute publication.
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst.add(offset), bytes.len()) };
        let desc = paging::user_leaf(frame, perm, alias.cache);
        if let Err(e) = self.tables.insert(&mut m.frames, va, desc) {
            m.frames.release(frame);
            return Err(e);
        }
        if perm != Access::ReadWrite {
            alias.protect(&mut m.frames, Access::ReadOnly);
        }
        if perm == Access::ReadExecute {
            paging::sync_user_code(dst as u64);
        }
        if !self
            .tables
            .descriptor(&m.frames, va)
            .is_ok_and(|installed| paging::matches(installed, desc))
        {
            Self::release_page(&self.tables, m, va);
            return Err(Error::Verification);
        }
        Ok(())
    }

    pub fn new(
        m: &mut Memory,
        image: &'static [u8],
        config: &[u8],
        private_ro: Option<&[u8]>,
    ) -> Result<Self> {
        if image.is_empty() {
            return Err(Error::Geometry);
        }
        let elf = if image.starts_with(b"\x7fELF") {
            Some(Elf::parse(image)?)
        } else {
            None
        };
        if elf.is_some() && private_ro.is_some() {
            return Err(Error::Geometry);
        }
        let mut s = Self {
            tables: AddressSpace::user(&mut m.frames, &m.space)?,
            mapped: [0; 4],
            len: 0,
            elf,
            loaded: 0,
            stack_extra: 0,
            heap_pages: 0,
        };
        let result = (|| {
            if let Some(e) = elf {
                for seg in e.segments() {
                    let file_end = seg.start + seg.filesz as u64;
                    for va in seg.pages() {
                        let lo = va.max(seg.start);
                        let size = (va + PAGE).min(file_end).saturating_sub(lo) as usize;
                        let offset = seg.offset + ((lo - seg.start) as usize).min(seg.filesz);
                        s.map(
                            m,
                            va,
                            seg.access,
                            (lo - va) as usize,
                            &image[offset..offset + size],
                        )?;
                        s.loaded += 1;
                    }
                }
                for va in (e.stack..ELF_TOP - PAGE).step_by(PAGE as usize) {
                    s.map(m, va, Access::ReadWrite, 0, &[])?;
                    s.stack_extra += 1;
                }
            } else {
                s.add(m, CODE, Access::ReadExecute, image)?;
            }
            s.add(m, DATA, Access::ReadWrite, config)?;
            s.add(m, s.stack_top() - PAGE, Access::ReadWrite, &[])?;
            if let Some(payload) = private_ro {
                s.add(m, PRIVATE, Access::ReadOnly, payload)?;
            }
            for guard in [s.stack_bottom() - PAGE, s.stack_top()] {
                if s.tables.descriptor(&m.frames, guard) != Err(Error::Missing) {
                    return Err(Error::Verification);
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            s.release(m);
            return Err(e);
        }
        Ok(s)
    }

    pub(crate) fn page_counts(&self) -> (usize, usize) {
        (
            self.len - 1 + self.loaded + self.heap_pages,
            1 + self.stack_extra,
        )
    }

    pub fn entry(&self) -> u64 {
        self.elf.map_or(CODE, |e| e.entry)
    }
    pub fn stack_top(&self) -> u64 {
        if self.elf.is_some() { ELF_TOP } else { TOP }
    }
    pub fn stack_bottom(&self) -> u64 {
        self.elf.map_or(STACK, |e| e.stack)
    }
    pub fn executable(&self, pc: u64) -> bool {
        self.elf.map_or_else(
            || (CODE..CODE + PAGE).contains(&pc),
            |e| {
                e.segments()
                    .any(|s| s.access == Access::ReadExecute && (s.start..s.end).contains(&pc))
            },
        )
    }
    fn owns(&self, page: u64) -> bool {
        self.mapped[..self.len].contains(&page)
            || (self.stack_bottom()..self.stack_top() - PAGE).contains(&page)
            || (HEAP..self.heap_end()).contains(&page)
            || self.elf.is_some_and(|e| {
                e.segments()
                    .any(|s| (s.start & !(PAGE - 1)) <= page && page < s.end)
            })
    }
    fn heap_end(&self) -> u64 {
        HEAP + self.heap_pages as u64 * PAGE
    }
    /// Comparaison de l'ancienne borne et déplacement atomique en pages.
    pub fn resize_heap(&mut self, m: &mut Memory, expected: u64, end: u64) -> Result<u64> {
        let old = self.heap_end();
        if expected == 0 && end == 0 {
            return Ok(old);
        }
        if expected != old || !(HEAP..=(1 << 47)).contains(&end) || end % PAGE != 0 {
            return Err(Error::Geometry);
        }
        if end > old {
            if (end - old) / PAGE > m.frames.available {
                return Err(Error::Exhausted);
            }
            for va in (old..end).step_by(PAGE as usize) {
                if let Err(e) = self.map(m, va, Access::ReadWrite, 0, &[]) {
                    for mapped in (old..va).step_by(PAGE as usize) {
                        Self::release_page(&self.tables, m, mapped)
                    }
                    return Err(e);
                }
            }
        } else {
            for va in (end..old).step_by(PAGE as usize) {
                Self::release_page(&self.tables, m, va)
            }
        }
        self.heap_pages = ((end - HEAP) / PAGE) as usize;
        Ok(end)
    }
    fn frame_for(&self, m: &Memory, va: u64, write: bool) -> Option<u64> {
        let page = va & !(PAGE - 1);
        if !self.owns(page) {
            return None;
        }
        let desc = self.tables.descriptor(&m.frames, page).ok()?;
        let frame = desc & PHYS_MASK;
        if !m.frames.managed(frame) {
            return None;
        }
        let attr = paging::cache(desc, 3);
        let allows = |access| paging::matches(desc, paging::user_leaf(frame, access, attr));
        if allows(Access::ReadWrite)
            || (!write && (allows(Access::ReadOnly) || allows(Access::ReadExecute)))
        {
            Some(frame)
        } else {
            None
        }
    }

    // Résoudre toute la plage AVANT le premier accès. Au plus deux pages,
    // sous le même emprunt exclusif de Memory que la copie. Aucun pointeur
    // traduit ne sort du verrou ; un retrait de mapping ne peut s’intercaler.
    fn segments(
        &self,
        m: &Memory,
        va: u64,
        len: usize,
        write: bool,
    ) -> Option<[(*mut u8, usize); 2]> {
        if len == 0 || len > PAGE as usize || va.checked_add(len as u64)? > (1 << 47) {
            return None;
        }
        let mut parts = [(core::ptr::null_mut(), 0); 2];
        let mut done = 0;
        for part in &mut parts {
            if done == len {
                break;
            }
            let addr = va + done as u64;
            let offset = (addr % PAGE) as usize;
            let count = (PAGE as usize - offset).min(len - done);
            *part = (
                m.frames
                    .direct(self.frame_for(m, addr, write)?)
                    .cast::<u8>()
                    .wrapping_add(offset),
                count,
            );
            done += count;
        }
        Some(parts)
    }

    pub(crate) fn copy_from_user(&self, m: &mut Memory, va: u64, out: &mut [u8]) -> bool {
        let Some(parts) = self.segments(m, va, out.len(), false) else {
            return false;
        };
        let mut offset = 0;
        for (ptr, n) in parts.into_iter().filter(|p| p.1 != 0) {
            // Sûreté : cadre RAM privé validé, alias HHDM lisible, plage
            // destination noyau disjointe et bornée, verrou conservé.
            unsafe { core::ptr::copy_nonoverlapping(ptr, out.as_mut_ptr().add(offset), n) };
            offset += n;
        }
        true
    }

    pub(crate) fn copy_to_user(&self, m: &mut Memory, va: u64, input: &[u8]) -> bool {
        let Some(parts) = self.segments(m, va, input.len(), true) else {
            return false;
        };
        let mut offset = 0;
        for (ptr, n) in parts.into_iter().filter(|p| p.1 != 0) {
            // Sûreté : mêmes bornes, accès RW vérifié sur chaque feuille avant
            // la première écriture. Les cadres restent possédés jusqu’au retour.
            unsafe { core::ptr::copy_nonoverlapping(input.as_ptr().add(offset), ptr, n) };
            offset += n;
        }
        true
    }

    pub(crate) fn writable(&self, m: &Memory, va: u64, len: usize) -> bool {
        self.segments(m, va, len, true).is_some()
    }
    // Accès borné par le cadre privé. Aucun pointeur fourni par un programme
    // n’est déréférencé par le noyau.
    fn word(&self, m: &Memory, index: usize) -> *mut u64 {
        assert!(index < PAGE as usize / size_of::<u64>());
        let frame = self
            .tables
            .descriptor(&m.frames, DATA)
            .expect("page privée")
            & PHYS_MASK;
        m.frames.direct(frame).wrapping_add(index)
    }

    pub fn read_data(&self, m: &Memory, index: usize) -> u64 {
        // Mot de la sonde partagé avec les accès 64 bits alignés en assembleur utilisateur.
        unsafe {
            core::sync::atomic::AtomicU64::from_ptr(self.word(m, index))
                .load(core::sync::atomic::Ordering::Acquire)
        }
    }
    pub fn write_data(&self, m: &Memory, index: usize, value: u64) {
        // La racine est retenue pendant la publication du signal de fin à la sonde.
        unsafe {
            core::sync::atomic::AtomicU64::from_ptr(self.word(m, index))
                .store(value, core::sync::atomic::Ordering::Release)
        };
    }

    fn release_page(tables: &AddressSpace, m: &mut Memory, va: u64) {
        let frame = tables.remove(&mut m.frames, va).expect("mapping privé");
        let alias = m
            .space
            .direct_alias(&m.frames, frame)
            .expect("alias du cadre");
        alias.protect(&mut m.frames, Access::ReadWrite);
        m.frames.release(frame);
    }

    pub(crate) fn add_second_data(&mut self, m: &mut Memory) -> Result<()> {
        self.add(m, DATA + PAGE, Access::ReadWrite, &[])
    }
    pub(crate) fn remove_second_data(&mut self, m: &mut Memory) {
        assert_eq!(self.mapped[self.len - 1], DATA + PAGE);
        self.len -= 1;
        Self::release_page(&self.tables, m, DATA + PAGE);
    }

    /// Deux pages RW adjacentes, puis deuxième page retirée. Vérifie les
    /// octets des deux côtés et l’absence d’écriture partielle après retrait.
    pub(crate) fn test_copy(&mut self, m: &mut Memory) {
        let next = DATA + PAGE;
        self.add_second_data(m).unwrap();
        let addr = next - 3;
        let original = [1, 2, 3, 4, 5, 6];
        assert!(self.copy_to_user(m, addr, &original));
        let mut buf = [0u8; 6];
        assert!(self.copy_from_user(m, addr, &mut buf));
        assert_eq!(buf, original);
        self.remove_second_data(m);
        assert!(!self.copy_to_user(m, addr, &[9; 6]));
        assert!(self.copy_from_user(m, addr, &mut buf[..3]));
        assert_eq!(buf, original);
        buf.fill(0);
        assert!(!self.copy_from_user(m, addr, &mut buf));
        assert_eq!(buf, [0; 6]);
    }

    /// Sonde de révocation : le thread est bloqué dans wait. DATA n’est pas
    /// utilisée comme pile ni comme code. Le prochain copyout doit échouer.
    pub(crate) fn revoke_demo_data(&mut self, m: &mut Memory) {
        let index = self.mapped[..self.len]
            .iter()
            .position(|&va| va == DATA)
            .unwrap();
        Self::release_page(&self.tables, m, DATA);
        self.len -= 1;
        self.mapped[index] = self.mapped[self.len];
    }

    /// Espace inactif, dernier thread commuté. Seules les branches basses sont possédées.
    pub fn release(self, m: &mut Memory) {
        if let Some(e) = self.elf {
            for va in e.segments().flat_map(|s| s.pages()).take(self.loaded) {
                Self::release_page(&self.tables, m, va)
            }
            for i in 0..self.stack_extra {
                Self::release_page(&self.tables, m, e.stack + i as u64 * PAGE)
            }
        }
        for va in (HEAP..self.heap_end()).step_by(PAGE as usize) {
            Self::release_page(&self.tables, m, va)
        }
        for va in self.mapped[..self.len].iter().copied().rev() {
            Self::release_page(&self.tables, m, va);
        }
        assert!(m.frames.empty_prefix(
            self.tables.root,
            if paging::SHARED_UPPER { 256 } else { 512 }
        ));
        m.frames.release_table(self.tables.root);
    }
}

/// Les images mutées restent des octets du banc. Le parseur doit les rejeter avant tout mapping.
pub(crate) fn check_elf_headers(image: &[u8]) {
    assert!(Elf::parse(image).is_ok());
    let ph = u64::from_le_bytes(image[32..40].try_into().unwrap()) as usize;
    let va = u64::from_le_bytes(image[ph + 16..ph + 24].try_into().unwrap());
    let mut trial = image.to_vec();
    for (offset, value) in [
        (32, u64::MAX),
        (24, 0),
        (24, HEAP),
        (ph + 8, u64::MAX),
        (ph + 16, u64::MAX - 1),
        (ph + 16, 0xffff_8000_0000_0000),
        (ph + 16, DATA),
        (ph + 32, u64::MAX),
        (ph + 48, 3),
        (ph + 56 + 16, va),
        (ph + 8, 1),
    ] {
        trial[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        assert!(Elf::parse(&trial).is_err());
        trial[offset..offset + 8].copy_from_slice(&image[offset..offset + 8]);
    }
    trial[ph + 4..ph + 8].copy_from_slice(&7u32.to_le_bytes());
    assert!(Elf::parse(&trial).is_err());
    assert!(Elf::parse(&image[..63]).is_err());
    assert!(Elf::parse(&image[..ph + 55]).is_err());
}
