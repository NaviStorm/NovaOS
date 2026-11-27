//! Constitution de deux canaux point à point avant publication des acteurs.
use super::*;
pub(in crate::sched::process) fn installer(
    s: &mut Core,
    a: Pid,
    b: Pid,
    boite: bool,
    client_invite: bool,
) -> Res<([u64; 2], [u64; 2])> {
    let req = s.ipc.create(a)?;
    let rep = s.ipc.create(a)?;
    let data = s.ipc.derive(a, req, b, u64::from(RECEIVE | SEND))?;
    let back = s.ipc.derive(
        a,
        rep,
        b,
        u64::from(if client_invite { RECEIVE } else { SEND }),
    )?;
    if boite {
        match s.ipc.bind_lane(a, req, b, data) {
            Ok(()) | Err(LIMIT | NO_MEMORY) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(([req, rep], [data, back]))
}
#[cfg(feature = "gestionnaire-4d")]
pub(in crate::sched::process) fn controle(s: &mut Core, client: Pid, manager: Pid) -> Res<([u64;2],[u64;2])> {
    let req = s.ipc.create(client)?;
    let rep = s.ipc.create(client)?;
    let rx = s.ipc.derive(client, req, manager, u64::from(RECEIVE))?;
    let tx = s.ipc.derive(client, rep, manager, u64::from(SEND))?;
    Ok(([req,rep],[rx,tx]))
}
/// Huit liens ordinaires de laboratoire réellement provisionnés avant l'ouverture Ethernet.
pub(in crate::sched::process) fn remplir(s: &mut Core, ids: [Pid; 3]) -> Res<()> {
    for owner in &ids[..2] {
        for _ in 0..4 {
            let root = s.ipc.create(*owner)?;
            let peer = s
                .ipc
                .derive(*owner, root, ids[2], u64::from(SEND | RECEIVE))?;
            s.ipc.bind_lane(*owner, root, ids[2], peer)?;
        }
    }
    let roots = ids.map(|id| s.ipc.racines_pilotes(id));
    if roots != [4, 4, 0] || s.ipc.pages_bulk_count() != 8 {
        return Err(REFUSED);
    }
    hal::log(format_args!(
        "ETH3 préallocation boites=8 octets=32768 racines={roots:?} ipc_pages={}",
        s.ipc.pages()
    ));
    Ok(())
}
/// Les clés proviennent du lanceur, avant admission des acteurs.
pub(in crate::sched::process) fn retirer(s: &mut Core, owner: Pid, keys: [u64; 2]) {
    for key in keys {
        if let Ok(n) = s.ipc.lookup(owner, key, 0)
            && n.id == s.ipc.ends.get(n.endpoint.slot()).root
        {
            s.ipc.close(n);
        }
    }
    s.ipc.signal_heads();
    s.flush_ipc();
}
