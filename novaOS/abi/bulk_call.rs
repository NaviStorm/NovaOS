//! Arguments locaux à l’appel, jamais transmis comme pointeurs à un pair.
use crate::runtime as rt;
fn exchange(op: u64, cap: u64, tag: u64, address: u64, len: usize) -> rt::Result<()> {
    let q = rt::BulkV1 {
        version: rt::VERSION,
        size: 32,
        tag,
        address,
        length: len as u64,
    };
    let n = rt::value(rt::call(op, cap, (&raw const q) as u64, 32))?;
    if n == len as u64 {
        Ok(())
    } else {
        Err(rt::BAD_ARGUMENT)
    }
}
/// Une porte absente identifie un noyau antérieur. Une révocation reste une erreur.
pub fn features(cap: u64, direction: u32) -> rt::Result<u64> {
    match rt::value(rt::call(rt::IPC_FEATURES, cap, direction.into(), 1)) {
        Err(rt::UNKNOWN_CALL) => Ok(0),
        r => r,
    }
}

pub fn put(cap: u64, tag: u64, src: &[u8]) -> rt::Result<()> {
    exchange(rt::IPC_PUT, cap, tag, src.as_ptr() as u64, src.len())
}
pub fn take(cap: u64, tag: u64, dst: &mut [u8]) -> rt::Result<()> {
    exchange(rt::IPC_TAKE, cap, tag, dst.as_mut_ptr() as u64, dst.len())
}
