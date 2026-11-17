//! Le serveur P9 reste seul écrivain. Le noyau conserve les capacités locales,
//! l’identité des demandeurs et une seule opération remise à la fois au serveur.
use super::*;
use crate::abi::{
    BAD_ADDRESS, BAD_ARGUMENT, CANCELLED, CLOSED, DENIED, LIMIT, NO_MEMORY, OK, REFUSED, REVOKED,
    TIMED_OUT, WOULD_BLOCK,
};
#[allow(dead_code)]
#[path = "../../../../abi/files.rs"]
mod files;
#[allow(dead_code)]
#[path = "../../../../abi/tmp.rs"]
pub(super) mod wire;

#[path = "tmp_wait_probe.rs"]
pub(crate) mod wait_probe;

type Result<T> = core::result::Result<T, u64>;
const TIMEOUT: u64 = 15000;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Stage {
    Create,
    Live,
    Failed(u64),
    Remove,
}

struct Lease {
    instance: Instance,
    cap: u64,
    stage: Stage,
    closing: bool,
    managed: bool,
}

struct Request {
    ticket: u64,
    owner: Pid,
    instance: Instance,
    address: Option<u64>,
    frame: [u8; wire::FRAME],
    done: Option<u64>,
}

#[derive(Clone, Copy)]
struct Flight {
    ticket: u64,
    operation: u64,
    instance: Instance,
}

pub(super) struct Broker {
    server: Option<Pid>,
    storage_profile: bool,
    receiver: u64,
    serial: u64,
    leases: Vec<Lease>,
    requests: Vec<Request>,
    flight: Option<Flight>,
}

impl Broker {
    pub(super) const fn new() -> Self {
        Self {
            server: None,
            storage_profile: false,
            receiver: 0,
            serial: 0,
            leases: Vec::new(),
            requests: Vec::new(),
            flight: None,
        }
    }

    pub(super) fn has_service_lease(&self, instance: Option<Instance>) -> bool {
        self.leases.iter().any(|l| l.managed && instance.is_none_or(|k| l.instance == k))
    }

    pub(super) fn serves(&self, pid: Pid) -> bool {
        self.server == Some(pid)
    }

    pub(super) fn online(&self) -> bool {
        self.server.is_some()
    }

    pub(super) fn diskless_profile(&self) -> bool {
        !self.storage_profile
    }

    fn issue(&mut self) -> Result<u64> {
        self.serial = self.serial.checked_add(1).filter(|n| *n < 1 << 63).ok_or(LIMIT)?;
        Ok(self.serial)
    }

    fn release_empty_queue(&mut self) {
        if self.server.is_none() && self.requests.is_empty() {
            self.requests = Vec::new();
        }
    }

    fn live(&self, key: Instance, cap: u64) -> bool {
        cap != 0
            && self
                .leases
                .iter()
                .any(|l| l.instance == key && l.cap == cap && l.stage == Stage::Live && !l.closing)
    }

    // Après remise à l’écrivain, une annulation locale ne prouve pas que
    // l’opération a été interrompue avant sa publication.
    fn interruption(&self, ticket: u64, status: u64) -> u64 {
        if self.flight.is_some_and(|f| f.ticket == ticket) {
            files::UNCERTAIN
        } else {
            status
        }
    }

    fn offer(&mut self) -> Result<(Flight, [u8; wire::FRAME])> {
        if self.flight.is_some() {
            return Err(REFUSED);
        }
        // Les fermetures précèdent les nouvelles admissions. Un seul écrivain
        // traite toute opération déjà remise avant la suppression d’une racine.
        for (stage, operation) in [(Stage::Remove, wire::REMOVE), (Stage::Create, wire::CREATE)] {
            if let Some(l) = self.leases.iter().find(|l| l.stage == stage) {
                let instance = l.instance;
                return Ok((
                    Flight {
                        instance,
                        operation,
                        ticket: self.issue()?,
                    },
                    [0; wire::FRAME],
                ));
            }
        }
        // Les requêtes sont en ordre d’admission (remove, jamais swap_remove).
        let r = self
            .requests
            .iter()
            .find(|r| r.done.is_none())
            .ok_or(WOULD_BLOCK)?;
        Ok((
            Flight {
                ticket: r.ticket,
                operation: wire::IO,
                instance: r.instance,
            },
            r.frame,
        ))
    }
}

