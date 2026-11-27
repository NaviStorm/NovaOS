#![no_std]
#![no_main]
extern crate alloc;
mod lancement;
#[cfg(feature="reprise-4e1")]
mod reprise;
mod mesures_tas;
#[allow(dead_code)]
#[path = "../storage/runtime.rs"]
mod runtime;
use novaos_gestionnaire_sim::ipc::{Body, Op, Request};
use runtime::*;
#[path = "../heap.rs"]
mod heap;
fn allocation_probe() {}
fn session() -> Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let epoch = config(2);
    let ports = (config(0), config(1));
    let request = |op, id, revision, body| Request {
        op,
        id,
        epoch,
        revision,
        body,
    };
    lancement::appel(ports, request(Op::Hello, 1, 0, Body::default()))?;
    let resolve = request(Op::Resolve, 2, 1, Body::default());
    let resolved = lancement::appel(ports, resolve)?;
    if lancement::appel(ports, resolve)? != resolved {
        return Err(REFUSED);
    }
    let mut hash = Sha256::new();
    let mut cursor = 1u64;
    let mut id = 3;
    loop {
        let query = request(
            Op::ReadPlan,
            id,
            resolved.value,
            Body {
                a: cursor,
                ..Body::default()
            },
        );
        let chunk = lancement::appel(ports, query)?.digest;
        let next = u64::from_le_bytes(chunk[..8].try_into().unwrap());
        let n = chunk[8] as usize;
        if !(1..=23).contains(&n) || cursor + n as u64 > 32769 {
            return Err(REFUSED);
        }
        hash.update(&chunk[9..9 + n]);
        id += 1;
        if next == 0 {
            break;
        }
        if next != cursor + n as u64 {
            return Err(REFUSED);
        }
        cursor = next;
    }
    lancement::appel(
        ports,
        request(
            Op::Ack,
            id,
            resolved.value,
            Body {
                a: epoch,
                b: 2,
                ..Body::default()
            },
        ),
    )?;
    while config(19) != epoch {
        call(THREAD_YIELD, 0, 0, 0);
    }
    Ok(hash.finalize().into())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    #[cfg(feature="reprise-4e1")]
    let resultat = reprise::acteur(reprise::wire::LAUNCHER, session);
    #[cfg(not(feature="reprise-4e1"))]
    let resultat = session().and_then(lancement::servir);
    exit(match resultat {
        Ok(()) => 0,
        Err(e) => e,
    })
}
