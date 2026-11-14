//! Protocole privé du service bloc, sans pointeurs. Cette version ne modifie pas l'ABI IPC v1.
/// La réponse INFO fournit les droits effectifs du bail dans le u32 à +16.
pub const PORTAL: u64 = 0x0001_0040;
pub const INFO: u8 = 1;
pub const READ: u8 = 2;
pub const WRITE: u8 = 3;
pub const FLUSH: u8 = 4;
pub const CLOSE: u8 = 5;
pub const BEGIN: u8 = 0;
pub const PIECE: u8 = 1;
pub const END: u8 = 2;
/// Trame PAGE, phase 3 : exactement 4096 octets, transférés par la boîte dédiée.
pub const PAGE_PHASE: u8 = 3;
pub const PAGE_PORTAL: u64 = 0x10046;
pub const ALL_RIGHTS: u8 = 31;
pub const MAX: usize = 96;
pub type Wire = [u8; 128];
// 0 version, 1 opération, 2 phase, 3 réservé, 4..8 longueur de pièce,
// 8..16 LBA logique, 16..20 décalage, 20..32 réservés, 32..128 octets.
pub fn blank(op: u8) -> Wire {
    let mut w = [0; 128];
    w[0] = 1;
    w[1] = op;
    w
}
pub fn number(w: &Wire, n: usize) -> u32 {
    u32::from_le_bytes(w[n..n + 4].try_into().unwrap())
}
pub fn lba(w: &Wire) -> u64 {
    u64::from_le_bytes(w[8..16].try_into().unwrap())
}
pub fn put(w: &mut Wire, n: usize, v: u32) {
    w[n..n + 4].copy_from_slice(&v.to_le_bytes())
}
pub fn syntax(w: &Wire) -> bool {
    if w[1] == DISCOVER {
        return w[0] == 1 && w[2..32].iter().all(|&v| v == 0)
            && &w[32..40] == b"NVBQ\x01\0\0\0" && w[44..].iter().all(|&v| v == 0);
    }
    if w[0] != 1 || w[3] != 0 || w[20..32].iter().any(|&b| b != 0) {
        return false;
    }
    let n = number(w, 4) as usize;
    if matches!(w[1], READ | WRITE) && w[2] == PAGE_PHASE {
        return n == 4096 && number(w, 16) == 0 && w[32..].iter().all(|&b| b == 0);
    }
    if n > MAX {
        return false;
    }
    let shape = match (w[1], w[2]) {
        (INFO | FLUSH | CLOSE, BEGIN) => n == 0 && lba(w) == 0 && number(w, 16) == 0,
        (READ | WRITE, BEGIN | END) => n == 0 && number(w, 16) == 0,
        (READ | WRITE, PIECE) => n > 0,
        _ => false,
    };
    let occupied = if w[1] == WRITE && w[2] == PIECE { n } else { 0 };
    shape && !w[32 + occupied..].iter().any(|&b| b != 0)
}

/// Contrat de transport v1. Un masque annoncé ne délègue aucun droit sur le support.
pub const DISCOVER: u8 = 7;
pub const FEATURE_PAGE: u32 = 1;
pub const FEATURE_BATCH: u32 = 2;
pub const IMPLEMENTED: u32 = FEATURE_PAGE;

pub fn discovery(bits: u32, reply: bool) -> Wire {
    let mut f = blank(DISCOVER);
    f[32..36].copy_from_slice(if reply { b"NVBA" } else { b"NVBQ" });
    put(&mut f, 36, 1);
    put(&mut f, 40, bits);
    f
}
pub fn offered(f: &Wire, allowed: u32) -> Option<u32> {
    let bits = number(f, 40);
    (bits & !allowed == 0 && *f == discovery(bits, true)).then_some(bits)
}
