//! Deux threads invités se présentent ensemble à la porte ABI, puis oublient
//! volontairement le résultat du lancement. Seul le reçu interrogé sert au WAIT.
use super::*;

fn store(index: usize, value: u64) {
    word(index).store(value, Ordering::Release);
}

extern "C" fn contender(index: u64) -> ! {
    let key = rt::config(1);
    let id = index as usize;
    for variant in [
        0,
        terminal::PIPE_OPEN,
        terminal::PIPE_CAT,
        terminal::PIPE_WC,
        terminal::PIPE_HEAD,
    ] {
        check(call(terminal::SPAWN, variant, 0, 0).status, rt::DENIED, 81);
    }
    check(
        call(terminal::SPAWN, terminal::EXEC_TICKET, key, 1).status,
        rt::DENIED,
        82,
    );
    word(2).fetch_add(1, Ordering::AcqRel);
    while rt::config(2) != 2 {
        call(rt::THREAD_YIELD, 0, 0, 0);
    }
    let r = call(terminal::SPAWN, terminal::EXEC_TICKET, key, 0);
    if r.status == rt::OK {
        let mut result = [0u64; 4];
        check(
            call(rt::PROCESS_WAIT, r.value, result.as_mut_ptr() as u64, 32).status,
            rt::DENIED,
            103,
        );
    }
    store(6 + id, r.status);
    // La valeur du WAIT renvoyée au gagnant ne sort pas de ce thread.
    call(rt::THREAD_EXIT, 41 + index, 0, 0);
    rt::exit(83)
}

fn query(key: u64, tag: u64) -> (u64, u64) {
    rt::send(key, tag, b"NXQUERY1").unwrap();
    let m = rt::receive(key).unwrap();
    assert!(m.kind == 1 && m.len == 32 && m.tag == tag && &m.payload[..8] == b"NXREPLY1");
    let get = |i| u64::from_le_bytes(m.payload[i..i + 8].try_into().unwrap());
    check(get(8), rt::OK, 102);
    (get(16), get(24))
}

pub(super) fn run() -> ! {
    store(4, 1);
    wait_for(1);
    let mut tids = [0; 2];
    for (i, tid) in tids.iter_mut().enumerate() {
        let r = call(
            rt::THREAD_CREATE,
            contender as *const () as u64,
            i as u64,
            2,
        );
        check(r.status, rt::OK, 84);
        *tid = r.value;
        store(10 + i, *tid);
    }
    for (i, tid) in tids.into_iter().enumerate() {
        let r = call(rt::THREAD_JOIN, tid, 0, 0);
        check(r.status, rt::OK, 85);
        check(r.value, 41 + i as u64, 86);
    }
    store(4, 2);
    wait_for(2);
    let (pid, wait) = query(rt::config(1), 1);
    assert!(pid == wait & !(1 << 63) && pid != 0);
    assert_eq!(query(rt::config(1), 2), (pid, wait));
    let mut result = [0u64; 4];
    let out = result.as_mut_ptr() as u64;
    check(call(rt::PROCESS_WAIT, wait, out, 32).status, rt::OK, 87);
    check(result[1], wait & !(1 << 63), 88);
    check(result[2], 0, 89);
    check(result[3], 0, 90);
    check(call(rt::PROCESS_WAIT, wait, out, 32).status, rt::DENIED, 91);
    check(
        call(terminal::SPAWN, terminal::EXEC_TICKET, rt::config(1), 0).status,
        rt::REFUSED,
        92,
    );
    assert_eq!(query(rt::config(1), 3), (pid, 0));
    store(4, 3);
    wait_for(3);
    check(
        call(terminal::SPAWN, terminal::EXEC_TICKET, rt::config(1), 0).status,
        rt::REVOKED,
        93,
    );
    store(4, 4);
    wait_for(4);
    check(
        call(terminal::SPAWN, terminal::EXEC_TICKET, rt::config(1), 0).status,
        rt::TIMED_OUT,
        94,
    );
    rt::exit(0)
}
