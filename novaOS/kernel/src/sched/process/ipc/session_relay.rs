//! Profil QEMU. Un courtier remet une décision privée du vérificateur au
//! bénéficiaire enregistré. Aucun quota d'autorité n'est ajouté au vérificateur.
use super::traffic::{dword, input, word};
use super::*;

#[path = "exec_snapshot_lab.rs"]
mod snapshot;

const MAX_TRANSFERS: usize = 8;
#[derive(Clone, Copy)]
struct Lease {
    id: [u8; 32],
    limit: u64,
}
#[derive(Clone, Copy)]
enum Asset {
    Ticket(u64),
    Stream {
        source: Reference,
        leaf: Reference,
        direction: u32,
    },
}
#[derive(Clone, Copy)]
struct Offer {
    seq: u64,
    lease: Lease,
    recipient: Pid,
    instance: Instance,
    asset: Asset,
    deadline: u64,
}
pub(in crate::sched::process) struct Route {
    shell: Pid,
    issuer: Pid,
    relay: Pid,
    storage: Pid,
    authorizations: Reference,
    attestations: Reference,
    admission: Option<snapshot::Admission>,
    shell_instance: Instance,
    auth_port: u64,
    relay_port: u64,
    live: Option<Lease>,
    down: bool,
    last: u64,
    offer: Option<Offer>,
    staged: Option<Offer>,
    received: Vec<(Reference, Pid)>,
    receipt: Option<(Offer, Res<u64>)>,
}

fn live(s: &Core, pid: Pid) -> Res<&Process> {
    s.processes.slots[pid.slot]
        .as_ref()
        .filter(|p| p.pid == pid && p.space.is_some() && p.outcome.is_none())
        .ok_or(REVOKED)
}
fn full(p: &Process) -> Res<[u8; 32]> {
    p.instance
        .identity(crate::boot_id::current())
        .map(|id| id.encode())
        .ok_or(REFUSED)
}
fn time_ms() -> u64 {
    ticks().saturating_mul(1000) / u64::from(HZ)
}

pub(super) fn register(
    s: &mut Core,
    shell: Pid,
    issuer: Pid,
    relay: Pid,
    storage: Pid,
    auth_port: u64,
    relay_port: u64,
) -> Res<()> {
    if s.processes.relay.as_ref().is_some_and(|r| !r.down)
        || shell == issuer
        || issuer == relay
        || relay == shell
    {
        return Err(REFUSED);
    }
    let shell_instance = live(s, shell)?.instance;
    live(s, issuer)?;
    live(s, relay)?;
    let mut received = Vec::new();
    received
        .try_reserve_exact(MAX_TRANSFERS)
        .map_err(|_| NO_MEMORY)?;
    // Retrouver les deux routes privées câblées avant le démarrage du courtier.
    // Aucun handle ni PID lu dans une page modifiable par un invité ne fait foi.
    let lane = |direction| {
        let mut roots = s.ipc.ends.ids().filter_map(|i| {
            let end = s.ipc.ends.get(i);
            let root = s.ipc.node(end.root)?;
            (end.owner == issuer.number
                && root.rights == direction
                && s.ipc.nodes.ids().any(|j| {
                    let n = s.ipc.nodes.get(j);
                    n.parent == root.id
                        && n.holder as usize == storage.slot
                        && n.rights == (if direction == SEND { RECEIVE } else { SEND })
                }))
            .then_some(end.id)
        });
        let only = roots.next().ok_or(DENIED)?;
        if roots.next().is_some() {
            return Err(REFUSED);
        }
        Ok(only)
    };
    let authorizations = lane(SEND)?;
    let attestations = lane(RECEIVE)?;
    s.processes.relay = Some(Route {
        shell,
        issuer,
        relay,
        storage,
        authorizations,
        attestations,
        admission: None,
        shell_instance,
        auth_port,
        relay_port,
        live: None,
        down: false,
        last: 0,
        offer: None,
        staged: None,
        received,
        receipt: None,
    });
    let count = |p: Pid| {
        s.ipc
            .ends
            .ids()
            .filter(|&i| s.ipc.ends.get(i).owner == p.number)
            .count()
    };
    hal::log(format_args!(
        "NXRELAY branche shell={} verificateur={} courtier={} racines_auth={} racines_courtier={}",
        shell.number,
        issuer.number,
        relay.number,
        count(issuer),
        count(relay)
    ));
    Ok(())
}