fn owner(s: &Core) -> Pid {
    s.thread(s.current()).owner.unwrap()
}

pub(super) fn copy_in<const N: usize>(s: &Core, pid: Pid, addr: u64, writable: bool) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    let space = s.processes.get(pid).space.as_ref().ok_or(CLOSED)?;
    let ok = heap::with_pages(|m| {
        Ok((!writable || space.writable(m, addr, N)) && space.copy_from_user(m, addr, &mut bytes))
    })
    .unwrap();
    ok.then_some(bytes).ok_or(BAD_ADDRESS)
}

pub(super) fn copy_out(s: &Core, pid: Pid, addr: u64, bytes: &[u8]) -> Result<()> {
    let space = s.processes.get(pid).space.as_ref().ok_or(CLOSED)?;
    let ok = heap::with_pages(|m| Ok(space.copy_to_user(m, addr, bytes))).unwrap();
    ok.then_some(()).ok_or(BAD_ADDRESS)
}

pub(super) fn wake(s: &mut Core, ticket: u64) {
    for slot in 0..s.slots.len() {
        if s.slots[slot].as_ref().is_some_and(
            |t| matches!(t.state, State::Blocked(Wait::Temporary { token, .. }) if token == ticket),
        ) {
            s.wake(slot);
        }
    }
}

pub(super) fn notice(s: &mut Core) {
    if let Some(server) = s.processes.temporary.server {
        let rx = s.processes.temporary.receiver;
        if rx != 0 {
            s.ipc.tmp_notice(server, rx);
            s.flush_ipc();
        }
    }
}

pub(super) fn new_lease(s: &mut Core, pid: Pid) -> Result<u64> {
    let instance = s.processes.get(pid).instance;
    instance
        .identity(crate::boot_id::current())
        .ok_or(REFUSED)?;
    let managed = ipc::service::owns_family(s, pid);
    let broker = &mut s.processes.temporary;
    if broker.server.is_none() {
        return Err(CLOSED);
    }
    if broker.leases.len() >= MAX {
        return Err(LIMIT);
    }
    broker.leases.try_reserve(1).map_err(|_| NO_MEMORY)?;
    let cap = broker.issue()?;
    broker.leases.push(Lease {
        instance,
        cap,
        stage: Stage::Create,
        closing: false,
        managed,
    });
    s.processes.get_mut(pid).tmp_cap = cap;
    notice(s);
    Ok(cap)
}

pub(super) fn install(s: &mut Core, server: Pid, shell: Pid) -> Result<()> {
    if s.processes.temporary.server.is_some() {
        return Err(DENIED);
    }
    s.processes.temporary.storage_profile = true;
    s.processes.temporary.server = Some(server);
    if let Err(status) = new_lease(s, shell) {
        s.processes.temporary.server = None;
        return Err(status);
    }
    Ok(())
}

pub(super) fn removal_acknowledged(key: Instance) -> bool {
    with(|s| !s.processes.temporary.leases.iter().any(|lease| lease.instance == key))
}

pub(super) fn inaccessible(s: &Core, pid: Pid, cap: u64) -> bool {
    authority(s, pid, cap).is_err()
}

pub(crate) fn ready_shell() -> bool {
    with(|s| {
        s.processes.slots.iter().flatten().any(|p| {
            p.outcome.is_none()
                && s.processes.instances.app(p.instance) == Some(iw::SHELL)
                && s.processes.temporary.live(p.instance, p.tmp_cap)
        })
    })
}

