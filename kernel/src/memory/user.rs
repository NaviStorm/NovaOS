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

    /// Espace inactif, dernier thread commuté. Seules les branches basses sont possédées.
    pub fn release(self, m: &mut Memory) {
        for va in self.mapped[..self.len].iter().copied().rev() {
            let frame = self
                .tables
                .remove(&mut m.frames, va)
                .expect("mapping privé");
            let alias = m
                .space
                .direct_alias(&m.frames, frame)
                .expect("alias du cadre");
            alias.protect(&mut m.frames, Access::ReadWrite);
            m.frames.release(frame);
        }
        assert!(m.frames.empty_prefix(
            self.tables.root,
            if paging::SHARED_UPPER { 256 } else { 512 }
        ));
        m.frames.release(self.tables.root);
    }
}
