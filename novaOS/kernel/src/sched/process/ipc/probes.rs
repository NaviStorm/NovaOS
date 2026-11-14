//! Orchestration de processus réels par leurs pages DATA privées. Le banc ne
//! fournit aucune porte privilégiée à l’image utilisateur.
use super::*;
mod bulk;
#[cfg(feature = "ipc-smp-lab")]
pub(crate) mod multicore;
mod contract;
mod limits;
mod perf;
mod races;
pub(super) static PAUSED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static EXIT_ON_RESUME: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static QUIET: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
pub(crate) fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}
pub(super) fn after_wake(pid: u64) {
    // Injection de mort d'un thread admis depuis sa véritable pile syscall.
    if EXIT_ON_RESUME
        .compare_exchange(pid, 0, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        hal::interrupts::mask();
        finish(Outcome::Exit(37), user::CODE, "sonde");
    }
    while PAUSED.load(Ordering::Relaxed) == pid {
        yield_now();
    }
}
const IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ipc.bin"));
const DESC: u64 = user::DATA + 256;
const OUTPUT: u64 = user::DATA + 512;
#[derive(Clone, Copy)]
struct Actor(Pid);
fn group<const N: usize>(rights: [u32; N]) -> ([Child; N], [Actor; N]) {
    let children = launch(rights.map(|r| (IMAGE, r))).unwrap();
    let actors = core::array::from_fn(|i| Actor(children[i].0));
    (children, actors)
}
fn until(mut f: impl FnMut() -> bool) {
    let start = ticks();
    while !f() {
        assert!(ticks() - start < 20000, "délai du banc IPC");
        yield_now();
    }
}
impl Actor {
    fn memory<R>(self, f: impl FnOnce(&mut UserSpace, &mut crate::memory::Memory) -> R) -> R {
        with(|s| {
            let p = s.processes.get_mut(self.0);
            assert!(p.outcome.is_none(), "acteur terminé {:?}", p.outcome);
            heap::with_pages(|m| Ok(f(p.space.as_mut().unwrap(), m))).unwrap()
        })
    }
    fn put(self, addr: u64, bytes: &[u8]) {
        self.memory(|s, m| assert!(s.copy_to_user(m, addr, bytes)));
    }
    fn bytes<const N: usize>(self, addr: u64) -> [u8; N] {
        self.memory(|s, m| {
            let mut buf = [0; N];
            assert!(s.copy_from_user(m, addr, &mut buf));
            buf
        })
    }
    fn start(self, op: u64, a: u64, b: u64, c: u64) {
        self.memory(|s, m| {
            assert_eq!(s.read_data(m, 0), 0);
            for (i, v) in [a, b, c].into_iter().enumerate() {
                s.write_data(m, i + 1, v);
            }
            s.write_data(m, 0, op);
        });
    }
    fn result(self) -> Res<u64> {
        until(|| self.memory(|s, m| s.read_data(m, 0) == 0));
        self.memory(|s, m| {
            let status = s.read_data(m, 4);
            let value = s.read_data(m, 5);
            if status == 0 {
                Ok(value)
            } else {
                assert_eq!(value, 0);
                Err(status)
            }
        })
    }
    fn call(self, op: u64, a: u64, b: u64, c: u64) -> Res<u64> {
        self.start(op, a, b, c);
        self.result()
    }
    fn boot(self) -> u64 {
        self.call(IPC_BOOTSTRAP, 0, 0, 0).unwrap()
    }
    fn blocked(self) {
        until(|| {
            with(|s| {
                s.slots.iter().flatten().any(|t| {
                    t.owner == Some(self.0) && matches!(t.state, State::Blocked(Wait::Ipc(_)))
                })
            })
        });
    }
    fn send(self, key: u64, tag: u64, len: u32, delay: u64) {
        self.put(DESC, &send_wire(tag, len, delay));
        self.start(IPC_SEND, key, DESC, 160);
    }
    fn recv(self, key: u64, delay: u64, accept: bool) {
        self.put(OUTPUT, &receive_wire(delay, accept));
        self.start(IPC_RECEIVE, key, OUTPUT, 184);
    }
    fn give(self, canal: u64, src: u64, rights: u32, mode: u32) {
        self.put(DESC, &give_wire(src, rights, mode));
        self.start(IPC_GIVE, canal, DESC, 48);
    }
}
fn send_wire(tag: u64, len: u32, delay: u64) -> [u8; 160] {
    let mut b = [0; 160];
    b[..4].copy_from_slice(&1u32.to_le_bytes());
    b[4..8].copy_from_slice(&160u32.to_le_bytes());
    b[8..16].copy_from_slice(&delay.to_le_bytes());
    b[16..24].copy_from_slice(&tag.to_le_bytes());
    b[24..28].copy_from_slice(&len.to_le_bytes());
    b[28..32].copy_from_slice(&u32::from(delay != 0).to_le_bytes());
    for (i, x) in b[32..].iter_mut().enumerate() {
        *x = tag.wrapping_add(i as u64) as u8;
    }
    b
}
fn receive_wire(delay: u64, accept: bool) -> [u8; 184] {
    let mut b = [0xa5; 184];
    b[..4].copy_from_slice(&1u32.to_le_bytes());
    b[4..8].copy_from_slice(&184u32.to_le_bytes());
    b[8..16].copy_from_slice(&delay.to_le_bytes());
    b[16..20].copy_from_slice(&u32::from(accept).to_le_bytes());
    b[20..24].fill(0);
    b
}
fn give_wire(src: u64, rights: u32, mode: u32) -> [u8; 48] {
    let mut b = [0; 48];
    for (i, v) in [
        1 | (48 << 32),
        15000,
        src,
        9,
        u64::from(rights) | (u64::from(mode) << 32),
        0,
    ]
    .into_iter()
    .enumerate()
    {
        b[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }
    b
}
fn u64_at(buf: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(buf[i..i + 8].try_into().unwrap())
}
fn stop<const N: usize>(children: [Child; N], actors: [Actor; N]) {
    for a in actors {
        a.start(crate::abi::PROCESS_EXIT, 0, 0, 0);
    }
    for c in children {
        assert_eq!(c.join(), Outcome::Exit(0));
    }
}
fn t10() {
    let base = heap::audit(false);
    let (children, [a, b, c]) = group([ALL, ALL, RECEIVE]);
    let root = a.boot();
    let rx = b.boot();
    let addr = user::DATA + crate::memory::PAGE - 96;
    b.memory(|s, m| s.add_second_data(m).unwrap());
    let sentinel = receive_wire(15000, false);
    b.put(addr, &sentinel);
    b.start(IPC_RECEIVE, rx, addr, 184);
    b.blocked();
    assert_eq!(b.bytes::<184>(addr), sentinel);
    b.memory(|s, m| s.remove_second_data(m));
    a.send(root, 42, 128, 0);
    assert_eq!(a.result(), Ok(128));
    assert_eq!(b.result(), Err(BAD_ADDRESS));
    assert_eq!(b.bytes::<96>(addr), sentinel[..96]);
    let valid = c.boot();
    c.recv(valid, 0, false);
    assert_eq!(c.result(), Ok(1));
    let out = c.bytes::<184>(OUTPUT);
    assert_eq!(u64_at(&out, 32), 42);
    assert_eq!(&out[56..], &send_wire(42, 128, 0)[32..]);
    // Les trois structures peuvent traverser la frontière RW/RW. Une fois
    // l'offre inscrite, même une réécriture intégrale conserve le snapshot.
    a.memory(|s, m| s.add_second_data(m).unwrap());
    b.memory(|s, m| s.add_second_data(m).unwrap());
    for (op, size) in [(IPC_SEND, 160), (IPC_RECEIVE, 184), (IPC_GIVE, 48)] {
        for invalid in [user::TOP, 0xffff_ffff_8000_0000, u64::MAX - 7] {
            assert_eq!(b.call(op, rx, invalid, size), Err(BAD_ADDRESS));
        }
        assert_eq!(b.call(op, rx, addr + 1, size), Err(BAD_ARGUMENT));
    }
    assert_eq!(b.call(IPC_RECEIVE, rx, user::CODE, 184), Err(BAD_ADDRESS));
    a.put(addr, &send_wire(71, 32, 15000));
    a.start(IPC_SEND, root, addr, 160);
    a.blocked();
    a.put(addr, &[0xff; 160]);
    b.put(addr, &receive_wire(0, false));
    assert_eq!(b.call(IPC_RECEIVE, rx, addr, 184), Ok(1));
    assert_eq!(a.result(), Ok(32));
    let actual = b.bytes::<184>(addr);
    assert_eq!(u64_at(&actual, 32), 71);
    assert_eq!(&actual[56..88], &send_wire(71, 32, 0)[32..64]);
    b.put(addr, &receive_wire(15000, true));
    b.start(IPC_RECEIVE, rx, addr, 184);
    b.blocked();
    b.put(addr, &[0xef; 184]);
    let give_addr = user::DATA + crate::memory::PAGE - 16;
    a.put(give_addr, &give_wire(root, 1, 1));
    a.start(IPC_GIVE, root, give_addr, 48);
    assert_eq!(a.result(), Ok(0));
    assert_eq!(b.result(), Ok(3));
    let got = b.bytes::<184>(addr);
    assert_eq!(&got[..24], &receive_wire(15000, true)[..24]);
    assert_eq!(u64_at(&got, 48), 1);
    let copied = u64_at(&got, 40);
    assert_eq!(b.call(IPC_NOTIFY, copied, 1, 0), Ok(0));
    stop(children, [a, b, c]);
    assert_eq!(heap::audit(false), base);
    hal::log(format_args!(
        "IPC T10 bloque=1 pages_rw=2 temoin=96 tete=42 cadres={} OK",
        base.free_pages
    ));
}
pub(crate) fn run(scenario: &[u8]) {
    #[cfg(feature = "ipc-smp-lab")]
    if scenario.starts_with(b"ipc-smp-") {
        multicore::run(scenario);
    }
    if scenario == b"ipc-perf" {
        perf::run();
        hal::log(format_args!("IPC validation OK"));
        return;
    }
    if scenario == b"ipc-races" {
        races::long();
        hal::log(format_args!("IPC validation OK"));
        return;
    }
    if scenario == b"ipc-endurance" {
        QUIET.store(true, Ordering::Relaxed);
        hal::log(format_args!("IPC endurance debut"));
        for period in 1u64.. {
            let start = ticks();
            let mut rounds = 0;
            while ticks() - start < 250 {
                contract::epoch();
                rounds += 1;
            }
            hal::log(format_args!(
                "IPC endurance periode={period} cycles={rounds} libres={}",
                heap::audit(false).free_pages
            ));
        }
    }
    let baseline = heap::audit(false);
    let hz = hal::timer::counter_hz();
    measure("ipc_repos");
    for (name, test) in [
        ("T01", contract::formats as fn()),
        ("T02", contract::rights),
        ("T03", contract::tree),
        ("T04", contract::gifts),
        ("T05/T06", contract::fifo),
        ("T07/T15", contract::notifications),
        ("T06/T08/T18", races::timing),
        ("T10", t10),
        ("lots", bulk::contract),
        ("T11", contract::revoke_inflight),
        ("T12/T18", contract::deaths),
        ("T04/T11", races::stale_and_move),
        ("T12", contract::foreign_death),
        ("T13", limits::quotas),
        ("T16", contract::epoch),
        ("T18", races::composed),
        ("T14", limits::saturation),
    ] {
        let start = hal::timer::counter();
        test();
        let ns = u128::from(hal::timer::counter() - start) * 1_000_000_000 / u128::from(hz);
        hal::log(format_args!("IPC cas={name} ns={ns} OK"));
    }
    assert_eq!(heap::audit(false), baseline);
    measure("ipc_detruits");
    hal::log(format_args!("IPC validation OK"));
}

pub(crate) use perf::smp_roundtrip;
