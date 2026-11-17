//! Piles et TLS natifs. Les feuilles appartiennent à la racine du processus.
//! Seul l'ordonnanceur conserve le lien entre une tranche et son TCB vivant.
use super::*;
use core::sync::atomic::{AtomicU64, Ordering};

pub(crate) const MAX_SECONDARY_THREADS: usize = 8;
pub(crate) const MAX_THREAD_STACK_PAGES: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThreadMemory {
    pub bottom: u64,
    pub top: u64,
    pub tls: u64,
}

impl ThreadMemory {
    fn secondary(bottom: u64, pages: usize) -> Self {
        let top = bottom + pages as u64 * PAGE;
        Self {
            bottom,
            top,
            tls: top + PAGE,
        }
    }
}

impl UserSpace {
    fn active_layouts(&self) -> impl Iterator<Item = ThreadMemory> + '_ {
        self.thread_stacks
            .iter()
            .zip(&self.thread_pages)
            .filter(|(_, n)| **n != 0)
            .map(|(&bottom, &pages)| ThreadMemory::secondary(bottom, pages.into()))
    }

    /// Recherche à la demande. Le lancement historique ne consomme aucun cadre
    /// TLS et ne suppose aucun trou supplémentaire dans les PT_LOAD admis.
    fn free_thread_range(&self, pages: usize) -> Result<u64> {
        let size = pages as u64 * PAGE;
        let (first, low) = if self.elf.is_some() {
            (DATA + 2 * PAGE, PAGE)
        } else {
            (TOP + PAGE, TOP + PAGE)
        };
        for (mut base, limit) in [(first, HEAP), (low, first)] {
            while base + size <= limit {
                let mut next = base;
                let mut exclude = |start: u64, end: u64| {
                    if base < end && start < base + size {
                        next = next.max(end);
                    }
                };
                // DATA+PAGE est aussi la seconde page du banc copyin/copyout.
                exclude(DATA, DATA + 2 * PAGE);
                exclude(self.stack_bottom() - PAGE, self.stack_top() + PAGE);
                if let Some(elf) = self.elf.as_ref().map(Layout::view) {
                    for seg in elf.segments() {
                        exclude(seg.start & !(PAGE - 1), seg.end.next_multiple_of(PAGE));
                    }
                }
                if self.main_tls != 0 {
                    exclude(self.main_tls, self.main_tls + PAGE);
                }
                for layout in self.active_layouts() {
                    exclude(layout.bottom - PAGE, layout.tls + PAGE);
                }
                if next == base {
                    return Ok(base);
                }
                base = next;
            }
        }
        Err(Error::Exhausted)
    }

    pub(crate) fn main_thread(&self) -> ThreadMemory {
        ThreadMemory {
            bottom: self.stack_bottom(),
            top: self.stack_top(),
            tls: self.main_tls,
        }
    }

    pub(crate) fn main_tls(&mut self, m: &mut Memory) -> Result<u64> {
        if !self.main_stack {
            return Err(Error::Missing);
        }
        if self.main_tls == 0 {
            let va = self.free_thread_range(1)?;
            self.map(m, va, Access::ReadWrite, 0, &[])?;
            paging::publish();
            self.main_tls = va;
        }
        Ok(self.main_tls)
    }

    pub(super) fn owns_thread_page(&self, page: u64) -> bool {
        (self.main_tls != 0 && page == self.main_tls)
            || self
                .active_layouts()
                .any(|t| page == t.tls || (t.bottom..t.top).contains(&page))
    }

    pub(crate) fn allocate_thread(&mut self, m: &mut Memory, pages: usize) -> Result<ThreadMemory> {
        if !(1..=MAX_THREAD_STACK_PAGES).contains(&pages) {
            return Err(Error::Geometry);
        }
        let slot = self
            .thread_pages
            .iter()
            .position(|&n| n == 0)
            .ok_or(Error::Exhausted)?;
        let base = self.free_thread_range(pages + 3)?;
        let layout = ThreadMemory::secondary(base + PAGE, pages);
        for guard in [layout.bottom - PAGE, layout.top] {
            if self.tables.descriptor(&m.frames, guard) != Err(Error::Missing) {
                return Err(Error::Verification);
            }
        }
        // Une pénurie peut aussi survenir dans l'allocation d'une table. Garder
        // le slot invisible au copyin et défaire chaque feuille déjà installée.
        for va in (layout.bottom..layout.top)
            .step_by(PAGE as usize)
            .chain([layout.tls])
        {
            if let Err(error) = self.map(m, va, Access::ReadWrite, 0, &[]) {
                for prior in (layout.bottom..va.min(layout.top)).step_by(PAGE as usize) {
                    Self::release_page(&self.tables, m, prior);
                }
                return Err(error);
            }
        }
        paging::publish();
        self.thread_stacks[slot] = layout.bottom;
        self.thread_pages[slot] = pages as u8;
        Ok(layout)
    }

    /// Le TCB doit avoir quitté tous les CPU. La racine peut rester active chez
    /// un frère. remove() attend l'invalidation distante avant tout recyclage.
    pub(crate) fn release_thread(&mut self, m: &mut Memory, layout: ThreadMemory) {
        let slot = self
            .thread_stacks
            .iter()
            .position(|&va| va == layout.bottom)
            .expect("pile inconnue");
        let pages = usize::from(self.thread_pages[slot]);
        assert!(pages > 0);
        assert_eq!(ThreadMemory::secondary(layout.bottom, pages), layout);
        for va in (layout.bottom..layout.top)
            .step_by(PAGE as usize)
            .chain([layout.tls])
        {
            Self::release_page(&self.tables, m, va);
        }
        self.thread_stacks[slot] = 0;
        self.thread_pages[slot] = 0;
    }

    fn release_main_tls(&mut self, m: &mut Memory) {
        if self.main_tls != 0 {
            Self::release_page(&self.tables, m, self.main_tls);
            self.main_tls = 0;
        }
    }

    pub(crate) fn release_main_thread(&mut self, m: &mut Memory) {
        assert!(self.main_stack);
        let top = self.stack_top() - PAGE;
        let index = self.mapped[..self.len]
            .iter()
            .position(|&va| va == top)
            .expect("pile principale");
        Self::release_page(&self.tables, m, top);
        self.len -= 1;
        self.mapped[index] = self.mapped[self.len];
        for i in 0..self.stack_extra {
            Self::release_page(&self.tables, m, self.stack_bottom() + i as u64 * PAGE);
        }
        self.stack_extra = 0;
        self.main_stack = false;
        self.release_main_tls(m);
    }

    pub(super) fn release_threads(&mut self, m: &mut Memory) {
        for (slot, pages) in self.thread_pages.into_iter().enumerate() {
            if pages != 0 {
                self.release_thread(
                    m,
                    ThreadMemory::secondary(self.thread_stacks[slot], pages.into()),
                );
            }
        }
        self.release_main_tls(m);
    }

    /// Mot partagé de l'attente native. L'alignement exclut tout franchissement
    /// de page. La feuille RW et le cadre restent stables jusqu'à la lecture.
    pub(crate) fn atomic_word(&self, m: &Memory, va: u64) -> Option<u64> {
        if va % align_of::<AtomicU64>() as u64 != 0 {
            return None;
        }
        let frame = self.frame_for(m, va, true)?;
        let word = m
            .frames
            .direct(frame)
            .wrapping_add((va % PAGE) as usize / size_of::<u64>());
        Some(unsafe { AtomicU64::from_ptr(word).load(Ordering::Acquire) })
    }
}

