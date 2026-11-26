//! Mesures de la veille RX sur l'ELF de diagnostic monoprocesseur.
use super::*;

struct Mesure {
    debut: AtomicU64,
    nombre: AtomicU64,
    ticks: AtomicU64,
}

impl Mesure {
    const fn vide() -> Self {
        Self {
            debut: ZERO,
            nombre: ZERO,
            ticks: ZERO,
        }
    }
    fn ouvrir(&self, t: u64) {
        self.debut.store(t, Relaxed);
    }
    fn fermer(&self) {
        let debut = self.debut.swap(0, Relaxed);
        if debut != 0 {
            self.ajouter(hal::timer::counter() - debut);
        }
    }
    fn ajouter(&self, ticks: u64) {
        self.nombre.fetch_add(1, Relaxed);
        self.ticks.fetch_add(ticks, Relaxed);
    }
    fn resultat(&self) -> [u64; 2] {
        [self.nombre.load(Relaxed), self.ticks.load(Relaxed)]
    }
}

static ARMEMENT: Mesure = Mesure::vide();
static NOTIFICATION: Mesure = Mesure::vide();
static REPRISE: Mesure = Mesure::vide();
static REARMEMENT_RX: Mesure = Mesure::vide();

fn actif() -> bool {
    detail::ACTIF && CASE.load(Relaxed) == 7
}

pub(super) fn controle(pid: u64, tag: u64, donnees: &[u8]) {
    if !actif() || tag != 0x45544831 || donnees.len() < 32 || donnees[4..6] != 7u16.to_le_bytes() {
        return;
    }
    // Le client de veille échange une seule requête ARM à la fois.
    if donnees[6] == 0 && pid == ACTEURS[0].load(Relaxed) {
        ARMEMENT.ouvrir(hal::timer::counter());
    } else if donnees[6] == 1 && pid == ACTEURS[1].load(Relaxed) {
        ARMEMENT.fermer();
    }
}

pub(super) fn notifier(pid: u64, id: u64, bits: u64) {
    if actif() && id == abi::IPC_NOTIFY && bits & 2 != 0 && pid == ACTEURS[1].load(Relaxed) {
        let t = hal::timer::counter();
        NOTIFICATION.ouvrir(t);
        REPRISE.ouvrir(t);
    }
}

pub(super) fn retour(pid: u64) {
    if actif() && pid == ACTEURS[1].load(Relaxed) {
        NOTIFICATION.fermer();
    }
}

pub(super) fn reprendre(pid: u64) {
    if actif() && pid == ACTEURS[0].load(Relaxed) {
        REPRISE.fermer();
    }
}

pub(super) fn rearmement(op: u64, ticks: u64) {
    if actif() && op == 6 {
        REARMEMENT_RX.ajouter(ticks);
    }
}

pub(super) fn reset() {
    if actif() {
        for mesure in [&ARMEMENT, &NOTIFICATION, &REPRISE, &REARMEMENT_RX] {
            mesure.debut.store(0, Relaxed);
            mesure.nombre.store(0, Relaxed);
            mesure.ticks.store(0, Relaxed);
        }
    }
}

pub(super) fn bilan() {
    if actif() {
        hal::log(format_args!(
            "E3B veille arm={:?} notify={:?} reprise={:?} rearmement_rx={:?}",
            ARMEMENT.resultat(),
            NOTIFICATION.resultat(),
            REPRISE.resultat(),
            REARMEMENT_RX.resultat()
        ));
    }
}
