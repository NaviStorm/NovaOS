//! Contrat du courtier du cache P9. Champs little-endian et octets réservés nuls.
include!("generated/cache.rs");

pub const OPEN: u64 = 1;
pub const READ: u64 = 2;
pub const WRITE: u64 = 3;
pub const PURGE: u64 = 4;
pub const PURGE_USER: u64 = 5;
pub const PURGE_SYSTEM: u64 = 6;
pub const RECOVER: u64 = 7;
pub const NEXT: u64 = 8;
pub const COMPLETE: u64 = 9;
pub const PRESSURE: u64 = 10;
pub const AUTO_PURGE: u64 = 11;
pub const TEST_WRITE_SENT: u64 = 12;
pub const TEST_PURGE_PENDING: u64 = 13;

pub const FRAME: usize = 224;
pub const KEY: usize = 0;
pub const GEN: usize = 32;
pub const CAP: usize = 40;
pub const BOOT: usize = 48;
pub const OP: usize = 64;
pub const STATUS: usize = 72;
pub const NAME_LEN: usize = 96;
pub const DATA_LEN: usize = 97;
pub const OFFSET: usize = 104;
pub const NAME: usize = 112;
pub const DATA: usize = 160;

pub fn get(frame: &[u8; FRAME], at: usize) -> u64 {
    u64::from_le_bytes(frame[at..at + 8].try_into().unwrap())
}
pub fn put(frame: &mut [u8; FRAME], at: usize, value: u64) {
    frame[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
