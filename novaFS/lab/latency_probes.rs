//! Sondes inclusives du banc de lecture. L'enregistrement est local au thread,
//! de taille fixe et sans allocation, y compris depuis l'allocateur global.
use std::{
    cell::{Cell, RefCell},
    time::Instant,
};

#[path = "thread_clock.rs"]
mod thread_clock;
pub use thread_clock::{calibrate, thread_cpu_ns};

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum Phase {
    Empty,
    ReadRequest,
    CacheLookup,
    DataMiss,
    MetadataMiss,
    Eviction,
    Extent,
    ImageRead,
    Allocation,
    Deallocation,
    Compression,
    Decompression,
}

const NAMES: [&str; 12] = [
    "sonde_vide",
    "lecture_moteur_64k",
    "recherche_cache",
    "defaut_donnees",
    "defaut_metadonnees",
    "eviction_clock",
    "parcours_extent",
    "lecture_image",
    "allocation_system",
    "liberation_system",
    "compression_lz4",
    "decompression_lz4",
];

// Nanosecondes exactes de 0 à 15, puis huit classes par puissance de deux.
// 496 compteurs couvrent u64::MAX. Aucune liste d'échantillons ne grandit.
const BINS: usize = 496;
#[derive(Clone, Copy)]
struct Histogram {
    bins: [u64; BINS],
    sum: u128,
    max: u64,
    n: u64,
}

impl Histogram {
    const ZERO: Self = Self {
        bins: [0; BINS],
        sum: 0,
        max: 0,
        n: 0,
    };

    fn record(&mut self, ns: u64) {
        let shift = ns.checked_ilog2().unwrap_or(0).saturating_sub(3);
        let bucket = (8 * u64::from(shift) + (ns >> shift)) as usize;
        self.bins[bucket] += 1;
        self.n += 1;
        self.sum += u128::from(ns);
        self.max = self.max.max(ns);
    }

    fn percentile_upper(&self, percent: u64) -> Option<u64> {
        if self.n == 0 {
            return None;
        }
        let rank = (u128::from(self.n) * u128::from(percent)).div_ceil(100);
        let mut seen = 0;
        for (bucket, &count) in self.bins.iter().enumerate() {
            seen += u128::from(count);
            if seen >= rank {
                let shift = bucket.saturating_sub(8) / 8;
                let top = (((bucket - shift * 8 + 1) as u128) << shift) - 1;
                return Some(self.max.min(top as u64));
            }
        }
        unreachable!("histogramme incompatible avec son effectif")
    }

    fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "n": self.n, "somme_ns": self.sum, "max_ns": self.max,
            "p50_borne_sup_ns": self.percentile_upper(50),
            "p95_borne_sup_ns": self.percentile_upper(95),
            "p99_borne_sup_ns": self.percentile_upper(99),
        })
    }
}

#[derive(Clone, Copy)]
struct Counters {
    wall: Histogram,
    cpu: Histogram,
}
const EMPTY: Counters = Counters {
    wall: Histogram::ZERO,
    cpu: Histogram::ZERO,
};
const COUNT: usize = NAMES.len();
/// État fixe des sondes de ce thread, hors tas Rust.
pub const TLS_BYTES: usize = size_of::<RefCell<[Counters; COUNT]>>() + size_of::<Cell<bool>>();

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNTERS: RefCell<[Counters; COUNT]> = const { RefCell::new([EMPTY; COUNT]) };
}

pub fn begin() {
    assert!(!ENABLED.with(Cell::get));
    COUNTERS.with(|c| c.borrow_mut().fill(EMPTY));
    ENABLED.with(|on| on.set(true));
}

/// Compteurs cumulatifs du thread pour le banc P6. La lecture ne réserve aucun tampon.
pub fn codec_cpu_ns() -> (u128, u128) {
    COUNTERS.with(|c| {
        let c = c.borrow();
        (
            c[Phase::Compression as usize].cpu.sum,
            c[Phase::Decompression as usize].cpu.sum,
        )
    })
}

/// Désactive avant la construction du JSON, dont le tas appartient au rapport.
pub fn finish() -> serde_json::Value {
    assert!(ENABLED.with(|on| on.replace(false)));
    COUNTERS.with(|c| {
        let counters = c.borrow();
        NAMES
            .iter()
            .zip(&*counters)
            .map(|(name, hist)| {
                (
                    (*name).to_owned(),
                    serde_json::json!({"mur": hist.wall.report(), "cpu_thread": hist.cpu.report()}),
                )
            })
            .collect::<serde_json::Map<_, _>>()
            .into()
    })
}

#[must_use]
pub struct Sample {
    phase: Phase,
    wall: Instant,
    cpu: u64,
}

/// Une sonde inactive ne lit aucune horloge. Les builds sans la feature
/// éliminent les appels aux sondes à la compilation.
#[inline]
pub fn enter(phase: Phase) -> Option<Sample> {
    ENABLED
        .try_with(Cell::get)
        .unwrap_or(false)
        .then(|| Sample {
            phase,
            wall: Instant::now(),
            cpu: thread_cpu_ns(),
        })
}

impl Drop for Sample {
    fn drop(&mut self) {
        let cpu = thread_cpu_ns().saturating_sub(self.cpu);
        let wall = self.wall.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        COUNTERS.with(|c| {
            let h = &mut c.borrow_mut()[self.phase as usize];
            h.wall.record(wall);
            h.cpu.record(cpu);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_et_rangs_aux_frontieres() {
        let samples = [0, 1, 15, 16, 17, 18, 31, 32, 127, 128, 255, 256, u64::MAX];
        let mut h = Histogram::ZERO;
        assert_eq!(h.percentile_upper(99), None);
        for &ns in &samples {
            h.record(ns);
        }
        for p in 1..=100 {
            let actual = samples[(samples.len() * p).div_ceil(100) - 1];
            let upper = h.percentile_upper(p as u64).unwrap();
            assert!(actual <= upper && upper - actual <= actual / 8);
        }
        assert_eq!(h.max, u64::MAX);
        assert_eq!(h.n, samples.len() as u64);
    }
}
