use super::*;

fn dword(buf: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(buf[i..i + 4].try_into().unwrap())
}
fn word(buf: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(buf[i..i + 8].try_into().unwrap())
}
fn input<const N: usize>(
    space: &UserSpace,
    addr: u64,
    size: u64,
    wire: usize,
    output: bool,
) -> Res<[u8; N]> {
    if size != wire as u64 || addr % 8 != 0 {
        return Err(BAD_ARGUMENT);
    }
    let mut bytes = [0; N];
    let ok = heap::with_pages(|m| {
        Ok((!output || space.writable(m, addr, wire)) && space.copy_from_user(m, addr, &mut bytes))
    })
    .unwrap();
    if !ok {
        return Err(BAD_ADDRESS);
    }
    if dword(&bytes, 0) != VERSION {
        return Err(BAD_VERSION);
    }
    if dword(&bytes, 4) != wire as u32 {
        return Err(BAD_ARGUMENT);
    }
    Ok(bytes)
}

#[derive(Clone, Copy)]
enum Head {
    Deposit,
    Notification,
    EndOfStream,
    Offer(usize),
}

impl Domain {
    fn next_rank(&mut self, ep: Reference) -> Res<u64> {
        let e = self.ends.get_mut(ep.slot());
        if e.sequence == u64::MAX
            && e.len == 0
            && e.bits == 0
            && !self.waits.ids().any(|i| {
                let w = self.waits.get(i);
                w.ep == ep && w.done.is_none()
            })
        {
            e.sequence = 0;
        }
        e.sequence = e.sequence.checked_add(1).ok_or(LIMIT)?;
        Ok(e.sequence)
    }
    /// Notifications dérivées d'une source matérielle privée, au format IPC existant.
    #[cfg(feature = "materiel-lab")]
    pub(in crate::sched::process) fn lab_notify(&mut self, pid: Pid, key: u64, bits: u64) -> bool {
        let Ok(node) = self.lookup(pid, key, RECEIVE) else {
            return false;
        };
        let ep = self.ends.get(node.endpoint.slot());
        if node.rights != RECEIVE || ep.root != node.id || ep.owner != pid.number {
            return false;
        }
        if self.add_notification(node.endpoint, bits).is_err() {
            return false;
        }
        self.signal_heads();
        true
    }

    pub(in crate::sched::process) fn tmp_arm(&self, service: Pid, key: u64) -> Res<()> {
        self.lookup(service, key, RECEIVE).map(|_| ())
    }

    pub(in crate::sched::process) fn tmp_notice(&mut self, service: Pid, key: u64) {
        if let Ok(node) = self.lookup(service, key, RECEIVE) {
            if self.add_notification(node.endpoint, tmp::wire::WAKE).is_ok() {
                self.signal_heads();
            }
        }
    }

    pub(super) fn add_notification(&mut self, ep: Reference, bits: u64) -> Res<()> {
        if bits == 0 {
            return Err(BAD_ARGUMENT);
        }
        if self.ends.get(ep.slot()).bits == 0 {
            let rank = self.next_rank(ep)?;
            let e = self.ends.get_mut(ep.slot());
            e.notified = rank;
            e.notified_at = metrics::stamp();
        }
        self.ends.get_mut(ep.slot()).bits |= bits;
        Ok(())
    }

