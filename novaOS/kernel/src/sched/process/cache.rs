//! Le courtier identifie les appelants par leur processus et leur incarnation.
//! L'unique écrivain P9 traite les tickets en ordre et vérifie l'OID sur disque.
use super::*;
use crate::abi::{
    BAD_ADDRESS, BAD_ARGUMENT, CANCELLED, CLOSED, DENIED, LIMIT, NO_MEMORY, OK, REFUSED, REVOKED,
    TIMED_OUT, WOULD_BLOCK,
};

#[allow(dead_code)]
#[path = "../../../../abi/files.rs"]
mod files;
#[allow(dead_code)]
#[path = "../../../../abi/cache.rs"]
pub(super) mod wire;
use wire::*;

type Result<T> = core::result::Result<T, u64>;
type Frame = [u8; FRAME];
type Key = [u8; 32];
const TIMEOUT: u64 = 15_000;

#[derive(Clone, Copy)]
struct View {
    owner: Pid,
    instance: Instance,
    key: Key,
    generation: u64,
    cap: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Queued,
    Sent,
    Done(u64),
}

struct Job {
    ticket: u64,
    op: u64,
    owner: Option<Pid>,
    address: Option<u64>,
    stage: Stage,
    // Le résultat d'une écriture remise est indépendant d'une révocation ultérieure.
    // Un OPEN remis ne peut plus attribuer un droit si une purge a été admise.
    revoked: bool,
    key: Key,
    frame: Frame,
}

fn mutates_storage(op: u64) -> bool {
    matches!(
        op,
        OPEN | WRITE | PURGE | PURGE_USER | PURGE_SYSTEM | AUTO_PURGE | RECOVER
    )
}

fn covers(op: u64, scope: &Key, key: &Key) -> bool {
    match op {
        PURGE | AUTO_PURGE => scope == key,
        PURGE_USER => scope[..16] == key[..16],
        PURGE_SYSTEM => true,
        _ => false,
    }
}

pub(super) struct Broker {
    serial: u64,
    supervisor: Option<Pid>,
    views: Vec<View>,
    jobs: Vec<Job>,
    proposal: Option<(Key, u64)>,
    peak: usize,
}

impl Broker {
    pub(super) const fn new() -> Self {
        Self {
            serial: 1 << 63,
            supervisor: None,
            views: Vec::new(),
            jobs: Vec::new(),
            proposal: None,
            peak: 0,
        }
    }

    pub(super) fn drained(&self) -> bool {
        self.views.capacity() == 0
            && self.jobs.capacity() == 0
            && self.supervisor.is_none()
            && self.proposal.is_none()
    }

    fn measure(&mut self) {
        let bytes = self.views.capacity() * core::mem::size_of::<View>()
            + self.jobs.capacity() * core::mem::size_of::<Job>();
        self.peak = self.peak.max(bytes);
    }

    pub(super) fn peak(&self) -> usize {
        self.peak
    }

    fn issue(&mut self) -> Result<u64> {
        self.serial = self.serial.checked_add(1).ok_or(LIMIT)?;
        Ok(self.serial)
    }

    fn pending_purge(&self, key: &Key) -> bool {
        self.jobs
            .iter()
            .any(|j| !matches!(j.stage, Stage::Done(_)) && covers(j.op, &j.key, key))
    }

    fn compact(&mut self) {
        self.jobs
            .retain(|j| j.owner.is_some() || j.stage == Stage::Sent);
        if self.jobs.is_empty() {
            self.jobs = Vec::new();
        }
        if self.views.is_empty() {
            self.views = Vec::new();
        }
    }
}

fn role(s: &Core, pid: Pid, required: u64) -> Result<(u64, u64)> {
    let session = s.processes.manager.ok_or(DENIED)?;
    let p = s.processes.get(pid);
    if !p.principal || p.service_owner != Some(session.manager) || p.outcome.is_some()
        || p.service_caps & required != required
    {
        return Err(DENIED);
    }
    Ok((session.profile, p.service_id))
}

// Cette table est l'autorité de provisionnement du lanceur d'essai. Elle est
// inatteignable aux processus du terminal ordinaire ; aucune chaîne, session
// interactive ou valeur de PID ne participe à la construction d'une identité.
fn provision(profile: u64, actor: u64) -> Result<Key> {
    if !matches!(profile, 78..=80 | 83..=86) {
        return Err(DENIED);
    }
    let (user, installation): (u128, u128) = match actor {
        11 | 12 => (1, 1),
        13 => (1, 2),
        14 => (2, 1),
        15 => (1, 3),
        _ => return Err(DENIED),
    };
    let mut key = [0; 32];
    key[..16].copy_from_slice(&user.to_le_bytes());
    key[16..].copy_from_slice(&installation.to_le_bytes());
    Ok(key)
}

