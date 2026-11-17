//! Les primitives de récupération utilisent exclusivement les tampons bornés.
//! Certaines dépendances exposent également des API alloc ; leur invocation
//! accidentelle doit échouer, jamais créer un tas non budgété.
use core::alloc::{GlobalAlloc, Layout};
struct Denied;
unsafe impl GlobalAlloc for Denied {
    unsafe fn alloc(&self, _:Layout)->*mut u8 {core::ptr::null_mut()}
    unsafe fn dealloc(&self,_:*mut u8,_:Layout) {}
}
#[global_allocator]
static ALLOCATOR:Denied=Denied;
