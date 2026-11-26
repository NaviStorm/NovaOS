//! Lanceur de la fixture. L'ouverture de la porte matérielle reste attachée
//! à la racine SEND exacte du seul pilote. Le service et le client utilisent l'IPC ordinaire.
use super::*;
use crate::abi;
use core::sync::atomic::{AtomicU64, Ordering::Relaxed};
const SERVICE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ethernet_port.bin"));
const CLIENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ethernet_client.bin"));
#[path = "profil_3b.rs"]
pub(crate) mod profil_3b;
const N: usize = 16384;
static VEILLE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
pub(crate) fn bloque(pid: u64, id: u64, ptr: u64) {
    profil_3b::attente(pid);
    if pid == CLIENT_PID.load(Relaxed)
        && id == abi::IPC_RECEIVE
        && ptr == user::DATA + 3072
        && !VEILLE.swap(true, Relaxed)
    {
        hal::log(format_args!("ETH3 RX attente bloquée"));
    }
}
static CLIENT_PID: AtomicU64 = AtomicU64::new(0);
static LOT: AtomicU64 = AtomicU64::new(0);
static START: AtomicU64 = AtomicU64::new(0);
static SAMPLES: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static COUNT: AtomicU64 = AtomicU64::new(0);
static ACTEURS: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static IPC: [[AtomicU64; 6]; 3] = [const { [const { AtomicU64::new(0) }; 6] }; 3];
pub(crate) fn compter(pid: u64, id: u64, args: [u64; 3], wakes: u64) {
    profil_3b::appel(pid, id, args, wakes);
    let colonne = match id {
        abi::IPC_SEND => 0,
        abi::IPC_RECEIVE => 1,
        abi::IPC_PUT => 2,
        abi::IPC_TAKE => 3,
        abi::IPC_FEATURES => 4,
        abi::IPC_CREATE..=abi::IPC_BOOTSTRAP => 5,
        _ => return,
    };
    if let Some(i) = ACTEURS.iter().position(|p| p.load(Relaxed) == pid) {
        IPC[i][colonne].fetch_add(1, Relaxed);
    }
}
static FIRST: AtomicU64 = AtomicU64::new(0);
static LAST: AtomicU64 = AtomicU64::new(0);
pub(crate) fn timing(pid: u64, id: u64, ptr: u64, end: bool, wakes: u64) {
    if pid != CLIENT_PID.load(Relaxed) || pid == 0 {
        return;
    }
    if !end
        && id == abi::IPC_SEND
        && (user::DATA + 2560..=user::DATA + 2616).contains(&ptr)
        && ptr % 8 == 0
    {
        LOT.store((ptr - user::DATA - 2560) / 8, Relaxed);
        profil_3b::debut(LOT.load(Relaxed) + 1, wakes);
        let t = hal::timer::counter();
        START.store(t, Relaxed);
        let _ = FIRST.compare_exchange(0, t, Relaxed, Relaxed);
    } else if end && id == abi::IPC_RECEIVE && ptr == user::DATA + 2816 && !profil_3b::actif() {
        fin_lot();
    }
}
fn fin_lot() {
    let start = START.swap(0, Relaxed);
    if start != 0 {
        let t = hal::timer::counter();
        let i = COUNT.fetch_add(1, Relaxed) as usize;
        if i < N {
            if let Some(v) = (t - start).checked_mul(8) {
                SAMPLES[i].store(v | LOT.load(Relaxed), Relaxed);
            }
        }
        LAST.store(t, Relaxed);
    }
}
/// Les trois bits bas identifient la taille du lot. Le calcul de rang
/// s'effectue après collecte des acteurs, sans solliciter le tas.
fn distribution(samples: &[AtomicU64], taille: Option<u64>) -> (usize, u64, [u64; 4]) {
    let valeurs = || {
        samples
            .iter()
            .map(|s| s.load(Relaxed))
            .filter(|v| taille.is_none_or(|n| v & 7 == n - 1))
            .map(|v| v >> 3)
    };
    let n = valeurs().count();
    let max = valeurs().max().unwrap_or(0);
    let qs = [500, 950, 990, 999].map(|q| {
        let rang = (n * q).div_ceil(1000);
        let (mut bas, mut haut) = (0, max);
        while bas < haut {
            let milieu = bas + (haut - bas) / 2;
            if valeurs().filter(|v| *v <= milieu).count() >= rang {
                haut = milieu
            } else {
                bas = milieu + 1;
            }
        }
        bas
    });
    (n, valeurs().sum(), qs)
}

