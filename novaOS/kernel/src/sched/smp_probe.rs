//! Chaque travailleur doit observer les autres dans le même rendez-vous,
// même lorsque les tâches ne cèdent pas volontairement la main.
use super::*;
use crate::smp;
use core::sync::atomic::{AtomicBool, AtomicUsize};

static ARRIVED: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicU64 = AtomicU64::new(0);
static OBSERVED: [AtomicUsize; smp::MAX_CPUS] = [const { AtomicUsize::new(0) }; smp::MAX_CPUS];
static PULSES: [AtomicU64; smp::MAX_CPUS] = [const { AtomicU64::new(0) }; smp::MAX_CPUS];
static RELEASE: AtomicBool = AtomicBool::new(false);
pub(super) fn pulse() {
    PULSES[smp::index()].fetch_add(1, Ordering::Relaxed);
}
fn worker() {
    let cpu = smp::index();
    let bit = 1 << cpu;
    let goal = (1 << smp::count()) - 1;
    let first = PULSES[cpu].load(Ordering::Relaxed);
    assert_eq!(ARRIVED.fetch_or(bit, Ordering::AcqRel) & bit, 0);
    while ARRIVED.load(Ordering::Acquire) != goal || PULSES[cpu].load(Ordering::Relaxed) - first < 4
    {
        core::hint::spin_loop();
    }
    OBSERVED[cpu].store(hal::cpu::id(), Ordering::Release);
    DONE.fetch_or(bit, Ordering::Release);
    while !RELEASE.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
}

pub(crate) fn run() -> ! {
    let n = smp::count();
    let memory = crate::heap::audit(true);
    hal::log(format_args!(
        "SMP memoire cpu={n} libres={} tas={}",
        memory.free_pages, memory.occupied
    ));
    process::smp_users();
    let mut tasks = Vec::new();
    for cpu in 0..n {
        tasks.push(spawn_on(worker, 1 << cpu).expect("travailleur SMP"));
    }
    let goal = (1 << n) - 1;
    let start = ticks();
    while DONE.load(Ordering::Acquire) != goal {
        assert!(
            ticks() - start < 5 * u64::from(HZ),
            "rendez-vous SMP expiré"
        );
        yield_now();
    }
    RELEASE.store(true, Ordering::Release);
    while tasks.iter().copied().any(live) {
        yield_now();
    }
    for cpu in 0..n {
        hal::log(format_args!(
            "SMP cpu={cpu} materiel={} ticks={}",
            OBSERVED[cpu].load(Ordering::Acquire),
            PULSES[cpu].load(Ordering::Relaxed)
        ));
    }
    if n > 1 {
        let from = smp::index();
        let migrator = spawn_on(migrate, 1 << from).unwrap();
        while live(migrator) {
            yield_now();
        }
        while with(|s| s.stats.exits != s.stats.reaped) {
            yield_now();
        }
    }
    smp::request_park();
    while smp::ONLINE.load(Ordering::Acquire) != 1 {
        assert!(
            ticks() - start < u64::from(HZ) * 10,
            "arrêt secondaire expiré"
        );
        yield_now();
    }
    hal::log(format_args!("SMP stationnes={} en_ligne=1", n - 1));
    hal::log(format_args!("SMP rendezvous={goal:#x} actifs={n} OK"));
    hal::cpu::halt()
}

fn migrate() {
    let from = smp::index();
    let to = (from + 1) % smp::count();
    super::set_affinity(1 << to).unwrap();
    assert_eq!(smp::index(), to);
    hal::log(format_args!("SMP migration={from}->{to}"));
}
