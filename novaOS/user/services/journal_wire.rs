//! Journal v1. Les mots des trames et des enregistrements sont little endian.
#![allow(dead_code)]
pub type Frame = [u64; 16];
pub type Event = [u64; 12];
pub type Identity = [u64; 4];
pub const VERSION: u64 = 1;
pub const EMIT: u64 = 1;
pub const STATUS: u64 = 2;
pub const NEXT: u64 = 3;
pub const WATCH: u64 = 4;
pub const ROTATE: u64 = 5;
pub const RETAIN: u64 = 6;
pub const PURGE: u64 = 7;
pub const SELF_NOTICE: u64 = 8;

pub const OWN: u64 = 1;
pub const SYSTEM_READ: u64 = 2;
pub const RETENTION_RIGHT: u64 = 4;
pub const ROTATION_RIGHT: u64 = 8;
pub const PURGE_RIGHT: u64 = 16;
pub const EMIT_RIGHT: u64 = 32;
pub const END: u64 = u64::MAX;
pub const SELF: u64 = u64::MAX;
pub const JOURNAL_ID: u64 = 16;

pub const TRACE: u8 = 0;
pub const DEBUG: u8 = 1;
pub const INFO: u8 = 2;
pub const WARN: u8 = 3;
pub const ERROR: u8 = 4;
pub const CRITICAL: u8 = 5;
pub const SYSTEM: u8 = 1;
pub const LIFECYCLE: u8 = 2;
pub const CONFIG: u8 = 3;
pub const DEPENDENCY: u8 = 4;
pub const CAPABILITY: u8 = 5;
pub const SERVICE: u8 = 6;
pub const MANAGER: u8 = 1;
pub const PRODUCER: u8 = 2;
pub const LOGGER: u8 = 3;
pub const UNKNOWN_TIME: u8 = 0;
pub const MONOTONIC: u8 = 1;
pub const BOOT: u16 = 1;
pub const SHUTDOWN: u16 = 2;
pub const STATE: u16 = 3;
pub const DISABLED: u16 = 4;
pub const ENABLED: u16 = 5;
pub const CONFIG_ERROR: u16 = 6;
pub const DEPENDENCY_WAIT: u16 = 7;
pub const WITHDRAWN: u16 = 8;
pub const RESTART: u16 = 9;
pub const DROPPED: u16 = 10;
pub const NOTICE: u16 = 11;
pub const ONLINE: u16 = 12;
// RESTRICT, RESTORE et CAP_REFUSED authentifient le port privé de
// maintenance. Les données contiennent la génération et le bit demandé.
pub const RESTRICT: u16 = 13;
pub const RESTORE: u16 = 14;
pub const CAP_REFUSED: u16 = 15;
pub const CAP_APPLIED: u16 = 16;
pub const CAP_MISSING: u16 = 17;

pub fn request(op: u64) -> Frame {
    let mut f = [0; 16];
    f[0] = VERSION;
    f[1] = op;
    f
}

pub fn encode(words: &Frame) -> [u8; 128] {
    let mut out = [0; 128];
    for (b, w) in out.chunks_exact_mut(8).zip(words) {
        b.copy_from_slice(&w.to_le_bytes());
    }
    out
}
pub fn decode(bytes: &[u8]) -> Option<Frame> {
    if bytes.len() != 128 {
        return None;
    }
    Some(core::array::from_fn(|i| {
        u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap())
    }))
}
pub fn bytes(words: &Event) -> [u8; 96] {
    let mut out = [0; 96];
    for (b, w) in out.chunks_exact_mut(8).zip(words) {
        b.copy_from_slice(&w.to_le_bytes());
    }
    out
}
pub fn from_bytes(bytes: &[u8; 96]) -> Event {
    core::array::from_fn(|i| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap()))
}
pub fn meta(service: u64, level: u8, category: u8, origin: u8, code: u16, quality: u8) -> u64 {
    VERSION
        | (u64::from(level) << 8)
        | (u64::from(category) << 16)
        | (u64::from(quality) << 20)
        | (u64::from(origin) << 24)
        | (u64::from(code) << 32)
        | (service << 48)
}
pub fn level(e: &Event) -> u8 {
    (e[8] >> 8) as u8
}
pub fn category(e: &Event) -> u8 {
    ((e[8] >> 16) & 15) as u8
}
pub fn quality(e: &Event) -> u8 {
    ((e[8] >> 20) & 15) as u8
}
pub fn origin(e: &Event) -> u8 {
    (e[8] >> 24) as u8
}
pub fn code(e: &Event) -> u16 {
    (e[8] >> 32) as u16
}
pub fn service(e: &Event) -> u64 {
    e[8] >> 48
}
pub fn valid_identity(id: Identity) -> bool {
    id[..2] != [0; 2] && id[2] != 0 && id[3] != 0
}
pub fn issuer(e: &Event) -> Identity {
    [e[2], e[3], e[6], e[7]]
}
pub fn subject(e: &Event) -> Identity {
    [e[2], e[3], e[4], e[5]]
}

/// L'identité de l'émetteur provient de la liaison IPC. Les deux mots
/// d'instance du sujet peuvent valoir zéro avant le lancement correspondant.
pub fn event(
    eid: u64,
    subject: Identity,
    issuer: Identity,
    service: u64,
    level: u8,
    category: u8,
    origin: u8,
    code: u16,
    ms: u64,
    data: [u64; 2],
) -> Frame {
    let mut f = request(EMIT);
    // Le schéma n'encode qu'un BootID partagé. Deux incarnations issues de
    // démarrages différents ne doivent jamais être rabattues sur ce même ID.
    if service > u16::MAX as u64
        || (subject != [0; 4] && (!valid_identity(subject) || subject[..2] != issuer[..2]))
    {
        return f; // EID nul : l'admission rend BAD_ARGUMENT.
    }
    f[4..].copy_from_slice(&[
        eid,
        0,
        issuer[0],
        issuer[1],
        subject[2],
        subject[3],
        issuer[2],
        issuer[3],
        meta(service, level, category, origin, code, MONOTONIC),
        ms,
        data[0],
        data[1],
    ]);
    f
}
pub fn valid(e: &Event) -> bool {
    e[0] != 0
        && valid_identity(issuer(e))
        && ((e[4] == 0) == (e[5] == 0))
        && e[8] as u8 == VERSION as u8
        && level(e) <= CRITICAL
        && (SYSTEM..=SERVICE).contains(&category(e))
        && quality(e) <= MONOTONIC
        && (quality(e) != UNKNOWN_TIME || e[9] == 0)
        && (MANAGER..=LOGGER).contains(&origin(e))
        && (BOOT..=CAP_MISSING).contains(&code(e))
}