fn ecrire(s: &Core, p: Pid, fields: &[(usize, u64)]) -> Result<(), u64> {
    let space = s.processes.get(p).space.as_ref().ok_or(abi::REVOKED)?;
    heap::with_pages(|m| {
        for &(i, v) in fields {
            space.write_data(m, i, v);
        }
        Ok(())
    })
    .map_err(|_| abi::NO_MEMORY)
}
fn setup(
    generation: u64,
    case: u64,
    total: u64,
) -> Option<(Child, Child, Child, u64, [[u64; 2]; 2])> {
    let _guard = Guard::new();
    let boot = crate::boot_id::current()?;
    let (driver, hw) = creer(3, None, generation)?;
    let mut group = [Some(driver), None, None];
    let mut reserves: [Option<Child>; 3] = [None, None, None];
    let mut racines = [[0; 2]; 2];
    let attempt = (|| -> Result<(), u64> {
        group[1] = Some(create_image(SERVICE, &[], None, None).map_err(ipc::launch_error)?);
        group[2] = Some(create_image(CLIENT, &[], None, None).map_err(ipc::launch_error)?);
        if case == 9 {
            for child in &mut reserves {
                *child = Some(create_image(CLIENT, &[], None, None).map_err(ipc::launch_error)?);
            }
            let ids = reserves.each_ref().map(|slot| slot.as_ref().unwrap().0);
            with(|s| ipc::liaison_ethernet::remplir(s, ids))?;
        }
        let [d, s, c] = group.each_ref().map(|v| v.as_ref().unwrap().0);
        with(|k| {
            let (front, client) = ipc::liaison_ethernet::installer(k, s, c, case != 2, true)?;
            let (back, adapter) = ipc::liaison_ethernet::installer(k, s, d, case != 2, false)?;
            racines = [front, back];
            let identite = k
                .processes
                .get(c)
                .instance
                .identity(Some(boot))
                .ok_or(abi::DENIED)?;
            let epoch = k.processes.get(s).instance.serial;
            ecrire(
                k,
                c,
                &[
                    (1, client[0]),
                    (2, client[1]),
                    (6, epoch),
                    (10, hw),
                    (11, case),
                    (12, total),
                ],
            )?;
            ecrire(
                k,
                s,
                &[
                    (1, front[0]),
                    (2, front[1]),
                    (3, back[0]),
                    (4, back[1]),
                    (5, c.number),
                    (6, epoch),
                    (18, identite.instance),
                    (7, u64::from_le_bytes(boot[..8].try_into().unwrap())),
                    (8, u64::from_le_bytes(boot[8..].try_into().unwrap())),
                    (9, front[0]),
                    (16, case),
                    (17, if case == 7 { 6 } else { 7 }),
                ],
            )?;
            ecrire(k, d, &[(14, adapter[0]), (15, adapter[1]), (16, case)])?;
            let roots = [c, s, d].map(|p| k.ipc.racines_pilotes(p));
            if roots != [0, 4, 1] {
                return Err(abi::LIMIT);
            }
            hal::log(format_args!(
                "ETH3 session client={} service={} pilote={} incarnation={} generation_instance={} epoque={epoch} generation_materiel={generation} mode={case} racines={roots:?} boites_globales={} endpoints_globaux={} ipc_pages={}",
                c.number,
                s.number,
                d.number,
                identite.instance,
                identite.generation,
                k.ipc.pages_bulk_count(),
                k.ipc.endpoint_count(),
                k.ipc.pages()
            ));
            Ok::<(), u64>(())
        })
    })();
    for child in reserves.into_iter().flatten() {
        with(|s| {
            s.ipc.death(child.0, ticks());
            s.ipc.collect();
        });
        ipc::undo_unpublished(child);
    }
    if attempt.is_err() {
        for child in group.into_iter().flatten() {
            with(|s| s.ipc.death(child.0, ticks()));
            ipc::undo_unpublished(child);
        }
        return None;
    }
    let [d, s, c] = group.map(Option::unwrap);
    Some((d, s, c, hw, racines))
}
#[cfg(feature = "net-q35-duo")]
static CAS_ADMIS: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(u8::MAX);

