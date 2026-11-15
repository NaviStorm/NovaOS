#![no_std]
#![no_main]
mod info_session;
mod runtime;

#[cfg(service_managed)]
#[path = "../services/node.rs"]
mod node;
#[cfg(service_managed)]
#[path = "../services/protocol.rs"]
mod protocol;
#[cfg(service_managed)]
#[allow(dead_code)]
#[path = "../../abi/service_process.rs"]
mod service_process;
use runtime::*;

#[cfg(not(service_managed))]
fn serve() -> Result<()> {
    let (requests, replies) = channels()?;
    let mut session = info_session::Session::new();
    loop {
        let msg = match receive_for(requests, if session.active() { TIMEOUT } else { 15_000 }) {
            Ok(msg) => msg,
            Err(TIMED_OUT) if !session.active() => continue,
            Err(CLOSED | DENIED) => return Ok(()),
            Err(status) => return Err(status),
        };
        if msg.kind == 1 {
            let answer = session.answer(msg.tag, &msg.payload[..msg.len as usize]);
            send(replies, msg.tag, &answer)?;
        }
    }
}

#[cfg(service_managed)]
fn serve() -> Result<()> {
    let mut session = info_session::Session::new();
    node::serve(true, |tag, packet| session.answer(tag, packet))
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(serve().err().unwrap_or(0))
}
