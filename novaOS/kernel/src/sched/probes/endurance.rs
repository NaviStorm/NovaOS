//! Chaque période repart de deux threads et revient au même bilan physique.
//! Seul le banc hôte fixe la durée de la campagne avec son horloge monotone.
use super::*;
use crate::heap;

const WORKERS: usize = 3;
const BATCH: usize = 12;
static PROGRESS: [AtomicU64; WORKERS] = [const { AtomicU64::new(0) }; WORKERS];
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DRAINS: AtomicU64 = AtomicU64::new(0);
static MUTATIONS: Mutex<u64> = Mutex::new(0);

fn fragment<const N: usize>() {
    let mut slots: [Vec<u8>; 16] = core::array::from_fn(|_| Vec::new());
    let mut round = 0usize;
    while !STOP.load(SeqCst) {
        // Parcours impair de tous les slots. Les voisins ont des durées de vie
        // différentes et les identités des charges diffèrent entre travailleurs.
        for step in 0..slots.len() {
            let index = (round + step * 7) % slots.len();
            let bytes = &mut slots[index];
            let pattern = (N * 32 + index + 1) as u8;
            assert!(bytes.iter().all(|&b| b == pattern));
            if (index + round) % 4 == 0 {
                *bytes = Vec::new();
            } else {
                let size = 1 + (index * 251 + round * 37 + N * 509) % 4096;
                if size > bytes.capacity() {
                    bytes.try_reserve_exact(size - bytes.len()).unwrap();
                    ALLOCATIONS.fetch_add(1, SeqCst);
                    assert!(bytes.iter().all(|&b| b == pattern));
                }
                bytes.resize(size, pattern);
            }
        }
        {
            let mut value = MUTATIONS.lock();
            let old = *value;
            sleep(1);
            assert_eq!(*value, old);
            *value += 1;
        }
        PROGRESS[N].fetch_add(1, SeqCst);
        round += 1;
    }
    for (i, bytes) in slots.iter().enumerate() {
        assert!(bytes.iter().all(|&b| b == (N * 32 + i + 1) as u8));
    }
}

fn progress() -> [u64; WORKERS] {
    core::array::from_fn(|i| PROGRESS[i].load(SeqCst))
}

pub(super) fn run() -> ! {
    let baseline = heap::audit(true);
    assert_eq!(with(|s| s.audit()), 1 + crate::smp::count());
    hal::log(format_args!(
        "ENDURANCE debut blocs={} occupes={} engages={} libres={}",
        baseline.blocks, baseline.occupied, baseline.committed, baseline.free_pages
    ));
    let mut children = 0;
    let mut checks = 0;
    let mut low = baseline.free_pages;
    for epoch in 1u64.. {
        let before = with(|s| s.stats);
        let start_workers = progress();
        let old_a = A.load(SeqCst);
        let old_b = B.load(SeqCst);
        STOP.store(false, SeqCst);
        let cpu = [spawn(a).unwrap(), spawn(b).unwrap()];
        if (epoch - 1) % 64 == 0 {
            heap::shortage();
            DRAINS.fetch_add(1, SeqCst);
        }
        let entries = [fragment::<0> as fn(), fragment::<1>, fragment::<2>];
        let workers: [Id; WORKERS] = core::array::from_fn(|i| {
            let cpu = (i + 1) % crate::smp::count();
            spawn_on(entries[i], 1 << cpu).unwrap()
        });
        let start = ticks();
        let mut last_check = start;
        let mut last_progress = start_workers;
        loop {
            let ids: [Id; BATCH] = core::array::from_fn(|_| spawn(transient).unwrap());
            wait_done(&ids);
            assert!(ids.iter().all(|&id| !live(id)));
            children += BATCH as u64;
            assert_eq!(CYCLES.load(SeqCst), children);
            let now = ticks();
            if now - last_check >= u64::from(HZ) {
                assert_eq!(
                    with(|s| s.audit()),
                    1 + crate::smp::count() + cpu.len() + workers.len()
                );
                let usage = heap::audit(false);
                assert!(usage.blocks > baseline.blocks && usage.free_pages < baseline.free_pages);
                low = low.min(usage.free_pages);
                let current = progress();
                assert!(
                    current.iter().zip(last_progress).all(|(&n, old)| n > old),
                    "travailleur bloqué"
                );
                last_progress = current;
                last_check = now;
                checks += 1;
            }
            if now - start >= u64::from(HZ) * 5 {
                break;
            }
        }
        STOP.store(true, SeqCst);
        wait_done(&workers);
        wait_done(&cpu);
        let stats = with(|s| {
            assert_eq!(s.audit(), 1 + crate::smp::count());
            s.stats
        });
        assert_eq!(stats.exits, stats.reaped);
        assert_eq!(stats.reaped, children + epoch * (WORKERS as u64 + 2));
        assert!(stats.event_waits > before.event_waits && stats.preemptions > before.preemptions);
        let rounds = progress().iter().sum::<u64>();
        assert_eq!(*MUTATIONS.lock(), rounds);
        assert!(A.load(SeqCst) > old_a && B.load(SeqCst) > old_b);
        let end = heap::audit(true);
        assert_eq!(baseline, end, "fuite de ressources");
        hal::log(format_args!(
            "ENDURANCE bilan periode={epoch} ticks={} preemptions={} collectes={} attentes={} mutex={rounds} A={} B={} allocations={} audits={checks} penuries={} minimum={} blocs={} occupes={} engages={} libres={}",
            ticks(),
            stats.preemptions,
            stats.reaped,
            stats.event_waits,
            A.load(SeqCst),
            B.load(SeqCst),
            ALLOCATIONS.load(SeqCst),
            DRAINS.load(SeqCst),
            low,
            end.blocks,
            end.occupied,
            end.committed,
            end.free_pages
        ));
    }
    unreachable!()
}