fn discard(s: &mut Core, o: Option<Offer>) {
    if let Some(Offer {
        asset: Asset::Stream { leaf, .. },
        ..
    }) = o
        && let Some(n) = s.ipc.node(leaf)
        && n.reserved
        && n.holder == ABSENT
    {
        s.ipc.nodes.remove(leaf.slot());
        s.ipc.prune(n.parent);
    }
}
fn withdraw(s: &mut Core, r: &mut Route) {
    r.admission = None;
    discard(s, r.offer.take());
    discard(s, r.staged.take());
    r.receipt = None;
    let lease = r.live.take();
    for (node, _) in r.received.drain(..) {
        // La remise de flux ne crée qu'une feuille, sans GRANT ni MANAGE.
        if let Some(n) = s.ipc.node(node) {
            s.ipc.close(n);
        }
    }
    if let Some(l) = lease {
        exec::end_session(s, r.shell, l.id);
    }
    s.flush_ipc();
}

pub(in crate::sched::process) fn retired(s: &mut Core, pid: Pid) {
    if let Some(mut r) = s.processes.relay.take() {
        if [r.shell, r.issuer, r.storage].contains(&pid) {
            withdraw(s, &mut r);
        } else if pid == r.relay {
            let pending = r.offer.is_some() || r.staged.is_some();
            let delivered = r.received.len();
            withdraw(s, &mut r);
            let outstanding = live(s, r.shell).map_or(0, |p| p.exec.len());
            hal::log(format_args!("NXRELAY apres retrait tickets_shell={outstanding}"));
            r.down = true;
            hal::log(format_args!(
                "NXRELAY panne en_attente={pending} feuilles_retirees={delivered}"
            ));
            s.processes.relay = Some(r);
        } else {
            if r.offer.is_some_and(|o| o.recipient == pid) {
                discard(s, r.offer.take());
            }
            if r.staged.is_some_and(|o| o.recipient == pid) {
                discard(s, r.staged.take());
            }
            r.received.retain(|(_, p)| *p != pid);
            s.processes.relay = Some(r);
        }
    }
}

pub(super) fn expire(s: &mut Core) {
    let now = time_ms();
    let Some(mut r) = s.processes.relay.take() else {
        return;
    };
    if r.live.is_some_and(|l| now >= l.limit) {
        withdraw(s, &mut r);
    }
    exec::reap_undelivered(s, r.shell);
    if r.offer.is_some_and(|o| now >= o.deadline) {
        discard(s, r.offer.take());
    }
    if r.staged.is_some_and(|o| now >= o.deadline) {
        discard(s, r.staged.take());
    }
    s.processes.relay = Some(r);
}

fn validate_recipient(s: &Core, r: &Route, id: [u8; 32], dest: &[u8]) -> Res<(Pid, Instance)> {
    let p = s
        .processes
        .slots
        .iter()
        .flatten()
        .find(|p| {
            p.space.is_some()
                && p.outcome.is_none()
                && full(p).is_ok_and(|identity| identity == dest)
        })
        .ok_or(DENIED)?;
    if p.pid != r.shell && !exec::session_child(s, r.shell, p.pid, id) {
        return Err(DENIED);
    }
    Ok((p.pid, p.instance))
}

