//! Domaine de sérialisation IPC du BSP. Aucun emprunt du domaine ni d’une
//! traduction utilisateur ne traverse dispatch. Les pages des slabs sont RW/NX.
use super::*;
use crate::abi::*;
pub(crate) mod lifecycle;
pub(crate) mod metrics;
mod pool;
use pool::{Bits, Pool};
mod probes;
mod traffic;
pub(crate) use probes::{quiet, run};

type Res<T> = core::result::Result<T, u64>;
const ALL: u32 = SEND | RECEIVE | GRANT | MANAGE;
const ABSENT: u8 = u8::MAX;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct Reference(u64);
impl Reference {
    fn slot(self) -> usize {
        self.0 as u8 as usize
    }
}
#[derive(Clone, Copy, Default)]
struct Entry {
    key: u64,
    node: Reference,
}
#[derive(Clone, Copy)]
struct LocalCaps {
    owner: u64,
    entries: [Entry; 32],
    bootstrap: u64,
}
#[derive(Clone, Copy)]
struct Node {
    id: Reference,
    parent: Reference,
    endpoint: Reference,
    key: u64,
    holder: u8,
    depth: u8,
    rights: u32,
    reserved: bool,
}
#[derive(Clone, Copy)]
struct Message {
    tag: u64,
    len: u32,
    bytes: [u8; 128],
}
impl Message {
    const EMPTY: Self = Self {
        tag: 0,
        len: 0,
        bytes: [0; 128],
    };
}
#[derive(Clone, Copy)]
struct Deposit {
    admitted: u64,
    rank: u64,
    msg: Message,
}
#[derive(Clone, Copy)]
struct Endpoint {
    id: Reference,
    root: Reference,
    owner: u64,
    sequence: u64,
    head: usize,
    len: usize,
    deposits: [Deposit; 8],
    bits: u64,
    notified: u64,
    notified_at: u64,
}

#[derive(Clone, Copy)]
enum Operation {
    Send(Message),
    Give {
        source: Entry,
        child: Reference,
        tag: u64,
        rights: u32,
    },
    Receive {
        address: u64,
        options: u32,
        timeout: u64,
    },
}
#[derive(Clone, Copy)]
struct Pending {
    thread: Id,
    cookie: u64,
    pid: Pid,
    channel: Entry,
    ep: Reference,
    deadline: u64,
    rank: u64,
    op: Operation,
    done: Option<Res<u64>>,
}
const _: () = assert!(
    size_of::<Node>() <= 64
        && size_of::<Entry>() <= 32
        && size_of::<Pending>() <= 256
        && size_of::<Domain>() <= 4096
);

