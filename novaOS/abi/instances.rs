//! Extension v1 additive. INFO ne consulte que l’appartenance de l’appelant.
pub const INFO: u64 = 0x10070;
pub const WORKER: u64 = 0x10071;
pub const STOP: u64 = 0x10072;

// Identités attribuées dans le manifeste immuable de la distribution embarquée.
// Les identités des services internes désignent le paquet système.
pub const SYSTEM: u64 = 0x4e4f_5641_0000_0001;
pub const SHELL: u64 = 0x4e4f_5641_0000_0010;
pub const WITNESS: u64 = 0x4e4f_5641_ffff_0001;
pub const OFFER_STOP: u64 = 0x10073;
pub const SAMPLE: u64 = 0x4e4f_5641_0000_0020;
pub const ALLOW: u64 = 1;
// Porte réservée au gestionnaire admis par le noyau.
pub const WATCH: u64 = 0x10074;
/// Lire l’offre d’arrêt uniquement avec le droit local de suivre ce principal.
pub const OFFERED: u64 = 0x10075;

/// Indice du programme de référence de l’installation embarquée.
pub const SAMPLE_INDEX: usize = 16;