fn admit(s: &mut Core, pid: Pid, op: u64, address: u64) -> Result<u64> {
    if threads::cancelled(s) {
        return Err(CANCELLED);
    }
    let service_op = matches!(op, RECOVER | PRESSURE | AUTO_PURGE);
    let required = if service_op { service_caps::CACHE_BACKEND } else { service_caps::CACHE_CLIENT };
    let (profile, actor) = role(s, pid, required)?;
    let mut key = if service_op {
        if actor != 10 {
            return Err(DENIED);
        }
        [0; 32]
    } else {
        let key = provision(profile, actor)?;
        if matches!(op, PURGE_USER | PURGE_SYSTEM) && actor != 15 {
            return Err(DENIED);
        }
        key
    };
    let mut frame: Frame = tmp::copy_in(s, pid, address, true)?;
    if frame[..GEN]
        .iter()
        .chain(&frame[OP..NAME_LEN])
        .any(|b| *b != 0)
    {
        return Err(BAD_ARGUMENT);
    }
    let boot = crate::boot_id::current().ok_or(REFUSED)?;
    let instance = s.processes.get(pid).instance;
    let broker = &mut s.processes.cache;
    if !s.processes.temporary.online() {
        return Err(CLOSED);
    }
    if op == RECOVER {
        if broker.supervisor.is_some()
            || broker
                .jobs
                .iter()
                .any(|j| j.op == RECOVER && !matches!(j.stage, Stage::Done(_)))
        {
            return Err(REFUSED);
        }
        if frame[128..].iter().any(|b| *b != 0) {
            return Err(BAD_ARGUMENT);
        }
        frame[128] = u8::from(!matches!(profile, 78..=80 | 83..=86));
    } else if broker.supervisor.is_none() || (service_op && broker.supervisor != Some(pid)) {
        return Err(CLOSED);
    }
    if matches!(op, OPEN | RECOVER | PRESSURE | AUTO_PURGE) {
        if frame[GEN..OP].iter().any(|v| *v != 0) {
            return Err(BAD_ARGUMENT);
        }
    } else {
        let view = broker
            .views
            .iter()
            .find(|v| {
                v.owner == pid
                    && v.instance == instance
                    && v.key == key
                    && v.generation == get(&frame, GEN)
                    && v.cap == get(&frame, CAP)
                    && frame[BOOT..OP] == boot
            })
            .ok_or(REVOKED)?;
        put(&mut frame, GEN, view.generation);
    }
    if op == AUTO_PURGE {
        let (selected, generation) = broker.proposal.ok_or(WOULD_BLOCK)?;
        key = selected;
        put(&mut frame, GEN, generation);
    }
    if !service_op && broker.pending_purge(&key) {
        return Err(REVOKED);
    }
    if broker.jobs.len() >= MAX {
        return Err(LIMIT);
    }
    broker.jobs.try_reserve(1).map_err(|_| NO_MEMORY)?;
    let ticket = broker.issue()?;
    let mut wakes = [0; MAX];
    if matches!(op, PURGE | PURGE_USER | PURGE_SYSTEM | AUTO_PURGE) {
        broker.proposal = None;
        broker.views.retain(|v| !covers(op, &key, &v.key));
        for (i, j) in broker.jobs.iter_mut().enumerate() {
            if matches!(j.op, OPEN | READ | WRITE) && covers(op, &key, &j.key) {
                j.revoked = true;
                if j.stage == Stage::Queued {
                    j.stage = Stage::Done(REVOKED);
                    wakes[i] = j.ticket;
                }
            }
        }
    }
    frame[..GEN].copy_from_slice(&key);
    frame[CAP..OP].fill(0);
    put(&mut frame, OP, op);
    broker.jobs.push(Job {
        ticket,
        op,
        owner: Some(pid),
        address: Some(address),
        stage: Stage::Queued,
        revoked: false,
        key,
        frame,
    });
    broker.measure();
    for token in wakes.into_iter().filter(|v| *v != 0) {
        tmp::wake(s, token);
    }
    tmp::notice(s);
    Ok(ticket)
}