// Les quatre fenêtres tiennent dans un seul intervalle de 2 Mio. Même en
// facturant trois tables neuves, 64+4+5+2+3 pages sont sous le budget de 128.
type Endpoints = Pool<Endpoint, 64, 0, 1>;
type Nodes = Pool<Node, 256, 64, 64>;
type Locals = Pool<LocalCaps, 32, 72, 7>;
type Waits = Pool<Pending, 32, 80, 16>;
const _: () = assert!(
    Endpoints::FIRST + Endpoints::MAX_PAGES <= Nodes::FIRST
        && Nodes::FIRST + Nodes::MAX_PAGES <= Locals::FIRST
        && Locals::FIRST + Locals::MAX_PAGES <= Waits::FIRST
        && Waits::FIRST + Waits::MAX_PAGES <= 512
        && (Endpoints::MAX_PAGES + Nodes::MAX_PAGES + Locals::MAX_PAGES + Waits::MAX_PAGES + 3)
            * 4096
            <= 512 * 1024
);
pub(crate) struct Domain {
    ends: Endpoints,
    nodes: Nodes,
    locals: Locals,
    waits: Waits,
    incarnation: u64,
    issued: u64,
    wakes: u32,
    wake_version: [u64; 32],
    operations: u64,
    probe_clock: Option<u64>,
    metrics: metrics::Metrics,
}
impl Domain {
    pub const fn new() -> Self {
        Self {
            ends: Pool::new(),
            nodes: Pool::new(),
            locals: Pool::new(),
            waits: Pool::new(),
            incarnation: 0,
            issued: 0,
            wakes: 0,
            wake_version: [0; 32],
            operations: 0,
            probe_clock: None,
            metrics: metrics::Metrics::new(),
        }
    }
    pub(super) fn pages(&self) -> usize {
        (self.ends.pages() + self.nodes.pages() + self.locals.pages() + self.waits.pages()) as usize
    }
    pub(super) fn collect(&mut self) {
        self.ends.collect();
        self.nodes.collect();
        self.locals.collect();
        self.waits.collect();
    }
    pub(in crate::sched) fn measured_peer(&self, pid: Option<Pid>) -> bool {
        pid.is_some_and(|p| self.metrics.includes(p))
    }
    pub(crate) fn finish_masked(&mut self, start: u64) {
        self.metrics.sample(0, start, metrics::stamp());
    }
    pub(crate) fn wake_cookie(&self, i: usize) -> u64 {
        self.wake_version[i]
    }
    fn wake_slot(&mut self, i: usize) {
        if self.metrics.woken[i] == 0 {
            self.metrics.woken[i] = metrics::stamp();
        }
        self.wake_version[i] = self.waits.get(i).cookie;
        self.wakes |= 1 << i;
    }
    pub(crate) fn take_wakes(&mut self) -> u32 {
        core::mem::take(&mut self.wakes)
    }
    pub(crate) fn waiter_generation(&self, i: usize) -> Option<u64> {
        self.waits
            .has(i)
            .then(|| self.waits.get(i).thread.generation)
    }
    fn reference(&mut self, i: usize) -> Res<Reference> {
        if self.incarnation >= u64::MAX >> 8 {
            return Err(LIMIT);
        }
        self.incarnation += 1;
        Ok(Reference((self.incarnation << 8) | i as u64))
    }
    fn key(&mut self) -> Res<u64> {
        if self.issued == (1 << 62) - 1 {
            return Err(LIMIT);
        }
        self.issued += 1;
        Ok((1 << 62) | self.issued)
    }
    fn caps(&self, p: Pid) -> Option<&LocalCaps> {
        self.locals
            .has(p.slot)
            .then(|| self.locals.get(p.slot))
            .filter(|t| t.owner == p.number)
    }
    fn lookup(&self, p: Pid, key: u64, need: u32) -> Res<Node> {
        if key == 0 {
            return Err(DENIED);
        }
        let entry = self
            .caps(p)
            .and_then(|t| t.entries.iter().find(|e| e.key == key))
            .ok_or(DENIED)?;
        let n = self
            .node(entry.node)
            .filter(|n| n.key == key && n.holder as usize == p.slot && n.rights & need == need)
            .ok_or(DENIED)?;
        Ok(n)
    }
    fn node(&self, id: Reference) -> Option<Node> {
        if id.0 == 0 || !self.nodes.has(id.slot()) {
            return None;
        }
        let n = *self.nodes.get(id.slot());
        (n.id == id).then_some(n)
    }
    fn free_local(&self, p: Pid) -> Res<usize> {
        match self.caps(p) {
            None => Ok(0),
            Some(t) => t.entries.iter().position(|e| e.key == 0).ok_or(LIMIT),
        }
    }
    fn provision(&mut self, p: Pid) -> Res<()> {
        if self.caps(p).is_none() {
            self.locals.insert(
                p.slot,
                LocalCaps {
                    owner: p.number,
                    entries: [Entry::default(); 32],
                    bootstrap: 0,
                },
            )?;
        }
        Ok(())
    }
    fn install(&mut self, p: Pid, slot: usize, id: Reference, key: u64) {
        let n = self.nodes.get_mut(id.slot());
        assert_eq!(n.id, id);
        n.holder = p.slot as u8;
        n.key = key;
        n.reserved = false;
        let e = &mut self.locals.get_mut(p.slot).entries[slot];
        assert_eq!(e.key, 0);
        *e = Entry { key, node: id };
    }
    fn unlink(&mut self, id: Reference) {
        let n = self.nodes.get_mut(id.slot());
        if n.holder == ABSENT {
            return;
        }
        let t = self.locals.get_mut(n.holder as usize);
        let slot = t
            .entries
            .iter_mut()
            .find(|e| e.key == n.key)
            .expect("handle installé");
        *slot = Entry::default();
        if t.bootstrap == n.key {
            t.bootstrap = 0;
        }
        n.holder = ABSENT;
        n.key = 0;
    }
    fn empty_locals(&mut self) {
        for i in self.locals.ids() {
            if self.locals.get(i).entries.iter().all(|e| e.key == 0) {
                self.locals.remove(i);
            }
        }
    }
    fn create(&mut self, p: Pid) -> Res<u64> {
        if self
            .ends
            .ids()
            .filter(|&i| self.ends.get(i).owner == p.number)
            .count()
            >= 4
        {
            return Err(LIMIT);
        }
        let ei = self.ends.vacant()?;
        let ni = self.nodes.vacant()?;
        let slot = self.free_local(p)?;
        let ep = self.reference(ei)?;
        let root = self.reference(ni)?;
        let key = self.key()?;
        self.provision(p)?;
        self.ends.insert(
            ei,
            Endpoint {
                id: ep,
                root,
                owner: p.number,
                sequence: 0,
                head: 0,
                len: 0,
                deposits: [Deposit {
                    admitted: 0,
                    rank: 0,
                    msg: Message::EMPTY,
                }; 8],
                bits: 0,
                notified: 0,
                notified_at: 0,
            },
        )?;
        if let Err(e) = self.nodes.insert(
            ni,
            Node {
                id: root,
                parent: Reference(0),
                endpoint: ep,
                key: 0,
                holder: ABSENT,
                depth: 0,
                rights: ALL,
                reserved: false,
            },
        ) {
            self.ends.remove(ei);
            return Err(e);
        }
        self.install(p, slot, root, key);
        Ok(key)
    }
    fn check_mask(n: Node, mask: u64) -> Res<u32> {
        if mask == 0 || mask & !u64::from(ALL) != 0 {
            return Err(BAD_ARGUMENT);
        }
        let rights = mask as u32;
        if rights & !n.rights != 0 {
            return Err(DENIED);
        }
        Ok(rights)
    }
    fn reserve_child(&mut self, parent: Node, rights: u32) -> Res<Reference> {
        if parent.depth >= 8
            || self
                .nodes
                .ids()
                .filter(|&i| self.nodes.get(i).endpoint == parent.endpoint)
                .count()
                >= 32
        {
            return Err(LIMIT);
        }
        let i = self.nodes.vacant()?;
        let id = self.reference(i)?;
        self.nodes.insert(
            i,
            Node {
                id,
                parent: parent.id,
                endpoint: parent.endpoint,
                key: 0,
                holder: ABSENT,
                rights,
                depth: parent.depth + 1,
                reserved: true,
            },
        )?;
        Ok(id)
    }
    fn derive(&mut self, from: Pid, parent_key: u64, to: Pid, mask: u64) -> Res<u64> {
        let parent = self.lookup(from, parent_key, GRANT)?;
        let rights = Self::check_mask(parent, mask)?;
        let slot = self.free_local(to)?;
        let key = self.key()?;
        let child = self.reserve_child(parent, rights)?;
        if let Err(e) = self.provision(to) {
            self.nodes.remove(child.slot());
            return Err(e);
        }
        self.install(to, slot, child, key);
        Ok(key)
    }
    fn descendants(&self, root: Node) -> Bits {
        let mut set = Bits::default();
        set.set(root.id.slot());
        // Parcours topologique borné sans récursion, ancêtres fermés compris.
        for depth in root.depth + 1..=8 {
            for i in self.nodes.ids() {
                let n = self.nodes.get(i);
                if n.endpoint == root.endpoint && n.depth == depth && set.has(n.parent.slot()) {
                    set.set(i);
                }
            }
        }
        set
    }
    fn prune(&mut self, mut id: Reference) {
        while let Some(n) = self.node(id) {
            if n.holder != ABSENT
                || n.reserved
                || self.nodes.ids().any(|i| self.nodes.get(i).parent == id)
            {
                break;
            }
            self.nodes.remove(id.slot());
            id = n.parent;
        }
    }
    fn complete(&mut self, i: usize, result: Res<u64>) {
        let w = *self.waits.get(i);
        if w.done.is_some() {
            return;
        }
        self.waits.get_mut(i).done = Some(result);
        if let Operation::Give { child, .. } = w.op {
            if let Some(n) = self.node(child).filter(|n| n.reserved) {
                self.nodes.remove(child.slot());
                self.prune(n.parent);
            }
        }
        self.wake_slot(i);
    }
    pub(crate) fn has_pending(&self) -> bool {
        self.waits.ids().any(|i| self.waits.get(i).done.is_none())
    }
    pub(crate) fn expire(&mut self, now: u64) {
        let mut expired = false;
        let now = self.probe_clock.unwrap_or(now);
        for i in self.waits.ids() {
            let w = self.waits.get(i);
            if w.done.is_none() && w.deadline <= now {
                self.complete(i, Err(TIMED_OUT));
                expired = true;
            }
        }
        if expired {
            self.signal_heads();
        }
    }
    fn sweep(&mut self) {
        for i in self.waits.ids() {
            let w = *self.waits.get(i);
            if w.done.is_some() {
                continue;
            }
            if !self.ends.has(w.ep.slot()) || self.ends.get(w.ep.slot()).id != w.ep {
                self.complete(i, Err(CLOSED));
                continue;
            }
            let valid = |entry: Entry| {
                self.node(entry.node)
                    .is_some_and(|n| n.key == entry.key && n.holder as usize == w.pid.slot)
            };
            if !valid(w.channel) || matches!(w.op,Operation::Give{source,..} if !valid(source)) {
                self.complete(i, Err(REVOKED));
            }
        }
        self.empty_locals();
    }
    fn revoke(&mut self, n: Node) -> u64 {
        let mut cut = self.descendants(n);
        cut.clear(n.id.slot());
        let mut count = 0;
        for i in cut.indices() {
            let victim = *self.nodes.get(i);
            if victim.key != 0 {
                count += 1;
                self.unlink(victim.id);
            }
            self.nodes.remove(i);
        }
        self.sweep();
        count
    }
    fn destroy(&mut self, ep: Reference) {
        for i in self.nodes.ids() {
            let n = *self.nodes.get(i);
            if n.endpoint == ep {
                self.unlink(n.id);
                self.nodes.remove(i);
            }
        }
        self.ends.remove(ep.slot());
    }
    fn close(&mut self, n: Node) {
        if n.id == self.ends.get(n.endpoint.slot()).root {
            self.destroy(n.endpoint);
        } else {
            self.unlink(n.id);
            self.prune(n.id);
        }
        self.sweep();
    }
    fn cancel(&mut self, n: Node) -> u64 {
        let branch = self.descendants(n);
        let mut count = 0;
        for i in self.waits.ids() {
            let w = self.waits.get(i);
            if w.done.is_none() && w.ep == n.endpoint && branch.has(w.channel.node.slot()) {
                self.complete(i, Err(CANCELLED));
                count += 1;
            }
        }
        count
    }
    pub(super) fn death(&mut self, p: Pid, now: u64) {
        self.expire(now);
        for i in self.waits.ids() {
            if self.waits.get(i).pid == p {
                self.complete(i, Err(CANCELLED));
                self.waits.remove(i);
                self.wakes &= !(1 << i);
            }
        }
        for i in self.ends.ids() {
            let ep = *self.ends.get(i);
            if ep.owner == p.number {
                self.destroy(ep.id);
            }
        }
        if let Some(t) = self.caps(p).copied() {
            for e in t.entries.into_iter().filter(|e| e.key != 0) {
                self.unlink(e.node);
                self.prune(e.node);
            }
        }
        self.sweep();
        self.signal_heads();
    }
}

