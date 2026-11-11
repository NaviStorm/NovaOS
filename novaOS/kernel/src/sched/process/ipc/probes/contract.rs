use super::*;

pub(super) fn transfer(
    tx: Actor,
    rx: Actor,
    channel: u64,
    receiver: u64,
    source: u64,
    rights: u32,
    mode: u32,
) -> u64 {
    tx.give(channel, source, rights, mode);
    tx.blocked();
    rx.recv(receiver, 0, true);
    assert_eq!(rx.result(), Ok(3));
    assert_eq!(tx.result(), Ok(0));
    let b = rx.bytes::<184>(OUTPUT);
    let h = u64_at(&b, 40);
    let mut want = receive_wire(0, true);
    want[24..].fill(0);
    want[24..28].copy_from_slice(&3u32.to_le_bytes());
    want[32..40].copy_from_slice(&9u64.to_le_bytes());
    want[40..48].copy_from_slice(&h.to_le_bytes());
    want[48..56].copy_from_slice(&(rights as u64).to_le_bytes());
    assert_ne!(h, 0);
    assert_ne!(h, source);
    assert_eq!(b, want);
    h
}
fn take_data(rx: Actor, h: u64, tag: u64, len: u32) {
    rx.recv(h, 0, false);
    assert_eq!(rx.result(), Ok(1));
    check_data(rx, tag, len);
}
pub(super) fn check_data(rx: Actor, tag: u64, len: u32) {
    let out = rx.bytes::<184>(OUTPUT);
    assert_eq!(u64_at(&out, 24), 1 | (u64::from(len) << 32));
    assert_eq!(u64_at(&out, 32), tag);
    assert_eq!(u64_at(&out, 40) | u64_at(&out, 48), 0);
    let n = len as usize;
    assert_eq!(&out[56..56 + n], &send_wire(tag, len, 0)[32..32 + n]);
    assert!(out[56 + n..].iter().all(|&b| b == 0));
}
pub(super) fn formats() {
    let (g, [a, b]) = group([ALL, ALL]);
    let root = a.boot();
    let h = b.boot();
    let tag = 0x0102030405060708;
    let mut send = send_wire(tag, 3, 0);
    send[32..35].copy_from_slice(&[0x41, 0, 0xff]);
    a.put(DESC, &send);
    assert_eq!(a.call(IPC_SEND, root, DESC, 160), Ok(3));
    b.recv(h, 0, false);
    assert_eq!(b.result(), Ok(1));
    let mut want = receive_wire(0, false);
    want[24..].fill(0);
    want[24] = 1;
    want[28] = 3;
    want[32..40].copy_from_slice(&tag.to_le_bytes());
    want[56..59].copy_from_slice(&[65, 0, 255]);
    assert_eq!(b.bytes::<184>(OUTPUT), want);
    for (offset, bad, error) in [
        (0, 2u32, BAD_VERSION),
        (4, 159, BAD_ARGUMENT),
        (24, 129, BAD_ARGUMENT),
        (28, 2, BAD_ARGUMENT),
    ] {
        let mut bad_input = send;
        bad_input[offset..offset + 4].copy_from_slice(&bad.to_le_bytes());
        a.put(DESC, &bad_input);
        assert_eq!(a.call(IPC_SEND, root, DESC, 160), Err(error));
    }
    assert_eq!(a.call(IPC_SEND, root, DESC, 159), Err(BAD_ARGUMENT));
    assert_eq!(a.call(0x1002a, 0, 0, 0), Err(UNKNOWN_CALL));
    assert_eq!(a.call(0x20020, 0, 0, 0), Err(BAD_VERSION));
    stop(g, [a, b]);
}

