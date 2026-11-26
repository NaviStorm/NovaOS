//! Messages privés entre service et adaptateur. Les identifiants de crédit
//! appartiennent au service et sont opaques pour le matériel.
pub const OUVRIR: u64 = 1;
pub const PREPARER: u64 = 2;
pub const PUBLIER: u64 = 3;
pub const FERMER: u64 = 4;
pub const COMPLETION: u64 = 5;
pub const RECEPTION: u64 = 6;
pub const ECOUTER: u64 = 7;
pub const CREDITS: usize = 8;
