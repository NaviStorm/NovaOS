//! R1 : ce module n'est inclus que par l'export administration QEMU.
//! La feuille permet de parler à NovaAuth, jamais de décider des droits métier.
use super::*;
const LIST: u64 = 0x4e5841444c495354;
const META: u64 = 0x4e5841444d455441;

#[derive(Clone, Copy)]
pub(in crate::sched::process::ipc) struct Permit {
    pub scope: u64,
    pub epoch: u64,
}
#[derive(Clone, Copy)]
pub(super) struct Channel {
    root: Reference,
    child: Pid,
    instance: Instance,
    binding: exec::Binding,
    permit: Permit,
    head: [u8; 32],
    leaf: u64,
    peer: u64,
}

pub(in crate::sched::process::ipc) fn attach(
    s: &mut Core,
    owner: Pid,
    child: Pid,
    binding: exec::Binding,
    permit: Permit,
    head: [u8; 32],
) -> Res<()> {
    let r = s.processes.relay.as_ref().ok_or(REVOKED)?;
    let lease = r.live.ok_or(REVOKED)?;
    if r.down
        || r.shell != owner
        || lease.id != binding.session
        || time_ms() >= lease.limit
        || permit.epoch == 0
        || permit.scope == 0
        || permit.scope & !15 != 0
        || head == [0; 32]
    {
        return Err(DENIED);
    }
    let at = r
        .administrative
        .iter()
        .position(Option::is_none)
        .ok_or(LIMIT)?;
    let verifier = r.issuer;
    let instance = live(s, child)?.instance;
    live(s, verifier)?;
    let key = s.ipc.create(child)?;
    let root = s.ipc.lookup(child, key, ALL)?;
    let result = (|| {
        let leaf = s.ipc.derive(child, key, child, (SEND | RECEIVE).into())?;
        let peer = s
            .ipc
            .derive(child, key, verifier, (SEND | RECEIVE).into())?;
        s.ipc.nodes.get_mut(root.id.slot()).rights = 0;
        // Page NXADMIN1, avant publication et première instruction de l'enfant.
        service::config(s, child, 8, &[leaf, 0]);
        s.processes.relay.as_mut().ok_or(REVOKED)?.administrative[at] = Some(Channel {
            root: root.id,
            child,
            instance,
            binding,
            permit,
            head,
            leaf,
            peer,
        });
        Ok(())
    })();
    if result.is_err() {
        s.ipc.revoke(root);
        if let Some(n) = s.ipc.node(root.id) {
            s.ipc.close(n);
        }
    }
    result
}
fn remove(s: &mut Core, r: &mut Route, i: usize) {
    if let Some(c) = r.administrative[i].take() {
        if let Some(n) = s.ipc.node(c.root) {
            s.ipc.revoke(n);
            if let Some(n) = s.ipc.node(c.root) {
                s.ipc.close(n);
            }
        }
    }
}
pub(super) fn revoke(s: &mut Core, r: &mut Route) {
    for i in 0..r.administrative.len() {
        remove(s, r, i);
    }
}
pub(super) fn retired(s: &mut Core, r: &mut Route, pid: Pid) {
    for i in 0..r.administrative.len() {
        if r.administrative[i].is_some_and(|c| c.child == pid) {
            remove(s, r, i);
        }
    }
}
pub(super) fn reap(s: &mut Core, r: &mut Route) {
    for i in 0..r.administrative.len() {
        if r.administrative[i].is_some_and(|c| {
            live(s, c.child).is_err()
                || s.ipc.lookup(c.child, c.leaf, SEND | RECEIVE).is_err()
                || s.ipc.lookup(r.issuer, c.peer, SEND | RECEIVE).is_err()
        }) {
            remove(s, r, i);
        }
    }
}
fn words(c: Channel, s: &Core, r: &Route) -> Res<[u64; 31]> {
    let mut out = [0; 31];
    out[0] = c.child.number;
    let identity = full(live(s, c.child)?)?;
    let issuer = full(live(s, r.issuer)?)?;
    // Instance enfant, session, titulaire, installation, domaine, empreinte ELF,
    // tête d'admission et instance du vérificateur. Jamais un PID fourni par argv.
    let mut cursor = 1;
    for b in [
        &identity[..],
        &c.binding.session[..],
        &c.binding.holder[..],
        &c.binding.installation[..],
        &c.binding.domain[..],
        &c.binding.digest[..],
        &c.head[..],
        &issuer[..],
    ] {
        for b in b.chunks_exact(8) {
            out[cursor] = u64::from_le_bytes(b.try_into().unwrap());
            cursor += 1;
        }
    }
    out[27] = c.permit.scope;
    out[28] = c.permit.epoch;
    out[29] = r.live.ok_or(REVOKED)?.limit;
    out[30] = c.binding.epoch;
    Ok(out)
}
pub(super) fn intercept(s: &Core, r: &Route, p: Pid, sys: u64, args: [u64; 3]) -> Option<Res<u64>> {
    if sys == IPC_NOTIFY && args[2] == LIST {
        return Some((|| {
            if p != r.issuer
                || args[1] >= 2
                || s.ipc.lookup(p, args[0], SEND)?.endpoint != r.authorizations
            {
                return Err(DENIED);
            }
            let lease = r.live.ok_or(REVOKED)?;
            if r.down || time_ms() >= lease.limit {
                return Err(REVOKED);
            }
            Ok(r.administrative[args[1] as usize].map_or(0, |c| c.peer))
        })());
    }
    let c =
        r.administrative.iter().flatten().find(|c| {
            (p == c.child && args[0] == c.leaf) || (p == r.issuer && args[0] == c.peer)
        })?;
    let check = (|| {
        if r.down
            || r.live
                .is_none_or(|l| l.id != c.binding.session || time_ms() >= l.limit)
            || live(s, c.child)?.instance != c.instance
        {
            return Err(REVOKED);
        }
        let n = s.ipc.lookup(p, args[0], SEND | RECEIVE)?;
        if n.parent != c.root || n.rights != SEND | RECEIVE {
            return Err(DENIED);
        }
        Ok(())
    })();
    if let Err(e) = check {
        return Some(Err(e));
    }
    match sys {
        IPC_NOTIFY if p == r.issuer && args[2] == META => Some((|| {
            let i = usize::try_from(args[1]).map_err(|_| BAD_ARGUMENT)?;
            words(*c, s, r)?.get(i).copied().ok_or(BAD_ARGUMENT)
        })()),
        IPC_SEND | IPC_RECEIVE | IPC_CLOSE => None,
        _ => Some(Err(DENIED)),
    }
}
