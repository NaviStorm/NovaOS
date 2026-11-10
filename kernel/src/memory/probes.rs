//! Scénarios explicites d’amorçage pour exercer les protections du processeur.
use super::*;
use crate::hal::memory::Probe;

#[repr(align(4))]
struct Code([u8; 8]);
// Sûreté : ce symbole de sonde réside dans une page de données RW/NX du noyau.
#[unsafe(link_section = ".data.nx_probe")]
static NX_TARGET: Code = Code(paging::RETURN_42);

impl Memory {
    pub fn test(&mut self, scenario: &[u8]) -> Result<()> {
        if scenario.is_empty() {
            return Ok(());
        }
        let before = self.frames.available;
        let a = self.frames.take()?;
        let b = self.frames.take()?;
        if a == b
            || !self.frames.covers(a, EntryType::USABLE)
            || !self.frames.covers(b, EntryType::USABLE)
        {
            return Err(Error::Verification);
        }
        // Sûreté : a est un cadre RAM exclusif en écriture via son HHDM.
        unsafe { self.frames.direct(a).write_volatile(0x1234) };
        self.frames.release(a);
        let recycled = self.frames.take()?;
        // Sûreté : recycled est un nouveau cadre exclusif, réinitialisé par take.
        if recycled != a || unsafe { self.frames.direct(recycled).read_volatile() } != 0 {
            return Err(Error::Verification);
        }
        self.frames.release(b);
        self.frames.release(recycled);
        let page = VirtualAddress(ARENA as usize);
        self.allocate_page(page, Access::ReadWrite)?;
        let pa = self.space.descriptor(&self.frames, ARENA)? & PHYS_MASK;
        // Sûreté : la page et son cadre sont mappés en RAM normale et réservés au test.
        unsafe {
            (ARENA as *mut u64).write_volatile(0xfeed_cafe);
        }
        // Sûreté : lecture volatile de la même RAM via le mappage direct valide.
        if unsafe { self.frames.direct(pa).read_volatile() } != 0xfeed_cafe
            || self.allocate_page(page, Access::ReadOnly) != Err(Error::Occupied)
            || self.allocate_page(VirtualAddress(page.0 + 1), Access::ReadWrite)
                != Err(Error::Alignment)
            || self.allocate_page(VirtualAddress(self.image.text as usize), Access::ReadWrite)
                != Err(Error::Reserved)
        {
            return Err(Error::Verification);
        }
        // Sûreté : aucune référence à la page ne survit à ce test.
        unsafe { self.release_page(page)? };
        // Sûreté : seconde tentative sur une page absente, sans référence.
        if unsafe { self.release_page(page) } != Err(Error::Missing)
            || self.frames.available != before
        {
            return Err(Error::Verification);
        }
        log(format_args!(
            "TEST cadres recyclage zero, map/unmap, erreurs et restitution OK"
        ));
        let (address, operation) = match scenario {
            b"mem-test" => {
                self.test_spaces()?;
                self.test_executable()?;
                self.test_exhaustion()?;
                log(format_args!(
                    "TEST racines independantes, RX=42 et epuisement transactionnel OK"
                ));
                return Ok(());
            }
            b"mem-ro" => (self.image.rodata, Probe::Write),
            b"mem-alias" => (
                self.frames.info.offset + self.image.pa(self.image.text),
                Probe::Write,
            ),
            b"mem-page-ro" => {
                self.allocate_page(page, Access::ReadOnly)?;
                (ARENA, Probe::Write)
            }
            b"mem-nx" => (NX_TARGET.0.as_ptr() as u64, Probe::Execute),
            b"mem-absent" => (ARENA, Probe::Read),
            _ => return Err(Error::Verification),
        };
        log(format_args!(
            "SONDE adresse={address:#x} acces={}",
            operation.label()
        ));
        // Sûreté : scénario d’amorçage explicite. La VM doit s’arrêter sur cette faute.
        unsafe { paging::probe(address, operation) };
        Err(Error::Verification)
    }