pub(super) fn rights() {
    let (g, [a, b]) = group([ALL, 0]);
    let root = a.boot();
    assert_eq!(a.boot(), root);
    assert_eq!(b.call(IPC_BOOTSTRAP, 0, 0, 0), Err(DENIED));
    for key in [0, root, a.0.number, a.0.number | (1 << 63)] {
        assert_eq!(b.call(IPC_SEND, key, u64::MAX, 0), Err(DENIED));
    }
    b.put(DESC, &root.to_le_bytes());
    assert_eq!(b.call(IPC_CANCEL, root, 0, 0), Err(DENIED));
    for bits in 1..=ALL {
        let leaf = a.call(IPC_DUP, root, bits as u64, 0).unwrap();
        for (op, need) in [
            (IPC_SEND, SEND),
            (IPC_RECEIVE, RECEIVE),
            (IPC_GIVE, SEND | GRANT),
        ] {
            assert_eq!(
                a.call(op, leaf, u64::MAX, 0),
                Err(if bits & need == need {
                    BAD_ARGUMENT
                } else {
                    DENIED
                })
            );
        }
        let dup = a.call(IPC_DUP, leaf, bits as u64, 0);
        if bits & GRANT != 0 {
            a.call(IPC_CLOSE, dup.unwrap(), 0, 0).unwrap();
        } else {
            assert_eq!(dup, Err(DENIED));
        }
        for op in [IPC_CANCEL, IPC_REVOKE] {
            assert_eq!(
                a.call(op, leaf, 0, 0),
                if bits & MANAGE != 0 {
                    Ok(0)
                } else {
                    Err(DENIED)
                }
            );
        }
        assert_eq!(
            a.call(IPC_NOTIFY, leaf, 0, 0),
            Err(if bits & SEND != 0 {
                BAD_ARGUMENT
            } else {
                DENIED
            })
        );
        a.call(IPC_CLOSE, leaf, 0, 0).unwrap();
        assert_eq!(a.call(IPC_CLOSE, leaf, 0, 0), Err(DENIED));
    }
    a.call(IPC_CLOSE, root, 0, 0).unwrap();
    assert_eq!(a.call(IPC_BOOTSTRAP, 0, 0, 0), Err(DENIED));
    stop(g, [a, b]);
}

pub(super) fn tree() {
    let (g, [a, b, c]) = group([ALL, ALL, ALL]);
    let (k, bk, ck) = (a.boot(), b.boot(), c.boot());
    let f = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let anchor = a.call(IPC_DUP, f, 15, 0).unwrap();
    let branch = transfer(a, b, k, bk, anchor, ALL, 1);
    let middle = b.call(IPC_DUP, branch, 5, 0).unwrap();
    let leaf = transfer(b, c, bk, ck, middle, SEND, 1);
    assert_eq!(b.call(IPC_CLOSE, middle, 0, 0), Ok(0));
    let sister = a.call(IPC_DUP, f, 1, 0).unwrap();
    assert_eq!(c.call(IPC_NOTIFY, leaf, 1, 0), Ok(0));
    assert_eq!(a.call(IPC_REVOKE, anchor, 0, 0), Ok(2));
    assert_eq!(b.call(IPC_DUP, branch, 1, 0), Err(DENIED));
    assert_eq!(c.call(IPC_NOTIFY, leaf, 1, 0), Err(DENIED));
    assert_eq!(a.call(IPC_REVOKE, anchor, 0, 0), Ok(0));
    assert_eq!(a.call(IPC_NOTIFY, sister, 2, 0), Ok(0));
    stop(g, [a, b, c]);
}

pub(super) fn gifts() {
    let (g, [a, b]) = group([ALL, ALL]);
    let (k, bk) = (a.boot(), b.boot());
    let f = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let n = a.call(IPC_DUP, f, 5, 0).unwrap();
    let kid = a.call(IPC_DUP, n, 1, 0).unwrap();
    for (src, mask, mode, status) in [
        (n, 16, 1, BAD_ARGUMENT),
        (n, 7, 1, DENIED),
        (f, 15, 2, DENIED),
        (n, 1, 2, BAD_ARGUMENT),
        (n, 5, 0, BAD_ARGUMENT),
    ] {
        a.give(k, src, mask, mode);
        assert_eq!(a.result(), Err(status));
    }
    a.give(k, n, 5, 2);
    a.blocked();
    b.recv(bk, 0, false);
    assert_eq!(b.result(), Err(REFUSED));
    assert_eq!(a.result(), Err(REFUSED));
    let br = b.call(IPC_CREATE, 0, 0, 0).unwrap();
    let mut last = 0;
    for _ in 0..30 {
        last = b.call(IPC_DUP, br, 1, 0).unwrap();
    }
    a.give(k, n, 5, 2);
    a.blocked();
    b.recv(bk, 0, true);
    assert_eq!(b.result(), Err(LIMIT));
    assert_eq!(a.result(), Err(LIMIT));
    b.call(IPC_CLOSE, last, 0, 0).unwrap();
    let moved = transfer(a, b, k, bk, n, 5, 2);
    assert_eq!(a.call(IPC_NOTIFY, n, 1, 0), Err(DENIED));
    assert_eq!(a.call(IPC_NOTIFY, kid, 1, 0), Ok(0));
    assert_eq!(b.call(IPC_NOTIFY, moved, 2, 0), Ok(0));
    assert_eq!(a.call(IPC_REVOKE, f, 0, 0), Ok(2));
    stop(g, [a, b]);
}