    fn head(&self, ep: Reference) -> Option<Head> {
        let e = self.ends.get(ep.slot());
        let mut first = (e.len != 0).then(|| (e.deposits[e.head].rank, Head::Deposit));
        if e.bits != 0 && first.is_none_or(|(r, _)| e.notified < r) {
            first = Some((e.notified, Head::Notification));
        }
        for i in self.waits.ids() {
            let w = self.waits.get(i);
            if w.ep == ep
                && w.done.is_none()
                && !matches!(w.op, Operation::Receive { .. })
                && first.is_none_or(|(r, _)| w.rank < r)
            {
                first = Some((w.rank, Head::Offer(i)));
            }
        }
        first.map(|(_, h)| h).or_else(|| e.pipe.filter(|p| p.writer_closed && !p.revoked).map(|_| Head::EndOfStream))
    }
    fn first_receiver(&self, ep: Reference) -> Option<usize> {
        self.waits
            .ids()
            .filter(|&i| {
                let w = self.waits.get(i);
                w.ep == ep && w.done.is_none() && matches!(w.op, Operation::Receive { .. })
            })
            .min_by_key(|&i| self.waits.get(i).rank)
    }
    pub(super) fn signal_heads(&mut self) {
        for i in self.waits.ids() {
            let w = self.waits.get(i);
            if w.done.is_none()
                && matches!(w.op, Operation::Receive { .. })
                && self.first_receiver(w.ep) == Some(i)
                && self.head(w.ep).is_some()
            {
                self.wake_slot(i);
            }
        }
    }
    // Un autre membre du PID peut retirer une pile ou une tranche du tas
    // pendant un RECEIVE. Ne jamais livrer plus tard dans un nouveau mapping
    // ayant repris la même adresse virtuelle.
    pub(in crate::sched::process) fn invalidate_receive(&mut self, pid: Pid, start: u64, end: u64) {
        for i in self.waits.ids() {
            let wait = self.waits.get(i);
            if wait.pid == pid && wait.done.is_none()
                && matches!(wait.op, Operation::Receive { address, .. }
                    if address < end && start < address.saturating_add(size_of::<ReceiveV1>() as u64))
            {
                self.complete(i, Err(BAD_ADDRESS));
            }
        }
        self.signal_heads();
    }

    fn check_wait(&self, pid: Pid, n: Node, timeout: u64, now: u64, receiver: bool) -> Res<()> {
        now.checked_add(timeout).ok_or(LIMIT)?;
        let mut by_pid = 0;
        let mut by_ep = 0;
        for i in self.waits.ids() {
            let w = self.waits.get(i);
            if w.done.is_some() {
                continue;
            }
            by_pid += usize::from(w.pid == pid);
            by_ep += usize::from(
                w.ep == n.endpoint && receiver == matches!(w.op, Operation::Receive { .. }),
            );
        }
        if by_pid >= 4 || by_ep >= 8 {
            return Err(LIMIT);
        }
        if self.operations == u64::MAX || self.waits.vacant().is_err() {
            return Err(LIMIT);
        }
        let ep = self.ends.get(n.endpoint.slot());
        if ep.sequence == u64::MAX
            && (ep.len != 0
                || ep.bits != 0
                || self.waits.ids().any(|i| {
                    let w = self.waits.get(i);
                    w.ep == n.endpoint && w.done.is_none()
                }))
        {
            return Err(LIMIT);
        }
        Ok(())
    }
    pub(super) fn suspend(
        &mut self,
        thread: Id,
        pid: Pid,
        n: Node,
        timeout: u64,
        now: u64,
        op: Operation,
    ) -> Res<Option<u64>> {
        self.check_wait(
            pid,
            n,
            timeout,
            now,
            matches!(op, Operation::Receive { .. }),
        )?;
        let deadline = now + timeout;
        self.operations = self.operations.checked_add(1).ok_or(LIMIT)?;
        let rank = self.next_rank(n.endpoint)?;
        self.waits.insert(
            thread.slot,
            Pending {
                thread,
                cookie: self.operations,
                pid,
                channel: Entry {
                    key: n.key,
                    node: n.id,
                },
                ep: n.endpoint,
                deadline,
                rank,
                op,
                done: None,
            },
        )?;
        self.metrics.admitted[thread.slot] = metrics::stamp();
        self.metrics.woken[thread.slot] = 0;
        Ok(None)
    }

