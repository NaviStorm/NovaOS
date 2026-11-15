//! Messages privés des acteurs du banc CACHE, utilisables uniquement depuis le
//! canal d'exploitation du gestionnaire sur les images de qualification.
pub const OPEN: u64 = 1;
pub const READ: u64 = 2;
pub const WRITE: u64 = 3;
pub const PURGE: u64 = 4;
pub const PURGE_USER: u64 = 5;
pub const PURGE_SYSTEM: u64 = 6;
pub const FOREIGN_READ: u64 = 7;
pub const FOREIGN_WRITE: u64 = 8;
pub const SAVE_TOKEN: u64 = 9;
pub const LOAD_TOKEN: u64 = 10;
pub const NAMES: u64 = 11;
pub const FORGE: u64 = 12;
pub const QUOTA: u64 = 13;
pub const TMP: u64 = 14;
pub const CRASH: u64 = 15;
pub const RACE_WRITE: u64 = 16;
pub const RACE_READ: u64 = 17;
pub const RACE_SENT: u64 = 18;
pub const RACE_RESULT: u64 = 19;
pub const EXPORT: u64 = 20; // quatre mots consécutifs
pub const IMPORT: u64 = 24; // quatre mots consécutifs
pub const PAD: u64 = 28; // charge reproductible, uniquement dans la vue du test
