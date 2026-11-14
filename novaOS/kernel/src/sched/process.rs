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
    terminal: u8,
    exit_counter: u64,
    // None pendant la vie du processus. Le statut reste disponible après collecte.
    outcome: Option<Outcome>,
    preemptions: u64,
    preempted_on: u64,
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
        let p = self.get_mut(pid);
        p.preemptions += 1;
        p.preempted_on |= 1 << crate::smp::index();
    }
}
fn enter() {
    let _irq = Guard::new();
    let (top, entry, user_top) = with(|s| {
        let t = s.thread(s.current());
        let space = s.processes.get(t.owner.unwrap()).space.as_ref().unwrap();
        (t.stack.top, space.entry(), space.stack_top())
    });
    // Sûreté : thread utilisateur fraîchement publié ; code RX et pile RW/NX
    // validés, racine active, IRQ masquées jusqu’au retour matériel.
    _irq.unlock_masked();
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
    prepare_and_publish(image, data, private_ro, wait_on, None)
}

// Le préparateur ne possède encore aucun slot ni droit publié. Le verrou des
// pages est relâché avant l'acquisition de celui de l'ordonnanceur.
fn prepare_and_publish(
    image: &'static [u8],
    data: &[u8],
    private_ro: Option<&[u8]>,
    wait_on: Option<Child>,
    mask: Option<u64>,
) -> Result<Child, Error> {
    let irq = hal::interrupts::Guard::new();
    if !irq.was_enabled() {
        return Err(Error::Critical);
    }
    drop(irq);
    reap();
    let space =
        heap::with_pages(|m| UserSpace::new(m, image, data, private_ro)).map_err(Error::Memory)?;
    let _gate = Guard::new();
    let child = publish_image(space, wait_on)?;
    if let Some(mask) = mask {
        with(|s| {
            s.slots
                .iter_mut()
                .flatten()
                .find(|t| t.owner == Some(child.0))
                .unwrap()
                .cpu_mask = mask
        });
    }
    Ok(child)
}

// Publication d'un groupe IPC : le créateur détient déjà la garde de l'ensemble.
fn create_image(
    image: &'static [u8],
    data: &[u8],
    private_ro: Option<&[u8]>,
    wait_on: Option<Child>,
) -> Result<Child, Error> {
    let space =
        heap::with_pages(|m| UserSpace::new(m, image, data, private_ro)).map_err(Error::Memory)?;
    publish_image(space, wait_on)
}
fn publish_image(space: UserSpace, wait_on: Option<Child>) -> Result<Child, Error> {
    reap();
    let Some((slot, proc_slot)) = with(|s| {
        Some((
            s.slots.iter().position(Option::is_none)?,
            s.processes.slots.iter().position(Option::is_none)?,
        ))
    }) else {
        heap::with_pages(|m| {
            space.release(m);
            Ok(())
        })
        .unwrap();
        return Err(Error::Full);
    };
    let root = space.id();
    // Le masque IRQ reste en place de la réservation des slots à la publication
    // du couple PID/TCB. Le nouveau thread ne peut observer une racine par défaut.
    if let Err(e) = create(
        slot,
        enter,
        Priority::Normal,
        crate::smp::ONLINE.load(Ordering::Acquire),
    ) {
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
            terminal: 0,
            exit_counter: 0,
            outcome: None,
            preemptions: 0,
            preempted_on: 0,
        });
        s.thread_mut(slot).owner = Some(pid);
        s.thread_mut(slot).space = root;
        Child(pid)
    }))
}

/// Appelé exclusivement depuis une exception synchrone dont le cadre matériel
/// atteste le mode utilisateur. La pile privée du noyau est active, IRQ masquées.
pub(crate) fn finish(result: Outcome, pc: u64, level: &'static str) -> ! {
    let entered = hal::timer::counter();
    let (pid, trace_exit) = with(|s| {
        let pid = s
            .thread(s.current())
            .owner
            .expect("exception hors processus");
        crate::hardware::block::died(pid.number);
        s.ipc.death(pid, ticks());
        s.flush_ipc();
        s.ipc.collect();
        let p = s.processes.get_mut(pid);
        assert!(p.outcome.replace(result).is_none());
        p.exit_counter = entered;
        (
            pid,
            p.terminal == 0
                || p.terminal & crate::terminal::QUALIFY != 0
                || matches!(result, Outcome::Fault(_)),
        )
    });
    if trace_exit && !ipc::quiet() {
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
                if matches!(phase, "cli_repos" | "cli_enfant") {
                    hal::log(format_args!(
                        "CLI mem pid={} privees={} pile={}",
                        p.pid.number, data, stack
                    ));
                }
                r.user += data;
                r.user_stacks += stack;
            }
        }
        heap::measure(phase, r);
    });
}