    fn deliver(
        &mut self,
        pid: Pid,
        space: &UserSpace,
        ep: Reference,
        head: Head,
        op: Operation,
    ) -> Res<u64> {
        let Operation::Receive {
            address,
            options,
            timeout,
        } = op
        else {
            unreachable!()
        };
        let mut out = [0u8; 184];
        out[..4].copy_from_slice(&VERSION.to_le_bytes());
        out[4..8].copy_from_slice(&184u32.to_le_bytes());
        out[8..16].copy_from_slice(&timeout.to_le_bytes());
        out[16..20].copy_from_slice(&options.to_le_bytes());
        // Ce verrou couvre la revalidation, la copie et la publication logique.
        // Le commit manipule exclusivement des slots déjà provisionnés.
        heap::with_pages(|m| {
            Ok((|| {
                if !space.writable(m, address, out.len()) {
                    return Err(BAD_ADDRESS);
                }
                let mut install = None;
                let (kind, msg, aux, key) = match head {
                    Head::Deposit => {
                        let e = self.ends.get(ep.slot());
                        (1u32, e.deposits[e.head].msg, 0, 0)
                    }
                    Head::EndOfStream => (2, Message::EMPTY, 1, 0),
                    Head::Notification => (2, Message::EMPTY, self.ends.get(ep.slot()).bits, 0),
                    Head::Offer(i) => match self.waits.get(i).op {
                        Operation::Send(msg) => (1, msg, 0, 0),
                        Operation::Give {
                            source,
                            child,
                            rights,
                            tag,
                        } => {
                            let offer = *self.waits.get(i);
                            if self
                                .lookup(offer.pid, offer.channel.key, SEND | GRANT)
                                .is_err()
                                || self.lookup(offer.pid, source.key, GRANT).is_err()
                            {
                                self.complete(i, Err(REVOKED));
                                return Err(REVOKED);
                            }
                            let admission = (|| {
                                if options & 1 == 0 {
                                    return Err(REFUSED);
                                }
                                Ok((self.free_local(pid)?, self.key()?))
                            })();
                            let (slot, key) = match admission {
                                Ok(v) => v,
                                Err(e) => {
                                    self.complete(i, Err(e));
                                    return Err(e);
                                }
                            };
                            let moved = child.0 == 0;
                            let id = if moved { source.node } else { child };
                            install = Some((slot, key, id, moved));
                            (
                                3,
                                Message {
                                    tag,
                                    ..Message::EMPTY
                                },
                                rights as u64,
                                key,
                            )
                        }
                        _ => unreachable!(),
                    },
                };
                out[24..28].copy_from_slice(&kind.to_le_bytes());
                out[28..32].copy_from_slice(&msg.len.to_le_bytes());
                out[32..40].copy_from_slice(&msg.tag.to_le_bytes());
                out[40..48].copy_from_slice(&key.to_le_bytes());
                out[48..56].copy_from_slice(&aux.to_le_bytes());
                out[56..].copy_from_slice(&msg.bytes);
                if !space.copy_to_user(m, address, &out) {
                    return Err(BAD_ADDRESS);
                }
                if let Some((slot, key, id, moved)) = install {
                    if moved {
                        self.unlink(id);
                    }
                    self.install(pid, slot, id, key);
                }
                if kind == 1 && let Some(pipe) = &mut self.ends.get_mut(ep.slot()).pipe
                    && pipe.trace {
                    if pipe.delivered == 0 {
                        hal::log(format_args!("PIPE premiere reception octets={}", msg.len));
                    }
                    pipe.delivered += u64::from(msg.len);
                    for b in &msg.bytes[..msg.len as usize] {
                        pipe.fingerprint = (pipe.fingerprint ^ u64::from(*b)).wrapping_mul(0x100000001b3);
                    }
                }
                let admitted = match head {
                    Head::Deposit => {
                        let e = self.ends.get(ep.slot());
                        e.deposits[e.head].admitted
                    }
                    Head::EndOfStream => metrics::stamp(),
                    Head::Notification => self.ends.get(ep.slot()).notified_at,
                    Head::Offer(i) => self.metrics.admitted[i],
                };
                match head {
                    Head::Deposit => {
                        let e = self.ends.get_mut(ep.slot());
                        e.deposits[e.head].msg = Message::EMPTY;
                        e.head = (e.head + 1) % 8;
                        e.len -= 1;
                    }
                    Head::EndOfStream => (),
                    Head::Notification => self.ends.get_mut(ep.slot()).bits = 0,
                    Head::Offer(i) => self.complete(i, Ok(u64::from(msg.len))),
                }
                if install.is_some_and(|v| v.3) {
                    self.sweep();
                }
                if self.metrics.includes(pid) {
                    self.metrics.sample(2, admitted, metrics::stamp());
                }
                Ok(kind as u64)
            })())
        })
        .unwrap()
    }

