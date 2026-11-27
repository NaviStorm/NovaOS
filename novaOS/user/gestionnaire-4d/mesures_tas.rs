//! Compteurs de l'allocateur EL0 et sonde sans croissance, uniquement dans le banc 4D.
use core::alloc::Layout;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

static BASE: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static FROZEN: AtomicBool = AtomicBool::new(false);

pub fn frozen() -> bool {
    FROZEN.load(Relaxed)
}
pub fn backing(base: usize) {
    let _ = BASE.compare_exchange(0, base, Relaxed, Relaxed);
}
pub fn allocated(bytes: usize) {
    let at = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(at, Relaxed);
}
pub fn freed(bytes: usize) {
    LIVE.fetch_sub(bytes, Relaxed);
}
pub fn live() -> usize {
    LIVE.load(Relaxed)
}

#[allow(dead_code)]
pub fn bilan() -> Option<[usize; 4]> {
    use crate::runtime::{HEAP_RESIZE, call, value};
    let end = value(call(HEAP_RESIZE, 0, 0, 0)).ok()? as usize;
    let reserve = end.checked_sub(BASE.load(Relaxed))?;
    let live = live();
    let peak = PEAK.load(Relaxed);
    let mut low = 0;
    let mut high = reserve.checked_sub(live)?;
    // Une seule tâche dans cet acteur. Les sondes n'ajoutent et ne retirent aucun cadre.
    FROZEN.store(true, Relaxed);
    while low < high {
        let n = low + (high - low).div_ceil(2);
        let layout = Layout::from_size_align(n, 8).expect("taille du tas du banc");
        // Sûreté : le bloc obtenu reste privé et est rendu immédiatement avec le même Layout.
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            high = n - 1;
        } else {
            unsafe { alloc::alloc::dealloc(ptr, layout) };
            low = n;
        }
    }
    FROZEN.store(false, Relaxed);
    Some([reserve, live, peak, low])
}
