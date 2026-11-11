//! Ordres imposés par le contrôleur hors du traitement de l'appel utilisateur.
use super::*;

pub(super) fn clock(now: Option<u64>) {
    with(|s| {
        s.ipc.probe_clock = now;
        s.ipc.expire(ticks());
        s.ipc.signal_heads();
        s.flush_ipc();
    });
}
fn pause(a: Actor) {
    PAUSED.store(a.0.number, Ordering::Relaxed);
}
fn resume() {
    PAUSED.store(0, Ordering::Relaxed);
}
fn inspect(a: Actor) -> (usize, Pending) {
    with(|s| {
        let i = s
            .ipc
            .waits
            .ids()
            .find(|&i| s.ipc.waits.get(i).pid == a.0)
            .unwrap();
        (i, *s.ipc.waits.get(i))
    })
}
pub(super) fn timing() {
    let (g, [a, b, c]) = group([ALL, ALL, ALL]);
    let (k, h, j) = (a.boot(), b.boot(), c.boot());
    for delay in [15001, u64::MAX] {
        b.recv(h, delay, false);
        assert_eq!(b.result(), Err(BAD_ARGUMENT));
    }
    b.recv(h, 1, false);
    assert_eq!(b.result(), Err(TIMED_OUT));
    for i in 0..3 {
        for action in 0..3 {
            let f = a.call(IPC_CREATE, 0, 0, 0).unwrap();
            let leaf = super::contract::transfer(a, b, k, h, f, ALL, 1);
            let base = 1000 + 100 * (3 * i + action);
            clock(Some(base));
            b.send(leaf, 7, 1, 10);
            b.blocked();
            pause(b);
            clock(Some(base + 9 + i));
            let outcome = match action {
                0 => {
                    a.recv(f, 0, false);
                    assert_eq!(a.result(), if i == 0 { Ok(1) } else { Err(WOULD_BLOCK) });
                    Ok(1)
                }
                1 => {
                    assert_eq!(a.call(IPC_CANCEL, f, 0, 0), Ok(u64::from(i == 0)));
                    Err(CANCELLED)
                }
                _ => {
                    assert_eq!(a.call(IPC_REVOKE, f, 0, 0), Ok(1));
                    Err(REVOKED)
                }
            };
            clock(Some(base + 11));
            resume();
            assert_eq!(b.result(), if i == 0 { outcome } else { Err(TIMED_OUT) });
            a.call(IPC_CLOSE, f, 0, 0).unwrap();
        }
    }
    clock(None);
    let sibling = super::contract::transfer(a, b, k, h, k, ALL, 1);
    b.recv(sibling, 15000, false);
    b.blocked();
    c.recv(j, 15000, false);
    c.blocked();
    // Le contrôleur exerce le droit réel de c depuis un second contexte du même PID.
    with(|s| {
        let n = s.ipc.lookup(c.0, j, MANAGE).unwrap();
        assert_eq!(s.ipc.cancel(n), 1);
        assert_eq!(s.ipc.cancel(n), 0);
        s.flush_ipc();
    });
    assert_eq!(c.result(), Err(CANCELLED));
    a.call(IPC_NOTIFY, k, 1, 0).unwrap();
    assert_eq!(b.result(), Ok(2));
    // Un receveur prêt garde sa place devant un nouvel essai immédiat.
    b.recv(h, 15000, false);
    b.blocked();
    pause(b);
    c.recv(j, 15000, false);
    c.blocked();
    a.call(IPC_NOTIFY, k, 1, 0).unwrap();
    a.recv(k, 0, false);
    assert_eq!(a.result(), Err(WOULD_BLOCK));
    resume();
    assert_eq!(b.result(), Ok(2));
    a.call(IPC_NOTIFY, k, 2, 0).unwrap();
    assert_eq!(c.result(), Ok(2));
    stop(g, [a, b, c]);
}

