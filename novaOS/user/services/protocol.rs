//! Protocole local de contrôle. Le droit d'envoi sur l'endpoint privé attribué
//! au lanceur authentifie la session ; un identifiant ou un tag ne confère aucun droit.
#![allow(dead_code)]
pub type Frame = [u64; 16];
pub const VERSION: u64 = 1;
pub const LIST: u64 = 1;
pub const STATUS: u64 = 2;
pub const START: u64 = 3;
pub const STOP: u64 = 4;
pub const RESTART: u64 = 5;
/// Réponse DEP : le mot 10 identifie le fournisseur. Le mot 11 vaut 1 pour
/// une dépendance obligatoire et 0 pour une dépendance optionnelle.
pub const DEP: u64 = 6;
pub const REGISTER: u64 = 7;
pub const QUERY: u64 = 8;
pub const SHUTDOWN: u64 = 9;
pub const INJECT: u64 = 10;
pub const LOG: u64 = 11;
/// Port d'administration du seul catalogue CACHE de qualification. Les mots
/// 3 et 4 désignent un essai applicatif et un argument scalaire.
pub const CACHE_TEST: u64 = 12;
pub const CACHE_ASYNC_WRITE: u64 = 13;
/// Un passage de la politique de pression sur le port d'exploitation.
pub const CACHE_MAINTAIN: u64 = 14;
/// SETTINGS: génération appliquée [4], fournisseur bloquant [5], modification
/// en attente [8], auto enregistré/appliqué [10/11], génération stockée [12],
/// relance/budget [13/14] et statut du magasin [15].
pub const SETTINGS: u64 = 25;
pub const ENABLE: u64 = 26;
pub const DISABLE: u64 = 27;

pub const REGISTERED: u64 = 1;
pub const WAITING: u64 = 2;
pub const STARTING: u64 = 3;
pub const READY: u64 = 4;
pub const RUNNING: u64 = 5;
pub const STOPPING: u64 = 6;
pub const STOPPED: u64 = 7;
pub const FAILED: u64 = 8;
pub const RESTARTING: u64 = 9;

// Opérations sur la paire d'endpoints privée d'une seule incarnation.
pub const ANNOUNCE: u64 = 16;
pub const PULSE: u64 = 17;
pub const VALUE: u64 = 18;
pub const LOST: u64 = 19;
pub const HALT: u64 = 20;
pub const SAMPLE: u64 = 21;
pub const DISTURB: u64 = 22;
pub const CACHE_STEP: u64 = 23;
pub const MAINTAIN: u64 = 24;
pub const NATIVE_INFO: u64 = u32::from_le_bytes(*b"NVFS") as u64;

pub fn request(operation: u64, id: u64, arg: u64) -> Frame {
    let mut words = [0; 16];
    words[..4].copy_from_slice(&[VERSION, operation, id, arg]);
    words
}

pub fn encode(words: &Frame) -> [u8; 128] {
    let mut bytes = [0; 128];
    for (field, value) in bytes.chunks_exact_mut(8).zip(words) {
        field.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn decode(bytes: &[u8]) -> Option<Frame> {
    if bytes.len() != 128 {
        return None;
    }
    let mut words = [0; 16];
    for (word, field) in words.iter_mut().zip(bytes.chunks_exact(8)) {
        *word = u64::from_le_bytes(field.try_into().ok()?);
    }
    Some(words)
}

/// Noms du catalogue installé. Un numéro désigne un service, jamais un PID.
const INSTALLED_NAMES: &[(u64, &str)] =
    &[(1, "clock"), (2, "tmp-health"), (9, "storage-info"), (10, "cache"), (16, "journal")];

pub fn service_name(id: u64) -> Option<&'static str> {
    INSTALLED_NAMES.iter().find(|(n, _)| *n == id).map(|(_, name)| *name)
}

pub fn service_id(s: &str) -> Option<u64> {
    INSTALLED_NAMES.iter().find(|(_, name)| *name == s).map(|(n, _)| *n)
        .or_else(|| s.parse::<u64>().ok().filter(|n| *n != 0))
}
