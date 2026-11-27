use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

pub struct Counter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static HIGH: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let bytes = LIVE.fetch_add(layout.size(), Relaxed) + layout.size();
            HIGH.fetch_max(bytes, Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}
/// Banc monothread, hors démarrage et création des entrées.
pub fn sample<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    let start = LIVE.load(Relaxed);
    HIGH.store(start, Relaxed);
    let value = f();
    (
        value,
        HIGH.load(Relaxed).saturating_sub(start),
        LIVE.load(Relaxed).saturating_sub(start),
    )
}
