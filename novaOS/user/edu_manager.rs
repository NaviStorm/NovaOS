//! Le lanceur fixe l'offre dans la page RO. Les deux handles du gestionnaire
//! ne donnent accès qu'à la supervision de cette instance.
use super::{abi::*, device_lifecycle::*, gate::call};
#[allow(dead_code)]
#[path = "edu_protocol.rs"]
mod wire;
use wire::*;
fn word(p: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(p[i * 8..i * 8 + 8].try_into().unwrap())
}
fn handle(i: usize) -> u64 {
    unsafe { (0x401000 as *const u64).add(i).read_volatile() }
}
fn send(tag: u64) -> bool {
    let m = SendV1 {
        version: VERSION,
        size: 160,
        timeout: 0,
        tag,
        len: 16,
        flags: 0,
        payload: [0; 128],
    };
    call(IPC_SEND, handle(0), (&raw const m) as u64, 160).status == OK
}
fn receive() -> Option<u64> {
    let mut m = ReceiveV1 {
        version: VERSION,
        size: 184,
        timeout: 1000,
        options: 0,
        reserved: 0,
        kind: 0,
        len: 0,
        tag: 0,
        handle: 0,
        aux: 0,
        payload: [0; 128],
    };
    (call(IPC_RECEIVE, handle(1), (&raw mut m) as u64, 184).status == OK && m.kind == 2)
        .then_some(m.aux)
}
pub(super) fn offer(page: &[u8]) -> bool {
    word(page, 0) == OFFER_MAGIC
}
pub(super) fn supervise(page: &[u8]) -> bool {
    let Ok(bdf) = u32::try_from(word(page, 1)) else {
        return false;
    };
    let id = word(page, 2);
    let catalog = [Driver {
        vendor: 0x1234,
        product: 0x11e8,
        image: IMAGE,
        required: RIGHTS,
    }];
    let mut d = Device::observed(bdf, id as u16, (id >> 16) as u16);
    let permit = Authorization {
        instance: Instance {
            location: BDF,
            generation: 1,
        },
        image: IMAGE,
        rights: RIGHTS,
    };
    let Ok(instance) = d.start(&catalog, Some(permit)) else {
        return false;
    };
    let child = word(page, 3);
    if child == 0 || !send(MANAGER_READY) {
        return false;
    }
    match receive() {
        Some(FAILED) => return d.stop(instance).is_ok() && d.retired(instance, true).is_ok(),
        Some(STARTED) => (),
        _ => return false,
    }
    if d.started(instance, child).is_err() || !send(MANAGER_RUNNING) {
        return false;
    }
    receive() == Some(STOPPED)
        && d.died(instance, child).is_ok()
        && d.retired(instance, true).is_ok()
        && send(MANAGER_RETIRED)
        && receive() == Some(CONFIRMED)
}
