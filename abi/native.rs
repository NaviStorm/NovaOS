//! Contrat binaire commun aux images utilisateur et au noyau, little-endian.
pub const VERSION: u32 = 1;
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
