//! Ordonnanceur du BSP. Les accès à CORE sont courts, IRQ masquées ; aucun
//! emprunt de la file ou d’un TCB ne traverse un changement de pile.
mod probes;
pub(crate) mod process;
mod stack;
mod sync;
use crate::hal::{self, context, interrupts::Guard};
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
    current: usize,
    generation: u64,
    stats: Stats,
}
struct Local(UnsafeCell<Option<Core>>);
// Sûreté : un seul CPU actif. Chaque emprunt est délimité par une fermeture sous
// masque IRQ. Les exceptions non masquables/fatales n’utilisent jamais CORE.
unsafe impl Sync for Local {}
static CORE: Local = Local(UnsafeCell::new(None));
fn with<R>(f: impl FnOnce(&mut Core) -> R) -> R {
    let _mask = Guard::new();
    // Sûreté : aucune fermeture appelée ici ne commute, ne réentre ni ne garde de référence.
    f(unsafe { (&mut *CORE.0.get()).as_mut().expect("ordonnanceur inactif") })
}

impl Core {
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
        self.thread_mut(i).state = State::Ready;
        self.ready.push(i);
        self.stats.wakes += 1;
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
        assert_eq!(self.thread(self.current).state, State::Running);
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
                assert_eq!(t.cpu_mask, 1);
                assert_eq!(t.priority == Priority::Idle, i == IDLE);
                assert_eq!(t.state == State::Running, i == self.current);
                assert_eq!(queued[i], i != IDLE && t.state == State::Ready);
                t.stack.check(if i == self.current {
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
        let old = self.current;
        let state = match why {
            Reason::Tick(_) => {
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
                if old == IDLE {
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
        // Un processus terminé doit libérer ses pages même si des travailleurs
        // restent prêts indéfiniment. Idle collecte depuis sa propre pile puis
        // repasse immédiatement à la file normale, hors du gestionnaire IRQ.
        let next = if matches!(why, Reason::Exit) && self.thread(old).owner.is_some() {
            IDLE
        } else {
            self.ready.pop().unwrap_or(IDLE)
        };
        let previous_space = self.thread(old).space;
        let t = self.thread_mut(next);
        assert_eq!(t.state, State::Ready);
        assert_eq!(t.cpu_mask, 1);
        t.state = State::Running;
        if old == next {
            return None;
        }
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
        self.current = next;
        if matches!(why, Reason::Tick(_)) && old != IDLE && next != IDLE {
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
    if let Some((save, load)) = with(|s| {
        let next = s.select(why);
        if masked_since != 0 {
            s.ipc.finish_masked(masked_since);
        }
        next
    }) {
        // Sûreté : TCB fixe, pile cible READY vérifiée, aucuns emprunts de CORE vivants.
        unsafe {
            context::switch(save, load);
        }
    }
}
pub(crate) fn tick(user: bool) {
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
            if i != s.current
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
            Self::UnsupportedCpu => f.write_str("affinité hors CPU0"),
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
    if cpu_mask != 1 {
        return Err(Error::UnsupportedCpu);
    }
    let guard = Guard::new();
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
    let entry = with(|s| s.thread(s.current).entry);
    context::enable();
    entry();
    let _guard = Guard::new();
    dispatch(Reason::Exit);
    unreachable!("thread terminé repris")
}
fn idle() {
    loop {
        let _guard = Guard::new();
        reap();
        if with(|s| s.ready.len != 0) {
            dispatch(Reason::Yield);
        } else {
            process::idle_sample();
            context::idle_wait();
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
            current: NONE,
            generation: 0,
            stats: Stats::default(),
        }));
    }
    create(IDLE, idle, Priority::Idle, 1).expect("pile idle");
    create(1, main, Priority::Normal, 1).expect("pile principale");
    let first = with(|s| {
        let i = s.ready.pop().unwrap();
        s.current = i;
        s.thread_mut(i).state = State::Running;
        s.thread(i).saved
    });
    hal::timer::start(HZ);
    let mut abandoned = 0;
    // Sûreté : abandon définitif de la pile Limine, IRQ encore masquées.
    unsafe {
        context::switch(&mut abandoned, first);
    }
    unreachable!()
}
pub(crate) use probes::test;
