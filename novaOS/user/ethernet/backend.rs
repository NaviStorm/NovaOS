//! Messages privés entre service et adaptateur. Les identifiants de crédit
//! appartiennent au service et sont opaques pour le matériel.
pub const OUVRIR: u64 = 1;
pub const PREPARER: u64 = 2;
pub const PUBLIER: u64 = 3;
pub const FERMER: u64 = 4;
pub const COMPLETION: u64 = 5;
pub const RECEPTION: u64 = 6;
pub const ECOUTER: u64 = 7;
pub const CREDITS: usize = 8;
pub const PREPARER_LOT: u64 = 8;
pub const COMPLETIONS: u64 = 9;
pub const RECEPTION_LOT: u64 = 10;

// Chaque entrée associe un coupon logiciel et une longueur Ethernet.
pub fn entree(word: u32) -> (usize, usize) {
    ((word >> 16) as usize, (word & 0xffff) as usize)
}
pub fn liste(op: u64, entries: &[u32], msg: &mut [u8; 48]) -> usize {
    msg[..8].copy_from_slice(&op.to_le_bytes());
    msg[8..16].copy_from_slice(&(entries.len() as u64).to_le_bytes());
    for (dst, v) in msg[16..].chunks_exact_mut(4).zip(entries) {
        dst.copy_from_slice(&v.to_le_bytes());
    }
    16 + 4 * entries.len()
}
