//! Les deux travailleurs CPU n’utilisent ni cession, ni sommeil, ni blocage.
mod endurance;
use super::*;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};

static STOP: AtomicBool = AtomicBool::new(false);
static A: AtomicU64 = AtomicU64::new(0);
static B: AtomicU64 = AtomicU64::new(0);
static TOTAL: Mutex<u64> = Mutex::new(0);
static CYCLES: AtomicU64 = AtomicU64::new(0);

// Sûreté : symboles de sondes C de la HAL ; testent des motifs distincts à
// travers deux changements de tick, en préservant l’ABI de l’appelant.
unsafe extern "C" {
    fn nova_probe_a() -> usize;
    fn nova_probe_b() -> usize;
}
fn compute(probe: unsafe extern "C" fn() -> usize, count: &AtomicU64) {
    while !STOP.load(SeqCst) {
        // Sûreté : sonde non réentrante par pile, timer actif, IRQ autorisées.
        assert_eq!(unsafe { probe() }, 1, "contexte CPU corrompu");
        count.fetch_add(1, SeqCst);
    }
}
fn a() {
    compute(nova_probe_a, &A);
}
fn b() {
    compute(nova_probe_b, &B);
}
fn counter() {
    for n in 0..20u64 {
        let mut data = Vec::new();
        data.try_reserve_exact(32).unwrap();
        data.resize(32, n);
        {
            let mut lock = TOTAL.lock();
            let old = *lock;
            sleep(1);
            assert_eq!(*lock, old);
            assert!(data.iter().all(|&v| v == n));
            *lock = old + 1;
        }
        yield_now();
    }
}
fn wait_done(ids: &[Id]) {
    while ids.iter().copied().any(live) {
        sleep(1);
    }
    reap();
}
fn free_pages() -> u64 {
    crate::heap::with_pages(|m| Ok(m.free_pages())).unwrap()
}
fn transient() {
    sleep(2);
    CYCLES.fetch_add(1, SeqCst);
}

fn parallel(stress: bool) {
    let ids = [spawn(a).unwrap(), spawn(b).unwrap()];
    let before = with(|s| s.stats);
    let start = ticks();
    // Le contrôleur reste lui-même prêt pendant toute la première phase.
    // Sans IRQ matérielle, aucun des trois threads ne peut faire avancer la sonde.
    while ticks() - start < 100 {
        core::hint::spin_loop();
    }
    let after = with(|s| s.stats);
    assert_eq!(after.yields, before.yields);
    assert!(
        A.load(SeqCst) > 5 && B.load(SeqCst) > 5 && after.preemptions - before.preemptions >= 80
    );
    hal::log(format_args!(
        "PREEMPT ticks={} switches={} A={} B={} yields=0",
        ticks() - start,
        after.preemptions - before.preemptions,
        A.load(SeqCst),
        B.load(SeqCst)
    ));
    if stress {
        let mut round = 0;
        while ticks() - start < u64::from(HZ) * 60 {
            let child = spawn(transient).unwrap();
            sleep(u64::from(HZ));
            assert!(!live(child));
            round += 1;
        }
        assert_eq!(CYCLES.load(SeqCst), round);
        let switches = with(|s| s.stats.preemptions) - before.preemptions;
        assert!(switches >= 14000 && A.load(SeqCst) >= 3000 && B.load(SeqCst) >= 3000);
        hal::log(format_args!(
            "STRESS ticks={} switches={switches} A={} B={} cycles={round}",
            ticks() - start,
            A.load(SeqCst),
            B.load(SeqCst)
        ));
    }
    STOP.store(true, SeqCst);
    wait_done(&ids);
}

fn critical() {
    let before;
    {
        let guard = Guard::new();
        assert!(guard.was_enabled());
        before = ticks();
        assert!(matches!(spawn(transient), Err(Error::Critical)));
        {
            let nested = Guard::new();
            assert!(!nested.was_enabled());
        }
        while !hal::timer::pending() {
            core::hint::spin_loop();
        }
        assert_eq!(ticks(), before);
    }
    while ticks() == before {
        core::hint::spin_loop();
    }
    let inside = crate::heap::with_pages(|_| {
        // Vérifie le masque local sans inverser l’ordre scheduler → allocateur.
        let nested = hal::interrupts::Guard::new();
        assert!(!nested.was_enabled());
        let now = ticks();
        while !hal::timer::pending() {
            core::hint::spin_loop();
        }
        assert_eq!(ticks(), now);
        Ok(now)
    })
    .unwrap();
    while ticks() == inside {
        core::hint::spin_loop();
    }
    hal::log(format_args!("IRQ timer pendante masquee puis livree OK"));
    hal::log(format_args!("BUDDY IRQ masquee sous verrou puis livree OK"));
}

