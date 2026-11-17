//! Droits locaux à usage unique. Seul le raccord privé d'admission peut en émettre.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Binding {
    pub session: [u8; 32],
    pub domain: [u8; 16],
    pub holder: [u8; 16],
    pub installation: [u8; 16],
    pub epoch: u64,
    /// Numéro stable attribué par le registre pour INFO v1 dans la plage des installations.
    pub app: u64,
    pub object: user::ObjectRevision,
    pub digest: [u8; 32],
}

/// Décision d'admission interne. La sonde n'ouvre aucune porte de provisionnement
/// aux applications. Un raccord NovaAuth devra fournir sa propre décision authentifiée.
pub(super) struct Permit {
    issuer: Pid,
    owner: Pid,
    instance: Instance,
    binding: Binding,
}

impl Permit {
    pub(super) fn fixture(
        _boot: &super::tmp_probe::TicketBoot,
        s: &Core,
        issuer: Pid,
        owner: Pid,
        binding: Binding,
    ) -> Res<Self> {
        if issuer == owner {
            return Err(DENIED);
        }
        current(s, issuer)?;
        Ok(Self {
            issuer,
            owner,
            instance: current(s, owner)?.instance,
            binding,
        })
    }
}

enum State {
    Ready(user::SealedImage, Vec<u8>),
    Claimed,
    Published(Pid),
    Failed(u64),
}

pub(in crate::sched::process) struct Ticket {
    key: u64,
    issuer: Pid,
    owner: Instance,
    binding: Binding,
    deadline: u64,
    revoked: bool,
    state: State,
}

pub(super) fn claiming(p: &Process) -> bool {
    p.exec
        .first()
        .is_some_and(|t| matches!(t.state, State::Claimed))
}

fn current(s: &Core, pid: Pid) -> Res<&Process> {
    s.processes.slots[pid.slot]
        .as_ref()
        .filter(|p| p.pid == pid && p.outcome.is_none())
        .ok_or(REVOKED)
}

fn validate(s: &Core, owner: Pid, t: &Ticket) -> Res<()> {
    let p = current(s, owner)?;
    s.ipc.lookup(owner, t.key, RECEIVE)?;
    current(s, t.issuer)?;
    if p.instance != t.owner || t.revoked {
        return Err(REVOKED);
    }
    if ticks() >= t.deadline {
        return Err(TIMED_OUT);
    }
    Ok(())
}

pub(super) fn issue(
    s: &mut Core,
    permit: Permit,
    image: user::SealedImage,
    config: Vec<u8>,
    deadline: u64,
) -> Res<u64> {
    let Permit {
        issuer,
        owner,
        instance,
        binding,
    } = permit;
    current(s, issuer)?;
    let p = current(s, owner)?;
    if p.instance != instance
        || p.instance.identity(crate::boot_id::current()).is_none()
        || !p.principal
        || binding.session == [0; 32]
        || binding.installation == [0; 16]
        || binding.holder == [0; 16]
        || binding.domain == [0; 16]
        || binding.epoch == 0
        || binding.app >> 63 != 1
        || image.object() != binding.object
        || image.digest() != binding.digest
    {
        return Err(DENIED);
    }
    if config.len() > crate::terminal::ARG_MAX + crate::terminal::HEADER {
        return Err(LIMIT);
    }
    if deadline <= ticks() {
        return Err(TIMED_OUT);
    }
    if !p.exec.is_empty() {
        return Err(LIMIT);
    }
    s.processes
        .get_mut(owner)
        .exec
        .try_reserve_exact(1)
        .map_err(|_| NO_MEMORY)?;
    let key = s.ipc.create(owner)?;
    let root = s.ipc.lookup(owner, key, ALL)?;
    // SEND est interprété uniquement par le protocole de reçu. Aucun GRANT/MANAGE.
    s.ipc.nodes.get_mut(root.id.slot()).rights = SEND | RECEIVE;
    let slot = s.processes.get_mut(owner);
    slot.exec.push(Ticket {
        key,
        issuer,
        owner: slot.instance,
        binding,
        deadline,
        revoked: false,
        state: State::Ready(image, config),
    });
    Ok(key)
}

pub(super) fn is_port(s: &Core, owner: Pid, key: u64) -> bool {
    s.processes
        .get(owner)
        .exec
        .first()
        .is_some_and(|t| t.key == key)
}