pub(super) fn fifo() {
    let (g, [a, b, c]) = group([ALL, SEND, SEND]);
    let (k, bk, ck) = (a.boot(), b.boot(), c.boot());
    for i in 0..8 {
        b.send(bk, i, [0, 1, 128][i as usize % 3], 0);
        assert!(b.result().is_ok());
    }
    b.send(bk, 8, 0, 0);
    assert_eq!(b.result(), Err(WOULD_BLOCK));
    b.send(bk, 0, 129, 0);
    assert_eq!(b.result(), Err(BAD_ARGUMENT));
    let [pa, pb, pc] = g;
    b.start(PROCESS_EXIT, 0, 0, 0);
    assert_eq!(pb.join(), Outcome::Exit(0));
    c.send(ck, 8, 3, 15000);
    c.blocked();
    take_data(a, k, 0, 0);
    a.send(k, 9, 1, 0);
    assert_eq!(a.result(), Ok(1));
    for i in 1..8 {
        take_data(a, k, i, [0, 1, 128][i as usize % 3]);
    }
    take_data(a, k, 8, 3);
    assert_eq!(c.result(), Ok(3));
    take_data(a, k, 9, 1);
    stop([pa, pc], [a, c]);
}
pub(super) fn notifications() {
    let (g, [a, b]) = group([ALL, ALL]);
    let (k, h) = (a.boot(), b.boot());
    a.send(k, 3, 128, 0);
    assert_eq!(a.result(), Ok(128));
    a.call(IPC_NOTIFY, k, 1, 0).unwrap();
    b.send(h, 8, 0, 15000);
    b.blocked();
    for bits in [4, 4] {
        a.call(IPC_NOTIFY, k, bits, 0).unwrap();
    }
    take_data(a, k, 3, 128);
    a.recv(k, 0, false);
    assert_eq!(a.result(), Ok(2));
    let mut expected = receive_wire(0, false);
    expected[24..].fill(0);
    expected[24] = 2;
    expected[48] = 5;
    assert_eq!(a.bytes::<184>(OUTPUT), expected);
    a.call(IPC_NOTIFY, k, 8, 0).unwrap();
    take_data(a, k, 8, 0);
    assert_eq!(b.result(), Ok(0));
    a.recv(k, 0, false);
    assert_eq!(a.result(), Ok(2));
    assert_eq!(u64_at(&a.bytes::<184>(OUTPUT), 48), 8);
    a.recv(k, 0, false);
    assert_eq!(a.result(), Err(WOULD_BLOCK));
    assert_eq!(a.call(IPC_NOTIFY, k, 0, 0), Err(BAD_ARGUMENT));
    stop(g, [a, b]);
}
pub(super) fn revoke_inflight() {
    let (g, [a, b, c]) = group([ALL, ALL, ALL]);
    let (k, bk, ck) = (a.boot(), b.boot(), c.boot());
    let f = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let src = transfer(a, b, k, bk, f, 5, 1);
    b.give(bk, src, 1, 1);
    b.blocked();
    assert_eq!(a.call(IPC_REVOKE, f, 0, 0), Ok(1));
    assert_eq!(b.result(), Err(REVOKED));
    c.recv(ck, 0, true);
    assert_eq!(c.result(), Err(WOULD_BLOCK));
    let src = transfer(a, b, k, bk, f, 5, 1);
    b.give(bk, src, 5, 2);
    b.blocked();
    assert_eq!(a.call(IPC_REVOKE, f, 0, 0), Ok(1));
    assert_eq!(b.result(), Err(REVOKED));
    let src = transfer(a, b, k, bk, f, 5, 1);
    let dst = transfer(b, c, bk, ck, src, 1, 1);
    assert_eq!(a.call(IPC_REVOKE, f, 0, 0), Ok(2));
    assert_eq!(c.call(IPC_NOTIFY, dst, 1, 0), Err(DENIED));
    stop(g, [a, b, c]);
}
pub(super) fn deaths() {
    let (g, [a, b, c]) = group([ALL, ALL, ALL]);
    let (k, bk, ck) = (a.boot(), b.boot(), c.boot());
    let f = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let src = transfer(a, b, k, bk, f, 5, 1);
    b.give(bk, src, 1, 1);
    b.blocked();
    a.start(u64::MAX, 0, 0, 0);
    assert_eq!(b.result(), Err(CLOSED));
    assert_eq!(c.call(IPC_NOTIFY, ck, 1, 0), Err(DENIED));
    let [pa, pb, pc] = g;
    assert!(matches!(pa.join(), Outcome::Fault(_)));
    stop([pb, pc], [b, c]);
}

