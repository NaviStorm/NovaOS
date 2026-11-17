//! Arène fixe du décodage de politique, uniquement dans R22 en qualification.
//! Aucun syscall d'allocation, aucune croissance, aucun accès avant la fin de
//! l'échange cryptographique. Les refus d'allocation terminent le registre.
use core::{alloc::{GlobalAlloc,Layout},cell::UnsafeCell,ptr,
    sync::atomic::{AtomicBool,Ordering}};
use dlmalloc::Dlmalloc;
use spin::Mutex;
pub const BYTES:usize=64*1024;
#[repr(C,align(4096))]
struct Bytes(UnsafeCell<[u8;BYTES]>);
unsafe impl Sync for Bytes {}
static ARENA:Bytes=Bytes(UnsafeCell::new([0;BYTES]));
static ENABLED:AtomicBool=AtomicBool::new(false);
static TAKEN:AtomicBool=AtomicBool::new(false);
struct Backend;
unsafe impl dlmalloc::Allocator for Backend {
    fn alloc(&self,n:usize)->(*mut u8,usize,u32) {
        if n>BYTES || !ENABLED.load(Ordering::Acquire) ||
            TAKEN.swap(true,Ordering::AcqRel) {return (ptr::null_mut(),0,0);}
        (ARENA.0.get().cast::<u8>(),BYTES,1)
    }
    fn remap(&self,_:*mut u8,_:usize,_:usize,_:bool)->*mut u8 {ptr::null_mut()}
    fn free_part(&self,_:*mut u8,_:usize,_:usize)->bool {false}
    fn free(&self,_:*mut u8,_:usize)->bool {false}
    fn can_release_part(&self,_:u32)->bool {false}
    fn allocates_zeros(&self)->bool {true}
    fn page_size(&self)->usize {4096}
}
struct Fixed(Mutex<Dlmalloc<Backend>>);
unsafe impl GlobalAlloc for Fixed {
    unsafe fn alloc(&self,l:Layout)->*mut u8 {
        if !ENABLED.load(Ordering::Acquire) {return ptr::null_mut();}
        unsafe {self.0.lock().malloc(l.size(),l.align())}
    }
    unsafe fn dealloc(&self,p:*mut u8,l:Layout) {
        // Effacement de la charge utile avant réemploi ; aucune promesse
        // d'effacement des registres, des caches ou de copies hors de l'arène.
        for i in 0..l.size() {unsafe {p.add(i).write_volatile(0);}}
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        unsafe {self.0.lock().free(p,l.size(),l.align());}
    }
}
#[cfg_attr(not(test),global_allocator)]
static HEAP:Fixed=Fixed(Mutex::new(Dlmalloc::new_with_allocator(Backend)));
pub fn enable() {ENABLED.store(true,Ordering::Release);}


#[cfg(test)]
#[test]
fn arene_bornee_sans_droit_supplementaire() {
    unsafe {
        let l=Layout::from_size_align(1024,256).unwrap();
        assert!(HEAP.alloc(l).is_null());
        enable();
        let mut pointers=[core::ptr::null_mut();64];
        let mut n=0;
        for p in &mut pointers {
            *p=HEAP.alloc(l);
            if p.is_null() {break;}
            assert_eq!((*p as usize)%256,0);
            (*p).write_bytes(0x5a,1024);n+=1;
        }
        assert!(n>0 && n<64);
        assert!(HEAP.alloc(Layout::from_size_align(BYTES,16).unwrap()).is_null());
        for p in pointers.into_iter().take(n) {
            HEAP.dealloc(p,l);
            // Les premiers mots peuvent être des liens de liste dlmalloc.
            assert!(core::slice::from_raw_parts(p.add(64),960).iter().all(|&b|b==0));
        }
        let p=HEAP.alloc(l);assert!(!p.is_null());HEAP.dealloc(p,l);
    }
}