// Les démos de calcul ne font aucun yield. Le contrôleur exige un compteur
// utilisateur en progrès sur chaque PE avant de libérer les processus.
pub(super) fn smp_users() {
    let n = crate::smp::count();
    let mut children = alloc::vec::Vec::new();
    assert!(n <= 4, "sonde dimensionnée pour 1 à 4 CPU");
    // Deux calculs par CPU imposent une permutation, y compris sur les AP.
    for i in 0..2 * n {
        let child = demo::spawn_pinned(i % n, 27 + i as u64);
        children.push(child);
    }
    let start = ticks();
    loop {
        let ready = with(|s| {
            children.iter().all(|child| {
                let p = s.processes.get(child.0);
                p.preemptions > 0
                    && heap::with_pages(|m| Ok(p.space.as_ref().unwrap().read_data(m, 3))).unwrap()
                        > 0
            })
        });
        if ready {
            break;
        }
        assert!(
            ticks() - start < u64::from(HZ) * 10,
            "processus utilisateur sans progression"
        );
        sleep(1);
    }
    for (i, child) in children.into_iter().enumerate() {
        let cpu = i % n;
        with(|s| {
            let p = s.processes.get(child.0);
            assert_eq!(p.preempted_on, 1 << cpu);
            let progress = heap::with_pages(|m| {
                let space = p.space.as_ref().unwrap();
                let count = space.read_data(m, 3);
                space.write_data(m, 4, 1);
                Ok(count)
            })
            .unwrap();
            hal::log(format_args!(
                "SMP utilisateur pid={} cpu={} preemptions={} progres={}",
                child.0.number, cpu, p.preemptions, progress
            ));
        });
        assert_eq!(child.join(), Outcome::Exit(27 + i as u64));
    }
}

#[cfg(feature = "user-fpu")]
pub(crate) fn fp_test() -> ! {
    let n = crate::smp::count();
    let image = include_bytes!(concat!(env!("OUT_DIR"), "/fp_probe.bin"));
    for round in 0..2 {
        let mut children = Vec::new();
        for i in 0..2 * n {
            let seed = 17 + (round * 2 * n + i) as u64;
            let mut config = [0u8; 16];
            config[8..].copy_from_slice(&seed.to_le_bytes());
            children
                .push(prepare_and_publish(image, &config, None, None, Some(1 << (i % n))).unwrap());
        }
        for phase in 0..2 {
            let deadline = ticks() + u64::from(HZ) * 5;
            for (i, c) in children.iter().enumerate() {
                let cpu = (i + phase) % n;
                let mut first = None;
                while with(|s| {
                    let p = s.processes.get(c.0);
                    assert!(
                        p.outcome.is_none(),
                        "programme FP terminé tôt {:?}",
                        p.outcome
                    );
                    let base = *first.get_or_insert(p.preemptions);
                    let progress =
                        heap::with_pages(|m| Ok(p.space.as_ref().unwrap().read_data(m, 3)))
                            .unwrap();
                    p.preemptions < base + 2 || p.preempted_on & (1 << cpu) == 0 || progress == 0
                }) {
                    assert!(ticks() < deadline, "préemption FP absente");
                    sleep(1);
                }
            }
            if phase == 0 && n > 1 {
                with(|s| {
                    for (i, c) in children.iter().enumerate() {
                        s.slots
                            .iter_mut()
                            .flatten()
                            .find(|t| t.owner == Some(c.0))
                            .unwrap()
                            .cpu_mask = 1 << ((i + 1) % n);
                    }
                });
            }
        }
        for (i, c) in children.into_iter().enumerate() {
            with(|s| {
                let p = s.processes.get(c.0);
                heap::with_pages(|m| {
                    p.space.as_ref().unwrap().write_data(m, 4, 1);
                    Ok(())
                })
                .unwrap();
            });
            assert_eq!(c.join(), Outcome::Exit(17 + (round * 2 * n + i) as u64));
        }
        hal::log(format_args!(
            "SMP FPU cycle={round} clients={} migrations={} OK",
            2 * n,
            if n > 1 { 2 * n } else { 0 }
        ));
    }
    hal::cpu::halt()
}
