//! Ordonnanceur préemptif à file partagée sous exclusion interprocesseurs. Aucun
//! emprunt de la file ou d’un TCB ne traverse un changement de pile.
mod gate;
pub(crate) use gate::Guard;
mod probes;
pub(crate) mod process;
pub(crate) mod profile;
mod stack;
pub(crate) mod sync;
use crate::hal::{self, context};
use crate::memory::SpaceId;
use alloc::vec::Vec;
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU64, Ordering},
};
use stack::Stack;
pub(crate) use sync::Mutex;

pub const HZ: u32 = 250;
const MAX: usize = 32;
const IDLE: usize = 0;
const NONE: usize = usize::MAX;

// Sûreté : symbole unique, compteur atomique lu aussi par les sondes assembleur.
#[unsafe(no_mangle)]
static NOVA_TICKS: AtomicU64 = AtomicU64::new(0);
pub fn ticks() -> u64 {
    NOVA_TICKS.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Wait {
    Until(u64),
    Ipc(u64),
    Event(usize),
    Process(process::Pid),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Ready,
    Running,
    Blocked(Wait),
    Terminated,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Id {
    slot: usize,
    generation: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Priority {
    Idle,
    Normal,
}
struct Thread {
    id: Id,
    space: SpaceId,
    owner: Option<process::Pid>,
    state: State,
    stack: Stack,
    saved: usize,
    wait_order: u64,
    #[cfg(feature = "user-fpu")]
    fp_initialized: bool,
    blocked_since: u64,
    last_cpu: usize,
    in_syscall: bool,
    entry: fn(),
    priority: Priority,
    cpu_mask: u64,
}
struct Ready {
    items: [usize; MAX],
    head: usize,
    len: usize,
}
impl Ready {
    fn push(&mut self, id: usize) {
        assert!(self.len < MAX, "file prête pleine");
        self.items[(self.head + self.len) % MAX] = id;
        self.len += 1;
    }
    fn pop(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let id = self.items[self.head];
        self.head = (self.head + 1) % MAX;
        self.len -= 1;
        Some(id)
    }
}

#[derive(Default, Clone, Copy)]
struct Stats {
    preemptions: u64,
    ipc_preemptions: u64,
    yields: u64,
    wakes: u64,
    exits: u64,
    reaped: u64,
    idle_ticks: u64,
    event_waits: u64,
}
struct Core {
    kernel_space: SpaceId,
    ipc: process::ipc::Domain,
    processes: process::Table,
    slots: Vec<Option<Thread>>,
    ready: Ready,
    currents: [usize; crate::smp::MAX_CPUS],
    generation: u64,
    stats: Stats,
}
struct Local(UnsafeCell<Option<Core>>);
// Sûreté : chaque emprunt est délimité par une fermeture sous le verrou du scheduler. Les exceptions non masquables/fatales n’utilisent jamais CORE.
unsafe impl Sync for Local {}
static CORE: Local = Local(UnsafeCell::new(None));
#[track_caller]
fn with<R>(f: impl FnOnce(&mut Core) -> R) -> R {
    let _mask = Guard::new();
    // Sûreté : aucune fermeture appelée ici ne commute, ne réentre ni ne garde de référence.
    f(unsafe { (&mut *CORE.0.get()).as_mut().expect("ordonnanceur inactif") })
}

impl Core {
    fn current(&self) -> usize {
        self.currents[crate::smp::index()]
    }
    fn idle_slot(cpu: usize) -> usize {
        if cpu == 0 { IDLE } else { MAX - cpu }
    }
    fn choose(&mut self, cpu: usize) -> Option<usize> {
        let eligible = (0..self.ready.len).find(|&offset| {
            let i = self.ready.items[(self.ready.head + offset) % MAX];
            self.thread(i).cpu_mask & (1 << cpu) != 0
        })?;
        let chosen = self.ready.items[(self.ready.head + eligible) % MAX];
        for i in (1..=eligible).rev() {
            self.ready.items[(self.ready.head + i) % MAX] =
                self.ready.items[(self.ready.head + i - 1) % MAX];
        }
        self.ready.pop();
        Some(chosen)
    }

    fn flush_ipc(&mut self) {
        let mut mask = self.ipc.take_wakes();
        while mask != 0 {
            let slot = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            if self.slots[slot].as_ref().is_some_and(|t| {
                t.state == State::Blocked(Wait::Ipc(self.ipc.wake_cookie(slot)))
                    && self.ipc.waiter_generation(slot) == Some(t.id.generation)
            }) {
                self.wake(slot);
            }
        }
    }
    fn thread(&self, i: usize) -> &Thread {
        self.slots[i].as_ref().expect("thread absent")
    }
    fn thread_mut(&mut self, i: usize) -> &mut Thread {
        self.slots[i].as_mut().expect("thread absent")
    }
    fn wake(&mut self, i: usize) {
        assert!(matches!(self.thread(i).state, State::Blocked(_)));
        if self.thread(i).blocked_since != 0 {
            #[cfg(feature = "ipc-smp-lab")]
            process::ipc::multicore::pret(i);
            profile::ipc_woke(crate::smp::index(), self.thread(i).blocked_since);
        }
        self.thread_mut(i).blocked_since = 0;
        self.thread_mut(i).state = State::Ready;
        self.ready.push(i);
        self.stats.wakes += 1;
        let me = crate::smp::index();
        let other =
            crate::smp::ONLINE.load(Ordering::Acquire) & !(1 << me) & self.thread(i).cpu_mask;
        if other != 0 {
            let idle =
                |cpu: usize| other & (1 << cpu) != 0 && self.currents[cpu] == Self::idle_slot(cpu);
            let last = self.thread(i).last_cpu;
            // Réveiller un seul PE éligible. La FIFO et le masque restent
            // revérifiés sous la garde lors de la sélection effective.
            let target = if last < crate::smp::count() && idle(last) {
                Some(last)
            } else {
                (0..crate::smp::count()).find(|&cpu| idle(cpu))
            };
            if let Some(cpu) = target {
                profile::kick(me);
                crate::arch::kick_one(cpu);
            }
        }
    }
    fn signal(&mut self, wait: Wait) -> Option<Id> {
        let id = self
            .slots
            .iter()
            .flatten()
            .filter(|t| t.state == State::Blocked(wait))
            .min_by_key(|t| t.wait_order)?
            .id;
        self.wake(id.slot);
        Some(id)
    }

    fn audit(&self) -> usize {
        let mut queued = [false; MAX];
        assert!(self.ready.len < MAX && self.ready.head < MAX);
        for offset in 0..self.ready.len {
            let i = self.ready.items[(self.ready.head + offset) % MAX];
            assert!(i != IDLE && !queued[i] && self.thread(i).state == State::Ready);
            queued[i] = true;
        }
        assert_eq!(self.thread(self.current()).state, State::Running);
        let mut occupied = 0;
        let mut terminated = 0;
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(t) = slot {
                occupied += 1;
                terminated += u64::from(t.state == State::Terminated);
                assert_eq!(t.id.slot, i);
                assert_eq!(
                    t.space,
                    t.owner
                        .map_or(self.kernel_space, |p| self.processes.space(p))
                );
                assert_ne!(t.cpu_mask, 0);
                assert_eq!(
                    t.priority == Priority::Idle,
                    (0..crate::smp::count()).any(|c| Self::idle_slot(c) == i)
                );
                assert_eq!(t.state == State::Running, self.currents.contains(&i));
                assert_eq!(
                    queued[i],
                    t.priority != Priority::Idle && t.state == State::Ready
                );
                if self.currents.contains(&i) && i != self.current() {
                    continue;
                }
                t.stack.check(if i == self.current() {
                    context::sp()
                } else {
                    t.saved
                });
            }
        }
        assert_eq!(self.stats.exits - self.stats.reaped, terminated);
        occupied
    }

    fn select(&mut self, why: Reason) -> Option<(*mut usize, usize)> {
        let cpu = crate::smp::index();
        let old = self.currents[cpu];
        let state = match why {
            Reason::Tick(user) => {
                profile::tick(cpu, user, old == Self::idle_slot(cpu));
                if crate::smp::index() == 0 {
                    let now = ticks().saturating_add(1);
                    NOVA_TICKS.store(now, Ordering::Relaxed);
                    self.ipc.expire(now);
                    self.flush_ipc();
                    for i in 0..self.slots.len() {
                        if matches!(self.slots[i].as_ref().map(|t| t.state),Some(State::Blocked(Wait::Until(end))) if end<=now)
                        {
                            self.wake(i);
                        }
                    }
                }
                if self.thread(old).priority == Priority::Idle {
                    self.stats.idle_ticks += 1;
                }
                State::Ready
            }
            Reason::Yield => {
                self.stats.yields += 1;
                State::Ready
            }
            Reason::Block(wait) => {
                if matches!(wait, Wait::Event(_)) {
                    self.stats.event_waits = self
                        .stats
                        .event_waits
                        .checked_add(1)
                        .expect("attentes épuisées");
                    self.thread_mut(old).wait_order = self.stats.event_waits;
                }
                if matches!(wait, Wait::Ipc(_)) {
                    self.thread_mut(old).blocked_since = profile::stamp();
                }
                State::Blocked(wait)
            }
            Reason::Exit => {
                self.stats.exits += 1;
                State::Terminated
            }
        };
        let t = self.thread_mut(old);
        assert_eq!(t.state, State::Running);
        t.stack.check(context::sp());
        t.state = state;
        if t.priority != Priority::Idle && state == State::Ready {
            self.ready.push(old);
        }
        // Une IRQ peut reprendre idle juste avant reap(). La dette de collecte
        // reste observable jusqu'au retrait du TCB sur une autre pile.
        let idle_slot = Self::idle_slot(cpu);
        let next = if cpu == 0 && self.stats.exits != self.stats.reaped {
            idle_slot
        } else {
            self.choose(cpu).unwrap_or(idle_slot)
        };
        #[cfg(feature = "user-fpu")]
        if old != next {
            let previous = self.thread(old).owner.is_some();
            let following = self.thread(next).owner.is_some();
            if previous || following {
                // Chaque slot a un seul propriétaire sous la garde scheduler.
                // Le premier dispatch du nouveau TCB efface la génération précédente.
                let target = if following {
                    let buffer = unsafe { &mut *FP_STATE[next].0.get() };
                    if !self.thread(next).fp_initialized {
                        crate::arch::fpu::initial(buffer);
                        self.thread_mut(next).fp_initialized = true;
                    }
                    Some(buffer.as_ptr())
                } else {
                    None
                };
                unsafe {
                    crate::arch::fpu::exchange(
                        previous.then(|| FP_STATE[old].0.get().cast()),
                        target,
                    );
                }
            }
        }
        let previous_space = self.thread(old).space;
        let t = self.thread_mut(next);
        assert_eq!(t.state, State::Ready);
        assert_ne!(t.cpu_mask & (1 << cpu), 0);
        t.state = State::Running;
        if old == next {
            return None;
        }
        #[cfg(feature = "ipc-smp-lab")]
        if profile::ACTIVE.load(Ordering::Relaxed) {
            process::ipc::multicore::elu(next);
        }
        profile::switched(cpu, t.last_cpu != NONE && t.last_cpu != cpu);
        t.last_cpu = cpu;
        t.stack.check(t.saved);
        let load = t.saved;
        if t.owner.is_some() {
            context::kernel_stack(t.stack.top);
        }
        if previous_space != t.space {
            // Sûreté : IRQ masquées ; les deux racines et piles restent vivantes
            // jusqu’à la collecte sur une pile tierce. La moitié haute est commune.
            unsafe { t.space.activate() };
        }
        self.currents[cpu] = next;
        if matches!(why, Reason::Tick(_)) && old != idle_slot && next != idle_slot {
            self.stats.preemptions += 1;
            if self.ipc.has_pending() {
                self.stats.ipc_preemptions += 1;
            }
        }
        if matches!(why, Reason::Tick(false)) && self.thread(old).in_syscall {
            self.processes.syscall_switches += 1;
        }
        if matches!(why, Reason::Tick(true)) {
            let pid = self
                .thread(old)
                .owner
                .expect("IRQ utilisateur sans processus");
            self.processes.preempted(pid);
        }
        Some((&raw mut self.thread_mut(old).saved, load))
    }
}
#[derive(Clone, Copy)]
enum Reason {
    Tick(bool),
    Yield,
    Block(Wait),
    Exit,
}

/// IRQ déjà masquées par l’appelant ; le masque reste attaché à la pile du thread.
fn dispatch(why: Reason) {
    dispatch_measured(why, 0);
}
fn dispatch_measured(why: Reason, masked_since: u64) {
    use process::ipc::metrics;
    let _gate = Guard::new();
    let start = metrics::switch_stamp();
    if let Some((save, load)) = with(|s| {
        let old = s.current();
        let next = s.select(why);
        if start != 0 && next.is_some() {
            metrics::switch_out(
                start,
                s.ipc.measured_peer(s.thread(old).owner)
                    && s.ipc.measured_peer(s.thread(s.current()).owner),
            );
        }
        if masked_since != 0 {
            s.ipc.finish_masked(masked_since);
        }
        next
    }) {
        // Sûreté : TCB fixe, pile cible READY vérifiée, aucuns emprunts de CORE vivants.
        unsafe {
            let depth = gate::outgoing();
            context::switch(save, load, &gate::OWNER);
            gate::resumed(depth);
        }
        metrics::switch_in();
    }
}
pub(crate) fn tick(user: bool) {
    crate::smp::check_stop();
    smp_probe::pulse();
    let irq = Guard::new();
    assert!(!irq.was_enabled(), "tick hors interruption");
    dispatch(Reason::Tick(user));
}
/// Canal stable pendant l’attente. Le prédicat appartient au pilote ou au mutex.
pub(crate) struct Event {
    _identity: u8,
}
impl Event {
    pub const fn new() -> Self {
        Self { _identity: 0 }
    }
    pub fn wait(&self, guard: &Guard) {
        assert!(
            guard.was_enabled(),
            "attente depuis une section non préemptible"
        );
        dispatch(Reason::Block(Wait::Event(self as *const _ as usize)));
    }
    pub fn notify_one(&self) -> Option<Id> {
        with(|s| s.signal(Wait::Event(self as *const _ as usize)))
    }
}
pub fn yield_now() {
    let guard = Guard::new();
    assert!(guard.was_enabled(), "yield en section critique");
    dispatch(Reason::Yield);
}
pub fn sleep(delay: u64) {
    if delay == 0 {
        return yield_now();
    }
    let guard = Guard::new();
    assert!(guard.was_enabled(), "sleep en section critique");
    dispatch(Reason::Block(Wait::Until(ticks().saturating_add(delay))));
}

fn reap() {
    let _guard = Guard::new();
    if with(|s| s.stats.exits == s.stats.reaped) {
        return;
    }
    for i in 1..MAX {
        let stack = with(|s| {
            if !s.currents.contains(&i)
                && s.slots[i]
                    .as_ref()
                    .is_some_and(|t| t.state == State::Terminated)
            {
                s.stats.reaped += 1;
                Some(s.slots[i].take().unwrap())
            } else {
                None
            }
        });
        if let Some(thread) = stack {
            thread.stack.release();
            if let Some(pid) = thread.owner {
                process::detach(pid);
            }
        }
    }
}

pub enum Error {
    UnsupportedCpu,
    Critical,
    Full,
    Memory(crate::memory::Error),
}
impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedCpu => f.write_str("affinité hors topologie"),
            Self::Critical => f.write_str("section critique"),
            Self::Full => f.write_str("table des threads pleine"),
            Self::Memory(e) => write!(f, "pile : {e:?}"),
        }
    }
}
fn create(slot: usize, entry: fn(), priority: Priority, cpu_mask: u64) -> Result<Id, Error> {
    let stack = Stack::allocate(slot).map_err(Error::Memory)?;
    // Sûreté : pile vide appartenant exclusivement au nouveau slot.
    let saved = unsafe { context::initial(stack.top, trampoline) };
    Ok(with(|s| {
        s.generation = s.generation.checked_add(1).expect("identifiants épuisés");
        let id = Id {
            slot,
            generation: s.generation,
        };
        s.slots[slot] = Some(Thread {
            id,
            space: s.kernel_space,
            owner: None,
            state: State::Ready,
            stack,
            saved,
            wait_order: 0,
            #[cfg(feature = "user-fpu")]
            fp_initialized: false,
            blocked_since: 0,
            last_cpu: NONE,
            in_syscall: false,
            entry,
            priority,
            cpu_mask,
        });
        if priority != Priority::Idle {
            s.ready.push(slot);
        }
        id
    }))
}
pub fn spawn(entry: fn()) -> Result<Id, Error> {
    spawn_on(entry, 1)
}
pub fn spawn_on(entry: fn(), cpu_mask: u64) -> Result<Id, Error> {
    let guard = Guard::new();
    if cpu_mask == 0 || cpu_mask & !crate::smp::ONLINE.load(Ordering::Acquire) != 0 {
        return Err(Error::UnsupportedCpu);
    }
    if !guard.was_enabled() {
        return Err(Error::Critical);
    }
    reap();
    let slot = with(|s| s.slots.iter().position(Option::is_none)).ok_or(Error::Full)?;
    create(slot, entry, Priority::Normal, cpu_mask)
}
pub fn live(id: Id) -> bool {
    with(|s| {
        s.slots[id.slot]
            .as_ref()
            .is_some_and(|t| t.id == id && t.state != State::Terminated)
    })
}
extern "C" fn trampoline() -> ! {
    let cpu = crate::smp::index();
    let entry = with(|s| s.thread(s.current()).entry);
    crate::smp::ONLINE.fetch_or(1 << cpu, Ordering::Release);
    context::enable();
    entry();
    let _guard = Guard::new();
    dispatch(Reason::Exit);
    unreachable!("thread terminé repris")
}
fn idle() {
    loop {
        if crate::smp::index() == 0 {
            process::before_reap_probe();
        }
        let _guard = Guard::new();
        let cpu = crate::smp::index();
        if cpu != 0 && crate::smp::stop_requested() {
            // À cette frontière le PE ne possède que sa pile idle. La sonde
            // a récolté les travailleurs. Le retrait du masque précède le HLT/WFI.
            crate::heap::with_pages(|_| {
                crate::arch::poll_translation();
                crate::smp::record_park(cpu);
                crate::smp::ONLINE.fetch_and(!(1 << cpu), Ordering::Release);
                Ok(())
            })
            .expect("retrait du CPU");
            _guard.unlock_masked();
            hal::cpu::park_local();
        }
        if cpu == 0 {
            reap();
        }
        if with(|s| {
            (0..s.ready.len).any(|n| {
                s.thread(s.ready.items[(s.ready.head + n) % MAX]).cpu_mask
                    & (1 << crate::smp::index())
                    != 0
            })
        }) {
            dispatch(Reason::Yield);
        } else {
            if crate::smp::index() == 0 {
                process::idle_sample();
            }
            _guard.unlock_masked();
            context::idle_wait();
            context::enable();
        }
    }
}
pub fn start(main: fn(), kernel_space: SpaceId) -> ! {
    hal::interrupts::mask();
    context::prepare();
    let mut slots = Vec::new();
    slots.try_reserve_exact(MAX).expect("TCB");
    slots.resize_with(MAX, || None);
    // Sûreté : initialisation unique sur la pile d’amorçage, IRQ masquées.
    unsafe {
        CORE.0.get().write(Some(Core {
            kernel_space,
            processes: process::Table::new(),
            ipc: process::ipc::Domain::new(),
            slots,
            ready: Ready {
                items: [0; MAX],
                head: 0,
                len: 0,
            },
            currents: [NONE; crate::smp::MAX_CPUS],
            generation: 0,
            stats: Stats::default(),
        }));
    }
    create(IDLE, idle, Priority::Idle, 1).expect("pile idle");
    create(1, main, Priority::Normal, 1).expect("pile principale");
    let first = with(|s| {
        let i = s.ready.pop().unwrap();
        s.currents[0] = i;
        s.thread_mut(i).state = State::Running;
        s.thread(i).saved
    });
    for cpu in 1..crate::smp::count() {
        let slot = Core::idle_slot(cpu);
        create(slot, idle, Priority::Idle, 1 << cpu).expect("pile idle secondaire");
    }
    hal::timer::start(HZ);
    let mut abandoned = 0;
    // Sûreté : abandon définitif de la pile Limine, IRQ encore masquées.
    unsafe {
        context::switch(&mut abandoned, first, core::ptr::null());
    }
    unreachable!()
}
pub(crate) use probes::test;

