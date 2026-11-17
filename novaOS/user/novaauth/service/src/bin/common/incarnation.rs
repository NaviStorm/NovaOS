//! Preuve de possession pour la reprise de laboratoire. Ce transcript ne peut autoriser une mutation.
use crate::runtime as rt;
use sha2::{Digest, Sha256};

pub const TAG: u64 = u64::from_le_bytes(*b"NAREINC1");
pub const SERVER: [u8; 16] = *b"NovaOS-QEMU-Srv1";
pub const DOMAIN: [u8; 16] = [7; 16];

pub fn identity() -> [u8; 32] {
    let mut bytes = [0; 32];
    for (i, part) in bytes.chunks_exact_mut(8).enumerate() {
        part.copy_from_slice(&rt::config(10 + i).to_le_bytes());
    }
    bytes
}

pub fn transcript(registry: &[u8; 32], challenge: &[u8; 32]) -> [u8; 32] {
    let mut d = Sha256::new();
    d.update(b"NovaAuth/QEMU/registry-possession/v1\0");
    d.update(SERVER);
    d.update(DOMAIN);
    d.update(registry);
    d.update(challenge);
    d.finalize().into()
}

#[cfg(feature = "horloge-guest")]
pub const EXCHANGE: u64 = u64::from_le_bytes(*b"NATLS001");