// Le chargeur restitue sa mémoire après handoff. Seul ce discriminant validé demeure.
#[cfg(feature = "net-q35-duo")]
pub(crate) fn admettre(scenario: &[u8]) -> bool {
    let mode = match scenario {
        b"net-q35-duo-nvme-charge" => 0,
        b"net-q35-duo-nvme-panne-nic" => 3,
        _ => return false,
    };
    CAS_ADMIS.store(mode, core::sync::atomic::Ordering::Release);
    true
}
type Groupe = (Child, Child, Child, u64, [[u64; 2]; 2]);
static DERNIERE_GENERATION: AtomicU64 = AtomicU64::new(0);
static DERNIERE_CLE: AtomicU64 = AtomicU64::new(0);

fn reserver_generation(generation: u64) -> Option<u64> {
    let precedente = DERNIERE_GENERATION.fetch_max(generation, core::sync::atomic::Ordering::AcqRel);
    (generation > precedente).then_some(precedente)
}

pub(crate) struct EthernetActor {
    groupe: Option<Groupe>,
    generation: u64,
    case: u64,
    base: u64,
}

impl EthernetActor {
    pub(crate) fn reserve(generation: u64) -> Option<Self> {
        let case = match option_env!("NOVAOS_ETH3_CASE") {
            Some("data") => 2,
            Some("veille") => 7,
            Some("court" | "cycles-court" | "preuve-absente" | "recu-vtd-absent" | "recu-vtd-differe") => 8,
            Some("revocation") => 6,
            Some("deces" | "deces-recu-tx-absent") => 3,
            Some("endurance") => 5,
            Some("incertain") => 4,
            Some("boite") => 1,
            Some("plein") => 9,
            Some("mixte100k") => 20,
            Some("ancien") => 21,
            Some("choix-data") => 22,
            Some("segment-interrompu" | "rattrapage") => 23,
            Some("recu-collecte") => 24,
            Some("echo60") => 12,
            Some("echo1514") => 13,
            Some("tx60") => 14,
            Some("tx1514") => 15,
            Some("rx60") => 16,
            Some("rx1514") => 17,
            Some("duplex60") => 18,
            Some("duplex1514") => 19,
            Some("mesure60") => 10,
            Some("mesure1514") => 11,
            _ => 0,
        };
        let total = match case {
            5 => 65552,
            10 | 11 => 4608,
            12..=19 => 13 * 1024,
            20 => 100000,
            7 => 16,
            8 => 1,
            _ => 144,
        };
        #[cfg(feature = "net-q35-duo")]
        if u64::from(CAS_ADMIS.load(core::sync::atomic::Ordering::Acquire)) != case {
            return None;
        }
        // Une incarnation refusée ou abandonnée ne peut jamais rouvrir un numéro consommé.
        let precedente = reserver_generation(generation)?;
        if precedente != 0 && matches!(option_env!("NOVAOS_ETH3_CASE"), Some("cycles" | "cycles-court" | "rattrapage" | "recu-arm-differe" | "recu-vtd-differe"))
            && (reserver_generation(precedente).is_some() || reserver_generation(generation).is_some()) {
            return None;
        }
        measure("eth3_initial");
        let base = heap::audit(false).free_pages;
        let groupe = setup(generation, case, total)?;
        let ancienne_cle = DERNIERE_CLE.swap(groupe.3, Relaxed);
        let ancienne_valide = with(|k| k.ipc.lab_can_submit(groupe.0.0, ancienne_cle));
        let acteur = Self { groupe: Some(groupe), generation, case, base };
        if ancienne_valide {
            return None;
        }
        hal::log(format_args!("E3C admission generation={generation} precedente={precedente} ancienne_capacite_valide={ancienne_valide} generation_verrouillee=true"));
        Some(acteur)
    }
    #[cfg(feature = "net-q35-duo")]
    pub(crate) fn owner(&self) -> u64 {
        let (pilote, _, _, _, _) = self.groupe.as_ref().unwrap();
        pilote.0.number
    }
    #[cfg(feature = "net-q35-duo")]
    pub(in crate::sched::process) fn audit(&self, verifier: impl Fn(Pid) -> bool) -> bool {
        let (d, s, c, _, _) = self.groupe.as_ref().unwrap();
        [d.0, s.0, c.0].into_iter().all(verifier)
    }
    #[cfg(feature = "net-q35-duo")]
    pub(crate) fn prepare_local_fault(&mut self, nic: bool) -> bool {
        nic && self.case == 3
    }
    #[cfg(feature = "net-q35-duo")]
    pub(crate) fn publish_clock(&self, hz: u64) -> bool {
        hz != 0 && matches!(self.case, 0 | 3)
    }
    #[cfg(feature = "net-q35-duo")]
    pub(crate) fn cancel(self) {
        drop(self);
    }
}

