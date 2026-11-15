//! Déclarations natives version 1 embarquées dans l'ELF du gestionnaire embarqué
//! dans l'image système de confiance. Le profil de démarrage choisit une tranche immuable.
use super::policy::{Declaration as D, Restart};
pub const PRIVATE_TMP: u64 = 1;
pub const SYSTEM_IMAGE: u64 = 1;
pub const STORAGE_IMAGE: u64 = 2;
pub const CACHE_BACKEND: u64 = 2;
pub const CACHE_CLIENT: u64 = 4;

const BASE: D = D {
    version: 1,
    id: 1,
    image: SYSTEM_IMAGE,
    args: &[0],
    required: &[],
    optional: &[],
    caps: PRIVATE_TMP,
    auto: true,
    restart: Restart::Failure,
    start_ticks: 5_000,
    stop_ticks: 800,
    heartbeat_ticks: 3_000,
    retries: 2,
    window: 12_000,
    backoff: 100,
    log: true,
};

pub const JOURNAL_IMAGE: u64 = 3;
pub const JOURNAL_STORE: u64 = 8;
pub const JOURNAL: D = D {
    id: 16, image: JOURNAL_IMAGE, caps: JOURNAL_STORE,
    start_ticks: 20_000, heartbeat_ticks: 20_000, ..BASE
};

const CLOCK: D = BASE;
const TMP_HEALTH: D = D { id: 2, ..BASE };
const STORAGE_INFO: D = D {
    id: 9,
    image: STORAGE_IMAGE,
    ..BASE
};
// L'accès au stockage P9 est provisionné avant son admission. Ni le service
// CACHE ni les clients du cache n'ont besoin d'un tmp pour démarrer.
const CACHE: D = D {
    id: 10,
    required: &[9],
    caps: CACHE_BACKEND,
    ..BASE
};
pub const INSTALLED: &[D] = &[CLOCK, TMP_HEALTH, STORAGE_INFO, CACHE];
const MANUAL: D = D {
    auto: false,
    restart: Restart::Never,
    ..BASE
};
const ACTOR: D = D {
    caps: PRIVATE_TMP | CACHE_CLIENT,
    ..MANUAL
};
// Les acteurs n'ont pas de dépendance de vie sur CACHE : leur processus doit
// pouvoir constater la révocation pendant l'arrêt de ce fournisseur.
pub const CACHE_QUALIFICATION: &[D] = &[
    CLOCK,
    TMP_HEALTH,
    STORAGE_INFO,
    CACHE,
    D { id: 11, ..ACTOR },
    D { id: 12, ..ACTOR },
    D { id: 13, ..ACTOR },
    D { id: 14, ..ACTOR },
    D { id: 15, ..ACTOR },
];

pub const QUALIFICATION: &[D] = &[
    CLOCK,
    TMP_HEALTH,
    STORAGE_INFO,
    D {
        id: 3,
        required: &[1, 2],
        ..MANUAL
    },
    D {
        id: 4,
        start_ticks: 700,
        ..MANUAL
    },
    D {
        id: 5,
        restart: Restart::Failure,
        ..MANUAL
    },
    D {
        id: 6,
        stop_ticks: 300,
        ..MANUAL
    },
    D {
        id: 7,
        required: &[8],
        optional: &[2],
        ..MANUAL
    },
    D {
        id: 8,
        restart: Restart::Failure,
        start_ticks: 20_000,
        ..MANUAL
    },
];

// Catalogues rejetés à l'enregistrement par la même admission que le graphe de démarrage.
pub const CYCLE: &[D] = &[
    D {
        id: 100,
        required: &[101],
        ..MANUAL
    },
    D {
        id: 101,
        required: &[100],
        ..MANUAL
    },
];
pub const MISSING: &[D] = &[D {
    id: 101,
    required: &[102],
    ..MANUAL
}];