// Construction atomique par le lanceur avant de rendre les acteurs exécutables.
// En cas de refus, aucune racine ni capacité partielle ne subsiste.
fn bootstrap(s: &mut Core, owner: Pid, clients: &[(Pid, u32)]) -> Res<u64> {
    if s.ipc.caps(owner).is_some() || clients.iter().any(|&(p, _)| s.ipc.caps(p).is_some()) {
        return Err(BAD_ARGUMENT);
    }
    let key = s.ipc.create(owner)?;
    for (i, &(p, rights)) in clients.iter().enumerate() {
        if p == owner || clients[..i].iter().any(|&(other, _)| other == p) {
            let root = s.ipc.lookup(owner, key, 0)?;
            s.ipc.close(root);
            return Err(BAD_ARGUMENT);
        }
        match s.ipc.derive(owner, key, p, rights.into()) {
            Ok(h) => s.ipc.locals.get_mut(p.slot).bootstrap = h,
            Err(e) => {
                let root = s.ipc.lookup(owner, key, 0)?;
                s.ipc.close(root);
                return Err(e);
            }
        }
    }
    s.ipc.locals.get_mut(owner.slot).bootstrap = key;
    Ok(key)
}

pub(super) fn call(id: u64, args: [u64; 3]) -> Res<u64> {
    traffic::invoke(id, args)
}

