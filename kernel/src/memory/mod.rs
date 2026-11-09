//! Noyau commun : cadres utilisables, espaces à quatre niveaux et permissions.
mod frames;
mod probes;
mod tables;

use crate::{
    boot::MemoryInfo,
    hal::{
        log,
        memory::{Access, PhysicalAddress, VirtualAddress, VirtualMemory},
        paging,
    },
};
pub(crate) use frames::Frames;
use frames::PHYS_MASK;
pub(crate) use frames::mapped;
use limine::memory_map::EntryType;
pub(crate) use tables::AddressSpace;
use tables::{ARENA, check_page};

pub const PAGE: u64 = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Mode,
    Geometry,
    BootTable,
    Exhausted,
    Missing,
    Occupied,
    Reserved,
    Alignment,
    Verification,
}
pub type Result<T> = core::result::Result<T, Error>;

struct Image {
    start: u64,
    text: u64,
    rodata: u64,
    data: u64,
    end: u64,
    physical: u64,
}

impl Image {
    fn new(info: &MemoryInfo) -> Result<Self> {
        // Sûreté : symboles d’adresse du script ELF commun, jamais déréférencés ici.
        unsafe extern "C" {
            static __image_start: u8;
            static __text_start: u8;
            static __rodata_start: u8;
            static __data_start: u8;
            static __image_end: u8;
        }
        let image = Self {
            start: (&raw const __image_start) as u64,
            text: (&raw const __text_start) as u64,
            rodata: (&raw const __rodata_start) as u64,
            data: (&raw const __data_start) as u64,
            end: (&raw const __image_end) as u64,
            physical: info.physical_base,
        };
        if image.start != info.virtual_base
            || image.physical % PAGE != 0
            || [image.start, image.text, image.rodata, image.data, image.end]
                .windows(2)
                .any(|w| w[1] < w[0] || (w[0] | w[1]) % PAGE != 0)
            || image
                .physical
                .checked_add(image.end - image.start)
                .is_none()
        {
            return Err(Error::Geometry);
        }
        Ok(image)
    }
    fn access(&self, va: u64) -> Access {
        if (self.text..self.rodata).contains(&va) {
            Access::ReadExecute
        } else if va >= self.data {
            Access::ReadWrite
        } else {
            Access::ReadOnly
        }
    }
    fn pa(&self, va: u64) -> u64 {
        self.physical + (va - self.start)
    }
}

/// Identité opaque de la racine de traduction. La politique des threads ne
/// manipule aucune adresse physique ni registre d’architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpaceId(u64);

pub struct Memory {
    frames: Frames,
    space: AddressSpace,
    cache: u64,
    image: Image,
}

impl Memory {
    /// # Safety
    /// Une seule prise de contrôle, au démarrage sur le BSP, avant tout autre
    /// usage des cadres USABLE. La carte et le HHDM proviennent du Limine actif.
    pub unsafe fn init(info: MemoryInfo) -> Result<Self> {
        let inherited = paging::root()?;
        let image = Image::new(&info)?;
        let entries = info.entries;
        let mut frames = Frames::new(info)?;
        let space = AddressSpace::new(&mut frames)?;
        let (_, cache) = frames.inherited(inherited, image.start)?;
        for e in entries
            .iter()
            .copied()
            .filter(|e| mapped(e.entry_type) && e.length != 0)
        {
            let start = e.base & !(PAGE - 1);
            let end = (e.base + e.length).next_multiple_of(PAGE);
            for pa in (start..end).step_by(PAGE as usize) {
                let va = frames.info.offset.checked_add(pa).ok_or(Error::Geometry)?;
                if !paging::kernel_address(va) || check_page(va).is_ok() {
                    return Err(Error::Geometry);
                }
                let (old_pa, attr) = frames.inherited(inherited, va)?;
                if old_pa != pa {
                    return Err(Error::Geometry);
                }
                let access = if (image.physical..image.pa(image.data)).contains(&pa) {
                    Access::ReadOnly
                } else {
                    Access::ReadWrite
                };
                space.insert(&mut frames, va, paging::leaf(pa, access, attr))?;
            }
        }
        for va in (image.start..image.end).step_by(PAGE as usize) {
            let pa = image.pa(va);
            if !frames.covers(pa, EntryType::EXECUTABLE_AND_MODULES)
                || frames.inherited(inherited, va)? != (pa, cache)
            {
                return Err(Error::Geometry);
            }
            space.insert(&mut frames, va, paging::leaf(pa, image.access(va), cache))?;
        }
        paging::map_devices(&space, &mut frames)?;
        paging::activate(&space, &mut frames)?;
        let mem = Self {
            frames,
            space,
            cache,
            image,
        };
        mem.verify()?;
        log(format_args!(
            "MMU : sections et alias verifies, {} pages libres.",
            mem.frames.available
        ));
        Ok(mem)
    }