impl Drop for EthernetActor {
    fn drop(&mut self) {
        let Some((d, s, c, _, _)) = self.groupe.take() else {
            return;
        };
        for child in [c, s, d] {
            with(|k| {
                if k.processes.get(child.0).outcome.is_none() {
                    stop_process(k, child.0, Outcome::Exit(125), hal::timer::counter());
                }
            });
            child.join();
        }
    }
}

#[cfg(feature = "net-q35-duo")]
static PILOTE_COLLECTE: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "net-q35-duo")]
pub(crate) fn nic_exited(pid: u64) -> bool {
    pid != 0 && PILOTE_COLLECTE.load(core::sync::atomic::Ordering::Acquire) == pid
}

// Le profil signé autorise uniquement le disque sain. Si une autre trajectoire
// atteignait le superviseur, ses rendez-vous seraient refusés et sa clôture exigée.
#[cfg(feature = "net-q35-duo")]
pub(crate) fn disk_isolated() -> bool {
    true
}
#[cfg(feature = "net-q35-duo")]
pub(crate) fn await_first_batch() -> bool {
    false
}
#[cfg(feature = "net-q35-duo")]
pub(crate) fn permit_next_batch() -> bool {
    false
}

pub(crate) fn ethernet(
    generation: u64,
    pendant: impl FnOnce() -> bool,
    operation: impl FnMut(u64, usize, &mut [u8]) -> Result<usize, u64>,
) -> bool {
    let Some(a) = EthernetActor::reserve(generation) else {
        hal::log(format_args!(
            "ETH3 refus ouverture bootid={} cadres={}",
            crate::boot_id::current().is_some(),
            heap::audit(false).free_pages
        ));
        return false;
    };
    ethernet_actor(a, pendant, operation, false)
}