fn next(s: &mut Core, pid: Pid, rx: u64, address: u64) -> Result<u64> {
    if !s.processes.temporary.serves(pid) {
        return Err(DENIED);
    }
    s.ipc.tmp_arm(pid, rx)?;
    let b = &s.processes.cache;
    if b.jobs.iter().any(|j| j.stage == Stage::Sent) {
        return Err(REFUSED);
    }
    let index = b
        .jobs
        .iter()
        .position(|j| j.stage == Stage::Queued)
        .ok_or(WOULD_BLOCK)?;
    tmp::copy_out(s, pid, address, &b.jobs[index].frame)?;
    let b = &mut s.processes.cache;
    b.jobs[index].stage = Stage::Sent;
    Ok(b.jobs[index].ticket)
}

fn complete(s: &mut Core, pid: Pid, ticket: u64, address: u64) -> Result<u64> {
    if !s.processes.temporary.serves(pid) {
        return Err(DENIED);
    }
    let frame: Frame = tmp::copy_in(s, pid, address, false)?;
    let status = get(&frame, STATUS);
    if status > 255 {
        return Err(BAD_ARGUMENT);
    }
    let b = &mut s.processes.cache;
    let i = b
        .jobs
        .iter()
        .position(|j| j.ticket == ticket && j.stage == Stage::Sent)
        .ok_or(REVOKED)?;
    let j = &b.jobs[i];
    let (op, key, owner) = (j.op, j.key, j.owner);
    if get(&frame, OP) != op || (op != PRESSURE && frame[..GEN] != key) {
        return Err(BAD_ARGUMENT);
    }
    let mut reported = status;
    let mut result = [0u8; FRAME];
    if status == OK {
        match op {
            RECOVER if !j.revoked => {
                b.supervisor = owner;
            }
            OPEN if j.revoked => reported = REVOKED,
            OPEN if owner.is_some() => {
                let owner = owner.unwrap();
                // Les jobs d'un processus libéré ont déjà perdu leur owner.
                let process = s.processes.slots[owner.slot].as_ref().ok_or(REVOKED)?;
                if process.pid != owner {
                    return Err(REVOKED);
                }
                let instance = process.instance;
                let generation = get(&frame, GEN);
                if generation == 0 {
                    return Err(BAD_ARGUMENT);
                }
                let cap = if let Some(v) = b.views.iter().find(|v| {
                    v.owner == owner
                        && v.instance == instance
                        && v.generation == generation
                        && v.key == key
                }) {
                    Ok(v.cap)
                } else {
                    let fresh = b.issue()?;
                    b.views.retain(|v| v.owner != owner);
                    b.views
                        .try_reserve(1)
                        .map(|()| {
                            b.views.push(View {
                                owner,
                                instance,
                                key,
                                generation,
                                cap: fresh,
                            });
                            fresh
                        })
                        .map_err(|_| NO_MEMORY)
                };
                match cap {
                    Ok(cap) => {
                        put(&mut result, GEN, generation);
                        put(&mut result, CAP, cap);
                        result[BOOT..OP]
                            .copy_from_slice(&crate::boot_id::current().ok_or(REFUSED)?);
                    }
                    Err(code) => reported = code,
                }
            }
            READ | WRITE => {
                // L'identité filaire et l'autorité de maintenance restent au courtier.
                result[NAME_LEN..].copy_from_slice(&frame[NAME_LEN..]);
            }
            PRESSURE if !j.revoked && b.supervisor == owner => {
                let victim: Key = frame[..GEN].try_into().unwrap();
                let generation = get(&frame, GEN);
                if generation == 0 || victim[..16] == [0; 16] || victim[16..] == [0; 16] {
                    return Err(BAD_ARGUMENT);
                }
                b.proposal = Some((victim, generation));
            }
            _ => (),
        }
    } else if op == PRESSURE {
        b.proposal = None;
    }
    b.measure();
    b.jobs[i].stage = Stage::Done(reported);
    b.jobs[i].frame = result;
    b.compact();
    tmp::wake(s, ticket);
    tmp::notice(s);
    Ok(0)
}

fn await_result(pid: Pid, ticket: u64) -> Result<u64> {
    let deadline = ticks().saturating_add(TIMEOUT);
    loop {
        let done = with(|s| -> Result<bool> {
            threads::status(s)?;
            let cancelled = threads::cancelled(s);
            let b = &mut s.processes.cache;
            let i = b
                .jobs
                .iter()
                .position(|j| j.ticket == ticket && j.owner == Some(pid))
                .ok_or(REVOKED)?;
            let j = &mut b.jobs[i];
            if let Stage::Done(status) = j.stage {
                let j = b.jobs.remove(i);
                b.compact();
                if status != OK {
                    return Err(status);
                }
                // Une écriture acquittée reste un succès même si le client a depuis
                // retiré le tampon d'entrée. Seuls OPEN et READ ont une sortie.
                if matches!(j.op, OPEN | READ) {
                    tmp::copy_out(s, pid, j.address.ok_or(BAD_ADDRESS)?, &j.frame)?;
                }
                return Ok(true);
            }
            let reason = if j.address.is_none() {
                BAD_ADDRESS
            } else if cancelled {
                CANCELLED
            } else if ticks() >= deadline {
                TIMED_OUT
            } else {
                return Ok(false);
            };
            let uncertain = j.stage == Stage::Sent && mutates_storage(j.op);
            j.owner = None;
            j.revoked = true;
            b.compact();
            Err(if uncertain { files::UNCERTAIN } else { reason })
        })?;
        if done {
            return Ok(0);
        }
        dispatch(Reason::Block(Wait::Temporary {
            token: ticket,
            deadline,
        }));
    }
}

