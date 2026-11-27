//! Contrat privé de reprise. La vue renvoyée ne contient aucune adresse DMA ni capacité.
pub const MANAGER: u64 = 1;
pub const LAUNCHER: u64 = 2;
pub const READ: u64 = 1;
pub const PREPARE: u64 = 2;
pub const LAUNCH: u64 = 3;
pub const RECEIPT: u64 = 4;
pub const END: u64 = 5;
pub const ACK: u64 = 0x4100;
pub const RESTART: usize = 64;
pub const TX: usize = 65;
pub const RX: usize = 66;
pub const LINK: usize = 67;
pub const COMMAND: usize = 68;
pub const NUMBER: usize = 69;
pub const OPERATION: usize = 73;
pub const OLD_KEY: usize = 70;
pub const OLD_LINK: usize = 71;
pub const STATUS: usize = 72;
pub const SNAP: usize = 80;
// BootID, contrôleur, groupe, inventaire, génération, issue, PID/instance/génération,
// cadre prêté et mappings, reçu de clôture, publication, plan, état du journal et intention.
#[path = "reprise_etat.rs"]
mod etat;
pub use etat::*;
pub fn digest(role: u64, epoch: u64, id: u64, data: &[u64; WORDS]) -> [u64; 4] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"NovaOS/4E.1/recu/v1");
    for w in [role, epoch, id].iter().chain(data.iter()) {
        hash.update(w.to_le_bytes())
    }
    let bytes = hash.finalize();
    core::array::from_fn(|i| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap()))
}
