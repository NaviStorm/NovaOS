//! Déclarations natives version 1 embarquées dans l'ELF du gestionnaire embarqué
//! dans l'image système de confiance. Le profil de démarrage choisit une tranche immuable.
use super::policy::{Declaration as D, Restart};
pub const PRIVATE_TMP: u64 = 1;
pub const SYSTEM_IMAGE: u64 = 1;
pub const STORAGE_IMAGE: u64 = 2;

const BASE: D = D {
    version: 1, id: 1, image: SYSTEM_IMAGE, args: &[0], required: &[], optional: &[],
    caps: PRIVATE_TMP, auto: true, restart: Restart::Failure, start_ticks: 5_000,
    stop_ticks: 800, heartbeat_ticks: 3_000, retries: 2, window: 12_000,
    backoff: 100, log: true,
};

const CLOCK: D = BASE;
const TMP_HEALTH: D = D { id: 2, ..BASE };
const STORAGE_INFO: D = D { id: 9, image: STORAGE_IMAGE, ..BASE };
pub const INSTALLED: &[D] = &[CLOCK, TMP_HEALTH, STORAGE_INFO];
const MANUAL: D = D { auto: false, restart: Restart::Never, ..BASE };
pub const QUALIFICATION: &[D] = &[
    CLOCK, TMP_HEALTH, STORAGE_INFO,
    D { id: 3, required: &[1, 2], ..MANUAL },
    D { id: 4, start_ticks: 700, ..MANUAL },
    D { id: 5, restart: Restart::Failure, ..MANUAL },
    D { id: 6, stop_ticks: 300, ..MANUAL },
    D { id: 7, required: &[8], optional: &[2], ..MANUAL },
    D { id: 8, restart: Restart::Failure, start_ticks: 20_000, ..MANUAL },
];
