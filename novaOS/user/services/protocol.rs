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
pub const DEP: u64 = 6;
pub const REGISTER: u64 = 7;
pub const QUERY: u64 = 8;
pub const SHUTDOWN: u64 = 9;
pub const INJECT: u64 = 10;
pub const LOG: u64 = 11;

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
    if bytes.len() != 128 { return None; }
    let mut words = [0; 16];
    for (word, field) in words.iter_mut().zip(bytes.chunks_exact(8)) {
        *word = u64::from_le_bytes(field.try_into().ok()?);
    }
    Some(words)
}
