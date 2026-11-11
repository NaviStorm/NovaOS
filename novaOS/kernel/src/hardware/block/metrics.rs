//! Mesure facultative en VM de qualification. Les IRQ ne prennent aucun verrou.
use crate::{hal, sched::sync::Mutex};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STATS: Mutex<Stats> = Mutex::new(Stats {
    rows: Vec::new(),
    batch: 0,
    begin: 0,
    hz: 1,
});
struct Row {
    bins: [u32; 256],
    n: u64,
    top: u64,
    sum: u64,
}
struct Stats {
    rows: Vec<Row>,
    batch: u32,
    begin: u64,
    hz: u64,
}
pub(crate) fn start(hz: u64) {
    let mut s = STATS.lock();
    s.hz = hz;
    s.batch = 0;
    s.begin = 0;
    for _ in 0..6 {
        s.rows.push(Row {
            bins: [0; 256],
            n: 0,
            top: 0,
            sum: 0,
        })
    }
    ACTIVE.store(true, Ordering::Release)
}
pub(super) fn stamp() -> u64 {
    if ACTIVE.load(Ordering::Acquire) {
        hal::timer::counter()
    } else {
        0
    }
}
pub(super) fn record(idx: usize, begin: u64) {
    if begin == 0 {
        return;
    }
    let d = hal::timer::counter().wrapping_sub(begin).max(1);
    let p = d.ilog2() as usize;
    let n = (p * 8 + (((d - (1 << p)) as u128 * 8) >> p) as usize).min(255);
    let mut s = STATS.lock();
    let r = &mut s.rows[idx];
    r.bins[n] += 1;
    r.n += 1;
    r.top = r.top.max(d);
    r.sum = r.sum.saturating_add(d);
}
fn ns(c: u64, hz: u64) -> u64 {
    (u128::from(c) * 1_000_000_000 / u128::from(hz)) as u64
}
pub(super) fn begin() {
    if !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let mut s = STATS.lock();
    if s.batch % 32 == 0 && s.begin == 0 {
        s.begin = hal::timer::counter()
    }
}
pub(super) fn end(size: usize) {
    if !ACTIVE.load(Ordering::Acquire) {
        return;
    }
    let mut s = STATS.lock();
    s.batch += 1;
    if s.batch % 32 == 0 {
        let d = ns(hal::timer::counter() - s.begin, s.hz);
        let k = s.batch / 32 - 1;
        s.begin = 0;
        hal::log(format_args!(
            "BLOC parcours i={k} octets={} ns={d}",
            32 * size
        ));
    }
}
pub(crate) fn report() {
    ACTIVE.store(false, Ordering::Release);
    let mut s = STATS.lock();
    for (name, r) in [
        "read",
        "write",
        "flush",
        "validation",
        "copie_read",
        "copie_write",
    ]
    .iter()
    .zip(&s.rows)
    {
        if r.n == 0 {
            continue;
        }
        let quant = |q: u64| {
            let mut total = 0u64;
            for (i, &count) in r.bins.iter().enumerate() {
                total += u64::from(count);
                if total * 100 >= r.n * q {
                    return ns(
                        if i == 255 {
                            r.top
                        } else {
                            ((1u64 << (i / 8)) * (9 + (i % 8) as u64))
                                .div_ceil(8)
                                .min(r.top)
                        },
                        s.hz,
                    );
                }
            }
            0
        };
        hal::log(format_args!(
            "BLOC mesure type={name} n={} p50={} p95={} p99={} max={} moyenne={}",
            r.n,
            quant(50),
            quant(95),
            quant(99),
            ns(r.top, s.hz),
            ns(r.sum / r.n, s.hz)
        ));
    }
    s.rows = Vec::new();
}
