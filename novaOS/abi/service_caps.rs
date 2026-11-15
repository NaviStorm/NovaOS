//! Bits stables des capacités de lancement des services supervisés.
//! La sélection de l'image et l'admission du profil restent indépendantes du masque.
pub const PRIVATE_TMP: u64 = 1;
pub const CACHE_BACKEND: u64 = 2;
pub const CACHE_CLIENT: u64 = 4;
pub const JOURNAL_STORE: u64 = 8;
pub const START_MASK_VERSION: u64 = 1;

pub const CAPABILITY_NAMES: &[(u64, &str)] = &[
    (PRIVATE_TMP, "private-tmp"),
    (CACHE_BACKEND, "cache-backend"),
    (CACHE_CLIENT, "cache-client"),
    (JOURNAL_STORE, "journal-store"),
];

pub const fn maximum(id: u64) -> Option<u64> {
    match id {
        1..=9 => Some(PRIVATE_TMP),
        10 => Some(CACHE_BACKEND),
        11..=15 => Some(PRIVATE_TMP | CACHE_CLIENT),
        16 => Some(JOURNAL_STORE),
        _ => None,
    }
}
