//! Raccord du seul profil QEMU entre une sélection P9 et un producteur du catalogue.
//! Aucune des clés de rendez-vous n'est un endpoint, une capacité d'arrêt ou un droit VFS.
use super::*;
use sha2::{Digest, Sha256};
#[path = "selection_vfs_lab/reservation.rs"]
mod reservation;
use reservation::{Destination, Reservations};

const PREPARE: u64 = 0x4e5856505245;
const VALIDATE: u64 = 0x4e585656414c;
const READER: u64 = 0x4e5856524452;
const CANCEL: u64 = 0x4e585643414e;
const VFS_BASE: usize = 144;
const WINDOW: u64 = 10000;

#[derive(Default)]
pub(super) struct Bridge {
    pending: Reservations,
    sequence: u64,
    writer: u64,
    reader: u64,
    image: u64,
    control: u64,
    root: Option<Reference>,
    admission: Option<pipe_admission::ReaderAdmission>,
    active: Option<(Pid, [Reference; 2])>,
    deadline: u64,
}
fn destination(s: &Core, r: &Route, image: u64) -> Res<Destination> {
    let l = r.live.ok_or(REVOKED)?;
    let mask = crate::terminal::LAUNCH | crate::terminal::OUTPUT | crate::terminal::ERROR;
    if live(s, r.shell)?.terminal & mask != mask {
        return Err(DENIED);
    }
    if l.limit <= time_ms() || live(s, r.shell)?.instance != r.shell_instance {
        return Err(REVOKED);
    }
    Ok(Destination {
        shell: full(live(s, r.shell)?)?,
        session: l.id,
        pipe: r.bridge.root.ok_or(REVOKED)?.0,
        image,
    })
}
fn catalog(image: u64) -> Res<spawn::Application> {
    let (app, fixture, _) =
        spawn::Application::entry(usize::try_from(image).map_err(|_| BAD_ARGUMENT)?)?;
    if fixture {
        return Err(DENIED);
    }
    Ok(app)
}
pub(super) fn remove(s: &mut Core, r: &mut Route) {
    let mut b = core::mem::take(&mut r.bridge);
    b.pending.retirer(b.writer);
    if let Some(root) = b.root {
        pipe_admission::fail_ticket(s, r.shell, root);
        if let Some((_, nodes)) = b.active.take() {
            for id in nodes {
                if let Some(n) = s.ipc.node(id) {
                    s.ipc.revoke(n);
                    s.ipc.close(n);
                }
            }
        }
        if live(s, r.storage).is_ok() {
            service::config(s, r.storage, VFS_BASE, &[0, 0, 0, 0]);
        }
        s.flush_ipc();
    }
}
pub(super) fn tick(s: &mut Core, r: &mut Route) {
    if let Some(root) = r.bridge.root {
        let missing = s.ipc.node(root).is_none_or(|n| {
            s.ipc
                .ends
                .get(n.endpoint.slot())
                .pipe
                .is_none_or(|p| p.revoked)
        });
        let waiting = r.bridge.active.is_none() || r.bridge.admission.is_some();
        if missing || waiting && time_ms() >= r.bridge.deadline {
            remove(s, r);
        }
    }
}
pub(super) fn notify(s: &mut Core, r: &mut Route, p: Pid, a: [u64; 3]) -> Res<u64> {
    let [port, op, arg] = a;
    if op == VALIDATE {
        let n = s.ipc.lookup(p, port, SEND)?;
        if p != r.storage || n.endpoint != r.attestations {
            return Err(DENIED);
        }
        destination(s, r, r.bridge.image)?;
        return r
            .bridge
            .pending
            .valider_vfs(arg, time_ms())
            .then_some(0)
            .ok_or(REVOKED);
    }
    if p != r.shell || live(s, p)?.instance != r.shell_instance {
        return Err(DENIED);
    }
    threads::status(s)?;
    let root = s.ipc.lookup(p, port, MANAGE)?;
    if op == PREPARE {
        let mask = crate::terminal::LAUNCH | crate::terminal::OUTPUT | crate::terminal::ERROR;
        if live(s, p)?.terminal & mask != mask {
            return Err(DENIED);
        }
        let app = catalog(arg)?;
        let root = s.ipc.pipe_check(p, port, None)?;
        let l = r.live.filter(|l| l.limit > time_ms()).ok_or(REVOKED)?;
        tick(s, r);
        if r.bridge.root.is_some() {
            return Err(LIMIT);
        }
        let key = s.ipc.key()?;
        let limit = l.limit.min(time_ms().saturating_add(WINDOW));
        let d = Destination {
            shell: full(live(s, p)?)?,
            session: l.id,
            pipe: root.id.0,
            image: arg,
        };
        if !r.bridge.pending.preparer(key, d, time_ms(), limit) {
            return Err(TIMED_OUT);
        }
        r.bridge.sequence = key;
        r.bridge.writer = key;
        r.bridge.image = arg;
        r.bridge.control = port;
        r.bridge.root = Some(root.id);
        r.bridge.deadline = limit;
        let digest = Sha256::digest(app.bytes());
        let mut words = [0; 8];
        words[0] = key;
        words[1] = l.sequence;
        for (dst, bytes) in words[4..].iter_mut().zip(digest.chunks_exact(8)) {
            *dst = u64::from_le_bytes(bytes.try_into().unwrap());
        }
        // La clé publiée en dernier rend visibles le condensat et la session ensemble.
        service::config(s, r.storage, VFS_BASE + 1, &words[1..]);
        service::config(s, r.storage, VFS_BASE, &[key]);
        return Ok(key);
    }
    if Some(root.id) != r.bridge.root || port != r.bridge.control {
        return Err(REVOKED);
    }
    match op {
        CANCEL => {
            if arg != r.bridge.sequence {
                return Err(REVOKED);
            }
            remove(s, r);
            Ok(0)
        }
        READER => {
            threads::status(s)?;
            destination(s, r, arg)?;
            if r.bridge.admission.is_none() || r.bridge.reader != 0 {
                return Err(REFUSED);
            }
            if let Err(e) = catalog(arg) {
                remove(s, r);
                return Err(e);
            }
            r.bridge.reader = s.ipc.key()?;
            r.bridge.image = arg;
            Ok(r.bridge.reader)
        }
        _ => Err(DENIED),
    }
}
pub(super) fn handles_notify(s: &Core, r: &Route, p: Pid, a: [u64; 3]) -> bool {
    (p == r.storage
        && a[1] == VALIDATE
        && s.ipc
            .lookup(p, a[0], SEND)
            .is_ok_and(|n| n.endpoint == r.attestations))
        || (p == r.shell
            && matches!(a[1], PREPARE | READER | CANCEL)
            && s.ipc
                .lookup(p, a[0], MANAGE)
                .is_ok_and(|n| s.ipc.ends.get(n.endpoint.slot()).pipe.is_some()))
}
fn apply<T>(f: impl FnOnce(&mut Core, &mut Route) -> Res<T>) -> Res<T> {
    with(|s| {
        let mut r = s.processes.relay.take().ok_or(REVOKED)?;
        let res = f(s, &mut r);
        s.processes.relay = Some(r);
        res
    })
}
fn arguments(parent: Pid, addr: u64, len: u64) -> Res<Vec<u8>> {
    let count = usize::try_from(len).map_err(|_| BAD_ARGUMENT)?;
    if !(1..=crate::terminal::ARG_MAX).contains(&count) {
        return Err(BAD_ARGUMENT);
    }
    let mut data = Vec::new();
    data.try_reserve_exact(crate::terminal::HEADER + count)
        .map_err(|_| NO_MEMORY)?;
    data.resize(crate::terminal::HEADER + count, 0);
    let args = &mut data[crate::terminal::HEADER..];
    let ok = with(|s| {
        heap::with_pages(|m| {
            let sp = s.processes.get(parent).space.as_ref().unwrap();
            Ok(args.chunks_mut(DEBUG_MAX).enumerate().all(|(i, b)| {
                addr.checked_add((i * DEBUG_MAX) as u64)
                    .is_some_and(|k| sp.copy_from_user(m, k, b))
            }))
        })
        .unwrap()
    });
    if !ok {
        return Err(BAD_ADDRESS);
    }
    let mut rest = &args[1..];
    if usize::from(args[0]) > crate::terminal::ARG_COUNT {
        return Err(BAD_ARGUMENT);
    }
    for _ in 0..args[0] {
        if rest.len() < 2 {
            return Err(BAD_ARGUMENT);
        }
        let n = usize::from(u16::from_le_bytes([rest[0], rest[1]]));
        rest = rest.get(2 + n..).ok_or(BAD_ARGUMENT)?;
    }
    if !rest.is_empty() {
        return Err(BAD_ARGUMENT);
    }
    data[32..40].copy_from_slice(&(count as u64).to_le_bytes());
    Ok(data)
}
pub(in crate::sched::process) fn dispatch(a: [u64; 3]) -> Option<Res<u64>> {
    let guard = Guard::new();
    let (parent, is_reader, original, control, sequence) = with(|s| {
        let p = s.thread(s.current()).owner?;
        let r = s.processes.relay.as_ref()?;
        if p != r.shell || a[0] == 0 {
            return None;
        }
        if a[0] == r.bridge.writer {
            Some((
                p,
                false,
                r.bridge.root?,
                r.bridge.control,
                r.bridge.sequence,
            ))
        } else if a[0] == r.bridge.reader {
            Some((p, true, r.bridge.root?, r.bridge.control, r.bridge.sequence))
        } else {
            None
        }
    })?;
    let mut claimed = false;
    let result = (|| {
        with(|s| threads::status(s))?;
        let data = arguments(parent, a[1], a[2])?;
        if is_reader {
            let (admission, image, d) = apply(|s, r| {
                if r.bridge.sequence != sequence || r.bridge.reader != a[0] {
                    return Err(REVOKED);
                }
                let d = destination(s, r, r.bridge.image)?;
                if time_ms() >= r.bridge.deadline {
                    return Err(TIMED_OUT);
                }
                let image = catalog(r.bridge.image)?;
                let admission = r.bridge.admission.take().ok_or(REFUSED)?;
                claimed = true;
                r.bridge.reader = 0;
                Ok((admission, image, d))
            })?;
            let status = admission
                .admit_status(|| {
                    let ports = u64::from(crate::terminal::OUTPUT | crate::terminal::ERROR);
                    let plan = image.admission(ports, false)?;
                    let child = image.principal(&data, plan)?;
                    let valid = apply(|s, r| {
                        if r.bridge.sequence != sequence
                            || time_ms() >= r.bridge.deadline
                            || destination(s, r, d.image)? != d
                        {
                            return Err(REVOKED);
                        }
                        s.processes.get_mut(child.0).terminal = 0;
                        Ok(())
                    });
                    if let Err(e) = valid.and_then(|()| {
                        with(|s| display::attach_catalog_reader(s, parent, child.0, d.session))
                    }) {
                        retire_unpublished(child);
                        return Err(e);
                    }
                    Ok(child)
                })
                .map_err(|e| e.status);
            let _ = with(|s| pipe_admission::acknowledge(s, parent, control));
            return status;
        }
        let (image, d, control) = apply(|s, r| {
            let d = destination(s, r, r.bridge.image)?;
            if !r.bridge.pending.prendre(a[0], d, time_ms()) {
                return Err(REFUSED);
            }
            claimed = true;
            Ok((catalog(d.image)?, d, r.bridge.control))
        })?;
        let (wait, admission) = pipe_admission::start_writer(parent, control, || {
            let ports = u64::from(crate::terminal::ERROR);
            let plan = image.admission(ports, true)?;
            let child = image.principal(&data, plan)?;
            let setup = apply(|s, r| {
                if destination(s, r, d.image)? != d
                    || !r.bridge.pending.terminer(a[0], d, time_ms())
                {
                    return Err(REVOKED);
                }
                let pid = child.0;
                let (tx, read) = service::channel(s, pid, Some(r.storage), SEND)?;
                let (rx, write) = service::channel(s, pid, Some(r.storage), RECEIVE)?;
                let refs = [
                    s.ipc.lookup(pid, tx, SEND)?.id,
                    s.ipc.lookup(pid, rx, RECEIVE)?.id,
                ];
                bind_grants(s, pid, [tx, rx])?;
                s.ipc.locals.get_mut(pid.slot).bootstrap = tx;
                service::config(s, pid, 0, &[rx]);
                s.processes.get_mut(pid).terminal = ports as u8;
                r.bridge.active = Some((pid, refs));
                service::config(s, r.storage, VFS_BASE + 2, &[read, write]);
                Ok(())
            });
            if let Err(e) = setup {
                retire_unpublished(child);
                return Err(e);
            }
            Ok(child)
        })?;
        apply(|s, r| {
            if r.bridge.root.map(|v| v.0) != Some(d.pipe) || destination(s, r, d.image)? != d {
                return Err(REVOKED);
            }
            r.bridge.admission = Some(admission);
            r.bridge.writer = 0;
            Ok(wait)
        })
    })();
    if result.is_err() && claimed {
        let _ = apply(|s, r| {
            if r.bridge.sequence == sequence
                && r.bridge.root == Some(original)
                && r.bridge.control == control
            {
                remove(s, r);
            }
            Ok(())
        });
    }
    drop(guard);
    Some(result)
}