    fn start_call(
        &mut self,
        p: Pid,
        t: Id,
        space: &UserSpace,
        id: u64,
        [a, b, c]: [u64; 3],
        now: u64,
    ) -> Res<Option<u64>> {
        if id == IPC_CREATE {
            return self.create(p).map(Some);
        }
        if id == IPC_BOOTSTRAP {
            return self
                .caps(p)
                .map(|v| v.bootstrap)
                .filter(|&k| k != 0)
                .map(Some)
                .ok_or(DENIED);
        }
        let need = match id {
            IPC_DUP => GRANT,
            IPC_CLOSE => 0,
            IPC_REVOKE | IPC_CANCEL => MANAGE,
            IPC_SEND | IPC_NOTIFY => SEND,
            IPC_RECEIVE => RECEIVE,
            IPC_GIVE => SEND | GRANT,
            _ => return Err(UNKNOWN_CALL),
        };
        let n = self.lookup(p, a, need)?;
        let ep = n.endpoint;
        if let Some(pipe) = self.ends.get(ep.slot()).pipe {
            if id == IPC_NOTIFY { return Err(DENIED); }
            if id == IPC_SEND && pipe.reader_closed { return Err(CLOSED); }
        }
        let value = match id {
            IPC_DUP => self.derive(p, a, p, b)?,
            IPC_CLOSE => {
                self.close(n);
                0
            }
            IPC_REVOKE => self.revoke(n),
            IPC_CANCEL => self.cancel(n),
            IPC_NOTIFY => {
                self.add_notification(ep, b)?;
                0
            }
            IPC_SEND => {
                let buf = input::<160>(space, b, c, 160, false)?;
                let timeout = word(&buf, 8);
                let len = dword(&buf, 24);
                let flags = dword(&buf, 28);
                if len > 128 || flags > 1 || timeout > 15000 || (flags == 0) != (timeout == 0) {
                    return Err(BAD_ARGUMENT);
                }
                let mut msg = Message {
                    tag: word(&buf, 16),
                    len,
                    ..Message::EMPTY
                };
                msg.bytes[..len as usize].copy_from_slice(&buf[32..32 + len as usize]);
                if flags == 1 {
                    let pending = self.suspend(t, p, n, timeout, now, Operation::Send(msg))?;
                    let end = self.ends.get_mut(ep.slot());
                    if let Some(pipe) = &mut end.pipe && pipe.trace {
                        if pipe.blocked == 0 {
                            let bytes: u32 = (0..end.len)
                                .map(|i| end.deposits[(end.head + i) % end.deposits.len()].msg.len)
                                .sum();
                            hal::log(format_args!("PIPE attente producteur depots={} octets={bytes}", end.len));
                        }
                        pipe.blocked = pipe.blocked.saturating_add(1);
                    }
                    return Ok(pending);
                }
                if self.ends.get(ep.slot()).len == 8 {
                    return Err(WOULD_BLOCK);
                }
                let rank = self.next_rank(ep)?;
                let e = self.ends.get_mut(ep.slot());
                e.deposits[(e.head + e.len) % 8] = Deposit {
                    rank,
                    msg,
                    admitted: metrics::stamp(),
                };
                e.len += 1;
                if let Some(pipe) = &mut e.pipe && pipe.trace { pipe.peak = pipe.peak.max(e.len); }
                if self.metrics.includes(p) {
                    self.metrics.sample(1, self.metrics.entry, metrics::stamp());
                }
                len as u64
            }
            IPC_RECEIVE => {
                let buf = input::<24>(space, b, c, 184, true)?;
                let timeout = word(&buf, 8);
                let options = dword(&buf, 16);
                if options > 1 || dword(&buf, 20) != 0 || timeout > 15000 {
                    return Err(BAD_ARGUMENT);
                }
                now.checked_add(timeout).ok_or(LIMIT)?;
                let op = Operation::Receive {
                    address: b,
                    options,
                    timeout,
                };
                if self.first_receiver(ep).is_none() {
                    if let Some(head) = self.head(ep) {
                        return self.deliver(p, space, ep, head, op).map(Some);
                    }
                }
                if timeout == 0 {
                    return Err(WOULD_BLOCK);
                }
                let pending = self.suspend(t, p, n, timeout, now, op)?;
                if self.ends.get(ep.slot()).pipe.is_some_and(|p| p.trace && p.delivered == 0) {
                    hal::log(format_args!("PIPE attente lecteur pid={}", p.number));
                }
                return Ok(pending);
            }
            IPC_GIVE => {
                let buf = input::<48>(space, b, c, 48, false)?;
                let timeout = word(&buf, 8);
                let mode = dword(&buf, 36);
                let mask = dword(&buf, 32);
                if !(1..=15000).contains(&timeout)
                    || !(1..=2).contains(&mode)
                    || word(&buf, 40) != 0
                    || mask == 0
                    || mask & !ALL != 0
                {
                    return Err(BAD_ARGUMENT);
                }
                let source = self.lookup(p, word(&buf, 16), GRANT)?;
                let rights = Self::check_mask(source, mask.into())?;
                if mode == 2 {
                    if source.parent.0 == 0 {
                        return Err(DENIED);
                    }
                    if source.key == a || source.rights != rights {
                        return Err(BAD_ARGUMENT);
                    }
                }
                self.check_wait(p, n, timeout, now, false)?;
                let child = if mode == 1 {
                    self.reserve_child(source, rights)?
                } else {
                    Reference(0)
                };
                let op = Operation::Give {
                    source: Entry {
                        key: source.key,
                        node: source.id,
                    },
                    child,
                    tag: word(&buf, 24),
                    rights,
                };
                let r = self.suspend(t, p, n, timeout, now, op);
                if r.is_err() && child.0 != 0 {
                    self.nodes.remove(child.slot());
                }
                return r;
            }
            _ => unreachable!(),
        };
        Ok(Some(value))
    }
    pub(super) fn step(
        &mut self,
        p: Pid,
        t: Id,
        space: &UserSpace,
        id: u64,
        args: [u64; 3],
        now: u64,
    ) -> Res<Option<u64>> {
        let now = self.probe_clock.unwrap_or(now);
        self.expire(now);
        let result = if self.waits.has(t.slot) {
            let w = *self.waits.get(t.slot);
            assert_eq!(w.thread, t);
            if let Some(done) = w.done {
                done.map(Some)
            } else if matches!(w.op, Operation::Receive { .. })
                && self.first_receiver(w.ep) == Some(t.slot)
            {
                if let Some(head) = self.head(w.ep) {
                    self.deliver(p, space, w.ep, head, w.op).map(Some)
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        } else {
            self.start_call(p, t, space, id, args, now)
        };
        if result != Ok(None) && self.waits.has(t.slot) {
            self.waits.remove(t.slot);
        }
        self.empty_locals();
        self.signal_heads();
        self.collect();
        result
    }
}

pub(super) fn invoke(id: u64, args: [u64; 3]) -> Res<u64> {
    let entry = metrics::stamp();
    loop {
        let irq = Guard::new();
        let begin = metrics::stamp();
        let (outcome, cookie, pid) = crate::sched::with_guard(&irq, |s| {
            let t = s.thread(s.current());
            let p = t.owner.unwrap();
            let tid = t.id;
            // death() a retiré les attentes du PID. Une pile réveillée pendant
            // la destruction ne doit pas réémettre son ancienne opération.
            if s.processes.get(p).outcome.is_some() {
                return (Err(CANCELLED), 0, p.number);
            }
            let space = s.processes.get(p).space.as_ref().unwrap();
            let resuming = s.ipc.waits.has(tid.slot);
            s.ipc.metrics.entry = entry;
            if resuming && s.ipc.metrics.includes(p) {
                let w = core::mem::take(&mut s.ipc.metrics.woken[tid.slot]);
                s.ipc.metrics.sample(5, w, begin);
            }
            if p.number == s.ipc.metrics.focus
                && !resuming
                && matches!(id, IPC_SEND | IPC_NOTIFY | IPC_GIVE)
            {
                s.ipc.metrics.round = entry;
            }
            let r = if !resuming
                && matches!(id, IPC_SEND | IPC_NOTIFY | IPC_CLOSE)
                && exec::is_port(s, p, args[0])
            {
                ticket_request(s, p, id, args).map(Some)
            } else {
                s.ipc.step(p, tid, space, id, args, ticks())
            };
            s.flush_ipc();
            let cookie = if r == Ok(None) {
                s.ipc.waits.get(tid.slot).cookie
            } else {
                0
            };
            let end = metrics::stamp();
            if r.is_ok_and(|v| v.is_some()) && s.ipc.metrics.includes(p) {
                s.ipc.metrics.sample(6, entry, end);
                if p.number == s.ipc.metrics.focus && id == IPC_RECEIVE {
                    let round = core::mem::take(&mut s.ipc.metrics.round);
                    s.ipc.metrics.sample(4, round, end);
                }
            }
            s.ipc.metrics.section(begin, end, id, entry, r);
            (r, cookie, p.number)
        });
        if let Some(value) = outcome? {
            return Ok(value);
        }
        dispatch_measured(Reason::Block(Wait::Ipc(cookie)), begin);
        drop(irq);
        probes::after_wake(pid);
    }
}

/// Consultation synchrone du reçu sur une capacité IPC locale non transmissible.
/// La réponse emprunte la file bornée et le chemin RECEIVE ordinaires.
fn ticket_request(s: &mut Core, owner: Pid, id: u64, [key, address, size]: [u64; 3]) -> Res<u64> {
    if id == IPC_CLOSE {
        // Une admission en vol garde son reçu interrogeable après un retour perdu.
        if exec::claiming(s.processes.get(owner)) {
            return Err(WOULD_BLOCK);
        }
        let root = s.ipc.lookup(owner, key, 0)?;
        s.processes.get_mut(owner).exec.clear();
        s.ipc.close(root);
        return Ok(0);
    }
    if id != IPC_SEND {
        return Err(DENIED);
    }
    let root = s.ipc.lookup(owner, key, SEND)?;
    let space = s.processes.get(owner).space.as_ref().ok_or(CANCELLED)?;
    let q = input::<160>(space, address, size, 160, false)?;
    let flags = dword(&q, 28);
    let timeout = word(&q, 8);
    if flags > 1
        || timeout > 15000
        || (flags == 0) != (timeout == 0)
        || dword(&q, 24) != 8
        || &q[32..40] != b"NXQUERY1"
    {
        return Err(BAD_ARGUMENT);
    }
    let ep = root.endpoint;
    if s.ipc.ends.get(ep.slot()).len == 8 {
        return Err(WOULD_BLOCK);
    }
    let (status, pid, wait) = match exec::outcome(s, owner, key) {
        Ok((pid, wait)) => (OK, pid.number, wait.unwrap_or(0)),
        Err(e) => (e, 0, 0),
    };
    let rank = s.ipc.next_rank(ep)?;
    let mut msg = Message {
        tag: word(&q, 16),
        len: 32,
        ..Message::EMPTY
    };
    msg.bytes[..8].copy_from_slice(b"NXREPLY1");
    for (i, v) in [status, pid, wait].into_iter().enumerate() {
        msg.bytes[8 + i * 8..16 + i * 8].copy_from_slice(&v.to_le_bytes());
    }
    let end = s.ipc.ends.get_mut(ep.slot());
    let at = (end.head + end.len) % end.deposits.len();
    end.deposits[at] = Deposit {
        rank,
        msg,
        admitted: metrics::stamp(),
    };
    end.len += 1;
    s.ipc.signal_heads();
    Ok(8)
}