fn ticket(s: &Core, owner: Pid, key: u64) -> Res<&Ticket> {
    current(s, owner)?
        .exec
        .first()
        .filter(|t| t.key == key)
        .ok_or(DENIED)
}

fn invalidate(t: &mut Ticket, reason: u64) {
    t.revoked = true;
    if matches!(t.state, State::Ready(..)) {
        t.state = State::Failed(reason);
    }
}

pub(super) fn revoke(s: &mut Core, issuer: Pid, owner: Pid, binding: Binding) -> Res<()> {
    current(s, owner)?;
    let t = s.processes.get_mut(owner).exec.first_mut().ok_or(DENIED)?;
    if t.issuer != issuer || t.binding != binding {
        return Err(DENIED);
    }
    invalidate(t, REVOKED);
    Ok(())
}

pub(in crate::sched::process) fn retired(s: &mut Core, dead: Pid) {
    s.processes.get_mut(dead).exec = Vec::new();
    for p in s.processes.slots.iter_mut().flatten() {
        if let Some(t) = p.exec.first_mut()
            && t.issuer == dead
        {
            invalidate(t, REVOKED);
        }
    }
}

pub(super) fn outcome(s: &Core, owner: Pid, key: u64) -> Res<(Pid, Option<u64>)> {
    let t = ticket(s, owner, key)?;
    match t.state {
        State::Published(child) => Ok((
            child,
            current(s, owner)?
                .wait_right
                .filter(|w| w.target == child)
                .map(WaitRight::key),
        )),
        State::Failed(code) => Err(code),
        _ => Err(WOULD_BLOCK),
    }
}

pub(super) fn acknowledge(s: &mut Core, owner: Pid, key: u64) -> Res<()> {
    if !matches!(
        ticket(s, owner, key)?.state,
        State::Published(_) | State::Failed(_)
    ) {
        return Err(WOULD_BLOCK);
    }
    let root = s.ipc.lookup(owner, key, 0)?;
    s.processes.get_mut(owner).exec.clear();
    s.ipc.close(root);
    Ok(())
}

pub(in crate::sched::process) fn consume(key: u64) -> Res<u64> {
    let owner = with(|s| s.thread(s.current()).owner.unwrap());
    let (image, config, app) = with(|s| {
        let t = ticket(s, owner, key)?;
        if !matches!(t.state, State::Ready(..)) {
            return Err(match t.state {
                State::Failed(e) => e,
                _ => REFUSED,
            });
        }
        if let Err(e) = validate(s, owner, t) {
            invalidate(&mut s.processes.get_mut(owner).exec[0], e);
            return Err(e);
        }
        let p = s.processes.get_mut(owner);
        if p.wait_right.is_some() || p.pending_reader.is_some() || p.pipe_reader_wait.is_some() {
            return Err(LIMIT);
        }
        let t = &mut p.exec[0];
        let State::Ready(image, config) = core::mem::replace(&mut t.state, State::Claimed) else {
            unreachable!()
        };
        Ok((image, config, t.binding.app))
    })?;
    let result = (|| {
        let space =
            heap::with_pages(|m| UserSpace::installed(m, &image, &config)).map_err(|e| {
                if e == crate::memory::Error::Exhausted {
                    NO_MEMORY
                } else {
                    REFUSED
                }
            })?;
        drop(image);
        drop(config);
        let _guard = Guard::new();
        let child = publish_image(space, None, app, None).map_err(launch_error)?;
        let admitted = tmp::prepare(child.0).and_then(|()| {
            with(|s| {
                validate(s, owner, ticket(s, owner, key)?)?;
                let p = s.processes.get_mut(owner);
                if p.wait_right.is_some() {
                    return Err(LIMIT);
                }
                let wait = WaitRight { target: child.0 };
                p.wait_right = Some(wait);
                p.exec[0].state = State::Published(child.0);
                Ok(wait.key())
            })
        });
        if admitted.is_ok() {
            core::mem::forget(child);
        } else {
            retire_unpublished(child);
        }
        admitted
    })();
    if let Err(e) = result {
        with(|s| {
            if ticket(s, owner, key).is_ok() {
                s.processes.get_mut(owner).exec[0].state = State::Failed(e);
            }
        });
    }
    result
}
