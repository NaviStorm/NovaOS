//! Image du banc. Simule un arrêt après acceptation, avant la réponse.
#![no_std]
#![no_main]
#[allow(dead_code)]
mod runtime;
use runtime::*;

// Sûreté : point d'entrée Rust unique appelé par _start dans cette image.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let request = channels().and_then(|(rx, _)| receive(rx));
    call(THREAD_YIELD, 0, 0, 0);
    exit(if request.is_ok() { 70 } else { 71 })
}
