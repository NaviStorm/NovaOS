//! Instrumentation de qualification, ajoutée uniquement au noyau de mesure.
//! Les sept lots suivent trois groupes pour limiter l'effet des lectures d'horloge.
use crate::{hal, smp};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

#[derive(Clone, Copy)]
pub(crate) enum Site {
    Vide, Capacite, Droits, Offre, Receveur, Publication, Attente, Signaux, Collecte,
    TablesLocales, RegionRam, Translation, CopieDepuis, CopieVers, ChargeUtile, Traitement,
    Identite, Choix, Reveil, Bascule, Espace, IpiEmission, IrqService, Tlb,
}
use Site::*;
const SITES: &[(Site, &str, usize)] = &[
    (Vide,"vide",0), (Capacite,"capacite",1), (Droits,"droits",1),
    (Offre,"offre",1), (Receveur,"receveur",1), (Publication,"publication",1),
    (Attente,"attente",1), (Signaux,"signaux",1), (Collecte,"collecte",1),
    (TablesLocales,"tables_locales",1), (RegionRam,"region_ram",2),
    (Translation,"translation",2), (CopieDepuis,"copie_depuis",2),
    (CopieVers,"copie_vers",2), (ChargeUtile,"charge_utile",2),
    (Traitement,"traitement",2), (Identite,"identite",3), (Choix,"choix",3),
    (Reveil,"reveil",3), (Bascule,"bascule",3), (Espace,"espace",3),
    (IpiEmission,"ipi_emission",3), (IrqService,"irq_service",3), (Tlb,"tlb",3),
];

// Un compteur distinct par lot évite d'attribuer la fin tardive d'un événement au lot suivant.
const LOTS: usize = 8;
static LOT: AtomicUsize = AtomicUsize::new(LOTS);
struct Compteur {
    appels: AtomicU64,
    ticks: AtomicU64,
}
static COUTS: [[Compteur; SITES.len()]; LOTS] = [const {
    [const { Compteur { appels: AtomicU64::new(0), ticks: AtomicU64::new(0) } }; SITES.len()]
}; LOTS];
static DEPART_CPU: [AtomicU64; smp::MAX_CPUS] = [const { AtomicU64::new(0) }; smp::MAX_CPUS];
static LOT_CPU: [AtomicUsize; smp::MAX_CPUS] = [const { AtomicUsize::new(LOTS) }; smp::MAX_CPUS];

fn groupe(lot: usize) -> usize { if lot == 0 { 1 } else { (lot - 1) % 3 + 1 } }

pub(crate) struct Mesure {
    debut: u64,
    lot: usize,
    site: Site,
}

#[inline]
pub(crate) fn mesurer(site: Site) -> Mesure {
    let lot = LOT.load(Relaxed);
    let groupe_site = SITES[site as usize].2;
    let actif = lot < LOTS && (groupe_site == 0 || groupe(lot) == groupe_site);
    Mesure { debut: if actif { hal::timer::counter() } else { 0 }, lot, site }
}

#[inline]
fn enregistrer(lot: usize, site: Site, debut: u64, fin: u64) {
    if debut != 0 && LOT.load(Relaxed) == lot {
        let cout = &COUTS[lot][site as usize];
        cout.ticks.fetch_add(fin.saturating_sub(debut), Relaxed);
        cout.appels.fetch_add(1, Relaxed);
    }
}
impl Drop for Mesure {
    #[inline]
    fn drop(&mut self) {
        if self.debut != 0 {
            enregistrer(self.lot, self.site, self.debut, hal::timer::counter());
        }
    }
}

// L'horodatage appartient au PE du changement de pile, jamais au thread suspendu.
pub(crate) fn sortant() {
    let lot = LOT.load(Relaxed);
    if lot < LOTS && groupe(lot) == 3 {
        let cpu = smp::index();
        LOT_CPU[cpu].store(lot, Relaxed);
        DEPART_CPU[cpu].store(hal::timer::counter(), Relaxed);
    }
}
pub(crate) fn entrant() {
    let lot = LOT.load(Relaxed);
    if lot < LOTS && groupe(lot) == 3 {
        let fin = hal::timer::counter();
        let cpu = smp::index();
        if LOT_CPU[cpu].load(Relaxed) == lot {
            enregistrer(lot, Bascule, DEPART_CPU[cpu].swap(0, Relaxed), fin);
        }
    }
}

pub(crate) fn commencer(lot: usize) {
    assert!(lot < LOTS);
    LOT.store(lot, Relaxed);
    for _ in 0..128 {
        drop(mesurer(Vide));
    }
}
pub(crate) fn terminer() {
    LOT.store(LOTS, Relaxed);
}
pub(crate) fn rapport(lot: usize) {
    for &(site, nom, famille) in SITES {
        if famille == 0 || famille == groupe(lot) {
            let c = &COUTS[lot][site as usize];
            hal::log(format_args!("P4 site={nom} lot={lot} groupe={} appels={} ticks={}",
                groupe(lot), c.appels.load(Relaxed), c.ticks.load(Relaxed)));
        }
    }
}
