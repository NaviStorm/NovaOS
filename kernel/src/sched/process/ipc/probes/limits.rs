//! Saturation des structures de production et pénuries du gestionnaire physique.
use super::*;

fn with_budget(budget: u64, f: impl FnOnce()) {
    let base = heap::audit(false);
    let mut held = heap::with_pages(|m| m.hold_for_probe(budget)).unwrap();
    f();
    assert_eq!(heap::audit(false).free_pages, budget);
    heap::with_pages(|m| {
        held.restore_to(m, base.free_pages);
        Ok(())
    })
    .unwrap();
    assert_eq!(heap::audit(false), base);
}

fn shortage() {
    let c = spawn_image(IMAGE, &[], None, None).unwrap();
    let a = Actor(c.0);
    let base = heap::audit(false);
    let mut held = heap::with_pages(|m| m.hold_for_probe(0)).unwrap();
    let mut success = false;
    for budget in 0..=8 {
        heap::with_pages(|m| {
            held.restore_to(m, budget);
            Ok(())
        })
        .unwrap();
        match a.call(IPC_CREATE, 0, 0, 0) {
            Ok(k) => {
                a.call(IPC_CLOSE, k, 0, 0).unwrap();
                success = true;
            }
            Err(status) => assert_eq!(status, NO_MEMORY),
        }
        assert_eq!(with(|s| s.ipc.pages()), 0);
        assert_eq!(heap::audit(false).free_pages, budget);
        if success {
            hal::log(format_args!("IPC creation premier_budget={budget}"));
            break;
        }
    }
    assert!(success);
    heap::with_pages(|m| {
        held.restore_to(m, base.free_pages);
        Ok(())
    })
    .unwrap();
    assert_eq!(base, heap::audit(false));
    stop([c], [a]);
    measure("ipc_reprise");
}

pub(super) fn quotas() {
    let base = heap::audit(false);
    assert!(matches!(
        launch([(IMAGE, ALL), (IMAGE, SEND), (IMAGE, 16)]),
        Err(BAD_ARGUMENT)
    ));
    assert_eq!(heap::audit(false), base);
    shortage();
    let (children, actors) = group({
        let mut r = [0; 17];
        r[0] = ALL;
        r
    });
    let mut roots = [[0; 4]; 16];
    measure("ipc_actifs");
    for (i, a) in actors[..16].iter().copied().enumerate() {
        for (j, k) in roots[i].iter_mut().enumerate() {
            *k = if i == 0 && j == 0 {
                a.boot()
            } else {
                a.call(IPC_CREATE, 0, 0, 0).unwrap()
            };
        }
        assert_eq!(a.call(IPC_CREATE, 0, 0, 0), Err(LIMIT));
    }
    assert_eq!(actors[16].call(IPC_CREATE, 0, 0, 0), Err(LIMIT));
    let a = actors[0];
    let k = roots[0][0];
    with_budget(0, || {
        assert_eq!(a.call(IPC_DUP, k, 1, 0), Err(NO_MEMORY));
        a.give(k, roots[0][1], 1, 1);
        assert_eq!(a.result(), Err(NO_MEMORY));
    });
    with_budget(1, || {
        a.give(k, roots[0][1], 1, 1);
        assert_eq!(a.result(), Err(NO_MEMORY));
    });
    for i in 0..16 {
        for _ in 0..12 {
            actors[i].call(IPC_DUP, roots[i][0], 1, 0).unwrap();
        }
    }
    assert_eq!(a.call(IPC_DUP, k, 1, 0), Err(LIMIT));
    // 32 TCB et quatre attentes par PID. Les slots de l'idle et du contrôleur
    // sont empruntés uniquement dans cette transaction de comptabilité, sans dispatch.
    with(|s| {
        for i in 0..32 {
            let owner = actors[i / 4].0;
            let n = s.ipc.lookup(owner, roots[i / 4][0], SEND).unwrap();
            let id = Id {
                slot: i,
                generation: u64::MAX - i as u64,
            };
            assert_eq!(
                s.ipc
                    .suspend(id, owner, n, 100, ticks(), Operation::Send(Message::EMPTY)),
                Ok(None)
            );
            if i == 3 {
                assert_eq!(
                    s.ipc.suspend(
                        Id {
                            slot: 4,
                            generation: 99
                        },
                        owner,
                        n,
                        100,
                        ticks(),
                        Operation::Send(Message::EMPTY)
                    ),
                    Err(LIMIT)
                );
            }
        }
        let owner = actors[8].0;
        let n = s.ipc.lookup(owner, roots[8][0], SEND).unwrap();
        assert_eq!(
            s.ipc.suspend(
                Id {
                    slot: 32,
                    generation: 99
                },
                owner,
                n,
                100,
                ticks(),
                Operation::Send(Message::EMPTY)
            ),
            Err(LIMIT)
        );
        assert_eq!(s.ipc.waits.ids().count(), 32);
        hal::log(format_args!(
            "IPC attentes_synthetiques=32 max_pid=4 pages_attentes={} pages_totales={}",
            s.ipc.waits.pages(),
            s.ipc.pages()
        ));
        for i in 0..32 {
            s.ipc.waits.remove(i);
        }
        s.ipc.collect();
    });
    measure("ipc_saturation");
    with(|s| {
        let d = &s.ipc;
        let octets = d.ends.ids().count() * size_of::<Endpoint>()
            + d.nodes.ids().count() * size_of::<Node>()
            + d.locals.ids().count() * size_of::<LocalCaps>()
            + d.waits.ids().count() * size_of::<Pending>();
        hal::log(format_args!(
            "IPC saturation endpoints={} noeuds={} locales={} attentes={} octets_vivants={octets} statique={} pages={}",
            d.ends.pages(),
            d.nodes.pages(),
            d.locals.pages(),
            d.waits.pages(),
            size_of::<Domain>(),
            d.pages()
        ));
        assert_eq!(d.nodes.ids().count(), 256);
        assert!((d.pages() + 3) * 4096 <= 512 * 1024);
    });
    stop(children, actors);
    let (g, [a, b]) = group([ALL, ALL]);
    let (k, bk) = (a.boot(), b.boot());
    let mut leaf = k;
    for _ in 0..8 {
        leaf = a.call(IPC_DUP, leaf, ALL.into(), 0).unwrap();
    }
    assert_eq!(a.call(IPC_DUP, leaf, 1, 0), Err(LIMIT));
    for _ in 10..32 {
        a.call(IPC_DUP, k, 1, 0).unwrap();
    }
    assert_eq!(b.call(IPC_DUP, bk, 1, 0), Err(LIMIT));
    let other = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    with_budget(0, || {
        b.give(bk, bk, 1, 1);
        assert_eq!(b.result(), Err(LIMIT));
        a.give(k, other, 1, 1);
        assert_eq!(a.result(), Err(NO_MEMORY));
    });
    a.give(k, other, 1, 1);
    a.blocked();
    b.recv(bk, 0, true);
    assert_eq!(b.result(), Ok(3));
    assert_eq!(a.result(), Ok(0));
    stop(g, [a, b]);
    let (g, actors) = group([ALL; 10]);
    let handles = actors.map(Actor::boot);
    for receive in [false, true] {
        for i in 1..10 {
            if receive {
                actors[i].recv(handles[i], 15000, false)
            } else {
                actors[i].send(handles[i], i as u64, 0, 15000)
            }
            if i < 9 {
                actors[i].blocked();
            } else {
                assert_eq!(actors[i].result(), Err(LIMIT));
            }
        }
        assert_eq!(actors[0].call(IPC_CANCEL, handles[0], 0, 0), Ok(8));
        for a in &actors[1..9] {
            assert_eq!(a.result(), Err(CANCELLED));
        }
    }
    stop(g, actors);
}

