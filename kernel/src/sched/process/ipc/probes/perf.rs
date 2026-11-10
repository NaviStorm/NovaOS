//! Échanges chronométrés entre deux acteurs et charges concurrentes facultatives.
use super::*;
fn word(a: Actor, i: usize, v: u64) {
    a.put(user::DATA + i as u64 * 8, &v.to_le_bytes());
}
fn revoked(a: Actor, b: Actor, hz: u64) {
    let (k, h) = (a.boot(), b.boot());
    let root = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    hal::log(format_args!("IPC saturation T17 profondeur=8 noeuds=32"));
    with(|s| s.ipc.metrics.start(a.0.number, b.0.number));
    for _ in 0..100 {
        super::contract::transfer(a, b, k, h, root, SEND, 1);
        let mut leaf = root;
        for _ in 0..8 {
            leaf = a.call(IPC_DUP, leaf, ALL.into(), 0).unwrap();
        }
        for _ in 0..22 {
            a.call(IPC_DUP, root, 1, 0).unwrap();
        }
        assert_eq!(a.call(IPC_REVOKE, root, 0, 0), Ok(31));
    }
    with(|s| s.ipc.metrics.report(hz));
    a.call(IPC_CLOSE, root, 0, 0).unwrap();
}
fn batches(a: Actor, b: Actor, hz: u64, pressure: u8) {
    let root = a.boot();
    let peer = b.boot();
    let reply = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let b_reply = super::contract::transfer(a, b, root, peer, reply, SEND | GRANT, 1);
    word(a, 6, root);
    word(a, 7, reply);
    word(b, 6, b_reply);
    word(b, 7, peer);
    let source = a.call(IPC_CREATE, 0, 0, 0).unwrap();
    let leaf = a.call(IPC_DUP, source, 5, 0).unwrap();
    for mode in 0..6 {
        let size = [0, 32, 128, 0, 0, 0][mode];
        let description = ["DATA0", "DATA32", "DATA128", "NOTIFY", "COPY", "MOVE"][mode];
        for actor in [a, b] {
            actor.put(OUTPUT, &receive_wire(15000, true));
            actor.put(DESC, &send_wire(19, size, 0));
        }
        if mode >= 4 {
            a.put(DESC, &give_wire(leaf, 5, if mode == 4 { 1 } else { 2 }));
        }
        if mode == 5 {
            b.put(DESC, &give_wire(0, 5, 2));
        }
        for series in 0..=5 {
            let count = if series == 0 { 1000 } else { 10000 };
            if series == 1 {
                with(|s| s.ipc.metrics.start(a.0.number, b.0.number));
            }
            hal::log(format_args!(
                "IPC debut mode={description} charge={pressure} serie={series} tours={count}"
            ));
            b.start(u64::MAX - 1, count, mode as u64, 1);
            a.start(u64::MAX - 1, count, mode as u64, 0);
            assert_eq!(a.result(), Ok(count));
            assert_eq!(b.result(), Ok(count));
            hal::log(format_args!(
                "IPC fin mode={description} charge={pressure} serie={series} tours={count}"
            ));
        }
        with(|s| s.ipc.metrics.report(hz));
    }
    a.call(IPC_CLOSE, reply, 0, 0).unwrap();
    a.call(IPC_CLOSE, source, 0, 0).unwrap();
}
pub(super) fn run() {
    let hz = hal::timer::counter_hz();
    hal::log(format_args!("IPC T17 compteur_hz={hz}"));
    let (g, [a, b]) = group([ALL, ALL]);
    batches(a, b, hz, 0);
    revoked(a, b, hz);
    stop(g, [a, b]);
    let (g, [a, b, producer, cpu]) = group([ALL, ALL, 0, 0]);
    let own = producer.call(IPC_CREATE, 0, 0, 0).unwrap();
    word(producer, 6, own);
    producer.put(DESC, &send_wire(7, 32, 0));
    producer.put(OUTPUT, &receive_wire(0, false));
    producer.start(u64::MAX - 2, 1, 0, 0);
    cpu.start(u64::MAX - 2, 2, 0, 0);
    let pre = with(|s| s.stats.preemptions);
    batches(a, b, hz, 1);
    for actor in [producer, cpu] {
        word(actor, 10, 1);
    }
    let prod = producer.result().unwrap();
    let work = cpu.result().unwrap();
    let irq = with(|s| s.stats.preemptions) - pre;
    assert!(prod > 0 && work > 0 && irq > 0);
    hal::log(format_args!(
        "IPC contention livraisons={prod} calcul={work} preemptions={irq} erreurs=0"
    ));
    stop(g, [a, b, producer, cpu]);
}