pub(crate) fn secondary(cpu: usize) -> ! {
    let first = with(|s| {
        let i = Core::idle_slot(cpu);
        assert!(s.currents[cpu] == NONE);
        s.currents[cpu] = i;
        let t = s.thread_mut(i);
        t.state = State::Running;
        t.saved
    });
    hal::timer::start_secondary();
    let mut abandoned = 0;
    // Sûreté : le TCB idle et sa pile privée sont réservés à ce CPU.
    unsafe {
        context::switch(&mut abandoned, first, core::ptr::null());
    }
    unreachable!()
}

mod smp_probe;
pub(crate) use smp_probe::{run as smp_test, shutdown_test};

pub(crate) fn reschedule_idle() {
    crate::smp::check_stop();
    let _guard = Guard::new();
    if with(|s| s.current() == Core::idle_slot(crate::smp::index())) {
        dispatch(Reason::Yield);
    }
}

// Réservé aux threads noyau qui n'ont ni bail matériel ni affinité de pilote.
fn set_affinity(mask: u64) -> Result<(), Error> {
    let guard = Guard::new();
    if !guard.was_enabled() {
        return Err(Error::Critical);
    }
    if mask == 0 || mask & !crate::smp::ONLINE.load(Ordering::Acquire) != 0 {
        return Err(Error::UnsupportedCpu);
    }
    with(|s| {
        let t = s.thread_mut(s.current());
        assert!(t.priority == Priority::Normal && t.owner.is_none());
        t.cpu_mask = mask;
    });
    if mask & (1 << crate::smp::index()) == 0 {
        dispatch(Reason::Yield);
    }
    Ok(())
}

#[cfg(feature = "user-fpu")]
#[repr(align(64))]
struct FpState(UnsafeCell<[u8; crate::arch::fpu::SIZE]>);
#[cfg(feature = "user-fpu")]
// Les accès ont lieu sous la garde globale, avant le transfert de pile.
unsafe impl Sync for FpState {}
#[cfg(feature = "user-fpu")]
static FP_STATE: [FpState; MAX] =
    [const { FpState(UnsafeCell::new([0; crate::arch::fpu::SIZE])) }; MAX];
