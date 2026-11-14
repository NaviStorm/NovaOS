//! Histogrammes du banc, alloués seulement dans la VM de mesure. Huit classes
//! par octave, quantiles rendus par la borne supérieure, maximum exact.
use super::*;
use core::sync::atomic::AtomicBool;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static SWITCHES: AtomicBool = AtomicBool::new(false);
// Un marqueur par CPU, IRQ masquées. Le marqueur est transmis entre les deux piles noyau.
static HANDOFF: [AtomicU64; crate::smp::MAX_CPUS] =
    [const { AtomicU64::new(0) }; crate::smp::MAX_CPUS];
pub(crate) fn switch_stamp() -> u64 {
    if SWITCHES.load(Ordering::Relaxed) {
        hal::timer::counter()
    } else {
        0
    }
}
pub(crate) fn switch_out(start: u64, pair: bool) {
    HANDOFF[crate::smp::index()].store(if pair { start } else { 0 }, Ordering::Relaxed);
}
pub(crate) fn switch_in() {
    let end = switch_stamp();
    if end != 0 {
        let start = HANDOFF[crate::smp::index()].swap(0, Ordering::Relaxed);
        if start != 0 {
            with(|s| s.ipc.metrics.sample(7, start, end));
        }
    }
}
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
    window: Option<(u64, u64)>,
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
            window: None,
        }
    }
    pub fn start(&mut self, pid: u64, peer: u64) {
        self.start_series(pid, peer, 7);
    }
    fn start_series(&mut self, pid: u64, peer: u64, classes: usize) {
        self.focus = pid;
        self.peer = peer;
        self.round = 0;
        self.hist = alloc::vec![Histogram{bins:[0;256],samples:0,max:0};classes];
        ACTIVE.store(true, Ordering::Relaxed);
    }
    pub fn start_session(&mut self, client: u64, service: u64) {
        self.start_series(client, service, 8);
        self.window = None;
        for value in &HANDOFF {
            value.store(0, Ordering::Relaxed);
        }
        SWITCHES.store(true, Ordering::Relaxed);
    }
    pub fn includes(&self, pid: Pid) -> bool {
        pid.number == self.focus || pid.number == self.peer
    }
    pub fn sample(&mut self, i: usize, start: u64, end: u64) {
        if start != 0 && !self.hist.is_empty() {
            self.hist[i].record(end - start);
            if i == 4 && SWITCHES.load(Ordering::Relaxed) {
                let window = self.window.get_or_insert((start, end));
                window.1 = end;
            }
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
        SWITCHES.store(false, Ordering::Relaxed);
        if let Some((start, end)) = self.window.take() {
            let ns = u128::from(end - start) * 1_000_000_000 / u128::from(hz);
            hal::log(format_args!(
                "STOCKAGE trafic tours={} ns={ns}",
                self.hist[4].samples
            ));
        }
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
                    "appel",
                    "commutation"
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
