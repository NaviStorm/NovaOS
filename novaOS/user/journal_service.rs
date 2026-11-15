//! Journal persistant supervisé. Un seul écrivain sérialise les admissions et
//! les changements de rétention. Chaque réponse durable vient du serveur P9.
#![no_std]
#![no_main]
#[path = "services/journal_access.rs"]
mod access;
#[allow(dead_code)]
#[path = "../abi/files.rs"]
mod file;
#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod instance;
#[path = "services/journal_backend.rs"]
mod journal_backend;
#[path = "services/journal_wire.rs"]
mod journal_wire;
#[cfg(journal_pannes)]
#[path = "services/journal_probe.rs"]
mod probe;
#[path = "services/protocol.rs"]
mod protocol;
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;

#[path = "services/journal.rs"]
mod journal;
use journal::{Journal, reply, respond};
use journal_backend::Client;
use journal_wire as w;
use protocol as p;
use runtime as rt;

const WAIT_SLICE: u64 = 2;

fn decoded(msg: &rt::ReceiveV1) -> Option<w::Frame> {
    if msg.kind != 1 || msg.len != 128 {
        return None;
    }
    w::decode(&msg.payload)
}

fn serve() -> rt::Result<()> {
    let mgr_tx = rt::config(0);
    let mgr_rx = rt::config(1);
    let shell_rx = rt::config(7);
    let attempt = rt::config(4);
    let own_id = rt::config(2);
    let rights = rt::config(9);
    if own_id != w::JOURNAL_ID
        || attempt == 0
        || rights & !63 != 0
        || rt::value(rt::call(rt::IPC_BOOTSTRAP, 0, 0, 0))? != mgr_rx
    {
        return Err(rt::DENIED);
    }
    let shell = core::array::from_fn(|i| rt::config(i + 10));
    let manager = core::array::from_fn(|i| rt::config(i + 14));
    let mut own: w::Identity = [0; 4];
    rt::value(rt::call(instance::INFO, 2, own.as_mut_ptr() as u64, 32))?;
    if !w::valid_identity(own)
        || !w::valid_identity(manager)
        || own[..2] != manager[..2]
        || (rights != 0 && (!w::valid_identity(shell) || shell[..2] != own[..2]))
    {
        return Err(rt::DENIED);
    }
    let terminal_tx = rt::config(8);
    let mut j = Journal::new(
        Client::new(rt::config(5), rt::config(6)),
        shell,
        manager,
        rights,
        terminal_tx,
    );
    let stamp = w::event(
        1,
        own,
        own,
        own_id,
        w::INFO,
        w::SYSTEM,
        w::LOGGER,
        w::ONLINE,
        rt::config(18),
        [attempt, 0],
    );
    j.append(stamp[4..].try_into().unwrap())?;
    let mut lifecycle = p::request(p::ANNOUNCE, own_id, attempt);
    lifecycle[4..8].copy_from_slice(&own);
    respond(mgr_tx, 1, &lifecycle)?;
    lifecycle[1] = p::PULSE;
    let mut idle_ticks = 0;
    let mut turns = 0;
    let mut shell_open = shell_rx != 0;
    loop {
        let mut active = false;
        if shell_open {
            match rt::receive_for(shell_rx, 0) {
                Ok(msg) => {
                    active = true;
                    let answer = if let Some(frame) = decoded(&msg) {
                        match j.terminal(&frame, msg.tag) {
                            Ok(value) => value,
                            Err(e) => Some(reply(frame[1], e, 0)),
                        }
                    } else {
                        Some(reply(0, rt::BAD_ARGUMENT, 0))
                    };
                    if let Some(answer) = answer {
                        let sent = respond(terminal_tx, msg.tag, &answer);
                        #[cfg(journal_pannes)]
                        if sent.is_ok() && answer[1] == w::SELF_NOTICE && answer[2] == rt::OK {
                            if let Some(request) = decoded(&msg) {
                                j.acknowledged(request[4]);
                            }
                        }
                        let _ = sent;
                    }
                }
                Err(rt::WOULD_BLOCK | rt::TIMED_OUT) => (),
                Err(_) => {
                    shell_open = false;
                    j.drop_client();
                }
            }
        }
        let mut slept = 0;
        match rt::receive_for(mgr_rx, if active { 0 } else { WAIT_SLICE }) {
            Ok(msg) => {
                if let Some(frame) = decoded(&msg) {
                    if frame[0] == p::VERSION && frame[2..8] == lifecycle[2..8] {
                        if frame[1] == p::HALT {
                            return Ok(());
                        }
                        if frame[1] == p::SAMPLE {
                            let mut sample = lifecycle;
                            sample[1] = p::VALUE;
                            let status = j.refresh();
                            sample[8] = status.as_ref().map(|s| s.next).unwrap_or(0);
                            sample[9] = status.err().unwrap_or(rt::OK);
                            let _ = respond(mgr_tx, msg.tag, &sample);
                        }
                    } else if frame[1] == w::EMIT {
                        let result =
                            access::manager_event(&frame, manager).and_then(|e| j.append(e));
                        let answer = match result {
                            Ok(seq) => reply(w::EMIT, rt::OK, seq),
                            Err(e) => reply(w::EMIT, e, 0),
                        };
                        let _ = respond(mgr_tx, msg.tag, &answer);
                    }
                }
            }
            Err(rt::WOULD_BLOCK | rt::TIMED_OUT) if !active => slept = WAIT_SLICE,
            Err(rt::WOULD_BLOCK | rt::TIMED_OUT) => (),
            Err(e) => return Err(e),
        }
        j.advance(slept);
        idle_ticks += slept;
        turns += 1;
        if idle_ticks >= 24 || turns >= 8 {
            match respond(mgr_tx, 1, &lifecycle) {
                Ok(()) | Err(rt::WOULD_BLOCK) => (),
                Err(e) => return Err(e),
            }
            idle_ticks = 0;
            turns = 0;
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(rt::OK))
}
