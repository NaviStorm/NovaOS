//! Noyau commun : cadres utilisables, espaces à quatre niveaux et permissions.
mod frames;
mod probes;
mod tables;
pub(crate) mod user;

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

impl SpaceId {
    /// Sûreté requise : racine encore possédée, IRQ masquées, pile et code
    /// courants présents dans les branches superviseur de la racine cible.
    pub(crate) unsafe fn activate(self) {
        // Sûreté : le scheduler conserve les deux espaces pendant le transfert.
        unsafe { paging::switch_thread_root(self.0) };
    }
}

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
        if !self.frames.managed(frame.0) {
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
        SpaceId(paging::kernel_thread_root(self.space.root))
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

/// Compteurs exclusifs, en pages physiques. Les octets de métadonnées de la
/// table des processus sont déjà inclus dans l’image statique.
#[derive(Default)]
pub(crate) struct Resident {
    pub stacks: usize,
    pub ipc: usize,
    pub user: usize,
    pub user_stacks: usize,
    pub processes: usize,
    pub process_bytes: usize,
    pub tcb_bytes: usize,
    pub heap: usize,
    pub buddy: usize,
    pub reusable: usize,
}
impl Memory {
    pub(crate) fn report(&self, phase: &str, r: Resident) {
        let pages = |kind| {
            self.frames
                .regions()
                .filter(|e| e.entry_type == kind)
                .map(|e| e.length.div_ceil(PAGE))
                .sum::<u64>()
        };
        // Toutes les pages liées (.bss comprise) ont été vérifiées lors de
        // l’installation de la MMU. Les images v0.7 ne chargent aucun module.
        let image = (self.image.end - self.image.start) / PAGE;
        let exec_extra = pages(EntryType::EXECUTABLE_AND_MODULES)
            .checked_sub(image)
            .expect("image physique");

        let video = pages(EntryType::FRAMEBUFFER);
        let ram_video = if crate::arch::VIDEO_SYSTEM_RAM {
            video
        } else {
            0
        };
        let released = self.frames.reclaimed;
        let loader = pages(EntryType::BOOTLOADER_RECLAIMABLE) - released;
        let used = self.frames.initial - self.frames.available;
        let counted = self.frames.tables
            + (r.ipc + r.stacks + r.user + r.user_stacks + r.heap + r.buddy) as u64;
        assert_eq!(used, counted, "cadres sans attribution dans le bilan");
        log(format_args!(
            "RAM phase={phase} image={image} tables={} kpile={} upile={} heap={} buddy={} ipc={} utilisateur={} processus={} meta_octets={} meta_actifs={} tcb_octets={} heap_va={} libres={} cache={} chargeur={loader} restitue={released} extra_elf={exec_extra} video={video} ram_video={ram_video} resident={} retenu={}",
            self.frames.tables,
            r.stacks,
            r.user_stacks,
            r.heap,
            r.buddy,
            r.ipc,
            r.user,
            r.processes,
            crate::sched::process::metadata_bytes(),
            r.process_bytes,
            r.tcb_bytes,
            crate::heap::reserved_pages(),
            self.frames.available,
            r.reusable,
            used + image,
            used + image + loader + exec_extra + ram_video
        ));
    }
}

/// # Safety
/// CPU unique, pile et tables Limine abandonnées. Aucune référence aux réponses
/// (hors carte remplacée ici) ni rappel de firmware ne peut encore être utilisé.
pub(crate) unsafe fn reclaim_loader(video: Option<(usize, usize)>) -> Result<()> {
    let (early, offset) =
        crate::heap::with_pages(|m| Ok((m.frames.info.entries, m.frames.info.offset)))?;
    if let Some((base, bytes)) = video {
        let mut pa = (base as u64).checked_sub(offset).ok_or(Error::Geometry)?;
        let end = pa.checked_add(bytes as u64).ok_or(Error::Geometry)?;
        while pa < end {
            let e = early
                .iter()
                .find(|e| {
                    e.entry_type == EntryType::FRAMEBUFFER
                        && e.base <= pa
                        && pa < e.base.saturating_add(e.length)
                })
                .ok_or(Error::Geometry)?;
            pa = e.base.checked_add(e.length).ok_or(Error::Geometry)?;
        }
    }
    let mut map = alloc::vec::Vec::new();
    map.try_reserve_exact(early.len())
        .map_err(|_| Error::Exhausted)?;
    map.extend(early.iter().map(|e| **e));
    for (i, e) in map
        .iter()
        .enumerate()
        .filter(|(_, e)| e.entry_type == EntryType::BOOTLOADER_RECLAIMABLE)
    {
        let end = e.base.checked_add(e.length).ok_or(Error::Geometry)?;
        if (e.base | end) % PAGE != 0
            || map.iter().enumerate().any(|(j, other)| {
                j != i
                    && other.length != 0
                    && other.base < end
                    && other.base.saturating_add(other.length) > e.base
            })
        {
            return Err(Error::Geometry);
        }
    }
    crate::heap::with_pages(move |m| {
        assert!(m.frames.map.is_empty());
        m.frames.info.entries = &[];
        m.frames.map = map;
        for i in 0..m.frames.map.len() {
            let e = m.frames.map[i];
            if e.entry_type != EntryType::BOOTLOADER_RECLAIMABLE {
                continue;
            }
            for frame in (e.base.max(PAGE)..e.base + e.length).step_by(PAGE as usize) {
                // Le HHDM de ces cadres est RW/NX. La carte possédée conserve le
                // type original : le curseur USABLE ne peut jamais les redistribuer.
                m.frames.release(frame);
                m.frames.reclaimed += 1;
            }
        }
        m.frames.initial += m.frames.reclaimed;
        Ok(())
    })
}
