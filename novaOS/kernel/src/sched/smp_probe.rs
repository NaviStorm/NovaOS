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
    let freq = hal::timer::counter_hz();
    profile::ACTIVE.store(true, Ordering::Release);
    hal::log(format_args!("SMP frequence={freq}"));
    process::smp_users();
    compute(n, freq);
    process::ipc::smp_roundtrip(freq);
    let preemptions = with(|s| s.stats.preemptions);
    hal::log(format_args!("SMP preemptions={preemptions}"));
    if n > 1 {
        remote_translations();
    }
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
    profile::report();
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

// Les deux cadres restent possédés pendant la permutation. Le deuxième motif
// préexiste au changement de traduction, aucune écriture à VA identique ne
// peut satisfaire la vérification si un secondaire conserve l'ancien cadre.
static READ_OLD: AtomicU64 = AtomicU64::new(0);
static REPUBLISHED: AtomicBool = AtomicBool::new(false);
const FIRST: usize = 0xffff_f000_0000_0000;
const SECOND: usize = FIRST + 4096;
const OLD: u64 = 0x534d_502d_6f6c_6401;
const NEW: u64 = 0x534d_502d_6e65_7702;

fn read_mapping() {
    // Sûreté : l'AP lit uniquement dans les deux fenêtres publiées par le BSP.
    assert_eq!(unsafe { (FIRST as *const u64).read_volatile() }, OLD);
    READ_OLD.fetch_or(1 << smp::index(), Ordering::Release);
    while !REPUBLISHED.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    assert_eq!(unsafe { (FIRST as *const u64).read_volatile() }, NEW);
}

fn remote_translations() {
    use crate::hal::memory::{Access::ReadWrite, VirtualAddress, VirtualMemory};
    use crate::heap;
    let a = VirtualAddress(FIRST);
    let b = VirtualAddress(SECOND);
    heap::with_pages(|m| {
        m.allocate_page(a, ReadWrite)?;
        m.allocate_page(b, ReadWrite)
    })
    .expect("deux pages de sonde");
    // Sûreté : les deux pages viennent d'être acquises et aucun AP ne les lit.
    unsafe {
        (FIRST as *mut u64).write_volatile(OLD);
        (SECOND as *mut u64).write_volatile(NEW);
    }
    let readers: Vec<_> = (1..smp::count())
        .map(|cpu| spawn_on(read_mapping, 1 << cpu).unwrap())
        .collect();
    let mask = smp::ONLINE.load(Ordering::Acquire) & !1;
    let start = ticks();
    while READ_OLD.load(Ordering::Acquire) != mask {
        assert!(ticks() - start < 5 * u64::from(HZ), "lecteur TLB absent");
        yield_now();
    }
    heap::with_pages(|m| {
        // Sûreté : les AP ont cessé la première lecture et attendent REPUBLISHED.
        // Les cadres restent distincts et leur restitution suit la fin des lecteurs.
        unsafe {
            let old = m.unmap(a)?;
            let new = m.unmap(b)?;
            assert_ne!(old, new);
            m.map(a, new, ReadWrite)?;
            m.map(b, old, ReadWrite)
        }
    })
    .expect("permutation de cadres");
    REPUBLISHED.store(true, Ordering::Release);
    while readers.iter().copied().any(live) {
        assert!(
            ticks() - start < 10 * u64::from(HZ),
            "relecture TLB absente"
        );
        yield_now();
    }
    heap::with_pages(|m| unsafe {
        m.release_page(a)?;
        m.release_page(b)
    })
    .expect("restitution de la sonde");
    hal::log(format_args!(
        "SMP traductions distantes masque={mask:#x} OK"
    ));
}

// Charge entière fixe distribuée sur huit threads, indépendante de la vitesse du timer.
static COMPUTE_GO: AtomicBool = AtomicBool::new(false);
static COMPLETED: AtomicUsize = AtomicUsize::new(0);
fn calculate() {
    while !COMPUTE_GO.load(Ordering::Acquire) {
        yield_now();
    }
    let mut x = 123u64;
    for _ in 0..2_000_000 {
        x = core::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
    }
    assert_eq!(x, 0x4b49cfed50c817fb);
    COMPLETED.fetch_add(1, Ordering::Release);
}
fn compute(n: usize, freq: u64) {
    for i in 0..8 {
        spawn_on(calculate, 1 << (i % n)).unwrap();
    }
    let start = hal::timer::counter();
    COMPUTE_GO.store(true, Ordering::Release);
    while COMPLETED.load(Ordering::Acquire) != 8 {
        sleep(1);
    }
    let elapsed = hal::timer::counter() - start;
    hal::log(format_args!(
        "SMP calcul iterations=16000000 us={}",
        elapsed.saturating_mul(1_000_000) / freq
    ));
}

pub(crate) fn shutdown_test(scenario: &[u8]) -> ! {
    static MASKED: AtomicBool = AtomicBool::new(false);
    fn unresponsive() {
        hal::interrupts::mask();
        MASKED.store(true, Ordering::Release);
        loop {
            core::hint::spin_loop();
        }
    }
    if scenario == b"smp-panique" {
        if smp::count() == 1 {
            panic!("sonde de panique");
        }
        spawn_on(|| panic!("sonde de panique"), 2).unwrap();
        loop {
            yield_now();
        }
    }
    if scenario == b"smp-cpu-muet" {
        assert!(smp::count() > 1);
        spawn_on(unresponsive, 2).unwrap();
        while !MASKED.load(Ordering::Acquire) {
            yield_now();
        }
    }
    hal::cpu::halt()
}
