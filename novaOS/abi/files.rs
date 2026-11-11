//! Messages du service fichier, transportés dans le payload IPC. Entiers little-endian.
pub type Frame = [u8; 128];
pub const VERSION: u8 = 2;
pub const MOUNT: u8 = 1;
pub const INFO: u8 = 2;
pub const OPEN: u8 = 3;
pub const READ: u8 = 4;
pub const WRITE: u8 = 5;
pub const SYNC: u8 = 6;
pub const CLOSE: u8 = 7;
pub const READABLE: u8 = 1;
pub const WRITABLE: u8 = 2;
pub const CREATE: u8 = 4;
pub const DISCARD: u8 = 1;
pub const START: u8 = 1;
pub const CONTINUE: u8 = 2;
pub const PAYLOAD: usize = 40;
pub const FRAGMENT: usize = 128 - PAYLOAD;
pub const MAX_WRITE: usize = 64 * 1024;
pub const NOT_FOUND: u64 = 20;
pub const CORRUPT: u64 = 21;
pub const UNCERTAIN: u64 = 22;
pub const FULL: u64 = 23;
pub const BUSY: u64 = 24;
pub const UNSUPPORTED: u64 = 25;
pub const IO: u64 = 26;
pub fn frame(op: u8, session: u64, handle: u64) -> Frame {
    let mut f = [0; 128];
    f[0] = VERSION;
    f[1] = op;
    set(&mut f, 8, session);
    set(&mut f, 16, handle);
    f
}
pub fn word(f: &Frame, at: usize) -> u64 {
    u64::from_le_bytes(f[at..at + 8].try_into().unwrap())
}
pub fn set(f: &mut Frame, at: usize, v: u64) {
    f[at..at + 8].copy_from_slice(&v.to_le_bytes())
}
pub fn length(f: &Frame) -> usize {
    u16::from_le_bytes(f[4..6].try_into().unwrap()) as usize
}
pub fn size(f: &mut Frame, n: usize) {
    f[4..6].copy_from_slice(&(n as u16).to_le_bytes())
}
