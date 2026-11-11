//! Les TCB désignent le processus par PID. Le processus possède les pages,
//! le parent ne conserve qu’un identifiant de génération et récupère le statut.
use super::*;
use crate::{
    heap,
    memory::user::{self, UserSpace},
};

mod abi_probe;
mod demo;
pub(crate) mod ipc;
pub(crate) mod storage;
pub(super) use abi_probe::idle_sample;
pub(crate) use abi_probe::test as abi_test;
pub(crate) mod syscall;
pub(super) use demo::before_reap_probe;
pub(crate) use demo::{boot_demo, test};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Pid {
    slot: usize,
    number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fault {
    pub trap: &'static str,
    pub access: &'static str,
    pub cause: &'static str,
    pub address: u64,
    pub syndrome: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Exit(u64),
    Fault(Fault),
}

struct Process {
    pid: Pid,
    space: Option<UserSpace>,
    threads: usize,
    observed: bool,
    wait_right: Option<WaitRight>,
    debug_messages: u8,
    // None pendant la vie du processus. Le statut reste disponible après collecte.
    outcome: Option<Outcome>,
    preemptions: u64,
}
pub(super) struct Table {
    slots: [Option<Process>; MAX],
    serial: u64,
    pub(super) syscall_switches: u64,
    blocked_waits: u64,
}
impl Table {
    pub(super) const fn new() -> Self {
        Self {
            slots: [const { None }; MAX],
            serial: 0,
            syscall_switches: 0,
            blocked_waits: 0,
        }
    }
    fn get(&self, pid: Pid) -> &Process {
        let p = self.slots[pid.slot].as_ref().expect("PID absent");
        assert_eq!(p.pid, pid);
        p
    }
    fn get_mut(&mut self, pid: Pid) -> &mut Process {
        let p = self.slots[pid.slot].as_mut().expect("PID absent");
        assert_eq!(p.pid, pid);
        p
    }
    fn release_observer(&mut self, pid: Pid) {
        let p = self.get_mut(pid);
        p.observed = false;
        if p.space.is_none() {
            self.slots[pid.slot] = None;
        }
    }
    pub(super) fn space(&self, pid: Pid) -> SpaceId {
        self.get(pid).space.as_ref().unwrap().id()
    }
    pub(super) fn preempted(&mut self, pid: Pid) {
        self.get_mut(pid).preemptions += 1;
    }
}
fn enter() {
    let _irq = Guard::new();
    let (top, entry, user_top) = with(|s| {
        let t = s.thread(s.current);
        let space = s.processes.get(t.owner.unwrap()).space.as_ref().unwrap();
        (t.stack.top, space.entry(), space.stack_top())
    });
    // Sûreté : thread utilisateur fraîchement publié ; code RX et pile RW/NX
    // validés, racine active, IRQ masquées jusqu’au retour matériel.
    unsafe { context::enter_user(top, entry as usize, user_top as usize) }
}

/// Droit noyau de suivre ce fils et de récolter son statut. Le PID numérique
/// sert au diagnostic ; il ne donne pas accès à ce droit depuis un programme.
struct Child(Pid);

/// Une référence forte, typée pour la seule opération WAIT. La clé locale ne
/// permet jamais de retrouver un objet par un PID fourni par l’utilisateur.
#[derive(Clone, Copy)]
struct WaitRight {
    target: Pid,
}
impl WaitRight {
    fn key(self) -> u64 {
        self.target.number | (1 << 63)
    }
}

fn spawn_image(
    image: &'static [u8],
    data: &[u8],
    private_ro: Option<&[u8]>,
    wait_on: Option<Child>,
) -> Result<Child, Error> {
    let guard = Guard::new();
    if !guard.was_enabled() {
        return Err(Error::Critical);
    }
    create_image(image, data, private_ro, wait_on)
}

// L’appelant maintient le masque IRQ jusqu’à publication du groupe.
fn create_image(
    image: &'static [u8],
    data: &[u8],
    private_ro: Option<&[u8]>,
    wait_on: Option<Child>,
) -> Result<Child, Error> {
    reap();
    let (slot, proc_slot) = with(|s| {
        Some((
            s.slots.iter().position(Option::is_none)?,
            s.processes.slots.iter().position(Option::is_none)?,
        ))
    })
    .ok_or(Error::Full)?;
    let space =
        heap::with_pages(|m| UserSpace::new(m, image, data, private_ro)).map_err(Error::Memory)?;
    let root = space.id();
    // Le masque IRQ reste en place de la réservation des slots à la publication
    // du couple PID/TCB. Le nouveau thread ne peut observer une racine par défaut.
    if let Err(e) = create(slot, enter, Priority::Normal, 1) {
        heap::with_pages(|m| {
            space.release(m);
            Ok(())
        })
        .unwrap();
        return Err(e);
    }
    let wait_right = wait_on.map(|child| {
        let right = WaitRight { target: child.0 };
        core::mem::forget(child); // Transfert de l’unique droit, repris par wait ou detach.
        right
    });
    Ok(with(|s| {
        s.processes.serial = s
            .processes
            .serial
            .checked_add(1)
            .filter(|n| *n < 1 << 63)
            .expect("PID épuisés");
        let pid = Pid {
            slot: proc_slot,
            number: s.processes.serial,
        };
        s.processes.slots[proc_slot] = Some(Process {
            pid,
            space: Some(space),
            threads: 1,
            observed: true,
            wait_right,
            debug_messages: 0,
            outcome: None,
            preemptions: 0,
        });
        s.thread_mut(slot).owner = Some(pid);
        s.thread_mut(slot).space = root;
        Child(pid)
    }))
}

/// Appelé exclusivement depuis une exception synchrone dont le cadre matériel
/// atteste le mode utilisateur. La pile privée du noyau est active, IRQ masquées.
pub(crate) fn finish(result: Outcome, pc: u64, level: &'static str) -> ! {
    let pid = with(|s| {
        let pid = s.thread(s.current).owner.expect("exception hors processus");
        crate::hardware::block::died(pid.number);
        s.ipc.death(pid, ticks());
        s.flush_ipc();
        s.ipc.collect();
        let p = s.processes.get_mut(pid);
        assert!(p.outcome.replace(result).is_none());
        pid
    });
    if !ipc::quiet() {
        match result {
            Outcome::Exit(code) => hal::log(format_args!(
                "PROC pid={} niveau={level} pc={pc:#x} sortie={code}",
                pid.number
            )),
            Outcome::Fault(f) => hal::log(format_args!(
                "PROC pid={} niveau={level} pc={pc:#x} faute={} adresse={:#x} acces={} cause={} syndrome={:#x}",
                pid.number, f.trap, f.address, f.access, f.cause, f.syndrome
            )),
        }
    }
    dispatch(Reason::Exit);
    unreachable!("processus terminé repris")
}

pub(super) fn detach(pid: Pid) {
    let last = with(|s| {
        let p = s.processes.get_mut(pid);
        p.threads = p.threads.checked_sub(1).expect("compte threads");
        if p.threads == 0 {
            assert!(p.outcome.is_some());
            let space = p.space.take().expect("dernier espace");
            let orphan = !p.observed;
            let unused = p.wait_right.take();
            if orphan {
                s.processes.slots[pid.slot] = None;
            }
            if let Some(right) = unused {
                s.processes.release_observer(right.target);
            }
            s.signal(Wait::Process(pid));
            Some(space)
        } else {
            None
        }
    });
    if let Some(space) = last {
        heap::with_pages(|m| {
            space.release(m);
            Ok(())
        })
        .unwrap();
    }
}

impl Child {
    fn try_result(&self) -> Option<Outcome> {
        reap();
        with(|s| {
            let p = s.processes.get(self.0);
            if p.space.is_none() { p.outcome } else { None }
        })
    }

    fn join(self) -> Outcome {
        loop {
            if let Some(outcome) = self.try_result() {
                return outcome;
            }
            sleep(1);
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        with(|s| s.processes.release_observer(self.0));
    }
}

pub(crate) const fn metadata_bytes() -> usize {
    size_of::<Table>()
}

fn measure(phase: &str) {
    with(|s| {
        let mut r = crate::memory::Resident::default();
        r.ipc = s.ipc.pages();
        r.tcb_bytes = s.slots.capacity() * size_of::<Option<Thread>>();
        for t in s.slots.iter().flatten() {
            r.stacks += (t.stack.top - t.stack.low) / 4096;
        }
        for p in s.processes.slots.iter().flatten() {
            r.processes += 1;
            r.process_bytes += size_of::<Process>();
            if let Some(space) = &p.space {
                let (data, stack) = space.page_counts();
                r.user += data;
                r.user_stacks += stack;
            }
        }
        heap::measure(phase, r);
    });
}
