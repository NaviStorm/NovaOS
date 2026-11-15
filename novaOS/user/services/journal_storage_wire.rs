//! Protocole privé du journal vers NovaFS. Trame complète de 128 octets little endian.
pub const MAGIC: u64 = u64::from_le_bytes(*b"NVJIPC01");
pub const STATUS: u64 = 1;
pub const READ: u64 = 2;
pub const APPEND: u64 = 3;
pub const ROTATE: u64 = 4;
pub const RETAIN: u64 = 5;
pub const PURGE: u64 = 6;
pub const RECORD: usize = 96;
pub type Event = [u8; RECORD];
pub type Frame = [u8; 128];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Status {
    pub first: u64,
    pub next: u64,
    pub segments: u64,
    pub retention: u64,
    pub revision: u64,
    pub quota: u64,
    pub bytes: u64,
}

pub fn get(buf: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(buf[i * 8..i * 8 + 8].try_into().unwrap())
}
pub fn put(buf: &mut [u8], i: usize, n: u64) {
    buf[i * 8..i * 8 + 8].copy_from_slice(&n.to_le_bytes());
}
pub fn header(op: u64, a: u64, b: u64) -> Frame {
    let mut frame = [0; 128];
    for (i, word) in [MAGIC, op, a, b].iter().enumerate() {
        put(&mut frame, i, *word);
    }
    frame
}
