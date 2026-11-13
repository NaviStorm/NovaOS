//! Contrat binaire commun aux images utilisateur et au noyau, little-endian.
pub const VERSION: u32 = 1;
/// Borne exclusive du tas privé. Arguments : ancienne borne, nouvelle borne, zéro.
pub const HEAP_RESIZE: u64 = 0x0001_0041;
/// Heure civile UTC de la RTC, secondes entières depuis 1970, arguments à zéro.
pub const CLOCK_UTC: u64 = 0x10044;
pub const PROCESS_ID: u64 = 0x0001_0001;
pub const THREAD_ID: u64 = 0x0001_0002;
pub const THREAD_YIELD: u64 = 0x0001_0003;
pub const PROCESS_EXIT: u64 = 0x0001_0004;
pub const PROCESS_WAIT: u64 = 0x0001_0005;
pub const DEBUG_WRITE: u64 = 0x0001_0006;

pub const OK: u64 = 0;
pub const BAD_VERSION: u64 = 1;
pub const UNKNOWN_CALL: u64 = 2;
pub const BAD_ARGUMENT: u64 = 3;
pub const BAD_ADDRESS: u64 = 4;
pub const DENIED: u64 = 5;
pub const LIMIT: u64 = 6;
pub const DEBUG_MAX: usize = 256;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Reply {
    pub status: u64,
    pub value: u64,
}

#[repr(C)]
pub struct WaitInfo {
    pub version: u32,
    pub size: u32,
    pub process: u64,
    pub kind: u64,
    pub code: u64,
}
pub const WAIT_SIZE: usize = size_of::<WaitInfo>();
const _: () = assert!(WAIT_SIZE == 32 && align_of::<WaitInfo>() == 8);

pub const IPC_CREATE: u64 = 0x10020;
pub const IPC_DUP: u64 = 0x10021;
pub const IPC_CLOSE: u64 = 0x10022;
pub const IPC_REVOKE: u64 = 0x10023;
pub const IPC_SEND: u64 = 0x10024;
pub const IPC_RECEIVE: u64 = 0x10025;
pub const IPC_GIVE: u64 = 0x10026;
pub const IPC_NOTIFY: u64 = 0x10027;
pub const IPC_CANCEL: u64 = 0x10028;
pub const IPC_BOOTSTRAP: u64 = 0x10029;
/// Copie bornée sur un canal de lot configuré explicitement au lancement.
pub const IPC_PUT: u64 = 0x1002c;
pub const IPC_TAKE: u64 = 0x1002d;
pub const BULK_MAX: usize = 4096;
#[repr(C)]
pub struct BulkV1 {
    pub version: u32,
    pub size: u32,
    pub tag: u64,
    pub address: u64,
    pub length: u64,
}
pub const SEND: u32 = 1;
pub const RECEIVE: u32 = 2;
pub const GRANT: u32 = 4;
pub const MANAGE: u32 = 8;
pub const WOULD_BLOCK: u64 = 7;
pub const TIMED_OUT: u64 = 8;
pub const CANCELLED: u64 = 9;
pub const REVOKED: u64 = 10;
pub const CLOSED: u64 = 11;
pub const REFUSED: u64 = 12;
pub const NO_MEMORY: u64 = 13;

#[repr(C)]
pub struct SendV1 {
    pub version: u32,
    pub size: u32,
    pub timeout: u64,
    pub tag: u64,
    pub len: u32,
    pub flags: u32,
    pub payload: [u8; 128],
}
#[repr(C)]
pub struct ReceiveV1 {
    pub version: u32,
    pub size: u32,
    pub timeout: u64,
    pub options: u32,
    pub reserved: u32,
    pub kind: u32,
    pub len: u32,
    pub tag: u64,
    pub handle: u64,
    pub aux: u64,
    pub payload: [u8; 128],
}
#[repr(C)]
pub struct GiveV1 {
    pub version: u32,
    pub size: u32,
    pub timeout: u64,
    pub source: u64,
    pub tag: u64,
    pub rights: u32,
    pub mode: u32,
    pub reserved: u64,
}
const _: () = assert!(
    size_of::<SendV1>() == 160
        && size_of::<ReceiveV1>() == 184
        && size_of::<GiveV1>() == 48
        && align_of::<SendV1>() == 8
        && align_of::<ReceiveV1>() == 8
        && align_of::<GiveV1>() == 8
);
const _: () = assert!(
    core::mem::offset_of!(SendV1, version) == 0
        && core::mem::offset_of!(SendV1, size) == 4
        && core::mem::offset_of!(SendV1, timeout) == 8
        && core::mem::offset_of!(SendV1, tag) == 16
        && core::mem::offset_of!(SendV1, len) == 24
        && core::mem::offset_of!(SendV1, flags) == 28
        && core::mem::offset_of!(SendV1, payload) == 32
);
const _: () = assert!(
    core::mem::offset_of!(ReceiveV1, version) == 0
        && core::mem::offset_of!(ReceiveV1, size) == 4
        && core::mem::offset_of!(ReceiveV1, timeout) == 8
        && core::mem::offset_of!(ReceiveV1, options) == 16
        && core::mem::offset_of!(ReceiveV1, reserved) == 20
        && core::mem::offset_of!(ReceiveV1, kind) == 24
        && core::mem::offset_of!(ReceiveV1, len) == 28
        && core::mem::offset_of!(ReceiveV1, tag) == 32
        && core::mem::offset_of!(ReceiveV1, handle) == 40
        && core::mem::offset_of!(ReceiveV1, aux) == 48
        && core::mem::offset_of!(ReceiveV1, payload) == 56
);
const _: () = assert!(
    core::mem::offset_of!(GiveV1, version) == 0
        && core::mem::offset_of!(GiveV1, size) == 4
        && core::mem::offset_of!(GiveV1, timeout) == 8
        && core::mem::offset_of!(GiveV1, source) == 16
        && core::mem::offset_of!(GiveV1, tag) == 24
        && core::mem::offset_of!(GiveV1, rights) == 32
        && core::mem::offset_of!(GiveV1, mode) == 36
        && core::mem::offset_of!(GiveV1, reserved) == 40
);