pub(crate) fn ethernet_actor(
    mut acteur: EthernetActor,
    pendant: impl FnOnce() -> bool,
    mut operation: impl FnMut(u64, usize, &mut [u8]) -> Result<usize, u64>,
    stockage_partage: bool,
) -> bool {
    let (base, case, generation) = (acteur.base, acteur.case, acteur.generation);
    let (driver, service, client, hw, [front, backend]) = acteur.groupe.take().unwrap();
    let (pid, sp, cp) = (driver.0, service.0, client.0);
    let mut frame = [0u8; 1526];
    let mut sequence = 0u64;
    profil_3b::initialiser(case, front, hw);
    COUNT.store(0, Relaxed);
    for (i, p) in [cp, sp, pid].into_iter().enumerate() {
        for c in &IPC[i] {
            c.store(0, Relaxed);
        }
        ACTEURS[i].store(p.number, Relaxed);
    }
    START.store(0, Relaxed);
    FIRST.store(0, Relaxed);
    LAST.store(0, Relaxed);
    CLIENT_PID.store(cp.number, Relaxed);
    let free_open = heap::audit(false).free_pages;
    measure("eth3_ouvert");
    let profil = crate::sched::profile::ACTIVE.swap(true, Relaxed);
    let sw = crate::sched::profile::switches_bsp();
    let cpu = crate::sched::profile::temps_bsp();
    let mut mediation_ticks = 0u64;
    let wakes = with(|s| s.stats.wakes);
    let idle = with(|s| s.stats.idle_ticks);
    let initial = ticks();
    let mut publish = Some(pendant);
    let mut last = ticks();
    let mut status = Err(abi::TIMED_OUT);
    let mut calls = 0u64;
    let mut peak = free_open;
    loop {
        let request = with(|s| s.ipc.lab_pop(pid, hw));
        if with(|k| {
            if k.processes.get(cp).outcome.is_some() {
                ipc::liaison_ethernet::retirer(k, sp, front);
                ipc::liaison_ethernet::retirer(k, sp, backend);
                true
            } else if k.processes.get(pid).outcome.is_some() {
                ipc::liaison_ethernet::retirer(k, sp, backend);
                true
            } else {
                k.processes.get(sp).outcome.is_some()
            }
        }) {
            status = Err(abi::REVOKED);
            break;
        }
        let (op, ep, seq) = match request {
            Ok(Some(v)) => v,
            Ok(None) if ticks().wrapping_sub(last) < 250 * 15 => {
                yield_now();
                continue;
            }
            Err(e) => {
                status = Err(e);
                break;
            }
            Ok(None) => break,
        };
        if ep != generation || seq != sequence + 1 {
            status = Err(abi::DENIED);
            break;
        }
        sequence = seq;
        if op == 1 {
            // Lecture d'un témoin de qualification. Le noyau authentifie les droits séparément.
            let selection = with(|k| {
                k.processes
                    .get(cp)
                    .space
                    .as_ref()
                    .and_then(|space| heap::with_pages(|m| Ok(space.read_data(m, 64))).ok())
            });
            hal::log(format_args!("E3B transport_client={selection:?}"));
        }
        last = ticks();
        calls += 1;
        let debut_mediation = hal::timer::counter();
        let outcome = with(|k| {
            if !k.ipc.lab_can_submit(pid, hw) || k.processes.get(pid).outcome.is_some() {
                return Err(abi::REVOKED);
            }
            let space = k.processes.get(pid).space.as_ref().ok_or(abi::REVOKED)?;
            let len = heap::with_pages(|m| Ok(space.read_data(m, 6))).map_err(|_| abi::NO_MEMORY)?
                as usize;
            if op == 2 {
                if !(72..=1526).contains(&len)
                    || !heap::with_pages(|m| {
                        Ok(space.copy_from_user(m, 0x401100, &mut frame[..len]))
                    })
                    .unwrap_or(false)
                {
                    return Err(abi::BAD_ARGUMENT);
                }
            } else if len != 0 || !matches!(op, 1 | 4 | 6 | 7 | 8 | 32..=39) {
                return Err(abi::DENIED);
            }
            let ret = operation(op, len, &mut frame);
            if let Ok(n) = ret
                && op == 8
                && (n > frame.len()
                    || !heap::with_pages(|m| Ok(space.copy_to_user(m, 0x401100, &frame[..n])))
                        .unwrap_or(false))
            {
                return Err(abi::BAD_ADDRESS);
            }
            ecrire(
                k,
                pid,
                &[
                    (5, ret.unwrap_or(0) as u64),
                    (4, ret.err().unwrap_or(abi::OK)),
                    (3, seq),
                ],
            )?;
            if case == 6 && op >= 32 && ret.is_ok() {
                k.ipc.lab_close_sender(pid, hw);
            }
            Ok(ret)
        });
        let duree = hal::timer::counter() - debut_mediation;
        profil_3b::courtier(
            op,
            duree,
            outcome.as_ref().ok().and_then(|r| r.as_ref().ok()).copied(),
        );
        mediation_ticks = mediation_ticks.saturating_add(duree);
        let res = match outcome {
            Ok(r) => r,
            Err(e) => {
                status = Err(e);
                break;
            }
        };
        if case == 1 && op == 2 && res == Err(abi::WOULD_BLOCK) {
            hal::log(format_args!(
                "ETH3 préparation supplémentaire refusée par le courtier"
            ));
        }
        if op >= 32
            && res.is_ok()
            && let Some(check) = publish.take()
        {
            if !check() {
                status = Err(abi::REFUSED);
                break;
            }
            peak = peak.min(heap::audit(false).free_pages);
            measure("eth3_publication");
        }
        if op == 4 {
            status = res.map(|_| ());
            break;
        }
        if let Err(e) = res
            && e != abi::WOULD_BLOCK
        {
            status = Err(e);
            break;
        }
    }
    CLIENT_PID.store(0, Relaxed);
    with(|s| s.ipc.lab_close_sender(pid, hw));
    let rt = operation(5, 0, &mut frame);
    // Les appels ordinaires expirent aussi si le titulaire d'un prêt disparaît.
    // Le courtier a déjà fermé le bus et tenté le reset avant cette collecte.
    let jusqua = ticks() + u64::from(HZ) * 30;
    while ticks() < jusqua
        && [&client, &service, &driver]
            .iter()
            .any(|a| a.try_result().is_none())
    {
        sleep(1);
    }
    with(|s| {
        for p in [pid, sp, cp] {
            if s.processes.get(p).outcome.is_none() {
                stop_process(s, p, Outcome::Exit(125), hal::timer::counter());
            }
        }
    });
    let outcomes = [driver.join(), service.join(), client.join()];
    #[cfg(feature = "net-q35-duo")]
    if case == 3 && outcomes == [Outcome::Exit(9), Outcome::Exit(0), Outcome::Exit(0)] {
        PILOTE_COLLECTE.store(pid.number, core::sync::atomic::Ordering::Release);
    }
    profil_3b::bilan();
    let ipc = IPC
        .each_ref()
        .map(|row| row.each_ref().map(|n| n.load(Relaxed)));
    for p in &ACTEURS {
        p.store(0, Relaxed);
    }
    let taux = crate::sched::profile::temps_bsp();
    let periodes: [u64; 3] = core::array::from_fn(|i| taux[i] - cpu[i]);
    hal::log(format_args!(
        "ETH3 IPC colonnes=send,receive,put,take,features,autres client={:?} service={:?} pilote={:?} mediation_ticks={mediation_ticks} echantillons_cpu_usr_sys_idle={periodes:?}",
        ipc[0], ipc[1], ipc[2]
    ));
    let count = COUNT.load(Relaxed) as usize;
    let samples = &SAMPLES[..count.min(N)];
    let (_, sum, qs) = distribution(samples, None);
    for b in 1..=8 {
        let (n, duree, quantiles) = distribution(samples, Some(b));
        if n != 0 {
            hal::log(format_args!(
                "ETH3 lot={b} n={n} somme_ticks={duree} quantiles_ticks={quantiles:?}"
            ));
        }
    }
    let span = LAST.load(Relaxed).saturating_sub(FIRST.load(Relaxed));
    let hz = hal::timer::counter_hz();
    hal::log(format_args!(
        "ETH3 bilan mode={case} materiel={status:?} retrait={rt:?} acteurs={outcomes:?} appels_pilote={calls} ipc_client={} lots={count} mesure_ticks={sum} serie_ticks={span} quantiles_ticks={qs:?} hz={hz} changements={} reveils={} cpu_ticks={} idle_ticks={} pages_ouverture={} pages_publication={}",
        ipc[0].iter().sum::<u64>(),
        crate::sched::profile::switches_bsp() - sw,
        with(|s| s.stats.wakes) - wakes,
        ticks() - initial,
        with(|s| s.stats.idle_ticks) - idle,
        base.saturating_sub(free_open),
        base.saturating_sub(peak)
    ));
    crate::sched::profile::ACTIVE.store(profil, Relaxed);
    measure("eth3_retrait");
    let fin = heap::audit(false).free_pages;
    hal::log(format_args!("ETH3 cadres avant={base} apres={fin}"));
    status.is_ok()
        && rt.is_ok()
        && outcomes.iter().all(|r| *r == Outcome::Exit(0))
        && (count > 0 || case == 7)
        && count <= N
        && (base == fin || stockage_partage)
        && with(|s| [pid, sp, cp].iter().all(|p| s.ipc.racines_pilotes(*p) == 0))
}
