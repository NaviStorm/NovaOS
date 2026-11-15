//! Contrat privé du gestionnaire de temporaires. Aucun pointeur interprocessus.
include!("generated/tmp.rs");

pub const CREATE: u64 = 1;
pub const REMOVE: u64 = 2;
pub const IO: u64 = 3;
pub const FRAME: usize = 128;
/// Identité attestée (32), opération u64 little-endian (8), trame VFS (128).
pub const ENVELOPE: usize = 40 + FRAME;
pub const WAKE: u64 = 1 << 63;
