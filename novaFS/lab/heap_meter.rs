//! Instrumentation du laboratoire Darwin, partagée par le banc et la CLI de mesure.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(target_os = "macos")]
pub(crate) fn current_stack_size() -> usize {
    use std::ffi::c_void;
    unsafe extern "C" {
        fn pthread_self() -> *mut c_void;
        fn pthread_get_stacksize_np(thread: *mut c_void) -> usize;
    }
    // Sûreté : le pthread interrogé est le thread courant vivant ; la requête
    // renvoie la taille de réservation sans déréférencer sa pile.
    unsafe { pthread_get_stacksize_np(pthread_self()) }
}

pub(crate) static LIVE: AtomicUsize = AtomicUsize::new(0);
pub(crate) static PEAK: AtomicUsize = AtomicUsize::new(0);
pub(crate) static PHASE_PEAK: AtomicUsize = AtomicUsize::new(0);

pub(crate) static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct Measured;
#[global_allocator]
static HEAP: Measured = Measured;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn malloc_size(ptr: *const u8) -> usize;
}

/// L’allocateur System de Darwin utilise libSystem pour ses blocs, y compris
/// les allocations alignées. Ce compteur comprend l’arrondi malloc de chacun.
unsafe fn capacity(ptr: *mut u8, layout: Layout) -> usize {
    #[cfg(target_os = "macos")]
    {
        // Sûreté : ptr est une allocation System vivante appartenant à ce banc.
        unsafe { malloc_size(ptr) }.max(layout.size())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ptr;
        layout.size()
    }
}

fn record_peak(live: usize) {
    PEAK.fetch_max(live, Ordering::Relaxed);
    PHASE_PEAK.fetch_max(live, Ordering::Relaxed);
}

// Sûreté : délégation strictement symétrique à System avec son Layout original.
// Les compteurs atomiques et malloc_size n’appellent pas l’allocateur Rust.
unsafe impl GlobalAlloc for Measured {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Sûreté : le demandeur fournit un Layout valide selon GlobalAlloc.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            // Sûreté : l’allocation vient de réussir.
            let bytes = unsafe { capacity(ptr, layout) };
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            record_peak(LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Sûreté : le bloc est encore vivant à l’entrée dans dealloc.
        let bytes = unsafe { capacity(ptr, layout) };
        LIVE.fetch_sub(bytes, Ordering::Relaxed);
        // Sûreté : restitution au fournisseur d’origine avec le Layout d’origine.
        unsafe { System.dealloc(ptr, layout) }
    }
    // L’implémentation par défaut de realloc alloue, copie puis libère.
    // Le pic de coexistence des deux blocs est donc inclus dans la mesure.
}
