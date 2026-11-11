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
unsafe impl Allocator for Pages {
    fn alloc(&self, size: usize) -> (*mut u8, usize, u32) {
        let r = call(HEAP_RESIZE, 0, 0, 0);
        let base = r.value as usize;
        if r.status == OK
            && let Some(n) = size.checked_next_multiple_of(4096)
            && let Some(end) = base.checked_add(n)
            && resize(base, end)
        {
            // Profil de panne du lanceur : les pages sont attribuées et malloc est encore actif.
            crate::allocation_probe();
            return (base as *mut u8, n, 0);
        }
        (null_mut(), 0, 0)
    }
    fn remap(&self, _: *mut u8, _: usize, _: usize, _: bool) -> *mut u8 {
        null_mut()
    }
    fn free_part(&self, p: *mut u8, old: usize, new: usize) -> bool {
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
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // Sûreté : taille et alignement proviennent d'un Layout valide.
        unsafe { self.0.lock().malloc(l.size(), l.align()) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // Sûreté : GlobalAlloc exige le bloc vivant et son Layout d'origine.
        unsafe { self.0.lock().free(p, l.size(), l.align()) }
    }
}
