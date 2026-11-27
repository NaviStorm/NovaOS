#![no_std]
#![no_main]
#[macro_use]
extern crate alloc;
#[allow(dead_code)]
#[path = "../../user/killer-e2400/adaptateur.rs"]
mod adaptateur;
#[allow(dead_code)]
#[path = "../../user/killer-e2400/boucle.rs"]
mod boucle;
#[path = "../../user/killer-e2400/courtier_simule.rs"]
mod courtier_simule;
#[path = "../../user/heap.rs"]
mod heap;
#[path = "../../user/gestionnaire-4d/mesures_tas.rs"]
mod mesures_tas;
#[allow(dead_code)]
#[path = "../../user/killer-e2400/profil.rs"]
mod profil;
#[allow(dead_code, unused_imports)]
#[path = "../../user/storage/runtime.rs"]
mod runtime;
#[allow(dead_code)]
mod scenario_5b;
mod support;
use core::sync::atomic::{AtomicUsize, Ordering::Relaxed};
static BASE: AtomicUsize = AtomicUsize::new(0);
static MAX_BACKING: AtomicUsize = AtomicUsize::new(0);
fn heap_end() -> Option<usize> {
    runtime::value(runtime::call(runtime::HEAP_RESIZE, 0, 0, 0))
        .ok()
        .map(|n| n as usize)
}
// Appelé après une croissance réussie de dlmalloc. La lecture de la borne
// du processus utilise le sélecteur existant et n'alloue rien.
fn allocation_probe() {
    let Some(n) = heap_end().and_then(|end| end.checked_sub(BASE.load(Relaxed))) else {
        runtime::exit(12)
    };
    MAX_BACKING.fetch_max(n, Relaxed);
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let Some(start) = heap_end() else {
        runtime::exit(12)
    };
    BASE.store(start, Relaxed);
    let ok = scenario_5b::run(|| 0).is_ok();
    if ok {
        let Some([reserved, live, peak, _]) = mesures_tas::bilan() else {
            runtime::exit(12)
        };
        let backing = MAX_BACKING.load(Relaxed);
        if live != 0 || peak < 777 + 12248 || peak > backing || reserved > backing {
            runtime::exit(12);
        }
        let record = format!(
            "E2400 RAM tas_pic={peak} tas_vivant_fin={live} pages_pic={} pages_fin={}",
            backing / 4096,
            reserved / 4096
        );
        runtime::call(
            runtime::DEBUG_WRITE,
            record.as_ptr() as u64,
            record.len() as u64,
            0,
        );
        let msg =
            b"E2400 5B registre=259 tx=264 rx=264 quarantaine=259,130,0 retour=OK dma_physique=0";
        runtime::call(
            runtime::DEBUG_WRITE,
            msg.as_ptr() as u64,
            msg.len() as u64,
            0,
        );
    }
    runtime::exit(if ok { 0 } else { 12 })
}
