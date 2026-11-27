#![no_std]
#![no_main]
extern crate alloc;
mod mesures_tas;
#[allow(dead_code)]
#[path = "../storage/runtime.rs"]
mod runtime;
use novaos_gestionnaire_sim::ipc::{Body, Op, Reply as Response, Request, Status};
use runtime::*;
#[path = "../heap.rs"]
mod heap;
fn allocation_probe() {}
fn session() -> Result<()> {
    let epoch = config(2);
    let mut value = 1;
    for (op, id) in [
        (Op::Hello, 1),
        (Op::Resolve, 2),
        (Op::Resolve, 2),
        (Op::Ack, 3),
    ] {
        let ack = op == Op::Ack;
        let request = Request {
            op,
            epoch,
            id,
            revision: if op == Op::Hello {
                0
            } else if ack {
                value
            } else {
                1
            },
            body: Body {
                a: if ack { epoch } else { 0 },
                b: if ack { 2 } else { 0 },
                ..Body::default()
            },
        };
        let mut data = [0; 128];
        let n = request.encode(&mut data).map_err(|_| REFUSED)?;
        send_for(config(0), epoch, &data[..n], 2500)?;
        let msg = receive_for(config(1), 2500)?;
        if msg.kind != 1 || msg.tag != epoch || msg.len != 88 {
            return Err(REFUSED);
        }
        let reply = Response::decode(request, &msg.payload[..88]).map_err(|_| REFUSED)?;
        if reply.status != Status::Confirmed {
            return Err(REFUSED);
        }
        value = reply.value;
    }
    while config(19) != epoch {
        call(THREAD_YIELD, 0, 0, 0);
    }
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(match session() {
        Ok(()) => 0,
        Err(e) => e,
    })
}