// Le parent exerce sa propre autorité sur son contrôleur. La feuille est
// réservée ici avant l'offre. Le courtier ne peut ni la créer ni en choisir le masque.
fn stage(s: &mut Core, r: &mut Route, root: Node, q: &[u8]) -> Res<u64> {
    let id: [u8; 32] = q[8..40].try_into().unwrap();
    let lease = r.live.filter(|l| l.id == id).ok_or(REVOKED)?;
    if q[40..72] != full(live(s, r.shell)?)? || q[88..96] != root.key.to_le_bytes() {
        return Err(DENIED);
    }
    let (recipient, instance) = validate_recipient(s, r, id, &q[96..128])?;
    let direction = match word(q, 80) {
        2 => SEND,
        3 => RECEIVE,
        _ => return Err(BAD_ARGUMENT),
    };
    let deadline = word(q, 72);
    if deadline <= time_ms() || deadline > lease.limit {
        return Err(TIMED_OUT);
    }
    if r.staged.is_some() || r.offer.is_some() {
        return Err(WOULD_BLOCK);
    }
    if r.received
        .iter()
        .filter(|(n, _)| s.ipc.node(*n).is_some())
        .count()
        >= MAX_TRANSFERS
    {
        return Err(LIMIT);
    }
    s.ipc.relay_direction(r.shell, root, direction)?;
    let leaf = s.ipc.reserve_child(root, direction)?;
    let asset = Asset::Stream {
        source: root.id,
        leaf,
        direction,
    };
    r.staged = Some(Offer {
        seq: 0,
        lease,
        recipient,
        instance,
        asset,
        deadline,
    });
    Ok(128)
}

fn decision(s: &mut Core, r: &mut Route, seq: u64) -> Res<u64> {
    if let Some((prior, result)) = r.receipt
        && prior.seq == seq
    {
        let lease = r.live.ok_or(REVOKED)?;
        if lease.id != prior.lease.id
            || time_ms() >= prior.deadline
            || live(s, prior.recipient)?.instance != prior.instance
        {
            return Err(REVOKED);
        }
        let key = result?;
        match prior.asset {
            Asset::Ticket(_) => exec::delivery(s, prior.recipient, key, r.issuer, lease.id)?,
            Asset::Stream {
                leaf, direction, ..
            } => {
                if s.ipc.lookup(prior.recipient, key, direction)?.id != leaf {
                    return Err(REVOKED);
                }
            }
        }
        return Ok(key);
    }
    let o = r.offer.filter(|o| o.seq == seq).ok_or(DENIED)?;
    r.offer = None;
    let result = (|| {
        let lease = r.live.ok_or(REVOKED)?;
        if lease.id != o.lease.id || live(s, o.recipient)?.instance != o.instance {
            return Err(REVOKED);
        }
        if time_ms() >= lease.limit || time_ms() >= o.deadline {
            return Err(TIMED_OUT);
        }
        match o.asset {
            Asset::Ticket(key) => {
                exec::delivery(s, o.recipient, key, r.issuer, lease.id)?;
                Ok(key)
            }
            Asset::Stream {
                source,
                leaf,
                direction,
            } => {
                let root = s.ipc.node(source).ok_or(REVOKED)?;
                let n = s
                    .ipc
                    .node(leaf)
                    .filter(|n| {
                        n.parent == source
                            && n.reserved
                            && n.holder == ABSENT
                            && n.rights == direction
                    })
                    .ok_or(REVOKED)?;
                s.ipc.relay_direction(r.shell, root, direction)?;
                r.received.retain(|(node, _)| s.ipc.node(*node).is_some());
                if r.received.len() == MAX_TRANSFERS {
                    return Err(LIMIT);
                }
                let local = s.ipc.free_local(o.recipient)?;
                let key = s.ipc.key()?;
                s.ipc.provision(o.recipient)?;
                s.ipc.install(o.recipient, local, n.id, key);
                s.ipc.relay_attached(root, o.recipient, direction);
                r.received.push((leaf, o.recipient));
                Ok(key)
            }
        }
    })();
    if result.is_err() {
        discard(s, Some(o));
    }
    r.receipt = Some((o, result));
    result
}

