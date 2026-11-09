use super::{BASE, CAPACITY, Error, Heap, Result};
use crate::{hal, memory::Memory};
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) struct State {
    pub(super) heap: Heap,
    pub(super) pages: Memory,
}

pub(super) struct KernelAllocator {
    locked: AtomicBool,
    state: UnsafeCell<Option<State>>,
}

// Sûreté : tout accès à state, au fournisseur de pages et aux métadonnées du tas
// est protégé par le même verrou acquire/release. Aucune référence n’en sort.
unsafe impl Sync for KernelAllocator where State: Send {}

#[global_allocator]
pub(super) static GLOBAL: KernelAllocator = KernelAllocator {
    locked: AtomicBool::new(false),
    state: UnsafeCell::new(None),
};

struct Guard<'a> {
    locked: &'a AtomicBool,
    flags: usize,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::Release);
        hal::interrupts::restore(self.flags);
    }
}

impl KernelAllocator {
    fn with<R>(&self, f: impl FnOnce(&mut Option<State>) -> R) -> R {
        let flags = hal::interrupts::save_and_mask();
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        let _guard = Guard {
            locked: &self.locked,
            flags,
        };
        // Sûreté : propriétaire unique du verrou, IRQ locales masquées. Les
        // routines appelées ici utilisent uniquement la pile et les pages de la MMU.
        f(unsafe { &mut *self.state.get() })
    }
    pub(super) fn with_state<R>(&self, f: impl FnOnce(&mut State) -> Result<R>) -> Result<R> {
        self.with(|slot| f(slot.as_mut().ok_or(Error::NoMemory)?))
    }

    /// L’appelant conserve la propriété exclusive de ptr jusqu’au retour.
    unsafe fn resize(&self, ptr: *mut u8, old: Layout, new: Layout) -> *mut u8 {
        let target =
            pointer(self.with_state(|s| s.heap.prepare_resize(&mut s.pages, ptr, old, new)));
        if target.is_null() || target == ptr {
            return target;
        }
        // Sûreté : les deux allocations sont vivantes, distinctes et exclusives au
        // client. Leur durée de vie protège la copie même après libération du verrou.
        unsafe { ptr::copy_nonoverlapping(ptr, target, old.size().min(new.size())) };
        if let Err(e) = self.with_state(|s| s.heap.free(&mut s.pages, ptr, old)) {
            fatal(e);
        }
        target
    }
}

pub(crate) fn install(pages: Memory) {
    GLOBAL.with(|slot| {
        assert!(slot.is_none(), "tas déjà installé");
        *slot = Some(State {
            heap: Heap::new(BASE, CAPACITY).expect("fenêtre du tas"),
            pages,
        });
    });
}

pub(super) fn fatal(e: Error) -> ! {
    // Le diagnostic utilise core::fmt sur le port série et ne demande aucune allocation.
    hal::log(format_args!("Erreur fatale du tas : {e:?}"));
    hal::cpu::halt()
}

fn pointer(result: Result<*mut u8>) -> *mut u8 {
    match result {
        Ok(p) => p,
        Err(Error::NoMemory) => ptr::null_mut(),
        Err(e) => fatal(e),
    }
}

/// # Safety
/// ptr est un bloc vivant exclusif du tas global, de disposition old. Sur succès,
/// seule l’adresse rendue reste valide ; sur pénurie le bloc initial reste intact.
pub(crate) unsafe fn realign(ptr: *mut u8, old: Layout, new: Layout) -> *mut u8 {
    // Sûreté : le contrat de realign garantit la propriété exclusive.
    unsafe { GLOBAL.resize(ptr, old, new) }
}

// Sûreté : le moteur rend des blocs LIVE exclusifs et alignés, conserve l’ancien
// bloc si realloc échoue, et ne déroule jamais la pile (panique noyau terminale).
unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        pointer(self.with_state(|s| s.heap.allocate(&mut s.pages, layout)))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if let Err(e) = self.with_state(|s| s.heap.free(&mut s.pages, ptr, layout)) {
            fatal(e);
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let Ok(new) = Layout::from_size_align(new_size, old.align()) else {
            return ptr::null_mut();
        };
        // Sûreté : le contrat de GlobalAlloc::realloc impose un bloc valide et exclusif.
        unsafe { self.resize(ptr, old, new) }
    }
}