pub(super) fn stale_and_move() {
    let (g, [a, b]) = group([ALL, ALL]);
    let (k, h) = (a.boot(), b.boot());
    b.recv(h, 15000, true);
    b.blocked();
    let (_, old) = inspect(b);
    assert_eq!(a.call(IPC_CANCEL, k, 0, 0), Ok(1));
    assert_eq!(b.result(), Err(CANCELLED));
    stop(g, [a, b]);
    let (g, [a, b]) = group([ALL, ALL]);
    let (k2, h2) = (a.boot(), b.boot());
    assert_ne!(h, h2);
    assert_eq!(b.call(IPC_CLOSE, h, 0, 0), Err(DENIED));
    b.recv(h2, 15000, true);
    b.blocked();
    let (i, new) = inspect(b);
    assert_eq!(old.thread.slot, i);
    assert_ne!(old.thread.generation, new.thread.generation);
    assert_ne!(old.cookie, new.cookie);
    with(|s| {
        let count = s.stats.wakes;
        s.ipc.wake_version[i] = old.cookie;
        s.ipc.wakes |= 1 << i;
        s.flush_ipc();
        assert_eq!(count, s.stats.wakes);
        assert_eq!(s.thread(i).state, State::Blocked(Wait::Ipc(new.cookie)));
    });
    let src = a.call(IPC_DUP, k2, 5, 0).unwrap();
    // L'offre authentique précède une deuxième offre du même processus, injectée
    // car le lanceur de production ne fournit qu'un thread utilisateur par PID.
    pause(b);
    a.give(k2, src, 5, 2);
    // b était déjà bloqué. Geler sa reprise avant qu'un acteur puisse tourner.
    a.blocked();
    let (slot, offer) = inspect(a);
    with(|s| {
        let n = s.ipc.lookup(a.0, k2, SEND | GRANT).unwrap();
        let fake = Id {
            slot: 31,
            generation: u64::MAX,
        };
        assert_eq!(
            s.ipc.suspend(fake, a.0, n, 15000, ticks(), offer.op),
            Ok(None)
        );
        assert_ne!(slot, fake.slot);
    });
    resume();
    assert_eq!(b.result(), Ok(3));
    assert_eq!(a.result(), Ok(0));
    with(|s| {
        assert_eq!(s.ipc.waits.get(31).done, Some(Err(REVOKED)));
        s.ipc.waits.remove(31);
        s.ipc.collect();
    });
    assert_eq!(a.call(IPC_NOTIFY, src, 1, 0), Err(DENIED));
    stop(g, [a, b]);
}
pub(super) fn long() {
    let (g, [a, b]) = group([ALL, ALL]);
    let (k, h) = (a.boot(), b.boot());
    let before = with(|s| s.stats.ipc_preemptions);
    let mut previous = 0;
    for round in 1..=10000 {
        // Événement publié avant la vérification.
        a.send(k, round, 1, 0);
        assert_eq!(a.result(), Ok(1));
        b.recv(h, 0, false);
        assert_eq!(b.result(), Ok(1));
        super::contract::check_data(b, round, 1);
        // Événement postérieur à l'inscription.
        b.recv(h, 15000, false);
        b.blocked();
        a.send(k, round, 1, 0);
        assert_eq!(a.result(), Ok(1));
        assert_eq!(b.result(), Ok(1));
        super::contract::check_data(b, round, 1);
        for mode in 0..3 {
            let (channel, client) = if mode == 2 {
                let f = a.call(IPC_CREATE, 0, 0, 0).unwrap();
                (f, super::contract::transfer(a, b, k, h, f, RECEIVE, 1))
            } else {
                (k, h)
            };
            clock(Some(round * 100));
            pause(b);
            b.recv(client, 10, false);
            b.blocked();
            let (slot, w) = inspect(b);
            assert!(w.cookie > previous);
            previous = w.cookie;
            let wakes = with(|s| s.stats.wakes);
            a.send(channel, round, 1, 0);
            assert_eq!(a.result(), Ok(1));
            match mode {
                0 => assert_eq!(a.call(IPC_CANCEL, channel, 0, 0), Ok(1)),
                1 => clock(Some(round * 100 + 10)),
                _ => assert_eq!(a.call(IPC_CLOSE, channel, 0, 0), Ok(0)),
            }
            let terminal = [CANCELLED, TIMED_OUT, CLOSED][mode];
            with(|s| {
                assert_eq!(s.ipc.waits.get(slot).done, Some(Err(terminal)));
                assert_eq!(s.stats.wakes - wakes, 1);
                s.audit();
            });
            resume();
            assert_eq!(b.result(), Err(terminal));
            if mode != 2 {
                a.recv(channel, 0, false);
                assert_eq!(a.result(), Ok(1));
                super::contract::check_data(a, round, 1);
            }
            if round % 1000 == 0 {
                hal::log(format_args!(
                    "IPC trace tour={round} op={} rang={} echeance={} tick={} statut={terminal} reveils=1",
                    w.cookie,
                    w.rank,
                    w.deadline,
                    round * 100 + if mode == 1 { 10 } else { 0 }
                ));
            }
        }
    }
    clock(None);
    let preempt = with(|s| s.stats.ipc_preemptions) - before;
    assert!(preempt > 0);
    stop(g, [a, b]);
    hal::log(format_args!(
        "IPC T09 ordres=5 repetitions=10000 preemptions={preempt} OK"
    ));
}