fn authorized(s: &mut Core, r: &mut Route, seq: u64, q: &[u8]) -> Res<u64> {
    let shell = live(s, r.shell)?;
    if shell.instance != r.shell_instance || q[40..72] != full(shell)? {
        return Err(DENIED);
    }
    let id: [u8; 32] = q[8..40].try_into().unwrap();
    let deadline = word(q, 72);
    if id == [0; 32] || seq <= r.last {
        return Err(REFUSED);
    }
    match &q[..8] {
        b"NXRBIND1" => {
            if q[80..] != [0; 48]
                || r.live.is_some()
                || deadline <= time_ms()
                || deadline > time_ms().saturating_add(60_000)
            {
                return Err(DENIED);
            }
            r.live = Some(Lease {
                id,
                limit: deadline,
            });
        }
        b"NXRSTOP1" => {
            if q[72..] != [0; 56] || r.live.is_some_and(|l| l.id != id) {
                return Err(DENIED);
            }
            withdraw(s, r);
        }
        b"NXROFFR1" => {
            let lease = r.live.filter(|l| l.id == id).ok_or(REVOKED)?;
            if r.offer.is_some() {
                return Err(WOULD_BLOCK);
            }
            if deadline <= time_ms() || deadline > lease.limit {
                return Err(TIMED_OUT);
            }
            let kind = word(q, 80);
            let key = word(q, 88);
            let dest: [u8; 32] = q[96..128].try_into().unwrap();
            let (recipient, instance) = validate_recipient(s, r, id, &dest)?;
            let asset = match kind {
                1 => {
                    exec::delivery(s, recipient, key, r.issuer, id)?;
                    Asset::Ticket(key)
                }
                2 | 3 => {
                    let direction = if kind == 2 { SEND } else { RECEIVE };
                    let pending = r.staged.ok_or(DENIED)?;
                    let Asset::Stream {
                        source,
                        direction: mask,
                        ..
                    } = pending.asset
                    else {
                        return Err(DENIED);
                    };
                    let root = s.ipc.lookup(r.shell, key, MANAGE)?;
                    if pending.recipient != recipient
                        || pending.instance != instance
                        || pending.lease.id != id
                        || pending.deadline < deadline
                        || root.id != source
                        || direction != mask
                    {
                        return Err(DENIED);
                    }
                    r.staged = None;
                    pending.asset
                }
                _ => return Err(BAD_ARGUMENT),
            };
            r.offer = Some(Offer {
                seq,
                lease,
                recipient,
                instance,
                asset,
                deadline,
            });
            r.receipt = None;
        }
        _ => return Err(BAD_ARGUMENT),
    }
    r.last = seq;
    Ok(128)
}

