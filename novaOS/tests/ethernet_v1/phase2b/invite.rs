use super::*;
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering::Relaxed};
const ACTOR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ethernet_2b.bin"));
const OP: u64 = u64::MAX - 6;
const N: usize = 10_000;
static SAMPLES: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static FOCUS: AtomicU64 = AtomicU64::new(0);
static COUNT: AtomicU64 = AtomicU64::new(0);
static START: AtomicU64 = AtomicU64::new(0);
static FIRST: AtomicU64 = AtomicU64::new(0);
static LAST: AtomicU64 = AtomicU64::new(0);
static SW0: AtomicU64 = AtomicU64::new(0);
static SW1: AtomicU64 = AtomicU64::new(0);
static WK0: AtomicU64 = AtomicU64::new(0);
static WK1: AtomicU64 = AtomicU64::new(0);

pub(in crate::sched::process::ipc) fn start_sample(pid: u64, id: u64, address: u64, wakes: u64) {
    if id == IPC_SEND && address == user::DATA + 1024 && FOCUS.load(Relaxed) == pid {
        let now = hal::timer::counter();
        assert_eq!(START.swap(now, Relaxed), 0);
        if COUNT.load(Relaxed) == 0 {
            FIRST.store(now, Relaxed);
            SW0.store(crate::sched::profile::switches_bsp(), Relaxed);
            WK0.store(wakes, Relaxed);
        }
    }
}
pub(in crate::sched::process::ipc) fn end_sample(pid: u64, id: u64, address: u64, wakes: u64) {
    if id == IPC_RECEIVE && address == user::DATA + 1280 && FOCUS.load(Relaxed) == pid {
        let end = hal::timer::counter();
        let begin = START.swap(0, Relaxed);
        assert!(begin != 0 && end > begin);
        let index = COUNT.fetch_add(1, Relaxed) as usize;
        SAMPLES[index].store(end - begin, Relaxed);
        LAST.store(end, Relaxed);
        SW1.store(crate::sched::profile::switches_bsp(), Relaxed);
        WK1.store(wakes, Relaxed);
    }
}
static MED_SRC: AtomicU64 = AtomicU64::new(0);
static MED_DST: AtomicU64 = AtomicU64::new(0);
static MED_COUNT: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static MED_TICKS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
pub(crate) fn mediation_measured() -> bool {
    MED_SRC.load(Relaxed) != 0
}
pub(crate) fn timed_transfer(op: u64, key: u64, desc: u64, size: u64) -> Res<u64> {
    let pid = with(|s| s.thread(s.current()).owner.unwrap().number);
    let tracked = pid == MED_SRC.load(Relaxed) || pid == MED_DST.load(Relaxed);
    let start = if tracked { hal::timer::counter() } else { 0 };
    let result = super::super::transfer(op, key, desc, size);
    if tracked {
        let elapsed = hal::timer::counter() - start;
        let idx = usize::from(op == IPC_TAKE) * 2 + usize::from(result.is_err());
        MED_COUNT[idx].fetch_add(1, Relaxed);
        MED_TICKS[idx].fetch_add(elapsed, Relaxed);
    }
    result
}
struct Values(usize, usize);
impl fmt::Display for Values {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for value in &SAMPLES[self.0..self.1] {
            write!(f, " {:x}", value.load(Relaxed))?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct Case {
    mode: usize,
    profile: usize,
    b: usize,
    len: usize,
}
impl Case {
    fn config(self) -> u64 {
        (self.mode | (self.profile << 2) | (self.b << 8) | (self.len << 16)) as u64
    }
    fn banks(self) -> usize {
        match self.mode {
            0 => 0,
            1 => 1,
            _ => (self.b * self.len).div_ceil(4096),
        }
    }
}
struct Pair {
    children: [Child; 2],
    a: Actor,
    b: Actor,
    tx: [u64; 3],
    rx: [u64; 3],
}
fn word(a: Actor, i: usize) -> u64 {
    a.memory(|s, m| s.read_data(m, i))
}
fn set(a: Actor, i: usize, v: u64) {
    perf::word(a, i, v);
}
fn setup(c: Case, epoch: u64) -> Pair {
    let children = launch([(ACTOR, ALL), (ACTOR, RECEIVE)]).unwrap();
    let (a, b) = (Actor(children[0].0), Actor(children[1].0));
    let mut tx = [0; 3];
    let mut rx = [0; 3];
    tx[0] = a.boot();
    rx[0] = b.boot();
    let reply = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let answer = contract::transfer(a, b, tx[0], rx[0], reply, SEND, 1);
    if c.profile == 2 {
        let gate = b.call(IPC_CREATE, 0, 0, 0).unwrap();
        // La délégation de montage est fermée avant de démarrer le trafic.
        let setup = contract::transfer(a, b, tx[0], rx[0], reply, SEND | GRANT, 1);
        let notify = contract::transfer(b, a, setup, reply, gate, SEND, 1);
        assert_eq!(b.call(IPC_CLOSE, setup, 0, 0), Ok(0));
        set(a, 22, notify);
        set(b, 22, gate);
    }
    let banks = c.banks();
    for i in 0..banks {
        if i > 0 {
            tx[i] = a.call(IPC_CREATE, 0, 0, 0).unwrap();
            rx[i] = contract::transfer(a, b, tx[0], rx[0], tx[i], RECEIVE, 1);
        }
        with(|s| s.ipc.bind_lane(a.0, tx[i], b.0, rx[i])).unwrap();
        assert_eq!(a.call(IPC_FEATURES, tx[i], SEND.into(), 1), Ok(1));
        assert_eq!(b.call(IPC_FEATURES, rx[i], RECEIVE.into(), 1), Ok(1));
    }
    for (actor, handles, back, areas) in [(a, tx, reply, 1), (b, rx, answer, 2)] {
        set(actor, 6, handles[0]);
        set(actor, 7, back);
        for (i, v) in handles.into_iter().enumerate() {
            set(actor, 8 + i, v);
        }
        let base = actor.call(HEAP_RESIZE, 0, 0, 0).unwrap();
        let top = base + ((c.b * c.len).next_multiple_of(4096) * areas) as u64;
        assert_eq!(actor.call(HEAP_RESIZE, base, top, 0), Ok(top));
        set(actor, 11, base);
        set(actor, 12, epoch);
    }
    Pair {
        children,
        a,
        b,
        tx,
        rx,
    }
}
impl Pair {
    fn start(&self, c: Case, n: usize, background: bool) {
        let role = if background { 2 } else { 0 };
        self.b.start(OP, role + 1, n as u64, c.config());
        self.a.start(OP, role, n as u64, c.config());
    }
    fn finish(&self, n: usize) {
        let start = ticks();
        while word(self.a, 0) != 0 || word(self.b, 0) != 0 {
            for actor in [self.a, self.b] {
                if word(actor, 0) == 0 {
                    assert_eq!(actor.result(), Ok(n as u64));
                }
            }
            assert!(ticks() - start < 250 * 600, "série sans fin");
            sleep(2);
        }
        assert_eq!(self.a.result(), Ok(n as u64));
        assert_eq!(self.b.result(), Ok(n as u64));
    }
    fn stop(self) {
        stop(self.children, [self.a, self.b]);
    }
}
fn resources() -> (usize, usize, usize, usize, u64) {
    let (p, e, ipc, lanes) = with(|s| {
        (
            s.processes.slots.iter().flatten().count(),
            s.ipc.ends.ids().count(),
            s.ipc.pages(),
            s.ipc.pages_bulk.ids().count(),
        )
    });
    (p, e, ipc, lanes, heap::audit(false).free_pages)
}
fn pages(actor: Actor) -> (usize, usize) {
    actor.memory(|s, _| s.page_counts())
}
fn cell(c: Case, n: usize) {
    let base = resources();
    crate::sched::process::measure("eth2b_base");
    let pair = setup(c, 51);
    let bg_case = Case {
        mode: 1,
        profile: 0,
        b: 1,
        len: 60,
    };
    let bg = (c.profile == 3).then(|| setup(bg_case, 97));
    let opened = resources();
    let private_a = pages(pair.a);
    let private_b = pages(pair.b);
    let (mut bg_pages, mut progress) = (0, 0);
    if let Some(q) = &bg {
        for p in [q.a, q.b] {
            let (d, s) = pages(p);
            bg_pages += d + s;
        }
        q.start(bg_case, 0, true);
        until(|| word(q.a, 14) > 1);
    }
    pair.start(c, 32, false);
    pair.finish(32);
    if let Some(q) = &bg {
        progress = word(q.a, 14);
    }
    // La réception bloquée avant le premier envoi matérialise une vraie attente.
    pair.b.start(OP, 1, n as u64, c.config());
    pair.b.blocked();
    crate::sched::process::measure("eth2b_attente");
    let wait_free = heap::audit(false).free_pages;
    let hz = hal::timer::counter_hz();
    hal::log(format_args!(
        "ETH2B debut profil={} mode={} b={} len={} n={n} hz={hz}",
        c.profile, c.mode, c.b, c.len
    ));
    COUNT.store(0, Relaxed);
    START.store(0, Relaxed);
    FOCUS.store(pair.a.0.number, Relaxed);
    pair.a.start(OP, 0, n as u64, c.config());
    pair.finish(n);
    FOCUS.store(0, Relaxed);
    assert_eq!(COUNT.load(Relaxed), n as u64);
    assert_eq!(START.load(Relaxed), 0);
    let bg_count = bg.as_ref().map_or(0, |q| word(q.a, 14) - progress);
    let cts = |i| word(pair.a, i) + word(pair.b, i);
    let (calls, pressure, retries, idle) = (cts(16), cts(17), cts(18), cts(19));
    if c.profile == 2 {
        assert!(pressure >= n as u64);
    }
    if c.mode != 0 {
        for i in 0..4 {
            MED_COUNT[i].store(0, Relaxed);
            MED_TICKS[i].store(0, Relaxed);
        }
        MED_DST.store(pair.b.0.number, Relaxed);
        MED_SRC.store(pair.a.0.number, Relaxed);
        pair.start(c, 128, false);
        pair.finish(128);
        MED_SRC.store(0, Relaxed);
        let success = (128 * (c.b * c.len).div_ceil(4096)) as u64;
        assert_eq!(MED_COUNT[0].load(Relaxed), success);
        assert_eq!(MED_COUNT[2].load(Relaxed), success);
        for i in 0..4 {
            hal::log(format_args!(
                "ETH2B mediation type={i} appels={} ticks={}",
                MED_COUNT[i].load(Relaxed),
                MED_TICKS[i].load(Relaxed)
            ));
        }
    }
    if let Some(q) = bg {
        assert!(bg_count > 0);
        set(q.a, 13, 1);
        let completed = q.a.result().unwrap();
        assert_eq!(q.b.result(), Ok(completed));
        q.stop();
    }
    let bytes = c.b * c.len;
    let copies = if c.mode == 0 {
        4 * c.b * c.len.div_ceil(96)
    } else {
        2 * bytes.div_ceil(4096)
    };
    let copied = bytes * if c.mode == 0 { 4 } else { 2 };
    let open_delta = base.4 - opened.4;
    let wait_delta = base.4 - wait_free;
    let retire_start = hal::timer::counter();
    pair.stop();
    let retire = hal::timer::counter() - retire_start;
    assert_eq!(base, resources(), "restitution des cadres et tables");
    hal::log(format_args!(
        "ETH2B resultat n={n} ticks={} appels={calls} pression={pressure} reprises={retries} pauses={idle} voisin={bg_count} copies={copies} octets={copied} switches={} wakes={} p={} ep={} ipc={} boites={} priva={} pilea={} privb={} pileb={} privvoisin={bg_pages} cadres_ouverts={open_delta} cadres_attente={wait_delta} libres_base={} retrait={retire} solde=0",
        LAST.load(Relaxed) - FIRST.load(Relaxed),
        SW1.load(Relaxed) - SW0.load(Relaxed),
        WK1.load(Relaxed) - WK0.load(Relaxed),
        opened.0 - base.0,
        opened.1 - base.1,
        opened.2 - base.2,
        opened.3 - base.3,
        private_a.0,
        private_a.1,
        private_b.0,
        private_b.1,
        base.4
    ));
    for from in (0..n).step_by(128) {
        hal::log(format_args!(
            "ETH2B serie {from}{}",
            Values(from, n.min(from + 128))
        ));
    }
}
fn security() {
    for fault in [4, 5, 6, 7, 8] {
        let base = resources();
        let candidate = Case {
            mode: 2,
            profile: fault,
            b: 8,
            len: 1514,
        };
        let neighbor = Case {
            mode: 1,
            profile: 0,
            b: 1,
            len: 60,
        };
        let victim = setup(candidate, 51);
        let other = setup(neighbor, 97);
        let descriptor = [1u64 | (32 << 32), 4, word(victim.b, 11), 60];
        for (i, v) in descriptor.into_iter().enumerate() {
            set(victim.b, 32 + i, v);
        }
        assert_eq!(victim.b.call(IPC_TAKE, other.rx[0], DESC, 32), Err(DENIED));
        assert_eq!(
            other.b.call(IPC_FEATURES, victim.rx[0], RECEIVE.into(), 1),
            Err(DENIED)
        );
        other.start(neighbor, 0, true);
        until(|| word(other.a, 14) > 1);
        let before = word(other.a, 14);
        let begin = hal::timer::counter();
        victim.start(candidate, 1, false);
        let [ca, cb] = victim.children;
        match fault {
            5 => {
                assert_eq!(ca.join(), Outcome::Exit(23));
                assert_eq!(victim.b.result(), Ok(0));
                assert_eq!(word(victim.b, 21), 0);
                victim.b.start(crate::abi::PROCESS_EXIT, 0, 0, 0);
                assert_eq!(cb.join(), Outcome::Exit(0));
            }
            6 => {
                assert_eq!(cb.join(), Outcome::Exit(24));
                assert_eq!(victim.a.result(), Ok(0));
                victim.a.start(crate::abi::PROCESS_EXIT, 0, 0, 0);
                assert_eq!(ca.join(), Outcome::Exit(0));
            }
            _ => {
                let accepted = u64::from(fault == 7);
                assert_eq!(victim.a.result(), Ok(accepted));
                assert_eq!(victim.b.result(), Ok(accepted));
                assert_eq!(word(victim.b, 21), accepted);
                if fault != 7 {
                    for (i, &handle) in victim.tx.iter().enumerate() {
                        assert_ne!(victim.a.call(IPC_FEATURES, handle, SEND.into(), 1), Ok(1));
                        assert_eq!(
                            victim.b.call(IPC_FEATURES, victim.rx[i], RECEIVE.into(), 1),
                            Err(DENIED)
                        );
                    }
                }
                stop([ca, cb], [victim.a, victim.b]);
            }
        }
        let elapsed = hal::timer::counter() - begin;
        let at_exit = word(other.a, 14);
        until(|| word(other.a, 14) > at_exit + 1);
        set(other.a, 13, 1);
        let end = other.a.result().unwrap();
        assert_eq!(other.b.result(), Ok(end));
        assert!(end > before);
        other.stop();
        assert_eq!(resources(), base);
        hal::log(format_args!(
            "ETH2B surete cas={fault} admis={} premiere_page=1 croisement_refuse=2 voisin={} duree_ticks={elapsed} solde=0",
            u64::from(fault == 7),
            end - before
        ));
    }
}
pub(super) fn run(scenario: &[u8]) {
    if scenario == b"ipc-eth2b-surete" {
        security();
        hal::log(format_args!("ETH2B campagne OK"));
        return;
    }
    let smoke = scenario == b"ipc-eth2b-essai";
    let rare_long = scenario.starts_with(b"ipc-eth2b-rare");
    let rotation = match scenario {
        b"ipc-eth2b-mesure-1" | b"ipc-eth2b-rare-1" => 1,
        b"ipc-eth2b-mesure-2" | b"ipc-eth2b-rare-2" => 2,
        _ => 0,
    };
    hal::log(format_args!("ETH2B ordre rotation={rotation}"));
    crate::sched::profile::ACTIVE.store(true, Relaxed);
    for &(profile, b, len) in &[
        (0, 1, 60),
        (0, 4, 1514),
        (0, 1, 1514),
        (0, 8, 1514),
        (0, 4, 60),
        (0, 8, 60),
        (3, 1, 60),
        (2, 8, 1514),
        (1, 1, 60),
        (1, 1, 1514),
    ] {
        if rare_long && profile != 1 {
            continue;
        }
        for shift in 0..3 {
            let mode = (shift + b + len + profile + rotation) % 3;
            cell(
                Case {
                    mode,
                    profile,
                    b,
                    len,
                },
                if smoke {
                    16
                } else if profile == 1 && !rare_long {
                    1000
                } else {
                    N
                },
            );
        }
    }
    crate::sched::profile::ACTIVE.store(false, Relaxed);
    hal::log(format_args!("ETH2B campagne OK"));
}
