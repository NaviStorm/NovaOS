// Instrumentation de course, absente du candidat chronométré. Le verrou et les copies
// sont ceux des appels ordinaires. Les rendez-vous ne transportent aucun octet utilisateur.
use super::*;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::SeqCst};
static ARMED: AtomicBool = AtomicBool::new(false);
static STAGE: AtomicUsize = AtomicUsize::new(0);
static CLOSE_KEY: AtomicU64 = AtomicU64::new(0);
static COPY_KEY: AtomicU64 = AtomicU64::new(0);
static CLOSE_OP: AtomicU64 = AtomicU64::new(0);
static COPY_OP: AtomicU64 = AtomicU64::new(0);
static ADDRESS: AtomicU64 = AtomicU64::new(0);
static HZ_COUNTER: AtomicU64 = AtomicU64::new(0);
static CLOSE_CPU: AtomicUsize = AtomicUsize::new(usize::MAX);
static COPY_CPU: AtomicUsize = AtomicUsize::new(usize::MAX);

fn wait(wanted: usize) {
    let start = hal::timer::counter();
    while STAGE.load(SeqCst) < wanted {
        assert!(
            hal::timer::counter() - start < 5 * HZ_COUNTER.load(SeqCst),
            "délai du rendez-vous de copie"
        );
        core::hint::spin_loop();
    }
}
pub(crate) fn entry(op: u64, key: u64) {
    if !ARMED.load(SeqCst) {
        return;
    }
    if op == CLOSE_OP.load(SeqCst) && key == CLOSE_KEY.load(SeqCst) {
        CLOSE_CPU.store(crate::smp::index(), SeqCst);
        STAGE.store(1, SeqCst);
        wait(2);
        STAGE.store(3, SeqCst);
    } else if op == COPY_OP.load(SeqCst) && key == COPY_KEY.load(SeqCst) {
        COPY_CPU.store(crate::smp::index(), SeqCst);
        wait(1);
    }
}
pub(crate) fn contended(cpu: usize, owner: usize) {
    if ARMED.load(SeqCst)
        && STAGE.load(SeqCst) == 3
        && cpu == CLOSE_CPU.load(SeqCst)
        && owner == COPY_CPU.load(SeqCst) + 1
    {
        STAGE.store(4, SeqCst);
    }
}
pub(crate) fn segment(to_user: bool, va: u64, len: usize, done: usize) {
    if !ARMED.load(SeqCst)
        || va != ADDRESS.load(SeqCst)
        || len != 4096
        || crate::smp::index() != COPY_CPU.load(SeqCst)
        || to_user != (COPY_OP.load(SeqCst) == IPC_TAKE)
    {
        return;
    }
    if done == 2048 {
        assert_eq!(STAGE.swap(2, SeqCst), 1);
        wait(4); // Le CPU de la fermeture a réellement buté sur le verrou de cette copie.
    } else {
        assert_eq!(done, 4096);
        assert_eq!(STAGE.swap(5, SeqCst), 4);
    }
}
fn describe(p: Actor, addr: u64) {
    for (i, v) in [1 | (32 << 32), 71, addr, 4096].into_iter().enumerate() {
        perf::word(p, 32 + i, v);
    }
}
fn one(op: u64, close: u64) {
    let base = heap::audit(false).free_pages;
    let (children, [a, b]) = group([ALL, RECEIVE]);
    let key = a.boot();
    let leaf = b.boot();
    with(|s| s.ipc.bind_lane(a.0, key, b.0, leaf)).unwrap();
    let mut addr = 0;
    for p in [a, b] {
        let h = p.call(HEAP_RESIZE, 0, 0, 0).unwrap();
        assert_eq!(p.call(HEAP_RESIZE, h, h + 8192, 0), Ok(h + 8192));
        addr = h + 2048;
        describe(p, addr);
    }
    let mut source = [0u8; 4096];
    for (i, v) in source.iter_mut().enumerate() {
        *v = (i * 31 + 1) as u8;
    }
    a.put(addr, &source);
    b.put(addr, &[0x75; 4096]);
    let (copier, k, closer, other) = if op == IPC_TAKE {
        assert_eq!(a.call(IPC_PUT, key, DESC, 32), Ok(4096));
        (b, leaf, a, key)
    } else {
        (a, key, b, leaf)
    };
    HZ_COUNTER.store(hal::timer::counter_hz(), SeqCst);
    CLOSE_KEY.store(other, SeqCst);
    COPY_KEY.store(k, SeqCst);
    CLOSE_OP.store(close, SeqCst);
    COPY_OP.store(op, SeqCst);
    ADDRESS.store(addr, SeqCst);
    STAGE.store(0, SeqCst);
    COPY_CPU.store(usize::MAX, SeqCst);
    CLOSE_CPU.store(usize::MAX, SeqCst);
    // Les deux commandes sont visibles avant de laisser un acteur entrer dans sa porte.
    with(|s| {
        for (p, cpu, args) in [
            (closer, 1, [close, other, 0, 0]),
            (copier, 0, [op, k, DESC, 32]),
        ] {
            let t = s
                .slots
                .iter_mut()
                .flatten()
                .find(|t| t.owner == Some(p.0))
                .unwrap();
            t.cpu_mask = 1 << cpu;
            let space = s.processes.get(p.0).space.as_ref().unwrap();
            heap::with_pages(|m| {
                for i in 1..4 {
                    space.write_data(m, i, args[i]);
                }
                space.write_data(m, 0, args[0]);
                Ok(())
            })
            .unwrap();
        }
        ARMED.store(true, SeqCst);
    });
    assert_eq!(copier.result(), Ok(4096));
    assert!(closer.result().is_ok());
    ARMED.store(false, SeqCst);
    assert_eq!(STAGE.load(SeqCst), 5);
    assert_ne!(COPY_CPU.load(SeqCst), CLOSE_CPU.load(SeqCst));
    if op == IPC_TAKE {
        assert_eq!(b.bytes::<4096>(addr), source);
    } else {
        assert_eq!(b.bytes::<4096>(addr), [0x75; 4096]);
    }
    assert_eq!(a.call(IPC_PUT, key, DESC, 32), Err(DENIED));
    assert_eq!(b.call(IPC_TAKE, leaf, DESC, 32), Err(DENIED));
    stop(children, [a, b]);
    assert_eq!(heap::audit(false).free_pages, base);
    hal::log(format_args!(
        "ETH2B course copie={op} retrait={close} cpu_copie={} cpu_retrait={} premiere=2048 contention=1 fin=4096 anciens_refuses=2 solde=0",
        COPY_CPU.load(SeqCst),
        CLOSE_CPU.load(SeqCst)
    ));
}
pub(super) fn run() {
    assert_eq!(crate::smp::count(), 2);
    one(IPC_TAKE, IPC_CLOSE);
    one(IPC_TAKE, IPC_REVOKE);
    one(IPC_PUT, IPC_CLOSE);
    hal::log(format_args!("ETH2B campagne OK"));
}