// Seul le canal privé configuré au boot accepte les ordres. Le courtier ne
// choisit aucune identité et ne présente qu'un numéro d'offre sur son port.
pub(super) fn intercept(s: &mut Core, p: Pid, sys: u64, args: [u64; 3]) -> Option<Res<u64>> {
    let proc = s.processes.get(p);
    if proc.service_owner.is_some()
        && proc.service_id == service::RELAY_ID
        && matches!(
            sys,
            IPC_CREATE | IPC_DUP | IPC_GIVE | IPC_REVOKE | IPC_CANCEL
        )
    {
        return Some(Err(DENIED));
    }
    let route = s.processes.relay.as_ref()?;
    if sys == IPC_SEND
        && (p == route.issuer || p == route.storage)
        && let Ok(node) = s.ipc.lookup(p, args[0], SEND)
        && ((p == route.issuer && node.endpoint == route.authorizations)
            || (p == route.storage && node.endpoint == route.attestations))
    {
        let frame = match input::<160>(proc.space.as_ref()?, args[1], args[2], 160, false) {
            Ok(f) => f,
            Err(e) => return Some(Err(e)),
        };
        if &frame[32..35] == b"NXL" {
            if dword(&frame, 24) != 128 || frame[35] != b'D' {
                return Some(Err(BAD_ARGUMENT));
            }
            let mut r = s.processes.relay.take().unwrap();
            let status = snapshot::operation(s, &mut r, p, &frame[32..]);
            s.processes.relay = Some(r);
            return Some(status);
        }
    }
    if route.down && p == route.issuer && args[0] == route.auth_port {
        // La mort du courtier a déjà retiré toutes les délégations du canal.
        return Some(Err(REVOKED));
    }
    if p == route.shell
        && sys == IPC_SEND
        && let Ok(root) = s.ipc.lookup(p, args[0], MANAGE)
        && s.ipc.ends.get(root.endpoint.slot()).pipe.is_some()
    {
        let frame = match input::<160>(proc.space.as_ref()?, args[1], args[2], 160, false) {
            Ok(f) => f,
            Err(e) => return Some(Err(e)),
        };
        if &frame[32..40] != b"NXRSTAG1" {
            return None;
        }
        if dword(&frame, 24) != 128 {
            return Some(Err(BAD_ARGUMENT));
        }
        let mut r = s.processes.relay.take().unwrap();
        let status = stage(s, &mut r, root, &frame[32..]);
        s.processes.relay = Some(r);
        return Some(status);
    }
    if p == route.issuer && args[0] == route.auth_port && sys == IPC_NOTIFY && args[2] == 1 {
        if let Err(e) = s.ipc.lookup(p, args[0], SEND) {
            return Some(Err(e));
        }
        if route.receipt.is_none_or(|(o, _)| o.seq != args[1]) {
            return Some(Err(WOULD_BLOCK));
        }
        let mut r = s.processes.relay.take().unwrap();
        let res = decision(s, &mut r, args[1]).and_then(|key| {
            // Le courtier peut disparaître après sa décision. Seul le vérificateur
            // constate la remise avant d'annoncer un ticket consommable au shell.
            if let Some((offer, _)) = r.receipt
                && matches!(offer.asset, Asset::Ticket(_))
            {
                exec::release_to(s, offer.recipient, key)?;
            }
            Ok(key)
        });
        s.processes.relay = Some(r);
        return Some(res);
    }
    if p == route.issuer && args[0] == route.auth_port && sys != IPC_SEND {
        return Some(Err(DENIED));
    }
    if p == route.issuer && sys == IPC_SEND && args[0] == route.auth_port {
        let frame = match input::<160>(
            s.processes.get(p).space.as_ref()?,
            args[1],
            args[2],
            160,
            false,
        ) {
            Ok(f) => f,
            Err(e) => return Some(Err(e)),
        };
        if &frame[32..40] == b"NXRELAY1" && dword(&frame, 24) == 8 {
            let seq = word(&frame, 16);
            if route.offer.is_none_or(|o| o.seq != seq)
                && route.receipt.is_none_or(|(o, _)| o.seq != seq)
            {
                return Some(Err(DENIED));
            }
            return None;
        }
        if dword(&frame, 24) != 128
            || !matches!(&frame[32..40], b"NXRBIND1" | b"NXRSTOP1" | b"NXROFFR1")
        {
            return Some(Err(DENIED));
        }
        if let Err(e) = s.ipc.lookup(p, args[0], SEND) {
            return Some(Err(e));
        }
        let mut r = s.processes.relay.take().unwrap();
        let status = authorized(s, &mut r, word(&frame, 16), &frame[32..]);
        s.processes.relay = Some(r);
        return Some(status);
    }
    if p == route.relay && sys == IPC_SEND && args[0] == route.relay_port {
        let ack = match input::<160>(proc.space.as_ref()?, args[1], args[2], 160, false) {
            Ok(a) => a,
            Err(e) => return Some(Err(e)),
        };
        let Some((offer, result)) = route.receipt else {
            return Some(Err(DENIED));
        };
        let (status, value) = match result {
            Ok(key) => (OK, key),
            Err(e) => (e, 0),
        };
        if word(&ack, 16) != offer.seq
            || dword(&ack, 24) != 16
            || word(&ack, 32) != status
            || word(&ack, 40) != value
            || ack[48..].iter().any(|b| *b != 0)
        {
            return Some(Err(DENIED));
        }
        return None;
    }
    if p == route.relay && sys == IPC_NOTIFY && args[0] == route.relay_port {
        if args[2] != 0 {
            return Some(Err(BAD_ARGUMENT));
        }
        if let Err(e) = s.ipc.lookup(p, args[0], SEND) {
            return Some(Err(e));
        }
        let mut r = s.processes.relay.take().unwrap();
        let status = decision(s, &mut r, args[1]);
        s.processes.relay = Some(r);
        return Some(status);
    }
    None
}

pub(super) fn owns_session(s: &Core, shell: Pid, id: [u8; 32]) -> bool {
    s.processes.relay.as_ref().is_some_and(|r| {
        r.shell == shell && r.live.is_some_and(|l| l.id == id && time_ms() < l.limit)
    })
}
