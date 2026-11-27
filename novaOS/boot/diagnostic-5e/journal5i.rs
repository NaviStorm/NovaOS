//! Journal terminal RAM du profil passif. Un seul BSP, aucun callback ni export ABI.
use core::ptr;
const BYTES: usize = 24576;
#[repr(C)]
struct Log {
    magic: [u8; 16],
    location: u64,
    used: u64,
    bytes: [u8; BYTES],
}
static mut LOG: Log = Log {
    magic: [0; 16],
    location: 0,
    used: 0,
    bytes: [0; BYTES],
};

pub(crate) fn append(s: &str) -> Result<(), u64> {
    let log = unsafe { &mut *(&raw mut LOG) };
    let used = log.used as usize;
    let end = used
        .checked_add(s.len())
        .filter(|end| *end < BYTES)
        .ok_or(17u64)?;
    if !s.is_ascii() {
        return Err(17);
    }
    log.bytes[used..end].copy_from_slice(s.as_bytes());
    log.bytes[end] = b'\n';
    log.used = (end + 1) as u64;
    Ok(())
}
#[inline(never)]
pub(crate) fn seal() {
    let addr = (&raw mut LOG) as u64;
    unsafe {
        ptr::write_volatile(&raw mut LOG.location, addr);
        ptr::write_volatile(&raw mut LOG.magic, *b"NOVA5I-RAM-TRACE");
    }
}