pub(super) fn control(op: u64, b: u64, c: u64) -> Result<u64> {
    let _guard = Guard::new();
    let pid = with(|s| s.thread(s.current()).owner.unwrap());
    with(|s| threads::status(s))?;
    match op {
        NEXT => with(|s| next(s, pid, b, c)),
        COMPLETE => with(|s| complete(s, pid, b, c)),
        TEST_WRITE_SENT | TEST_PURGE_PENDING if c == 0 => with(|s| probe(s, pid, op, b)),
        OPEN..=RECOVER | PRESSURE | AUTO_PURGE if b == 0 => {
            let ticket = with(|s| admit(s, pid, op, c))?;
            await_result(pid, ticket)
        }
        _ => Err(BAD_ARGUMENT),
    }
}

// Le témoin voit uniquement la remise de l'écriture de sa propre installation.
// Aucun droit matériel, PID ou adresse utilisateur n'est communiqué.
fn probe(s: &Core, pid: Pid, op: u64, b: u64) -> Result<u64> {
    if s.processes.manager.is_none_or(|m| m.profile != 78) {
        return Err(DENIED);
    }
    let cache = &s.processes.cache;
    if op == TEST_WRITE_SENT {
        let (profile, actor) = role(s, pid, service_caps::CACHE_CLIENT)?;
        if actor != 11 || b != 0 {
            return Err(DENIED);
        }
        let key = provision(profile, actor)?;
        return cache
            .jobs
            .iter()
            .any(|j| {
                j.key == key
                    && j.op == WRITE
                    && j.stage == Stage::Sent
                    && j.frame[NAME_LEN] == 6
                    && &j.frame[NAME..NAME + 6] == b"course"
            })
            .then_some(0)
            .ok_or(WOULD_BLOCK);
    }
    if !s.processes.temporary.serves(pid) {
        return Err(DENIED);
    }
    let j = cache
        .jobs
        .iter()
        .find(|j| j.ticket == b && j.op == WRITE && j.stage == Stage::Sent)
        .ok_or(REVOKED)?;
    cache.pending_purge(&j.key).then_some(0).ok_or(WOULD_BLOCK)
}

pub(super) fn invalidate(s: &mut Core, pid: Pid, start: u64, end: u64) {
    for i in 0..s.processes.cache.jobs.len() {
        let j = &mut s.processes.cache.jobs[i];
        if j.owner == Some(pid)
            && j.address
                .is_some_and(|a| a < end && start < a.saturating_add(FRAME as u64))
        {
            j.address = None;
            if j.stage == Stage::Queued {
                j.stage = Stage::Done(BAD_ADDRESS);
            }
            let ticket = j.ticket;
            tmp::wake(s, ticket);
        }
    }
}

pub(super) fn retired(s: &mut Core, pid: Pid) {
    let storage = s.processes.temporary.serves(pid);
    let b = &mut s.processes.cache;
    let supervisor = b.supervisor == Some(pid);
    if storage || supervisor {
        b.supervisor = None;
        b.proposal = None;
        b.views.clear();
    }
    let mut wake = [0; MAX];
    for (i, j) in b.jobs.iter_mut().enumerate() {
        if j.owner == Some(pid) {
            j.owner = None;
            j.revoked = true;
        }
        if storage || supervisor {
            j.revoked = true;
            if j.stage == Stage::Queued {
                j.stage = Stage::Done(CLOSED);
            }
            if storage && j.stage == Stage::Sent {
                let outcome = if mutates_storage(j.op) {
                    files::UNCERTAIN
                } else {
                    CLOSED
                };
                j.stage = Stage::Done(outcome);
            }
            wake[i] = j.ticket;
        }
    }
    b.views.retain(|v| v.owner != pid);
    b.compact();
    for token in wake.into_iter().filter(|v| *v != 0) {
        tmp::wake(s, token);
    }
}
