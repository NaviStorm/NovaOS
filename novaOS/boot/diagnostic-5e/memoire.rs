use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

// Une seule invocation sur le BSP, aucun callback enregistré. Tous les objets
// temporaires appartiennent à la même transaction. Le dernier drop la clôt.
const BYTES: usize = 4 * 1024 * 1024;
#[repr(align(4096))]
struct Arena(UnsafeCell<[u8; BYTES]>);
unsafe impl Sync for Arena {}
static ARENA: Arena = Arena(UnsafeCell::new([0; BYTES]));
static END: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
pub struct Heap;
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.align() > 4096 {
            return ptr::null_mut();
        }
        let result = END.try_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            old.checked_next_multiple_of(layout.align())?
                .checked_add(layout.size())
                .filter(|n| *n <= BYTES)
        });
        match result {
            Ok(old) => {
                LIVE.fetch_add(1, Ordering::Relaxed);
                unsafe {
                    ARENA
                        .0
                        .get()
                        .cast::<u8>()
                        .add(old.next_multiple_of(layout.align()))
                }
            }
            Err(_) => ptr::null_mut(),
        }
    }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
        LIVE.fetch_sub(1, Ordering::Relaxed);
    }
}
pub fn close() -> (usize, usize) {
    let live = LIVE.load(Ordering::Relaxed);
    let used = END.load(Ordering::Relaxed);
    if live == 0 {
        unsafe {
            ptr::write_bytes(ARENA.0.get().cast::<u8>(), 0, used);
        }
        END.store(0, Ordering::Relaxed);
    }
    (used, live)
}
