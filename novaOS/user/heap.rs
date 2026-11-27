//! dlmalloc gère les blocs privés. Le noyau attribue et retire seulement des pages entières.
use crate::runtime::{HEAP_RESIZE, OK, call};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use dlmalloc::{Allocator, Dlmalloc};
use spin::Mutex;

struct Pages;
fn resize(old: usize, new: usize) -> bool {
    call(HEAP_RESIZE, old as u64, new as u64, 0).status == OK
}
// Sûreté : le noyau valide la borne courante du processus et garantit des pages privées zéro.
#[allow(unexpected_cfgs)] // Mesures réservées aux deux ELF du profil gestionnaire-4d.
unsafe impl Allocator for Pages {
    fn alloc(&self, size: usize) -> (*mut u8, usize, u32) {
        #[cfg(novaos_mesure_tas_4d)]
        if crate::mesures_tas::frozen() {
            return (null_mut(), 0, 0);
        }
        let r = call(HEAP_RESIZE, 0, 0, 0);
        let base = r.value as usize;
        if r.status == OK
            && let Some(n) = size.checked_next_multiple_of(4096)
            && let Some(end) = base.checked_add(n)
            && resize(base, end)
        {
            // Profil de panne du lanceur : les pages sont attribuées et malloc est encore actif.
            #[cfg(novaos_mesure_tas_4d)]
            crate::mesures_tas::backing(base);
            crate::allocation_probe();
            return (base as *mut u8, n, 0);
        }
        (null_mut(), 0, 0)
    }
    fn remap(&self, _: *mut u8, _: usize, _: usize, _: bool) -> *mut u8 {
        null_mut()
    }
    fn free_part(&self, p: *mut u8, old: usize, new: usize) -> bool {
        #[cfg(novaos_mesure_tas_4d)]
        if crate::mesures_tas::frozen() {
            return false;
        }
        let base = p as usize;
        new < old
            && base
                .checked_add(old)
                .is_some_and(|end| resize(end, base + new))
    }
    fn free(&self, p: *mut u8, size: usize) -> bool {
        self.free_part(p, size, 0)
    }
    fn can_release_part(&self, _: u32) -> bool {
        true
    }
    fn allocates_zeros(&self) -> bool {
        true
    }
    fn page_size(&self) -> usize {
        4096
    }
}
struct Heap(Mutex<Dlmalloc<Pages>>);
#[global_allocator]
static HEAP: Heap = Heap(Mutex::new(Dlmalloc::new_with_allocator(Pages)));
// Sûreté : un verrou par image protège les métadonnées. Le backend ne réentre pas dans alloc.
#[allow(unexpected_cfgs)]
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // Sûreté : taille et alignement proviennent d'un Layout valide.
        let p = unsafe { self.0.lock().malloc(l.size(), l.align()) };
        #[cfg(novaos_mesure_tas_4d)]
        if !p.is_null() && !crate::mesures_tas::frozen() {
            crate::mesures_tas::allocated(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // Sûreté : GlobalAlloc exige le bloc vivant et son Layout d'origine.
        #[cfg(novaos_mesure_tas_4d)]
        if !crate::mesures_tas::frozen() {
            crate::mesures_tas::freed(l.size());
        }
        unsafe { self.0.lock().free(p, l.size(), l.align()) }
    }
}