pub(super) fn saturation() {
    let (g, [a, b]) = group([ALL, ALL]);
    let (k, bk) = (a.boot(), b.boot());
    super::races::clock(Some(u64::MAX - 1));
    b.recv(bk, 2, false);
    assert_eq!(b.result(), Err(LIMIT));
    b.recv(bk, 1, false);
    b.blocked();
    super::races::clock(Some(u64::MAX));
    assert_eq!(b.result(), Err(TIMED_OUT));
    b.recv(bk, 1, false);
    assert_eq!(b.result(), Err(LIMIT));
    super::races::clock(None);
    with(|s| {
        let ep = s.ipc.lookup(a.0, k, 0).unwrap().endpoint;
        s.ipc.ends.get_mut(ep.slot()).sequence = u64::MAX - 1;
    });
    a.send(k, 1, 0, 0);
    assert_eq!(a.result(), Ok(0));
    a.send(k, 2, 0, 0);
    assert_eq!(a.result(), Err(LIMIT));
    b.recv(bk, 0, false);
    assert_eq!(b.result(), Ok(1));
    a.send(k, 3, 0, 0);
    assert_eq!(a.result(), Ok(0));
    with(|s| {
        s.ipc.incarnation = (u64::MAX >> 8) - 1;
        s.ipc.issued = (1 << 62) - 2;
        s.ipc.operations = u64::MAX - 1;
    });
    let last = a.call(IPC_DUP, k, 1, 0).unwrap();
    assert_eq!(last, (1 << 63) - 1);
    a.call(IPC_CLOSE, last, 0, 0).unwrap();
    assert_eq!(a.call(IPC_DUP, k, 1, 0), Err(LIMIT));
    assert_eq!(a.call(IPC_CREATE, 0, 0, 0), Err(LIMIT));
    assert_eq!(a.call(IPC_CLOSE, last, 0, 0), Err(DENIED));
    b.recv(bk, 0, false);
    assert_eq!(b.result(), Ok(1));
    b.recv(bk, 15000, false);
    b.blocked();
    assert_eq!(a.call(IPC_CANCEL, k, 0, 0), Ok(1));
    assert_eq!(b.result(), Err(CANCELLED));
    b.recv(bk, 15000, false);
    assert_eq!(b.result(), Err(LIMIT));
    stop(g, [a, b]);
}
