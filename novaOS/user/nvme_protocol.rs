//! Protocole privé du profil Q35 NVMe, indépendant des sélecteurs de l’ABI.
//! Une requête de courtier porte cinq mots LE, un seul appel en attente par PID.
pub const EPOCH: usize = 0;
pub const BROKER_SEND: usize = 1;
pub const BROKER_REPLY: usize = 2;
pub const FRONT_REQUEST: usize = 3;
pub const FRONT_REPLY: usize = 4;
pub const DRIVER_REQUEST: usize = 5;
pub const DRIVER_REPLY: usize = 6;
pub const MODE: usize = 7;
pub const RESPONSE_VA: u64 = 0x401100;

pub const DISCOVER: u64 = 1;
pub const ENABLE: u64 = 2;
pub const READY: u64 = 3;
pub const IDENTIFY_CONTROLLER: u64 = 4;
pub const LIST_NAMESPACES: u64 = 5;
pub const IDENTIFY_NAMESPACE: u64 = 6;
pub const NUMBER_QUEUES: u64 = 7;
pub const CREATE_CQ: u64 = 8;
pub const CREATE_SQ: u64 = 9;
pub const SUBMIT_READ: u64 = 10;
pub const POLL: u64 = 11;
pub const DELETE_SQ: u64 = 12;
pub const DELETE_CQ: u64 = 13;
pub const SHUTDOWN: u64 = 14;
pub const STOPPED: u64 = 15;
pub const GEOMETRY: u64 = 16;
pub const REGISTER: u64 = 17;
pub const COLLECT: u64 = 18;
pub const RELEASE: u64 = 19;
pub const CANCEL: u64 = 20;

pub const NSID: u64 = 1;
pub const RESPONSE_SIZE: usize = 128;
pub const HEADER: usize = 48;
pub const CHUNK: usize = RESPONSE_SIZE - HEADER;
pub const REQUEST_SIZE: usize = 40;
pub const MAX_TRANSFER: u64 = 4096;

pub fn word(frame: &[u8], index: usize) -> u64 {
    u64::from_le_bytes(frame[8 * index..8 * index + 8].try_into().unwrap())
}
pub fn put(frame: &mut [u8], index: usize, value: u64) {
    frame[8 * index..8 * index + 8].copy_from_slice(&value.to_le_bytes());
}

pub const NORMAL: u64 = 0;
pub const DRIVER_DEATH: u64 = 1;
pub const CLIENT_DEATH: u64 = 2;
pub const REVOKE: u64 = 3;
pub const TIMEOUT_TEST: u64 = 4;

// Identité du programme embarqué et droits demandés par l'offre du lanceur.
// Ces nombres classent l'offre privée ; ils n'accordent pas de capacité IPC.
pub const DRIVER_IMAGE: u32 = 0x4e56_1901;
pub const DRIVER_INTENTS_RO: u32 = 1;
