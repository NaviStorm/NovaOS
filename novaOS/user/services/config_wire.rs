//! Transport privé gestionnaire/NovaFS. Tous les entiers sont little endian.
pub type Frame = [u8; 128];
pub const VERSION: u64 = 1;
pub const LOAD: u64 = 1;
pub const BEGIN: u64 = 2;
pub const PART: u64 = 3;
pub const COMMIT: u64 = 4;
pub const PREVIOUS: u64 = 6;
pub const LIMIT: usize = 4096;
pub const DATA: usize = 48;
pub const CHUNK: usize = 128 - DATA;

pub fn get(f: &Frame, pos: usize) -> u64 {
    u64::from_le_bytes(f[pos..pos + 8].try_into().unwrap())
}
pub fn put(f: &mut Frame, pos: usize, value: u64) {
    f[pos..pos + 8].copy_from_slice(&value.to_le_bytes());
}
pub fn request(op: u64, generation: u64, offset: usize, len: usize) -> Frame {
    let mut f = [0; 128];
    put(&mut f, 0, VERSION);
    put(&mut f, 8, op);
    put(&mut f, 24, generation);
    put(&mut f, 32, offset as u64);
    put(&mut f, 40, len as u64);
    f
}
