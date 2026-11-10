//! Les TCB désignent le processus par PID. Le processus possède les pages,
//! le parent ne conserve qu’un identifiant de génération et récupère le statut.
use super::*;
use crate::{
    heap,
    memory::user::{self, UserSpace},
};

mod demo;
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
    // None pendant la vie du processus. Le statut reste disponible après collecte.
    outcome: Option<Outcome>,
    preemptions: u64,
}
pub(super) struct Table {
    slots: [Option<Process>; MAX],
    serial: u64,
}
impl Table {
    pub(super) const fn new() -> Self {
        Self {
            slots: [const { None }; MAX],
            serial: 0,
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
    pub(super) fn space(&self, pid: Pid) -> SpaceId {
        self.get(pid).space.as_ref().unwrap().id()
    }
    pub(super) fn preempted(&mut self, pid: Pid) {
        self.get_mut(pid).preemptions += 1;
    }
}
fn enter() {
    let _irq = Guard::new();
    let top = with(|s| s.thread(s.current).stack.top);
    // Sûreté : thread utilisateur fraîchement publié ; code RX et pile RW/NX
    // validés, racine active, IRQ masquées jusqu’au retour matériel.
    unsafe { context::enter_user(top, user::CODE as usize, user::TOP as usize) }
}

/// Droit noyau de suivre ce fils et de récolter son statut. Le PID numérique
/// sert au diagnostic ; il ne donne pas accès à ce droit depuis un programme.
struct Child(Pid);

fn spawn_image(image: &[u8], data: &[u8], private_ro: Option<&[u8]>) -> Result<Child, Error> {
    let guard = Guard::new();
    if !guard.was_enabled() {
        return Err(Error::Critical);
    }
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
    Ok(with(|s| {
        s.processes.serial = s.processes.serial.checked_add(1).expect("PID épuisés");
        let pid = Pid {
            slot: proc_slot,
            number: s.processes.serial,
        };
        s.processes.slots[proc_slot] = Some(Process {
            pid,
            space: Some(space),
            threads: 1,
            observed: true,
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
        let p = s.processes.get_mut(pid);
        assert!(p.outcome.replace(result).is_none());
        pid
    });
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
            if !p.observed {
                s.processes.slots[pid.slot] = None;
            }
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
        with(|s| {
            let p = s.processes.get_mut(self.0);
            p.observed = false;
            if p.space.is_none() {
                s.processes.slots[self.0.slot] = None;
            }
        });
    }
}