/// Une offre MOVE est admise avant la reprise du receveur. Varier la seule
/// transaction qui intervient à cette frontière et solder exactement l'offre.
pub(super) fn composed() {
    for mode in 0..5 {
        let (g, [a, b, c]) = group([ALL, ALL, ALL]);
        let (root, bh, ch) = (a.boot(), b.boot(), c.boot());
        let source = b.call(IPC_CREATE, 0, 0, 0).unwrap();
        let moved = b.call(IPC_DUP, source, 5, 0).unwrap();
        c.memory(|space, m| space.add_second_data(m).unwrap());
        let out = user::DATA + crate::memory::PAGE - 96;
        let sentinel = receive_wire(10, mode == 4);
        c.put(out, &sentinel);
        clock(Some(100));
        pause(c);
        c.start(IPC_RECEIVE, ch, out, 184);
        c.blocked();
        let (_, receiver) = inspect(c);
        b.give(bh, moved, 5, 2);
        b.blocked();
        let (slot, offer) = inspect(b);
        let stack = with(|s| {
            let t = s.thread(slot);
            t.stack.top - t.saved
        });
        c.memory(|space, m| space.remove_second_data(m));
        match mode {
            0 => {
                resume();
                assert_eq!(c.result(), Err(BAD_ADDRESS));
                assert_eq!(c.bytes::<96>(out), sentinel[..96]);
                assert!(inspect(b).1.done.is_none());
                a.recv(root, 0, false);
                assert_eq!(a.result(), Err(REFUSED));
                assert_eq!(b.result(), Err(REFUSED));
            }
            1 => {
                clock(Some(110));
                assert_eq!(a.call(IPC_CANCEL, root, 0, 0), Ok(1));
                resume();
                assert_eq!(c.result(), Err(TIMED_OUT));
                assert_eq!(b.result(), Err(CANCELLED));
            }
            2 => {
                assert_eq!(a.call(IPC_CANCEL, root, 0, 0), Ok(2));
                clock(Some(110));
                resume();
                assert_eq!(c.result(), Err(CANCELLED));
                assert_eq!(b.result(), Err(CANCELLED));
            }
            3 => {
                a.call(IPC_CLOSE, root, 0, 0).unwrap();
                clock(Some(110));
                resume();
                assert_eq!(c.result(), Err(CLOSED));
                assert_eq!(b.result(), Err(CLOSED));
            }
            _ => {
                c.memory(|space, m| space.add_second_data(m).unwrap());
                resume();
                assert_eq!(c.result(), Ok(3));
                a.call(IPC_CLOSE, root, 0, 0).unwrap();
                assert_eq!(b.result(), Ok(0));
                let got = c.bytes::<184>(out);
                let new = u64_at(&got, 40);
                assert_ne!(new, moved);
                c.call(IPC_NOTIFY, new, 1, 0).unwrap();
            }
        }
        if mode != 4 {
            assert_eq!(c.bytes::<96>(out), sentinel[..96]);
            assert_eq!(b.call(IPC_NOTIFY, moved, 1, 0), Ok(0));
        } else {
            assert_eq!(b.call(IPC_NOTIFY, moved, 1, 0), Err(DENIED));
        }
        if mode < 3 {
            a.recv(root, 0, true);
            assert_eq!(a.result(), Err(WOULD_BLOCK));
        }
        assert_eq!(with(|s| s.ipc.waits.ids().count()), 0);
        hal::log(format_args!(
            "IPC composition ordre={mode} receveur={} offre={} rang={} admis=1 livres={} retires={} pile_suspendue={stack}",
            receiver.cookie,
            offer.cookie,
            offer.rank,
            u8::from(mode == 4),
            u8::from(mode != 4)
        ));
        clock(None);
        stop(g, [a, b, c]);
        assert_eq!(with(|s| s.ipc.pages()), 0);
    }
}
