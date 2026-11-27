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
#[allow(dead_code)]
#[path = "../../user/killer-e2400/profil.rs"]
mod profil;
#[allow(dead_code, unused_imports)]
#[path = "../../user/storage/runtime.rs"]
mod runtime;
#[allow(dead_code)]
mod scenario_5b;
mod support;
fn allocation_probe() {}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let ok = scenario_5b::run(|| 0).is_ok();
    if ok {
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
