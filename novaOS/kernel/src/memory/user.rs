//! Propriété des mappings privés. Toutes les copies passent par le HHDM du cadre validé.
use super::*;

pub const CODE: u64 = 0x400000;
pub const DATA: u64 = 0x401000;
pub const PRIVATE: u64 = 0x600000;
pub const STACK: u64 = 0x800000;
pub const TOP: u64 = STACK + PAGE;

pub(crate) struct UserSpace {
    tables: AddressSpace,
    mapped: [u64; 4],
    len: usize,
}

impl UserSpace {
    pub(crate) fn audit_driver(&self, m: &Memory, dma: &[u64; 24]) -> bool {
        self.tables
            .audit_driver(&m.frames, &self.mapped[..self.len], dma)
    }

    pub fn id(&self) -> SpaceId {
        SpaceId(self.tables.root)
    }

    fn add(&mut self, m: &mut Memory, va: u64, perm: Access, bytes: &[u8]) -> Result<()> {
        if bytes.len() > PAGE as usize || self.len == self.mapped.len() {
            return Err(Error::Geometry);
        }
        let frame = m.frames.take()?;
        let alias = m
            .space
            .direct_alias(&m.frames, frame)
            .expect("HHDM du cadre privé");
        let dst = m.frames.direct(frame).cast::<u8>();
        // Sûreté : cadre neuf, exclusif et zéro ; la copie bornée précède toute publication.
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len()) };
        let desc = paging::user_leaf(frame, perm, alias.cache);
        if let Err(e) = self.tables.insert(&mut m.frames, va, desc) {
            m.frames.release(frame);
            return Err(e);
        }
        self.mapped[self.len] = va;
        self.len += 1;
        if perm != Access::ReadWrite {
            alias.protect(&mut m.frames, Access::ReadOnly);
        }
        if perm == Access::ReadExecute {
            paging::sync_user_code(dst as u64);
        }
        if !paging::matches(self.tables.descriptor(&m.frames, va)?, desc) {
            return Err(Error::Verification);
        }
        Ok(())
    }

    pub fn new(
        m: &mut Memory,
        image: &[u8],
        config: &[u8],
        private_ro: Option<&[u8]>,
    ) -> Result<Self> {
        if image.is_empty() {
            return Err(Error::Geometry);
        }
        let mut s = Self {
            tables: AddressSpace::user(&mut m.frames, &m.space)?,
            mapped: [0; 4],
            len: 0,
        };
        let result = (|| {
            s.add(m, CODE, Access::ReadExecute, image)?;
            s.add(m, DATA, Access::ReadWrite, config)?;
            s.add(m, STACK, Access::ReadWrite, &[])?;
            if let Some(payload) = private_ro {
                s.add(m, PRIVATE, Access::ReadOnly, payload)?;
            }
            for guard in [STACK - PAGE, TOP] {
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
        (self.len - 1, 1)
    }

    fn frame_for(&self, m: &Memory, va: u64, write: bool) -> Option<u64> {
        let page = va & !(PAGE - 1);
        if !self.mapped[..self.len].contains(&page) {
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
        if len == 0 || len > crate::abi::DEBUG_MAX || va.checked_add(len as u64)? > (1 << 47) {
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
        // Sûreté : champ aligné dans DATA, espace vivant, utilisateur suspendu sur CPU0.
        unsafe { self.word(m, index).read_volatile() }
    }
    pub fn write_data(&self, m: &Memory, index: usize, value: u64) {
        // Sûreté : accès aligné à la page de données privée RW, utilisateur suspendu.
        unsafe { self.word(m, index).write_volatile(value) };
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