#[cfg(feature = "thread-probe")]
pub(crate) fn test_thread_memory() {
    crate::heap::with_pages(|m| {
        let initial = (m.frames.available, m.frames.tables);
        let mut a = UserSpace::new(m, &[0], &[], None)?;
        let mut b = UserSpace::new(m, &[0], &[], None)?;
        assert_eq!(a.main_thread().tls, 0);
        assert_eq!(a.page_counts(), (2, 1));
        let tls = a.main_tls(m)?;
        assert_eq!(a.main_tls(m)?, tls);
        assert_eq!(b.main_tls(m)?, tls);
        let baseline = (m.frames.available, m.frames.tables);
        let main = a.main_thread();
        assert_eq!(a.atomic_word(m, main.tls), Some(0));
        assert!(a.copy_to_user(m, main.tls, &1u64.to_ne_bytes()));
        assert_eq!(b.atomic_word(m, b.main_thread().tls), Some(0));
        assert_eq!(a.allocate_thread(m, 0), Err(Error::Geometry));
        assert_eq!(
            a.allocate_thread(m, MAX_THREAD_STACK_PAGES + 1),
            Err(Error::Geometry)
        );

        // Ne laisser réellement que deux cadres disponibles dans cet invité.
        // Cette racine a déjà sa table : le troisième mapping échoue après
        // deux insertions. Garder la garde des pages jusqu'à leur restitution.
        {
            let candidate = ThreadMemory::secondary(a.free_thread_range(6)? + PAGE, 3);
            let mut held = m.hold_for_probe(2)?;
            let pinned = (m.frames.available, m.frames.tables);
            assert_eq!(a.allocate_thread(m, 3), Err(Error::Exhausted));
            assert_eq!(pinned, (m.frames.available, m.frames.tables));
            for va in (candidate.bottom..candidate.top)
                .step_by(PAGE as usize)
                .chain([candidate.tls])
            {
                assert_eq!(a.tables.descriptor(&m.frames, va), Err(Error::Missing));
            }
            held.restore_to(m, baseline.0);
            assert_eq!(baseline, (m.frames.available, m.frames.tables));
        }

        let t = a.allocate_thread(m, 3)?;
        let other = b.allocate_thread(m, 3)?;
        assert_eq!(t, other); // mêmes VA dans deux racines physiques distinctes
        assert_ne!(a.frame_for(m, t.tls, true), b.frame_for(m, other.tls, true));
        assert_ne!(
            a.frame_for(m, t.bottom, true),
            b.frame_for(m, other.bottom, true)
        );
        for guard in [t.bottom - PAGE, t.top, main.bottom - PAGE, main.top] {
            assert_eq!(a.tables.descriptor(&m.frames, guard), Err(Error::Missing));
            assert!(a.atomic_word(m, guard).is_none());
        }
        assert!(a.atomic_word(m, CODE).is_none());
        assert!(a.atomic_word(m, t.tls + 1).is_none());
        for va in [t.bottom, t.top - 8, t.tls, t.tls + PAGE - 8] {
            assert_eq!(a.atomic_word(m, va), Some(0));
            assert!(a.copy_to_user(m, va, &99u64.to_ne_bytes()));
            assert_eq!(a.atomic_word(m, va), Some(99));
            assert_eq!(b.atomic_word(m, va), Some(0));
        }
        assert_eq!(a.atomic_word(m, main.tls), Some(1));
        a.release_thread(m, t);
        for va in [t.bottom, t.top - PAGE, t.tls] {
            assert_eq!(a.tables.descriptor(&m.frames, va), Err(Error::Missing));
            assert!(a.atomic_word(m, va).is_none());
        }
        let recycled = a.allocate_thread(m, 3)?;
        assert_eq!(t, recycled);
        for va in [t.bottom, t.top - 8, t.tls, t.tls + PAGE - 8] {
            assert_eq!(a.atomic_word(m, va), Some(0));
        }
        a.release_thread(m, recycled);
        b.release_thread(m, other);
        assert_eq!(baseline, (m.frames.available, m.frames.tables));
        for i in 0..MAX_SECONDARY_THREADS {
            a.allocate_thread(m, if i == 0 { MAX_THREAD_STACK_PAGES } else { 1 })?;
        }
        assert_eq!(a.allocate_thread(m, 1), Err(Error::Exhausted));
        a.release_main_thread(m);
        assert_eq!(a.main_tls(m), Err(Error::Missing));
        for va in [main.top - PAGE, main.tls] {
            assert_eq!(a.tables.descriptor(&m.frames, va), Err(Error::Missing));
            assert!(a.atomic_word(m, va).is_none());
        }
        assert_eq!(
            a.page_counts().1,
            MAX_THREAD_STACK_PAGES + MAX_SECONDARY_THREADS - 1
        );
        // La destruction de toute la racine ramasse aussi les tranches restantes.
        a.release(m);
        b.release(m);
        assert_eq!(initial, (m.frames.available, m.frames.tables));
        Ok(())
    })
    .expect("tranches de threads");
    crate::hal::log(format_args!(
        "THREADS VM gardes=2 tls=zero recyclage=verifie rollback=verifie cadres=tables_rendus"
    ));
}