fn wait_live(pid: Pid, cap: u64) -> Result<()> {
    let deadline = ticks().saturating_add(TIMEOUT);
    loop {
        let r = with(|s| {
            if s.thread(s.current())
                .owner
                .is_some_and(|caller| s.processes.get(caller).outcome.is_some())
            {
                return Err(CANCELLED);
            }
            let p = s.processes.get(pid);
            let l = s
                .processes
                .temporary
                .leases
                .iter()
                .find(|l| l.instance == p.instance && l.cap == cap && !l.closing)
                .ok_or(CLOSED)?;
            match l.stage {
                Stage::Create if ticks() >= deadline => Err(TIMED_OUT),
                Stage::Create => Ok(false),
                Stage::Live => Ok(true),
                Stage::Failed(code) => Err(code),
                Stage::Remove => Err(REVOKED),
            }
        })?;
        if r {
            return Ok(());
        }
        dispatch(Reason::Block(Wait::Temporary {
            token: cap,
            deadline,
        }));
    }
}

// Le lanceur garde le TCB neuf hors de la file prête pendant que NovaFS écrit
// la racine. Il rétablit cet état sous la garde avant de publier les autres droits.
pub(super) fn prepare(pid: Pid) -> Result<()> {
    let (cap, slot) = with(|s| -> Result<_> {
        let cap = new_lease(s, pid)?;
        let slot = s
            .slots
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.owner == Some(pid)))
            .unwrap();
        assert_eq!(s.thread(slot).state, State::Ready);
        for _ in 0..s.ready.len {
            let k = s.ready.pop().unwrap();
            if k != slot {
                s.ready.push(k)
            }
        }
        s.thread_mut(slot).state = State::Blocked(Wait::TmpStart);
        Ok((cap, slot))
    })?;
    let result = wait_live(pid, cap);
    with(|s| s.wake(slot));
    result
}

// Le shell est publié en groupe avec la chaîne bloc. Cette barrière laisse
// démarrer le serveur et le pilote, mais aucune instruction utilisateur du shell.
pub(super) fn before_entry() {
    let waiting = with(|s| {
        let pid = owner(s);
        let p = s.processes.get(pid);
        (p.principal && p.tmp_cap != 0 && !s.processes.temporary.live(p.instance, p.tmp_cap))
            .then_some((pid, p.tmp_cap))
    });
    if let Some((pid, cap)) = waiting
        && let Err(e) = wait_live(pid, cap)
    {
        with(|s| {
            if s.processes.get(pid).outcome.is_none() {
                stop_process(s, pid, Outcome::Exit(e), hal::timer::counter());
            }
        });
        dispatch(Reason::Exit);
        unreachable!();
    }
}

fn authority(s: &Core, pid: Pid, cap: u64) -> Result<Instance> {
    let p = s.processes.get(pid);
    if p.tmp_cap != cap || !s.processes.temporary.live(p.instance, cap) || p.outcome.is_some() {
        return Err(DENIED);
    }
    Ok(p.instance)
}

pub(super) fn delegate(s: &mut Core, parent: Pid, child: Pid) -> Result<()> {
    let cap = s.processes.get(parent).tmp_cap;
    let family = authority(s, parent, cap)?;
    if s.processes.get(child).instance != family {
        return Err(DENIED);
    }
    s.processes.get_mut(child).tmp_cap = cap;
    Ok(())
}

