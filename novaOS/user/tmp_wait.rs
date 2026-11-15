//! Épreuve isolée des attentes du courtier. Le terminal et NovaFS ne l’appellent pas.
use super::*;

extern "C" fn issue(capability: u64) -> ! {
    let mut frame = fs::frame(fs::SYNC, 1, 1);
    let status = send(capability, &mut frame);
    call(rt::THREAD_EXIT, status, 0, 0);
    rt::exit(181)
}

pub(super) fn run(mode: u64) -> ! {
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