fn undo_unpublished(child: Child) {
    let pid = child.0;
    core::mem::forget(child);
    let (stack, space) = with(|s| {
        let index = s
            .slots
            .iter()
            .position(|t| t.as_ref().is_some_and(|t| t.owner == Some(pid)))
            .unwrap();
        for _ in 0..s.ready.len {
            let k = s.ready.pop().unwrap();
            if k != index {
                s.ready.push(k);
            }
        }
        let t = s.slots[index].take().unwrap();
        assert_eq!(t.state, State::Ready);
        let mut p = s.processes.slots[pid.slot].take().unwrap();
        (t.stack, p.space.take().unwrap())
    });
    stack.release();
    heap::with_pages(|m| {
        space.release(m);
        Ok(())
    })
    .unwrap();
}

fn launch_error(e: Error) -> u64 {
    match e {
        Error::Full => LIMIT,
        Error::Memory(crate::memory::Error::Exhausted) => NO_MEMORY,
        Error::Memory(_) => BAD_ARGUMENT,
        _ => panic!("lanceur IPC : {e:?}"),
    }
}

pub(super) fn launch<const N: usize>(members: [(&'static [u8], u32); N]) -> Res<[Child; N]> {
    assert!(N > 0 && N <= 32 && members[0].1 == ALL);
    let _irq = Guard::new();
    let mut group: [Option<Child>; N] = core::array::from_fn(|_| None);
    let attempt = (|| {
        for (c, &(image, _)) in group.iter_mut().zip(&members) {
            *c = Some(create_image(image, &[], None, None).map_err(launch_error)?);
        }
        let owner = group[0].as_ref().unwrap().0;
        let mut grants = [(owner, 0); N];
        let mut count = 0;
        for i in 1..N {
            if members[i].1 != 0 {
                grants[count] = (group[i].as_ref().unwrap().0, members[i].1);
                count += 1;
            }
        }
        with(|s| {
            let r = bootstrap(s, owner, &grants[..count]);
            s.ipc.empty_locals();
            s.ipc.collect();
            r
        })?;
        Ok(())
    })();
    if let Err(e) = attempt {
        for c in group.into_iter().flatten() {
            undo_unpublished(c);
        }
        return Err(e);
    }
    Ok(group.map(Option::unwrap))
}

/// Le lanceur conserve les deux racines créées par le noyau. L'admission est
/// séquentielle et limitée au même binaire client immuable pour cette version.
pub(super) struct Service {
    pub process: Child,
    requests: u64,
    replies: u64,
    previous: Pid,
    client_image: &'static [u8],
}

fn configure_session(s: &Core, pid: Pid, replies: u64, rounds: u64) {
    let space = s.processes.get(pid).space.as_ref().unwrap();
    heap::with_pages(|m| {
        space.write_data(m, 0, replies);
        space.write_data(m, 1, rounds);
        Ok(())
    })
    .unwrap();
}

impl Service {
    fn admission(&self, s: &Core) -> Res<()> {
        let owner = self.process.0;
        if s.processes.slots[self.previous.slot]
            .as_ref()
            .is_some_and(|p| p.pid == self.previous && p.space.is_some())
        {
            return Err(LIMIT);
        }
        let input = s.ipc.lookup(owner, self.requests, MANAGE)?.endpoint;
        let output = s.ipc.lookup(owner, self.replies, MANAGE)?.endpoint;
        // Aucun dépôt du client précédent ne doit franchir l'admission.
        // Le serveur doit déjà attendre la prochaine demande, sans réponse en vol.
        for id in [input, output] {
            let ep = s.ipc.ends.get(id.slot());
            if ep.len != 0 || ep.bits != 0 {
                return Err(WOULD_BLOCK);
            }
        }
        let listening = s.ipc.waits.ids().any(|i| {
            let w = s.ipc.waits.get(i);
            w.pid == owner
                && w.ep == input
                && w.done.is_none()
                && matches!(w.op, Operation::Receive { .. })
        });
        if listening { Ok(()) } else { Err(WOULD_BLOCK) }
    }

    pub(super) fn listening(&self) -> bool {
        with(|s| self.admission(s).is_ok())
    }

    pub(super) fn next_client(&mut self, rounds: u64) -> Res<Child> {
        let _irq = Guard::new();
        let owner = self.process.0;
        with(|s| self.admission(s))?;
        let child = create_image(self.client_image, &[], None, None).map_err(launch_error)?;
        let peer = child.0;
        let result = with(|s| {
            let writer = s.ipc.derive(owner, self.requests, peer, SEND.into())?;
            let reader = s.ipc.derive(owner, self.replies, peer, RECEIVE.into())?;
            s.ipc.locals.get_mut(peer.slot).bootstrap = writer;
            configure_session(s, peer, reader, rounds);
            Ok(())
        });
        if let Err(status) = result {
            retire_unpublished(child);
            return Err(status);
        }
        self.previous = peer;
        Ok(child)
    }

    pub(super) fn shutdown(&self) {
        with(|s| {
            for key in [self.requests, self.replies] {
                if let Ok(n) = s.ipc.lookup(self.process.0, key, MANAGE) {
                    s.ipc.close(n);
                }
            }
            s.flush_ipc();
            s.ipc.collect();
        });
    }
}

fn retire_unpublished(child: Child) {
    with(|s| {
        s.ipc.death(child.0, ticks());
        s.ipc.collect();
    });
    undo_unpublished(child);
}

pub(super) fn launch_session(
    service: &'static [u8],
    client: &'static [u8],
    rounds: u64,
) -> Res<(Service, Child)> {
    let _irq = Guard::new();
    let [server, reader] = launch([(service, ALL), (client, SEND)])?;
    let (owner, peer) = (server.0, reader.0);
    let result = with(|s| {
        let root = s.ipc.create(owner)?;
        let handle = s.ipc.derive(owner, root, peer, RECEIVE.into())?;
        configure_session(s, owner, root, 0);
        configure_session(s, peer, handle, rounds);
        Ok((s.ipc.caps(owner).unwrap().bootstrap, root))
    });
    match result {
        Ok((requests, replies)) => Ok((
            Service {
                process: server,
                requests,
                replies,
                previous: peer,
                client_image: client,
            },
            reader,
        )),
        Err(status) => {
            retire_unpublished(server);
            retire_unpublished(reader);
            Err(status)
        }
    }
}

pub(super) fn begin_measure(client: Pid, service: Pid) {
    with(|s| s.ipc.metrics.start_session(client.number, service.number));
}
pub(super) fn end_measure(hz: u64) {
    with(|s| s.ipc.metrics.report(hz));
}

/// Deux sessions privées : NovaFS ↔ service bloc ↔ pilote. Une image n'obtient
/// que les deux sens IPC de son voisin. Le droit matériel est lié au seul pilote.
pub(super) fn launch_block<const N: usize>(
    images: [(&'static [u8], u32); N],
    key: u64,
    mode: u64,
) -> Res<[Child; N]> {
    assert!(matches!(N, 3..=5));
    let private = crate::hardware::block::private_frames().map_err(|_| DENIED)?;
    let _guard = Guard::new();
    let group = launch(images)?;
    let (server, consumer, device) = (group[0].0, group[1].0, group[2].0);
    let configured = with(|s| {
        for pid in [device, server, consumer] {
            let space = s.processes.get(pid).space.as_ref().unwrap();
            if !heap::with_pages(|m| Ok(space.audit_driver(m, &private))).unwrap() {
                return Err(DENIED);
            }
        }
        let reply = s.ipc.create(server)?;
        let read_reply = s.ipc.derive(server, reply, consumer, RECEIVE.into())?;
        let input = s.ipc.create(device)?;
        let request = s.ipc.derive(device, input, server, SEND.into())?;
        let output = s.ipc.create(device)?;
        let response = s.ipc.derive(device, output, server, RECEIVE.into())?;
        s.ipc.locals.get_mut(device.slot).bootstrap = input;
        for (pid, words) in [
            (server, [reply, mode, request, response]),
            (device, [output, mode, key, 0]),
            (
                consumer,
                [read_reply, mode, if N == 3 { key } else { 0 }, 0],
            ),
        ] {
            let space = s.processes.get(pid).space.as_ref().unwrap();
            heap::with_pages(|m| {
                for (i, v) in words.into_iter().enumerate() {
                    space.write_data(m, i, v)
                }
                Ok(())
            })
            .unwrap();
        }
        for (i, member) in group[3..].iter().enumerate() {
            let control = member.0;
            let a = s.ipc.create(consumer)?;
            let b = s.ipc.create(consumer)?;
            let tx = s.ipc.derive(consumer, a, control, SEND.into())?;
            let rx = s.ipc.derive(consumer, b, control, RECEIVE.into())?;
            s.ipc.locals.get_mut(control.slot).bootstrap = tx;
            configure_session(s, control, rx, mode);
            let space = s.processes.get(consumer).space.as_ref().unwrap();
            let app = s.processes.get(control).space.as_ref().unwrap();
            // L'endpoint témoin appartient au client. Sa mort révoque la vue
            // RECEIVE du service sans toucher au canal de l'autre session.
            let life = if N == 5 {
                let token = s.ipc.create(control)?;
                s.ipc.derive(control, token, consumer, RECEIVE.into())?
            } else {
                0
            };
            heap::with_pages(|m| {
                app.write_data(m, 2, i as u64);
                space.write_data(m, 8 + i, life);
                space.write_data(m, 4 + 2 * i, a);
                space.write_data(m, 5 + 2 * i, b);
                Ok(())
            })
            .unwrap();
        }
        if N == 5 {
            // Canal du banc pour observer les deux CWD au même instant.
            let test = group[3].0;
            let peer = group[4].0;
            let token = s.ipc.create(test)?;
            let limited = s.ipc.derive(test, token, peer, (SEND | RECEIVE).into())?;
            for (pid, cap) in [(test, token), (peer, limited)] {
                let space = s.processes.get(pid).space.as_ref().unwrap();
                heap::with_pages(|m| {
                    space.write_data(m, 3, cap);
                    Ok(())
                })
                .unwrap();
            }
        }
        if mode == 36 {
            s.ipc
                .metrics
                .start_session(consumer.number, group[3].0.number);
        }
        if matches!(mode, 6 | 25) {
            s.ipc.metrics.start_session(consumer.number, server.number)
        }
        if !crate::hardware::block::attach(device.number, key) {
            return Err(DENIED);
        }
        Ok(())
    });
    if let Err(e) = configured {
        for c in group {
            retire_unpublished(c)
        }
        return Err(e);
    }
    Ok(group)
}

pub(super) fn close_block_channels(roles: &[Pid]) {
    with(|s| {
        for i in s.ipc.ends.ids() {
            let ep = *s.ipc.ends.get(i);
            if roles.iter().any(|p| p.number == ep.owner) {
                // Passer par close : il invalide également les attentes avant
                // leur réveil. destroy seul retirerait l'endpoint sous un dormeur.
                let root = s.ipc.node(ep.root).expect("racine de canal vivant");
                s.ipc.close(root)
            }
        }
        s.flush_ipc();
        s.ipc.collect();
    });
}