fn io(pid: Pid, cap: u64, address: u64) -> Result<u64> {
    let ticket = with(|s| -> Result<_> {
        let instance = authority(s, pid, cap)?;
        if threads::cancelled(s) {
            return Err(CANCELLED);
        }
        let frame = copy_in(s, pid, address, true)?;
        let broker = &mut s.processes.temporary;
        if broker.requests.len() >= MAX {
            return Err(LIMIT);
        }
        broker.requests.try_reserve(1).map_err(|_| NO_MEMORY)?;
        let ticket = broker.issue()?;
        broker.requests.push(Request {
            ticket,
            owner: pid,
            instance,
            address: Some(address),
            frame,
            done: None,
        });
        notice(s);
        Ok(ticket)
    })?;
    let deadline = ticks().saturating_add(TIMEOUT);
    loop {
        let ready = with(|s| -> Result<_> {
            // Le ticket de cette pile identifie une demande déjà admise. Le
            // retrait du bail interdit les appels suivants, mais son résultat
            // reste consommable une seule fois par le demandeur encore vivant.
            threads::status(s)?;
            let cancelled = threads::cancelled(s);
            let broker = &mut s.processes.temporary;
            let index = broker
                .requests
                .iter()
                .position(|r| r.ticket == ticket && r.owner == pid)
                .ok_or(REVOKED)?;
            if let Some(status) = broker.requests[index].done {
                let r = broker.requests.remove(index);
                broker.release_empty_queue();
                if status != OK {
                    return Err(status);
                }
                copy_out(s, pid, r.address.ok_or(BAD_ADDRESS)?, &r.frame)?;
                return Ok(true);
            }
            let reason = if broker.requests[index].address.is_none() {
                BAD_ADDRESS
            } else if cancelled {
                CANCELLED
            } else if ticks() >= deadline {
                TIMED_OUT
            } else {
                return Ok(false);
            };
            broker.requests.remove(index);
            Err(broker.interruption(ticket, reason))
        })?;
        if ready {
            return Ok(wire::FRAME as u64);
        }
        dispatch(Reason::Block(Wait::Temporary {
            token: ticket,
            deadline,
        }));
    }
}

fn next(s: &mut Core, pid: Pid, rx: u64, address: u64) -> Result<u64> {
    if s.processes.temporary.server != Some(pid) {
        return Err(DENIED);
    }
    s.ipc.tmp_arm(pid, rx)?;
    s.processes.temporary.receiver = rx;
    let (f, frame) = s.processes.temporary.offer()?;
    let mut envelope = [0; wire::ENVELOPE];
    envelope[..32].copy_from_slice(
        &f.instance
            .identity(crate::boot_id::current())
            .ok_or(REFUSED)?
            .encode(),
    );
    envelope[32..40].copy_from_slice(&f.operation.to_le_bytes());
    envelope[40..].copy_from_slice(&frame);
    copy_out(s, pid, address, &envelope)?;
    s.processes.temporary.flight = Some(f);
    Ok(f.ticket)
}

fn complete(s: &mut Core, pid: Pid, ticket: u64, address: u64, status: u64) -> Result<u64> {
    let broker = &s.processes.temporary;
    if broker.server != Some(pid) {
        return Err(DENIED);
    }
    let f = broker
        .flight
        .filter(|f| f.ticket == ticket)
        .ok_or(REVOKED)?;
    if status > 255 || (f.operation != wire::IO && address != 0) {
        return Err(BAD_ARGUMENT);
    }
    let reply = if f.operation == wire::IO && status == OK {
        Some(copy_in(s, pid, address, false)?)
    } else {
        None
    };
    let broker = &mut s.processes.temporary;
    broker.flight = None;
    if f.operation == wire::IO {
        if let Some(r) = broker.requests.iter_mut().find(|r| r.ticket == ticket)
            && r.done.is_none()
        {
            if let Some(frame) = reply {
                r.frame = frame
            }
            r.done = Some(status);
        }
        wake(s, ticket);
    } else {
        let i = broker
            .leases
            .iter()
            .position(|l| l.instance == f.instance)
            .ok_or(REVOKED)?;
        let cap = broker.leases[i].cap;
        let closing = broker.leases[i].closing;
        if f.operation == wire::REMOVE {
            let lease = broker.leases.swap_remove(i);
            if lease.managed && status != OK
                && let Some(session) = &mut s.processes.manager
            {
                session.purge_error.get_or_insert(status);
            }
            hal::log(format_args!(
                "TMP retrait instance={} statut={status}",
                f.instance.serial
            ));
        } else if closing && status != OK {
            broker.leases.swap_remove(i);
        } else {
            broker.leases[i].stage = match status {
                OK if closing => Stage::Remove,
                OK => Stage::Live,
                e => Stage::Failed(e),
            };
        }
        wake(s, cap);
    }
    Ok(0)
}

