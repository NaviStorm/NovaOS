//! Raccord privé réservé au noyau et au scénario de qualification.
use super::*;

pub(super) const AUTH: u64 = 17;
pub(super) const CLIENT: u64 = 18;
pub(super) const SECOND: u64 = 19;
pub(super) const PROVISIONER: u64 = 20;
pub(super) fn enabled() -> bool {
    crate::AUTH_SESSIONS_LAB.load(core::sync::atomic::Ordering::Acquire)
}

pub(super) fn elf(id: u64) -> Option<&'static [u8]> {
    match id {
        AUTH => Some(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/auth-verifier.elf"
        ))),
        CLIENT | SECOND | PROVISIONER => {
            Some(include_bytes!(concat!(env!("OUT_DIR"), "/auth-client.elf")))
        }
        _ => None,
    }
}
fn identity(s: &Core, pid: Pid) -> Res<[u64; 4]> {
    let encoded = s
        .processes
        .get(pid)
        .instance
        .identity(crate::boot_id::current())
        .ok_or(REFUSED)?
        .encode();
    Ok(core::array::from_fn(|i| {
        u64::from_le_bytes(encoded[i * 8..i * 8 + 8].try_into().unwrap())
    }))
}
// La racine compte au budget du provisionneur et se ferme à son départ.
// Seuls l'effecteur et le client possèdent les droits de transport.
fn conversation(s: &mut Core, owner: Pid, peer: Pid, fs: Pid) -> Res<(u64, u64)> {
    let root = s.ipc.create(owner)?;
    let node = s.ipc.lookup(owner, root, MANAGE)?;
    let mask = SEND | RECEIVE;
    let fs_key = match s.ipc.derive(owner, root, fs, mask.into()) {
        Ok(k) => k,
        Err(e) => {
            s.ipc.close(node);
            return Err(e);
        }
    };
    let child = match s.ipc.derive(owner, root, peer, mask.into()) {
        Ok(c) => c,
        Err(e) => {
            s.ipc.close(node);
            return Err(e);
        }
    };
    s.ipc.nodes.get_mut(node.id.slot()).rights = 0;
    Ok((child, fs_key))
}
pub(super) fn connect(s: &mut Core, m: Session, target: Pid, id: u64) -> Res<()> {
    if let Some(space) = s.processes.get(target).space.as_ref() {
        let (pages, stack) = space.page_counts();
        hal::log(format_args!(
            "NA3 memoire pid={} id={id} privees={pages} pile={stack} octets_page=4096",
            target.number
        ));
    }
    match id {
        PROVISIONER => (),
        AUTH => {
            let (ask, read) = channel(s, target, Some(m.fs), SEND)?;
            let (answer, write) = channel(s, target, Some(m.fs), RECEIVE)?;
            config(s, m.fs, 18, &[read, write]);
            config(s, target, 5, &[ask, answer]);
            config(s, target, 7, &identity(s, target)?);
        }
        CLIENT | SECOND => {
            let verifier = s
                .processes
                .slots
                .iter()
                .flatten()
                .find(|p| {
                    p.service_owner == Some(m.manager)
                        && p.service_id == AUTH
                        && p.space.is_some()
                        && p.outcome.is_none()
                })
                .ok_or(CLOSED)?
                .pid;
            let (ask, read) = channel(s, target, Some(verifier), SEND)?;
            let (answer, write) = channel(s, target, Some(verifier), RECEIVE)?;
            let offset = if id == CLIENT { 11 } else { 15 };
            let peer = identity(s, target)?;
            config(s, verifier, offset + 1, &[write, peer[2], peer[3]]);
            config(s, verifier, offset, &[read]);
            config(s, target, 7, &[ask, answer]);
            let provisioner = s
                .processes
                .slots
                .iter()
                .flatten()
                .find(|p| {
                    p.service_id == PROVISIONER
                        && p.service_owner == Some(m.manager)
                        && p.outcome.is_none()
                })
                .ok_or(CLOSED)?
                .pid;
            let (submit, incoming) = conversation(s, provisioner, target, m.fs)?;
            let (receive, respond) = (submit, incoming);
            let pair = if id == CLIENT { 24 } else { 26 };
            config(s, m.fs, pair, &[incoming, respond]);
            config(s, target, 9, &[submit, receive]);
        }
        _ => return Err(DENIED),
    }
    Ok(())
}
