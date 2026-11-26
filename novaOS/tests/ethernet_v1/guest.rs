use super::*;
#[allow(dead_code)]
mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/ethernet_v1/wire.rs"
    ));
}

#[derive(Clone, Copy)]
enum Path {
    Data,
    Box4k,
    Window,
}
impl Path {
    fn name(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Box4k => "boite",
            Self::Window => "fenetre",
        }
    }
}

fn message(
    from: Actor,
    tx: u64,
    to: Actor,
    rx: u64,
    tag: u64,
    payload: &[u8],
    counts: &mut Counts,
) -> [u8; 184] {
    let mut send = send_wire(tag, payload.len() as u32, 0);
    send[32..32 + payload.len()].copy_from_slice(payload);
    from.put(DESC, &send);
    assert_eq!(from.call(IPC_SEND, tx, DESC, 160), Ok(payload.len() as u64));
    counts.call();
    to.recv(rx, 0, false);
    assert_eq!(to.result(), Ok(1));
    counts.call();
    let got = to.bytes::<184>(OUTPUT);
    assert_eq!(u64_at(&got, 32), tag);
    assert_eq!(
        u32::from_le_bytes(got[28..32].try_into().unwrap()) as usize,
        payload.len()
    );
    assert_eq!(&got[56..56 + payload.len()], payload);
    got
}
fn bulk(who: Actor, op: u64, key: u64, tag: u64, addr: u64, len: usize, counts: &mut Counts) {
    let mut desc = [0u8; 32];
    for (dst, v) in desc
        .chunks_exact_mut(8)
        .zip([1u64 | (32 << 32), tag, addr, len as u64])
    {
        dst.copy_from_slice(&v.to_le_bytes());
    }
    who.put(DESC, &desc);
    assert_eq!(who.call(op, key, DESC, 32), Ok(len as u64));
    counts.call();
}
fn read(who: Actor, addr: u64, out: &mut [u8]) {
    for (i, part) in out.chunks_mut(4096).enumerate() {
        who.memory(|s, m| assert!(s.copy_from_user(m, addr + (i * 4096) as u64, part)));
    }
}

