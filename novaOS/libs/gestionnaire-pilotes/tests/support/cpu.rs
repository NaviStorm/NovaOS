//! Banc hôte Darwin, exécuté avec un seul test à la fois. Sous Rosetta le compteur
//! du thread peut progresser dans une autre unité que les nanosecondes du processus.
use std::{
    hint::black_box,
    mem::MaybeUninit,
    time::{Duration, Instant},
};

fn thread_ticks() -> u64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // Les deux appels Darwin du banc écrivent dans un objet local du type attendu.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) },
        0
    );
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

fn process_ns() -> u64 {
    let mut info = MaybeUninit::<libc::rusage>::uninit();
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, info.as_mut_ptr()) },
        0
    );
    let info = unsafe { info.assume_init() };
    let ns = |t: libc::timeval| t.tv_sec as u64 * 1_000_000_000 + t.tv_usec as u64 * 1_000;
    ns(info.ru_utime) + ns(info.ru_stime)
}

/// Pendant le calcul, les autres threads du processus de test sont au repos.
fn sample() -> (u64, u64) {
    let start = Instant::now();
    let process_start = process_ns();
    let thread_start = thread_ticks();
    let mut acc = 7u64;
    while start.elapsed() < Duration::from_millis(30) {
        for _ in 0..8192 {
            acc = black_box(acc.wrapping_mul(37).wrapping_add(19));
        }
    }
    black_box(acc);
    let ticks = thread_ticks() - thread_start;
    let elapsed = process_ns() - process_start;
    assert!(ticks > 0 && elapsed >= 5_000_000);
    (elapsed, ticks)
}

pub struct Clock {
    ns: u64,
    ticks: u64,
}
impl Clock {
    pub fn calibrate() -> Self {
        let mut factors = [sample(), sample(), sample()];
        factors
            .sort_unstable_by_key(|(ns, ticks)| u128::from(*ns) * 1_000_000 / u128::from(*ticks));
        let (ns, ticks) = factors[1];
        let clock = Self { ns, ticks };
        let (reference, reading) = sample();
        let adjusted = clock.scale(reading);
        assert!(
            adjusted.abs_diff(reference) < reference / 50,
            "Étalonnage CPU divergent"
        );
        println!(
            "horloge_cpu_sonde={factors:?} controle_ns={reference} ticks={reading} corrige_ns={adjusted}"
        );
        clock
    }
    fn scale(&self, raw: u64) -> u64 {
        (u128::from(raw) * u128::from(self.ns) / u128::from(self.ticks)) as u64
    }
    pub fn now(&self) -> u64 {
        self.scale(thread_ticks())
    }
}
