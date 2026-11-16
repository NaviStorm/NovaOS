#![no_std]
extern crate alloc;
pub use nova_sha256sum as sha256;
#[path = "../../novaOS/user/heap.rs"]
mod heap;
#[path = "../../novaOS/user/storage/runtime.rs"]
pub mod runtime;
// Le banc optionnel échantillonne la borne à chaque acquisition de pages par dlmalloc.
pub mod heap_profile {
    use super::runtime as rt;
    use core::sync::atomic::{AtomicU64, Ordering::Relaxed};
    static BASE: AtomicU64 = AtomicU64::new(0);
    static PEAK: AtomicU64 = AtomicU64::new(0);
    pub fn start() {
        if option_env!("NOVA_SHA256_MEMORY") == Some("1") {
            let base = rt::call(rt::HEAP_RESIZE, 0, 0, 0);
            if base.status == rt::OK {
                BASE.store(base.value, Relaxed);
                PEAK.store(base.value, Relaxed);
            }
        }
    }
    pub(super) fn acquired() {
        if option_env!("NOVA_SHA256_MEMORY") == Some("1") && BASE.load(Relaxed) != 0 {
            let end = rt::call(rt::HEAP_RESIZE, 0, 0, 0);
            if end.status == rt::OK {
                PEAK.fetch_max(end.value, Relaxed);
            }
        }
    }
    pub fn bytes() -> Option<u64> {
        let base = BASE.load(Relaxed);
        (base != 0).then(|| PEAK.load(Relaxed) - base)
    }
}
fn allocation_probe() {
    heap_profile::acquired();
}
pub mod command;
pub mod console;
#[path = "../../novaOS/user/storage/fileapi.rs"]
pub mod fileapi;
pub mod process;
#[path = "../../novaOS/abi/files.rs"]
pub mod protocol;
#[path = "../../novaOS/abi/terminal.rs"]
pub mod tty;

pub mod directory;
pub mod options;
pub mod vi_wire;

pub mod documentation;