pub(super) fn call(id: u64, a: u64, b: u64, c: u64) -> Result<u64> {
    let _guard = Guard::new();
    let pid = with(|s| owner(s));
    with(|s| threads::status(s))?;
    match id {
        wire::CAP if [a, b, c] == [0; 3] => with(|s| {
            let cap = s.processes.get(pid).tmp_cap;
            authority(s, pid, cap).map(|_| cap)
        }),
        wire::CALL if c == wire::FRAME as u64 => io(pid, a, b),
        wire::NEXT if c == wire::ENVELOPE as u64 => with(|s| next(s, pid, a, b)),
        wire::COMPLETE => with(|s| complete(s, pid, a, b, c)),
        _ => Err(BAD_ARGUMENT),
    }
}

pub(super) fn invalidate(s: &mut Core, pid: Pid, start: u64, end: u64) {
    for i in 0..s.processes.temporary.requests.len() {
        let broker = &mut s.processes.temporary;
        let r = &mut broker.requests[i];
        if r.owner == pid
            && r.address
                .is_some_and(|a| a < end && start < a.saturating_add(wire::FRAME as u64))
        {
            // Ne jamais copier la réponse dans une future page à cette adresse.
            r.address = None;
            let ticket = r.ticket;
            if broker.flight.is_none_or(|f| f.ticket != ticket) {
                // Une requête encore en file ne pourra plus être remise.
                r.done.get_or_insert(BAD_ADDRESS);
            }
            wake(s, ticket);
        }
    }
}

pub(super) fn retired(s: &mut Core, pid: Pid, outcome: Option<Outcome>) {
    if s.processes.temporary.server == Some(pid) {
        let broker = &mut s.processes.temporary;
        // Un refus de format du serveur avant son premier NEXT explique les
        // créations en attente. Conserver cette erreur jusqu’au retrait du
        // lanceur. Aucune opération n’a encore pu être remise au service.
        if broker.receiver == 0 && outcome == Some(Outcome::Exit(files::UNSUPPORTED)) {
            broker.leases.retain_mut(|lease| {
                if lease.stage == Stage::Create && !lease.closing {
                    lease.stage = Stage::Failed(files::UNSUPPORTED);
                    true
                } else {
                    false
                }
            });
        } else {
            broker.leases = Vec::new();
        }
        broker.server = None;
        broker.receiver = 0;
        let in_flight = broker.flight.take().map(|f| f.ticket);
        for r in &mut broker.requests {
            // Un seul NEXT a pu remettre une E/S. Un COMPLETE déjà enregistré
            // conserve son statut et sa trame, y compris après réinstallation.
            r.done.get_or_insert(if in_flight == Some(r.ticket) {
                files::UNCERTAIN
            } else {
                CLOSED
            });
        }
        broker.release_empty_queue();
        for i in 0..s.slots.len() {
            if s.slots[i]
                .as_ref()
                .is_some_and(|t| matches!(t.state, State::Blocked(Wait::Temporary { .. })))
            {
                s.wake(i)
            }
        }
        return;
    }
    let p = s.processes.get(pid);
    let (key, principal, cap) = (p.instance, p.principal, p.tmp_cap);
    if cap == 0 {
        return;
    }
    s.processes.get_mut(pid).tmp_cap = 0;
    let broker = &mut s.processes.temporary;
    broker.requests.retain(|r| r.owner != pid);
    broker.release_empty_queue();
    if !principal {
        return;
    }
    if let Some(i) = broker.leases.iter().position(|l| l.instance == key) {
        broker.leases[i].closing = true;
        match broker.leases[i].stage {
            Stage::Live => broker.leases[i].stage = Stage::Remove,
            Stage::Create
                if broker
                    .flight
                    .is_some_and(|f| f.instance == key && f.operation == wire::CREATE) =>
            {
                ()
            }
            Stage::Create | Stage::Failed(_) => {
                broker.leases.swap_remove(i);
            }
            Stage::Remove => (),
        }
        wake(s, cap);
        notice(s);
    }
}