pub(super) fn epoch() {
    let base = heap::audit(false);
    let (g, [a, b, c]) = group([ALL, ALL, ALL]);
    let (k, bk, ck) = (a.boot(), b.boot(), c.boot());
    let f = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let n = transfer(a, b, k, bk, f, 5, 1);
    let moved = transfer(b, c, bk, ck, n, 5, 2);
    c.send(moved, 42, 128, 15000);
    c.blocked();
    assert_eq!(a.call(IPC_CANCEL, f, 0, 0), Ok(1));
    assert_eq!(c.result(), Err(CANCELLED));
    assert_eq!(a.call(IPC_REVOKE, f, 0, 0), Ok(1));
    a.start(u64::MAX, 0, 0, 0);
    let [pa, pb, pc] = g;
    assert!(matches!(pa.join(), Outcome::Fault(_)));
    stop([pb, pc], [b, c]);
    assert_eq!(base, heap::audit(false));
    assert_eq!(with(|s| s.ipc.pages()), 0);
}

pub(super) fn foreign_death() {
    // Les unités bloquées ne peuvent pas exécuter leur propre instruction de
    // faute. Forcer leur sortie sur une reprise parasite, avant tout commit.
    for receive in [false, true] {
        let (g, [a, b]) = group([ALL, ALL]);
        let (k, h) = (a.boot(), b.boot());
        if receive {
            b.recv(h, 15000, true);
        } else {
            b.give(h, h, 1, 1);
        }
        b.blocked();
        with(|s| {
            let slot = s
                .ipc
                .waits
                .ids()
                .find(|&i| s.ipc.waits.get(i).pid == b.0)
                .unwrap();
            EXIT_ON_RESUME.store(b.0.number, Ordering::Relaxed);
            s.ipc.wake_slot(slot);
            s.flush_ipc();
        });
        let [pa, pb] = g;
        assert_eq!(pb.join(), Outcome::Exit(37));
        a.recv(k, 0, true);
        assert_eq!(a.result(), Err(WOULD_BLOCK));
        a.call(IPC_NOTIFY, k, 1, 0).unwrap();
        a.recv(k, 0, false);
        assert_eq!(a.result(), Ok(2));
        stop([pa], [a]);
    }
    let (g, [a, b, c]) = group([ALL, ALL, ALL]);
    let (k, h, j) = (a.boot(), b.boot(), c.boot());
    let f = c.call(IPC_CREATE, 0, 0, 0).unwrap();
    let imported = transfer(c, b, j, h, f, ALL, 1);
    let leaf = transfer(b, a, h, k, imported, SEND, 1);
    b.start(u64::MAX, 0, 0, 0);
    let [pa, pb, pc] = g;
    assert!(matches!(pb.join(), Outcome::Fault(_)));
    a.call(IPC_NOTIFY, leaf, 8, 0).unwrap();
    c.recv(f, 0, false);
    assert_eq!(c.result(), Ok(2));
    let remote = transfer(c, a, j, k, f, ALL, 1);
    c.give(f, j, 1, 1);
    c.blocked();
    a.start(u64::MAX, 0, 0, 0);
    assert!(matches!(pa.join(), Outcome::Fault(_)));
    assert_eq!(c.result(), Err(REVOKED));
    assert_eq!(c.call(IPC_CLOSE, remote, 0, 0), Err(DENIED));
    stop([pc], [c]);
}
