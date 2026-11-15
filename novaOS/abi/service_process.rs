//! Mécanismes du gestionnaire installé au démarrage. Aucun PID fourni par
//! l’appelant ne donne le droit de suivre ou d’arrêter un processus.
pub const START: u64 = 1;
pub const POLL: u64 = 2;
pub const KILL: u64 = 3;
pub const NOW: u64 = 4;
pub const DRAIN: u64 = 5;
pub const SHUTDOWN_NOTICE: u64 = 1 << 40;
pub const FRAME: usize = 128;
