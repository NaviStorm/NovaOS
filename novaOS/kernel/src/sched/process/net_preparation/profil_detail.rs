//! Chronométrage additionnel réservé aux ELF de diagnostic. Les durées des
//! acteurs se recouvrent. Les appels traversant la limite d'un lot sont exclus.
use super::*;
pub(crate) const ACTIF: bool = option_env!("NOVAOS_ETH3B_DETAIL").is_some();
const ZERO: AtomicU64 = AtomicU64::new(0);
// Chaque classe conserve son nombre de mesures et le cumul des ticks.
const CLASSES: usize = 13;
static DATA: [[[AtomicU64; 2]; CLASSES]; 3] =
    [const { [const { [const { ZERO }; 2] }; CLASSES] }; 3];
struct Debut {
    t: AtomicU64,
    serie: AtomicU64,
    classe: AtomicU64,
}
impl Debut {
    const fn vide() -> Self {
        Self {
            t: ZERO,
            serie: ZERO,
            classe: ZERO,
        }
    }
    fn ouvrir(&self, classe: usize) {
        self.serie.store(COUNT.load(Relaxed), Relaxed);
        self.classe.store(classe as u64, Relaxed);
        self.t.store(hal::timer::counter(), Relaxed);
    }
    fn fermer(&self) {
        let t = self.t.swap(0, Relaxed);
        if t != 0 && self.serie.load(Relaxed) == COUNT.load(Relaxed) {
            ajouter(self.classe.load(Relaxed) as usize, t);
        }
    }
}
static APPELS: [Debut; 3] = [const { Debut::vide() }; 3];
static BLOQUES: [Debut; 3] = [const { Debut::vide() }; 3];
static REQUETE: Debut = Debut::vide();
static REQUETE_ID: AtomicU64 = ZERO;
static REQUETE_OP: AtomicU64 = ZERO;

fn ajouter(classe: usize, t: u64) {
    let Some(row) = CELL.load(Relaxed).checked_sub(1) else {
        return;
    };
    let data = &DATA[row as usize][classe];
    data[0].fetch_add(1, Relaxed);
    data[1].fetch_add(hal::timer::counter() - t, Relaxed);
}
fn acteur(pid: u64) -> Option<usize> {
    ACTEURS.iter().position(|p| p.load(Relaxed) == pid)
}
pub(super) fn entrer(idx: usize, classe: usize) {
    if ACTIF {
        APPELS[idx].ouvrir(classe);
    }
}
pub(crate) fn sortir(pid: u64) {
    super::notifications::retour(pid);
    if ACTIF && let Some(i) = acteur(pid) {
        APPELS[i].fermer();
    }
}
pub(crate) fn reprise(pid: u64) {
    super::notifications::reprendre(pid);
    if ACTIF && let Some(i) = acteur(pid) {
        BLOQUES[i].fermer();
    }
}
pub(super) fn bloque(pid: u64) {
    if ACTIF
        && CELL.load(Relaxed) != 0
        && let Some(i) = acteur(pid)
    {
        BLOQUES[i].ouvrir(3 + i);
    }
}
pub(crate) fn debut_copie() -> u64 {
    if ACTIF && CELL.load(Relaxed) != 0 {
        hal::timer::counter()
    } else {
        0
    }
}
pub(crate) fn fin_copie(put: bool, t: u64) {
    if ACTIF && t != 0 {
        ajouter(if put { 6 } else { 7 }, t);
    }
}
pub(super) fn message(pid: u64, tag: u64, payload: &[u8]) {
    if !ACTIF || CELL.load(Relaxed) == 0 || tag != 0x45544831 || payload.len() < 32 {
        return;
    }
    let op = u16::from_le_bytes([payload[4], payload[5]]);
    let classe = match op {
        3 => 8,
        4 => 9,
        5 => 10,
        6 => 11,
        13 => 12,
        _ => return,
    };
    let id = u64::from_le_bytes(payload[24..32].try_into().unwrap());
    if pid == ACTEURS[0].load(Relaxed) && payload[6] == 0 {
        REQUETE_ID.store(id, Relaxed);
        REQUETE_OP.store(op as u64, Relaxed);
        REQUETE.ouvrir(classe);
    } else if pid == ACTEURS[1].load(Relaxed)
        && payload[6] == 1
        && REQUETE_OP.load(Relaxed) == u64::from(op)
        && REQUETE_ID.load(Relaxed) == id
    {
        REQUETE.fermer();
    }
}
pub(super) fn reset() {
    if !ACTIF {
        return;
    }
    for row in &DATA {
        for col in row {
            for value in col {
                value.store(0, Relaxed);
            }
        }
    }
    for v in APPELS
        .iter()
        .chain(&BLOQUES)
        .chain(core::iter::once(&REQUETE))
    {
        v.t.store(0, Relaxed);
    }
}
pub(super) fn bilan() {
    if !ACTIF {
        return;
    }
    for (b, row) in [1, 4, 8].into_iter().zip(&DATA) {
        let couples = row
            .each_ref()
            .map(|v| v.each_ref().map(|n| n.load(Relaxed)));
        hal::log(format_args!("E3B detail lot={b} mesures={couples:?}"));
    }
}
