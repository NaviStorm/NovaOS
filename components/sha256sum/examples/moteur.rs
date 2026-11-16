//! Coût du moteur seul sur Darwin. Le tampon est initialisé avant la mesure.
use nova_sha256sum::{Digest, Sha256};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut block = [0u8; 65536];
    for (i, byte) in block.iter_mut().enumerate() {
        *byte = (i % 251) as u8;
    }
    for _ in 0..5 {
        let mut h = Sha256::new();
        let start = Instant::now();
        for _ in 0..4096 {
            h.update(black_box(&block));
        }
        let sum = h.finalize();
        let secs = start.elapsed().as_secs_f64();
        println!(
            "{} {:.9} {:.3} {}",
            block.len() * 4096,
            secs,
            256. / secs,
            sum.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
    }
}