    fn verify(&self) -> Result<()> {
        if paging::root()? != self.space.root || !paging::lower_empty(&self.space, &self.frames) {
            return Err(Error::Verification);
        }
        for va in (self.image.start..self.image.end).step_by(PAGE as usize) {
            let pa = self.image.pa(va);
            let access = self.image.access(va);
            let alias = if access == Access::ReadWrite {
                Access::ReadWrite
            } else {
                Access::ReadOnly
            };
            for (mapped, perm) in [(va, access), (self.frames.info.offset + pa, alias)] {
                let pte = self.space.descriptor(&self.frames, mapped)?;
                if !paging::matches(pte, paging::leaf(pa, perm, self.cache)) {
                    return Err(Error::Verification);
                }
            }
        }
        Ok(())
    }
}

impl VirtualMemory for Memory {
    type Error = Error;
    const PAGE_BYTES: usize = PAGE as usize;

    unsafe fn map(
        &mut self,
        page: VirtualAddress,
        frame: PhysicalAddress,
        access: Access,
    ) -> Result<()> {
        let va = page.0 as u64;
        check_page(va)?;
        if frame.0 % PAGE != 0 {
            return Err(Error::Alignment);
        }
        if !self.frames.covers(frame.0, EntryType::USABLE) {
            return Err(Error::Reserved);
        }
        let alias = self.space.direct_alias(&self.frames, frame.0)?;
        if access != Access::ReadWrite {
            alias.protect(&mut self.frames, Access::ReadOnly);
        }
        if let Err(e) = self.space.insert(
            &mut self.frames,
            va,
            paging::leaf(frame.0, access, alias.cache),
        ) {
            if access != Access::ReadWrite {
                alias.protect(&mut self.frames, Access::ReadWrite);
            }
            return Err(e);
        }
        paging::invalidate(va);
        if access == Access::ReadExecute {
            paging::sync_code(self.frames.info.offset + frame.0, va);
        }
        Ok(())
    }

    unsafe fn unmap(&mut self, page: VirtualAddress) -> Result<PhysicalAddress> {
        let va = page.0 as u64;
        check_page(va)?;
        let frame = self.space.descriptor(&self.frames, va)? & PHYS_MASK;
        let alias = self.space.direct_alias(&self.frames, frame)?;
        let pa = self.space.remove(&mut self.frames, va)?;
        alias.protect(&mut self.frames, Access::ReadWrite);
        Ok(PhysicalAddress(pa))
    }
}

impl Memory {
    pub(crate) fn space_id(&self) -> SpaceId {
        SpaceId(self.space.root)
    }

    /// Nombre de cadres encore disponibles, sans exposer la liste des cadres.
    pub(crate) fn free_pages(&self) -> u64 {
        self.frames.available
    }

    /// Réserve un cadre zéro et installe une page dans la fenêtre du noyau.
    pub fn allocate_page(&mut self, page: VirtualAddress, access: Access) -> Result<()> {
        check_page(page.0 as u64)?;
        let frame = self.frames.take()?;
        // Sûreté : take fournit un cadre exclusif, initialisé et sans alias externe.
        if let Err(e) = unsafe { self.map(page, PhysicalAddress(frame), access) } {
            self.frames.release(frame);
            return Err(e);
        }
        Ok(())
    }

    /// # Safety
    /// Si elle est présente, la page est issue de allocate_page, sans pointeur ni exécution encore en cours.
    pub unsafe fn release_page(&mut self, page: VirtualAddress) -> Result<()> {
        // Sûreté : l’appelant garantit l’exclusivité ; unmap synchronise le TLB.
        let frame = unsafe { self.unmap(page)? };
        self.frames.release(frame.0);
        Ok(())
    }
}