struct Channel {
    a: Actor,
    b: Actor,
    tx: u64,
    rx: u64,
    reply: u64,
    answer: u64,
    src: u64,
    dst: u64,
}
struct Counts {
    calls: u64,
    copies: u64,
    bytes: u64,
    min_free: Option<u64>,
}
impl Counts {
    fn call(&mut self) {
        self.calls += 1;
        if let Some(min) = &mut self.min_free {
            *min = (*min).min(heap::audit(false).free_pages);
        }
    }
}
impl Channel {
    fn copy_slots(&self, batch: usize, len: usize, snapshot: &mut [u8]) -> Res<()> {
        if batch == 0 || batch > 8 || !(14..=1514).contains(&len) || snapshot.len() != batch * len {
            return Err(BAD_ARGUMENT);
        }
        with(|s| {
            let tx = s.ipc.lookup(self.a.0, self.tx, SEND)?;
            let rx = s.ipc.lookup(self.b.0, self.rx, RECEIVE)?;
            if tx.endpoint != rx.endpoint {
                return Err(DENIED);
            }
            let src = s.processes.get(self.a.0).space.as_ref().unwrap();
            let dst = s.processes.get(self.b.0).space.as_ref().unwrap();
            heap::with_pages(|m| {
                for (i, part) in snapshot.chunks_mut(len).enumerate() {
                    let offset = (i * len) as u64;
                    if !dst.writable(m, self.dst + offset, len)
                        || !src.copy_from_user(m, self.src + offset, part)
                    {
                        return Ok(Err(BAD_ADDRESS));
                    }
                }
                for (i, part) in snapshot.chunks(len).enumerate() {
                    assert!(dst.copy_to_user(m, self.dst + (i * len) as u64, part));
                }
                Ok(Ok(()))
            })
            .unwrap()
        })
    }
    fn lot(
        &self,
        mode: Path,
        batch: usize,
        len: usize,
        seq: u64,
        source: &[u8],
        got: &mut [u8],
        min_free: Option<u64>,
    ) -> Counts {
        let mut header = [0u8; 128];
        let mut entries = [0u8; 96];
        let mut answer = [0u8; 96];
        for i in 0..batch {
            entries[i * 12..i * 12 + 12]
                .copy_from_slice(&wire::descriptor(i as u16, len as u16, seq as u32));
            answer[i * 12..i * 12 + 2].copy_from_slice(&(i as u16).to_le_bytes());
            answer[i * 12 + 4..i * 12 + 12].copy_from_slice(&seq.to_le_bytes());
        }
        let cmd = wire::Header {
            op: wire::TX,
            reply: false,
            count: batch as u16,
            epoch: 1,
            request: seq,
        };
        let n = cmd.encode(&mut header, &entries[..batch * 12]).unwrap();
        let mut counts = Counts {
            calls: 0,
            copies: 0,
            bytes: 0,
            min_free,
        };
        let size = batch * len;
        match mode {
            Path::Data => {
                let mut packet = [0u8; 128];
                for (i, frame) in source[..size].chunks_exact(len).enumerate() {
                    for (k, part) in frame.chunks(96).enumerate() {
                        packet[..8].copy_from_slice(&seq.to_le_bytes());
                        packet[8..10].copy_from_slice(&(i as u16).to_le_bytes());
                        packet[10..12].copy_from_slice(&((k * 96) as u16).to_le_bytes());
                        packet[12..14].copy_from_slice(&(len as u16).to_le_bytes());
                        let offset = (i * len + k * 96) as u64;
                        read(self.a, self.src + offset, &mut packet[32..32 + part.len()]);
                        let out = message(
                            self.a,
                            self.tx,
                            self.b,
                            self.rx,
                            seq,
                            &packet[..32 + part.len()],
                            &mut counts,
                        );
                        self.b.put(self.dst + offset, &out[88..88 + part.len()]);
                        counts.copies += 4;
                    }
                }
            }
            Path::Box4k => {
                let per = 4096 / len;
                for (i, part) in source[..size].chunks(per * len).enumerate() {
                    let offset = (i * per * len) as u64;
                    bulk(
                        self.a,
                        IPC_PUT,
                        self.tx,
                        seq,
                        self.src + offset,
                        part.len(),
                        &mut counts,
                    );
                    bulk(
                        self.b,
                        IPC_TAKE,
                        self.rx,
                        seq,
                        self.dst + offset,
                        part.len(),
                        &mut counts,
                    );
                    counts.copies += 2;
                }
            }
            Path::Window => {}
        }
        counts.bytes = if matches!(mode, Path::Data) { 4 } else { 2 } * size as u64;
        let received = message(
            self.a,
            self.tx,
            self.b,
            self.rx,
            seq,
            &header[..n],
            &mut counts,
        );
        assert_eq!(wire::Header::decode(&received[56..56 + n]).unwrap().0, cmd);
        if matches!(mode, Path::Window) {
            self.copy_slots(batch, len, &mut got[..size]).unwrap();
            counts.copies = 2 * batch as u64;
        }
        read(self.b, self.dst, &mut got[..size]);
        assert_eq!(&got[..size], &source[..size]);
        let receipt = wire::Header { reply: true, ..cmd };
        let n = receipt.encode(&mut header, &answer[..batch * 12]).unwrap();
        message(
            self.b,
            self.answer,
            self.a,
            self.reply,
            seq,
            &header[..n],
            &mut counts,
        );
        counts
    }
}
fn inventory() -> (usize, usize, usize, usize, u64, u64) {
    with(|s| {
        (
            s.processes.slots.iter().flatten().count(),
            s.ipc.ends.ids().count(),
            s.ipc.pages(),
            s.ipc.pages_bulk.ids().count(),
            s.stats.wakes,
            crate::sched::profile::switches_bsp(),
        )
    })
}
fn cell(mode: Path, batch: usize, len: usize) {
    use alloc::vec;
    let size = batch * len;
    let mut source = vec![0u8; size];
    let mut got = vec![0u8; size];
    let start = inventory();
    let free_base = heap::audit(false).free_pages;
    let (children, [a, b]) = group([ALL, RECEIVE]);
    let tx = a.boot();
    let rx = b.boot();
    let reply = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let answer = contract::transfer(a, b, tx, rx, reply, SEND, 1);
    if matches!(mode, Path::Box4k) {
        with(|s| s.ipc.bind_lane(a.0, tx, b.0, rx)).unwrap();
    }
    let base = |actor: Actor| {
        let h = actor.call(HEAP_RESIZE, 0, 0, 0).unwrap();
        let n = h + (size.div_ceil(4096) * 4096) as u64;
        assert_eq!(actor.call(HEAP_RESIZE, h, n, 0), Ok(n));
        h
    };
    let ch = Channel {
        a,
        b,
        tx,
        rx,
        reply,
        answer,
        src: base(a),
        dst: base(b),
    };
    let open = inventory();
    let roots = with(|s| {
        [a, b].map(|actor| {
            s.ipc
                .ends
                .ids()
                .filter(|&j| s.ipc.ends.get(j).owner == actor.0.number)
                .count()
        })
    });
    let free = heap::audit(false).free_pages;
    let mut min_free = free;
    let hz = hal::timer::counter_hz();
    for round in 0..17u64 {
        for (k, x) in source.iter_mut().enumerate() {
            *x = (round as usize * 17 + k * 29 + k / len) as u8;
        }
        for frame in source.chunks_exact_mut(len) {
            frame[..14].copy_from_slice(&[2, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 1, 0x88, 0xb5]);
        }
        for (i, part) in source.chunks(4096).enumerate() {
            a.put(ch.src + (i * 4096) as u64, part);
        }
        got.fill(0);
        let before = inventory();
        let t = hal::timer::counter();
        let c = ch.lot(mode, batch, len, round + 1, &source, &mut got, None);
        let dt = hal::timer::counter() - t;
        let after = inventory();
        min_free = min_free.min(heap::audit(false).free_pages);
        hal::log(format_args!(
            "ETH2 lot mode={} b={batch} len={len} tour={round} ticks={dt} hz={hz} calls={} copies={} bytes={} switches={} wakes={}",
            mode.name(),
            c.calls,
            c.copies,
            c.bytes,
            after.5 - before.5,
            after.4 - before.4
        ));
    }
    let memory_pass = ch.lot(mode, batch, len, 18, &source, &mut got, Some(min_free));
    min_free = memory_pass.min_free.unwrap();
    let t = hal::timer::counter();
    a.call(IPC_REVOKE, tx, 0, 0).unwrap();
    b.recv(rx, 0, false);
    assert_eq!(b.result(), Err(DENIED));
    if matches!(mode, Path::Window) {
        assert_eq!(ch.copy_slots(batch, len, &mut got), Err(DENIED));
    }
    a.call(IPC_REVOKE, reply, 0, 0).unwrap();
    stop(children, [a, b]);
    let revoke = hal::timer::counter() - t;
    let end = inventory();
    assert_eq!(
        (start.0, start.1, start.2, start.3),
        (end.0, end.1, end.2, end.3)
    );
    hal::log(format_args!(
        "ETH2 ressources mode={} b={batch} len={len} base={}/{}/{}/{} ouvert={}/{}/{}/{} racines={}/{} libres_base={free_base} libres_ouvert={free} libres_min_echant={min_free} libres_fin={} retrait_ticks={revoke} hz={hz} solde=0",
        mode.name(),
        start.0,
        start.1,
        start.2,
        start.3,
        open.0,
        open.1,
        open.2,
        open.3,
        roots[0],
        roots[1],
        heap::audit(false).free_pages
    ));
}
fn isolation() {
    let initial = inventory();
    let (first, [a, b]) = group([ALL, RECEIVE]);
    let (second, [c, d]) = group([ALL, RECEIVE]);
    let slot = user::DATA + 1024;
    let mk = |a: Actor, b: Actor, tx, rx| Channel {
        a,
        b,
        tx,
        rx,
        reply: 0,
        answer: 0,
        src: slot,
        dst: slot,
    };
    let (tx, rx, other_tx, other_rx) = (a.boot(), b.boot(), c.boot(), d.boot());
    a.put(slot, &[0x31; 60]);
    c.put(slot, &[0x72; 60]);
    b.put(slot, &[0; 60]);
    d.put(slot, &[0; 60]);
    let mut snapshot = [0; 60];
    assert_eq!(
        mk(a, d, tx, other_rx).copy_slots(1, 60, &mut snapshot),
        Err(DENIED)
    );
    assert_eq!(d.bytes::<60>(slot), [0; 60]);
    mk(a, b, tx, rx).copy_slots(1, 60, &mut snapshot).unwrap();
    assert_eq!(b.bytes::<60>(slot), [0x31; 60]);
    assert_eq!(d.bytes::<60>(slot), [0; 60]);
    a.call(IPC_REVOKE, tx, 0, 0).unwrap();
    assert_eq!(
        mk(a, b, tx, rx).copy_slots(1, 60, &mut snapshot),
        Err(DENIED)
    );
    mk(c, d, other_tx, other_rx)
        .copy_slots(1, 60, &mut snapshot)
        .unwrap();
    assert_eq!(d.bytes::<60>(slot), [0x72; 60]);
    stop(first, [a, b]);
    stop(second, [c, d]);
    let (fresh, [e, f]) = group([ALL, RECEIVE]);
    for actor in [e, f] {
        let heap_base = actor.call(HEAP_RESIZE, 0, 0, 0).unwrap();
        actor
            .call(HEAP_RESIZE, heap_base, heap_base + 4096, 0)
            .unwrap();
        let mut page = alloc::vec![0xff;4096];
        read(actor, heap_base, &mut page);
        assert!(page.iter().all(|&x| x == 0));
    }
    stop(fresh, [e, f]);
    let closed = inventory();
    assert_eq!(
        (initial.0, initial.1, initial.2, initial.3),
        (closed.0, closed.1, closed.2, closed.3)
    );
    hal::log(format_args!(
        "ETH2 isolation paires=2 croisement_refuse=1 revocation_locale=1 voisin_intact=1 pages_neuves_zero=2 solde=0"
    ));
}
pub(super) fn run() {
    isolation();
    crate::sched::profile::ACTIVE.store(true, Ordering::Relaxed);
    for b in [1, 2, 4, 8] {
        for len in [60, 1514] {
            for mode in [Path::Data, Path::Box4k, Path::Window] {
                cell(mode, b, len);
            }
        }
    }
    crate::sched::profile::ACTIVE.store(false, Ordering::Relaxed);
    hal::log(format_args!("ETH2 campagne OK"));
}