    fn test_spaces(&mut self) -> Result<()> {
        let before = self.frames.available;
        let left = AddressSpace::new(&mut self.frames)?;
        let right = AddressSpace::new(&mut self.frames)?;
        let a = self.frames.take()?;
        let b = self.frames.take()?;
        let va = 0x400000;
        let ro = paging::leaf(a, Access::ReadOnly, self.cache);
        let rw = paging::leaf(b, Access::ReadWrite, self.cache);
        left.insert(&mut self.frames, va, ro)?;
        left.insert(&mut self.frames, va + PAGE, rw)?;
        right.insert(&mut self.frames, va, rw)?;
        if left.descriptor(&self.frames, va)? != ro || right.descriptor(&self.frames, va)? != rw {
            return Err(Error::Verification);
        }
        left.remove(&mut self.frames, va)?;
        if left.descriptor(&self.frames, va + PAGE)? != rw
            || right.descriptor(&self.frames, va)? != rw
        {
            return Err(Error::Verification);
        }
        left.remove(&mut self.frames, va + PAGE)?;
        right.remove(&mut self.frames, va)?;
        for root in [left.root, right.root] {
            if !self.frames.empty_prefix(root, 512) {
                return Err(Error::Verification);
            }
            self.frames.release(root);
        }
        self.frames.release(a);
        self.frames.release(b);
        if self.frames.available != before {
            return Err(Error::Verification);
        }
        Ok(())
    }

    fn test_executable(&mut self) -> Result<()> {
        let frame = self.frames.take()?;
        let page = VirtualAddress(ARENA as usize);
        // Sûreté : cadre exclusif, routine ABI C de la cible, puis mappage RX et
        // synchronisation des caches avant l’appel. Le cadre est repris après le retour.
        unsafe {
            self.frames
                .direct(frame)
                .cast::<[u8; 8]>()
                .write(paging::RETURN_42);
            self.map(page, PhysicalAddress(frame), Access::ReadExecute)?;
            let function: unsafe extern "C" fn() -> u32 = core::mem::transmute(page.0);
            if function() != 42 {
                return Err(Error::Verification);
            }
            let returned = self.unmap(page)?;
            self.frames.release(returned.0);
        }
        Ok(())
    }

    fn test_exhaustion(&mut self) -> Result<()> {
        let before = self.frames.available;
        // Ne laisser que le cadre de données et une des trois tables nécessaires.
        let mut held = self.hold_for_probe(2)?;
        let result = self.allocate_page(VirtualAddress(ARENA as usize), Access::ReadWrite);
        if result != Err(Error::Exhausted)
            || self.frames.available != 2
            || self.space.descriptor(&self.frames, ARENA) != Err(Error::Missing)
        {
            return Err(Error::Verification);
        }
        held.restore_to(self, before);
        if self.frames.available != before {
            return Err(Error::Verification);
        }
        Ok(())
    }
}

/// Liste temporaire de cadres appartenant uniquement aux sondes de pénurie.
pub(crate) struct HeldFrames(u64);

impl Memory {
    pub(crate) fn hold_for_probe(&mut self, remaining: u64) -> Result<HeldFrames> {
        let mut held = HeldFrames(0);
        while self.free_pages() > remaining {
            let frame = self.frames.take().expect("compte de cadres incohérent");
            // Sûreté : cadre exclusif, hors tables actives ; son premier mot
            // contient le lien de la chaîne de la sonde via le HHDM.
            unsafe { self.frames.direct(frame).write(held.0) };
            held.0 = frame;
        }
        Ok(held)
    }
}
impl HeldFrames {
    pub(crate) fn restore_to(&mut self, m: &mut Memory, remaining: u64) {
        while m.free_pages() < remaining {
            let frame = self.0;
            assert_ne!(frame, 0);
            // Sûreté : frame appartient encore à la sonde. Lire le lien avant
            // de rendre ce cadre et de permettre sa réutilisation.
            self.0 = unsafe { m.frames.direct(frame).read() };
            m.frames.release(frame);
        }
    }
}
