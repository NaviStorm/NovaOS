//! Contrat privé de l'image Ring 3 EDU VT-d. Arguments IPC v1, sans adresse DMA.
pub const NORMAL: u64 = 1;
pub const DEATH: u64 = 2;
pub const REVOKE: u64 = 3;
pub const REFUSED: u64 = 4;
pub const FAULT_PROBE: u64 = 5;
pub const WITNESS: u64 = 6;
pub const CONTENDER: u64 = 7;
pub const LATE: u64 = 8;

pub const READ: u64 = 1;
pub const WRITE: u64 = 2;
pub const START_INFLIGHT: u64 = 3;
pub const PROBE_READY: u64 = 4;
pub const PREPARE: u64 = 5;
pub const RETIRE_READY: u64 = 6;

pub const ACCEPTED: u64 = 1;
pub const REJECTED: u64 = 2;
pub const DRAINED: u64 = 4;
