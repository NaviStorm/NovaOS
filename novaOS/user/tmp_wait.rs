//! Épreuve isolée des attentes du courtier. Le terminal et NovaFS ne l’appellent pas.
use super::*;

extern "C" fn issue(capability: u64) -> ! {
    let mut frame = fs::frame(fs::SYNC, 1, 1);
    let status = send(capability, &mut frame);
    call(rt::THREAD_EXIT, status, 0, 0);
    rt::exit(181)
}

pub(super) fn run(mode: u64) -> ! {
    let phase = rt::config(2);
    if phase != 0 {
        if mode == 201 {
            service_fin(phase)
        }
        client_fin()
    }
    if mode == 201 {
        let key = rt::value(call(rt::IPC_CREATE, 0, 0, 0)).unwrap();
        word(1).store(key, Ordering::Release);
        wait_for(1);
        let message = rt::receive_for(key, 10000).unwrap();
        check(u64::from(message.kind), 2, 182);
        check(message.aux, tmp::WAKE, 183);
        word(4).store(1, Ordering::Release);
        wait_for(2);
        rt::exit(0)
    }
    let capability = cap();
    check(
        call(tmp::NEXT, 0, 0, tmp::ENVELOPE as u64).status,
        rt::DENIED,
        184,
    );
    check(call(tmp::COMPLETE, 0, 0, 0).status, rt::DENIED, 185);
    static READ_ONLY: fs::Frame = [0; 128];
    check(
        call(tmp::CALL, capability, READ_ONLY.as_ptr() as u64, 128).status,
        rt::BAD_ADDRESS,
        186,
    );
    for (index, expect) in [rt::CANCELLED, fs::UNCERTAIN].into_iter().enumerate() {
        let start = 2 * index as u64 + 1;
        wait_for(start);
        let t = call(rt::THREAD_CREATE, issue as *const () as u64, capability, 2);
        check(t.status, rt::OK, 187);
        wait_for(start + 1);
        check(call(rt::THREAD_CANCEL, t.value, 0, 0).status, rt::OK, 188);
        let joined = call(rt::THREAD_JOIN, t.value, 0, 0);
        check(joined.status, rt::OK, 189);
        check(joined.value, expect, 190);
        word(4).store(index as u64 + 1, Ordering::Release);
    }
    wait_for(5);
    issue(capability)
}

/// Le compteur et l'octet écrit sont privés au service témoin et observés
/// séparément du statut retourné. Aucun disque n'est attaché à ce scénario.
fn service_fin(phase: u64) -> ! {
    let ancien = rt::config(3);
    let rx = rt::value(call(rt::IPC_CREATE, 0, 0, 0)).unwrap();
    let mut enveloppe = [0; tmp::ENVELOPE];
    let adresse = enveloppe.as_mut_ptr() as u64;
    let prochain = || call(tmp::NEXT, rx, adresse, tmp::ENVELOPE as u64);
    let creation = rt::value(prochain()).unwrap();
    check(
        u64::from_le_bytes(enveloppe[32..40].try_into().unwrap()),
        tmp::CREATE,
        201,
    );
    check(call(tmp::COMPLETE, creation, 0, 0).status, rt::OK, 202);
    check(prochain().status, rt::WOULD_BLOCK, 203);
    if ancien != 0 {
        check(
            call(tmp::COMPLETE, ancien, 0, fs::FULL).status,
            rt::REVOKED,
            204,
        );
    }
    word(4).store(1, Ordering::Release);
    wait_for(1);
    if phase == 1 {
        rt::exit(0)
    }
    let ticket = rt::value(prochain()).unwrap();
    check(
        u64::from_le_bytes(enveloppe[32..40].try_into().unwrap()),
        tmp::IO,
        205,
    );
    let trame: &fs::Frame = enveloppe[40..].try_into().unwrap();
    check(u64::from(trame[1]), u64::from(fs::WRITE), 206);
    check(u64::from(trame[fs::PAYLOAD]), 1, 207);
    if ancien != 0 {
        check(
            call(tmp::COMPLETE, ancien, 0, fs::FULL).status,
            rt::REVOKED,
            208,
        );
    }
    if matches!(phase, 3 | 4) {
        word(9).store(u64::from(trame[fs::PAYLOAD]), Ordering::Release);
        word(6).fetch_add(1, Ordering::AcqRel);
    }
    word(3).store(ticket, Ordering::Release);
    word(4).store(2, Ordering::Release);
    wait_for(2);
    rt::exit(0)
}

extern "C" fn ecrire(tag: u64) -> ! {
    let mut trame = fs::frame(fs::WRITE, 1, 1);
    trame[2] = fs::START;
    trame[fs::PAYLOAD] = tag as u8;
    fs::size(&mut trame, 1);
    fs::set(&mut trame, 32, 1);
    let r = call(
        tmp::CALL,
        word(1).load(Ordering::Acquire),
        trame.as_mut_ptr() as u64,
        128,
    );
    if r.status == rt::OK {
        check(r.value, 128, 209);
        check(u64::from(trame[fs::PAYLOAD]), 0xa7, 210);
    }
    call(rt::THREAD_EXIT, r.status, 0, 0);
    rt::exit(211)
}

fn client_fin() -> ! {
    let droit = cap();
    word(1).store(droit, Ordering::Release);
    wait_for(1);
    let lancer =
        |tag| rt::value(call(rt::THREAD_CREATE, ecrire as *const () as u64, tag, 2)).unwrap();
    let premier = lancer(1);
    wait_for(2);
    let second = lancer(2);
    let joindre = |t| rt::value(call(rt::THREAD_JOIN, t, 0, 0)).unwrap();
    word(6).store(joindre(premier), Ordering::Release);
    word(7).store(joindre(second), Ordering::Release);
    let mut requete = fs::frame(fs::WRITE, 1, 1);
    check(send(droit, &mut requete), rt::DENIED, 212);
    check(call(tmp::CAP, 0, 0, 0).status, rt::DENIED, 213);
    word(4).store(1, Ordering::Release);
    wait_for(3);
    rt::exit(0)
}