static RELEASE: AtomicBool = AtomicBool::new(false);
fn gated() {
    while !RELEASE.load(SeqCst) {
        sleep(1);
    }
}
fn limit() {
    let before = free_pages();
    assert!(matches!(
        spawn_on(gated, 1 << crate::smp::count()),
        Err(Error::UnsupportedCpu)
    ));
    let mut ids = [Id {
        slot: NONE,
        generation: 0,
    }; MAX - 2];
    let ids = &mut ids[..MAX - 1 - crate::smp::count()];
    for id in ids.iter_mut() {
        *id = spawn(gated).expect("slot libre");
    }
    assert!(matches!(spawn(gated), Err(Error::Full)));
    RELEASE.store(true, SeqCst);
    wait_done(&ids);
    let next = spawn(transient).unwrap();
    assert!(ids.iter().all(|&id| !live(id)));
    wait_done(&[next]);
    assert_eq!(before, free_pages());
    hal::log(format_args!(
        "TCB saturation, generations et affinite CPU0 OK"
    ));
}

// Le propriétaire réessaie immédiatement après avoir libéré. Les deux
// dormeurs doivent passer dans leur ordre d’inscription, avant sa reprise.
static ORDER: Mutex<u8> = Mutex::new(0);

fn waiter<const EXPECTED: u8>() {
    let mut turn = ORDER.lock();
    assert_eq!(*turn, EXPECTED);
    *turn += 1;
}

fn handoff() {
    let lock = ORDER.lock();
    let mut ids = [with(|s| s.thread(s.current()).id); 2];
    for (slot, entry) in ids.iter_mut().zip([waiter::<0> as fn(), waiter::<1>]) {
        *slot = spawn(entry).unwrap();
        let deadline = ticks() + u64::from(HZ);
        while !with(|s| matches!(s.thread(slot.slot).state, State::Blocked(Wait::Event(_)))) {
            assert!(ticks() < deadline, "inscription mutex");
            yield_now();
        }
    }
    drop(lock);
    assert_eq!(*ORDER.lock(), 2);
    wait_done(&ids);
    hal::log(format_args!("MUTEX transfert FIFO sans depassement OK"));
}

fn lifecycle() {
    let now = ticks();
    let idle = with(|s| s.stats.idle_ticks);
    sleep(10);
    assert!(ticks() - now >= 10 && with(|s| s.stats.idle_ticks) > idle);
    let ids = [spawn(counter).unwrap(), spawn(counter).unwrap()];
    wait_done(&ids);
    assert_eq!(*TOTAL.lock(), 40);
    let stats = with(|s| s.stats);
    assert!(stats.event_waits > 0 && stats.yields >= 40 && stats.wakes >= 40 && stats.reaped >= 4);
    hal::log(format_args!(
        "THREADS compteur=40 attentes={} yields={} reveils={} collectes={} idle={}",
        stats.event_waits, stats.yields, stats.wakes, stats.reaped, stats.idle_ticks
    ));
    let before = free_pages();
    for _ in 0..48 {
        let id = spawn(transient).unwrap();
        wait_done(&[id]);
        assert!(!live(id));
    }
    assert_eq!(before, free_pages());
    hal::log(format_args!("PILES 48 creations et restitutions OK"));
}

fn overflow() {
    let low = with(|s| s.thread(s.current()).stack.low);
    hal::log(format_args!("SONDE adresse={:#x} acces=ecriture", low - 8));
    hal::interrupts::mask();
    // Sûreté : VM de test. L’écriture traverse réellement la garde avec SP en limite.
    unsafe {
        context::overflow(low);
    }
}
fn fpu() {
    // Sûreté : instruction volontairement interdite dans un thread sacrifiable.
    unsafe {
        context::forbidden_fp();
    }
    panic!("FPU accessible");
}
pub(crate) fn test(scenario: &[u8]) {
    match scenario {
        b"sched-endurance" => endurance::run(),
        b"sched-test" => {
            critical();
            parallel(false);
            lifecycle();
            limit();
            handoff();
        }
        b"sched-stress" => parallel(true),
        b"sched-guard" | b"sched-fpu" => {
            spawn(if scenario == b"sched-guard" {
                overflow
            } else {
                fpu
            })
            .unwrap();
            sleep(10);
            panic!("sonde terminale revenue");
        }
        _ => panic!("sonde ordonnanceur inconnue"),
    }
    hal::log(format_args!("ORDONNANCEUR validation OK"));
}
