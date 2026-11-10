//! Histogrammes du banc, alloués seulement dans la VM de mesure. Huit classes
//! par octave, quantiles rendus par la borne supérieure, maximum exact.
use super::*;
use core::sync::atomic::AtomicBool;
static ACTIVE: AtomicBool = AtomicBool::new(false);
pub(super) fn stamp() -> u64 {
    if ACTIVE.load(Ordering::Relaxed) {
        hal::timer::counter()
    } else {
        0
    }
}
#[derive(Clone)]
struct Histogram {
    bins: [u32; 256],
    samples: u64,
    max: u64,
}
impl Histogram {
    fn record(&mut self, delta: u64) {
        let d = delta.max(1);
        let power = d.ilog2() as usize;
        let frac = ((d - (1u64 << power)) as u128 * 8 / (1u128 << power)) as usize;
        self.bins[(power * 8 + frac).min(255)] += 1;
        self.samples += 1;
        self.max = self.max.max(delta);
    }
    fn quantile(&self, pc: u64) -> u64 {
        let mut count = 0;
        for (i, &n) in self.bins.iter().enumerate() {
            count += n as u64;
            if count > 0 && count * 100 >= self.samples * pc {
                if i == 255 {
                    return self.max;
                }
                return (((1u64 << (i / 8)) * (9 + (i % 8) as u64)).div_ceil(8)).min(self.max);
            }
        }
        0
    }
}
pub(super) struct Metrics {
    hist: Vec<Histogram>,
    pub focus: u64,
    peer: u64,
    pub entry: u64,
    pub admitted: [u64; 32],
    pub woken: [u64; 32],
    pub round: u64,
}
impl Metrics {
    pub const fn new() -> Self {
        Self {
            hist: Vec::new(),
            focus: 0,
            peer: 0,
            entry: 0,
            admitted: [0; 32],
            woken: [0; 32],
            round: 0,
        }
    }
    pub fn start(&mut self, pid: u64, peer: u64) {
        self.focus = pid;
        self.peer = peer;
        self.round = 0;
        self.hist = alloc::vec![Histogram{bins:[0;256],samples:0,max:0};7];
        ACTIVE.store(true, Ordering::Relaxed);
    }
    pub fn includes(&self, pid: Pid) -> bool {
        pid.number == self.focus || pid.number == self.peer
    }
    pub fn sample(&mut self, i: usize, start: u64, end: u64) {
        if start != 0 && !self.hist.is_empty() {
            self.hist[i].record(end - start);
        }
    }
    pub fn section(&mut self, begin: u64, end: u64, id: u64, entry: u64, r: Res<Option<u64>>) {
        if r != Ok(None) {
            self.sample(0, begin, end);
        }
        if r.is_ok() && id == IPC_REVOKE {
            self.sample(3, entry, end);
        }
    }
    pub fn report(&mut self, hz: u64) {
        ACTIVE.store(false, Ordering::Relaxed);
        for (i, h) in self.hist.iter().enumerate().filter(|(_, h)| h.samples > 0) {
            let ns = |v: u64| (u128::from(v) * 1_000_000_000 / u128::from(hz)) as u64;
            hal::log(format_args!(
                "IPC hist type={} n={} p50={} p95={} p99={} max={}",
                [
                    "irq",
                    "depot",
                    "livraison",
                    "revocation",
                    "aller_retour",
                    "reprise",
                    "appel"
                ][i],
                h.samples,
                ns(h.quantile(50)),
                ns(h.quantile(95)),
                ns(h.quantile(99)),
                ns(h.max)
            ));
        }
        self.hist = Vec::new();
    }
}
