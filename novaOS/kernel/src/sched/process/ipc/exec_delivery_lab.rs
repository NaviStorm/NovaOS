// Inclus seulement dans le profil de lancement QEMU.
pub(super) fn delivery(s: &Core, owner: Pid, key: u64, issuer: Pid, sid: [u8; 32]) -> Res<()> {
    let t = ticket(s, owner, key)?;
    if t.issuer != issuer || t.binding.session != sid || !matches!(t.state, State::Ready(..)) {
        return Err(DENIED);
    }
    validate(s, owner, t)
}

pub(super) fn session_child(s: &Core, parent: Pid, child: Pid, id: [u8; 32]) -> bool {
    current(s, child).is_ok_and(|p| p.session_origin == Some((parent, id)))
}

pub(super) fn end_session(s: &mut Core, owner: Pid, id: [u8; 32]) {
    if current(s, owner).is_ok() {
        let undisclosed =
            s.processes.get(owner).exec.first().and_then(|t| {
                (t.binding.session == id && t.relayed == Some(false)).then_some(t.key)
            });
        for t in &mut s.processes.get_mut(owner).exec {
            if t.binding.session == id {
                invalidate(t, REVOKED);
            }
        }
        if let Some(key) = undisclosed {
            let _ = acknowledge(s, owner, key);
        }
    }
    for i in 0..MAX {
        if let Some(p) = s.processes.slots[i].as_ref()
            && p.session_origin == Some((owner, id))
        {
            let child = p.pid;
            withdraw_grants(s, child);
            s.processes.get_mut(child).session_origin = None;
        }
    }
}

pub(super) fn issue_authenticated(
    s: &mut Core,
    issuer: Pid,
    owner: Pid,
    binding: Binding,
    image: user::SealedImage,
    config: Vec<u8>,
    deadline: u64,
    role: u8,
    control: u64,
    scope: [u64; 3],
) -> Res<u64> {
    let permit = Permit {
        issuer,
        owner,
        instance: current(s, owner)?.instance,
        binding,
    };
    let key = issue(s, permit, image, config, deadline)?;
    s.processes.get_mut(owner).exec[0].relayed = Some(false);
    s.processes.get_mut(owner).exec[0].flow = (role, control);
    s.processes.get_mut(owner).exec[0].scope = scope;
    Ok(key)
}

pub(super) fn release_to(s: &mut Core, owner: Pid, key: u64) -> Res<()> {
    let t = s
        .processes
        .get_mut(owner)
        .exec
        .first_mut()
        .filter(|t| t.key == key && t.relayed.is_some())
        .ok_or(DENIED)?;
    t.relayed = Some(true);
    Ok(())
}

/// Un ticket non remis n'a pas de détenteur susceptible de fermer son port.
pub(super) fn reap_undelivered(s: &mut Core, owner: Pid) {
    let expired = current(s, owner)
        .ok()
        .and_then(|p| p.exec.first())
        .and_then(|t| {
            (t.relayed == Some(false)
                && ticks() >= t.deadline
                && matches!(t.state, State::Ready(..)))
            .then_some(t.key)
        });
    if let Some(key) = expired {
        invalidate(&mut s.processes.get_mut(owner).exec[0], TIMED_OUT);
        let _ = acknowledge(s, owner, key);
    }
}

/// Écarter les décisions de lancement encore en attente à la frontière de
/// publication du registre. Un reçu déjà publié reste collectable.
pub(super) fn registry_fence(s: &mut Core, owner: Pid) {
    if current(s, owner).is_err() {
        return;
    }
    let mut undisclosed = None;
    for t in &mut s.processes.get_mut(owner).exec {
        if t.relayed.is_some() && matches!(t.state, State::Ready(..) | State::Claimed) {
            if t.relayed == Some(false) {
                undisclosed = Some(t.key);
            }
            invalidate(t, REVOKED);
        }
    }
    if let Some(key) = undisclosed {
        let _ = acknowledge(s, owner, key);
    }
}
