//! Laboratoire privé. Chaque couple possède deux canaux et deux processus réels.
use super::*;
use crate::{hal::timer, smp};
use core::sync::atomic::{AtomicBool, AtomicU64};

const TOURS: u64 = 1000;
static FIN: AtomicBool = AtomicBool::new(false);
static TRAVAIL: AtomicU64 = AtomicU64::new(0);

use core::sync::atomic::AtomicUsize;
static MAPPAGES: AtomicU64 = AtomicU64::new(0);
static RETRAITS: AtomicU64 = AtomicU64::new(0);
static PRETS: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
static DELAI_PRET: AtomicU64 = AtomicU64::new(0);
static REPRISES: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn page_attente(ajout: bool) {
    if crate::sched::profile::ACTIVE.load(Ordering::Relaxed) {
        (if ajout { &MAPPAGES } else { &RETRAITS }).fetch_add(1, Ordering::Relaxed);
    }
}

// Appel sous la garde de l’ordonnanceur. Le premier dispatch consomme la marque.
pub(crate) fn pret(slot: usize) {
    PRETS[slot].store(crate::sched::profile::stamp(), Ordering::Relaxed);
}
pub(crate) fn elu(slot: usize) {
    let debut = PRETS[slot].swap(0, Ordering::Relaxed);
    if debut != 0 {
        DELAI_PRET.fetch_add(timer::counter().saturating_sub(debut), Ordering::Relaxed);
        REPRISES.fetch_add(1, Ordering::Relaxed);
    }
}
fn calcul() {
    let mut x = 1u64;
    while !FIN.load(Ordering::Acquire) {
        for _ in 0..4096 {
            x = core::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
        }
        TRAVAIL.fetch_add(1, Ordering::Relaxed);
    }
}

fn memoire() {
    while !FIN.load(Ordering::Acquire) {
        let mut page = alloc::vec![0u8; 4096];
        for octet in page.iter_mut().step_by(64) {
            *octet = core::hint::black_box(23);
        }
        core::hint::black_box(&page);
        drop(page);
        TRAVAIL.fetch_add(1, Ordering::Relaxed);
    }
}

fn vfs() {
    while !FIN.load(Ordering::Acquire) {
        assert!(
            crate::sched::process::storage::volume(26),
            "session P9 de lecture"
        );
        TRAVAIL.fetch_add(1, Ordering::Release);
    }
}

struct Couple {
    enfants: [Child; 2],
    acteurs: [Actor; 2],
}

impl Couple {
    fn preparer(masques: [u64; 2]) -> Self {
        let (enfants, acteurs) = group([ALL, ALL]);
        let [a, b] = acteurs;
        let root = a.boot();
        let reply = a.call(IPC_CREATE, 0, 0, 0).unwrap();
        let peer = b.boot();
        let tx = contract::transfer(a, b, root, peer, reply, SEND | GRANT, 1);
        for ((acteur, (envoi, reception)), mask) in acteurs
            .into_iter()
            .zip([(root, reply), (tx, peer)])
            .zip(masques)
        {
            acteur.put(OUTPUT, &receive_wire(15000, true));
            acteur.put(DESC, &send_wire(19, 128, 0));
            perf::word(acteur, 6, envoi);
            perf::word(acteur, 7, reception);
            with(|s| {
                s.slots
                    .iter_mut()
                    .flatten()
                    .find(|t| t.owner == Some(acteur.0))
                    .unwrap()
                    .cpu_mask = mask
            });
        }
        Self { enfants, acteurs }
    }
}

pub(super) fn run(scenario: &[u8]) -> ! {
    let n = smp::count();
    let hz = timer::counter_hz();
    let lib = (1u64 << n) - 1;
    let sonde = scenario == b"ipc-smp-sonde";
    let paires = if scenario == b"ipc-smp-paires" { n } else { 1 };
    let charge: Option<fn()> = match scenario {
        b"ipc-smp-calcul" => Some(calcul),
        b"ipc-smp-memoire" => Some(memoire),
        b"ipc-smp-vfs" => Some(vfs),
        _ => None,
    };
    QUIET.store(true, Ordering::Relaxed);
    let ns = |d: u64| (u128::from(d) * 1_000_000_000 / u128::from(hz)) as u64;
    hal::log(format_args!(
        "P3 configuration hz={hz} cpu={n} couples={paires} tours={TOURS}"
    ));
    for serie in 0..=7 {
        let mut couples = Vec::new();
        for i in 0..paires {
            let masks = match scenario {
                b"ipc-smp-local" => [1, 1],
                b"ipc-smp-distant" => {
                    assert!(n >= 2);
                    [1, 2]
                }
                b"ipc-smp-paires" => [1 << i, 1 << i],
                _ => [lib, lib],
            };
            couples.push(Couple::preparer(masks));
        }
        FIN.store(false, Ordering::Release);
        TRAVAIL.store(0, Ordering::Relaxed);
        let travailleur =
            charge.map(|f| spawn_on(f, if scenario == b"ipc-smp-vfs" { 1 } else { lib }).unwrap());
        if travailleur.is_some() {
            while TRAVAIL.load(Ordering::Acquire) == 0 {
                sleep(1);
            }
        }
        // Un cycle inclut le lancement des services. Commencer le chronométrage
        // pendant les lectures du montage suivant, après deux soumissions réelles.
        if scenario == b"ipc-smp-vfs" {
            let precedent = crate::hardware::block::kicks();
            while crate::hardware::block::kicks() - precedent < 2 {
                sleep(1);
            }
        }
        let travail_avant = TRAVAIL.load(Ordering::Acquire);
        let bloc_avant = crate::hardware::block::kicks();
        hal::log(format_args!("P3 debut serie={serie}"));
        let start;
        {
            let _g = Guard::new();
            if sonde {
                for v in [&MAPPAGES, &RETRAITS, &DELAI_PRET] {
                    v.store(0, Ordering::Relaxed);
                }
                REPRISES.store(0, Ordering::Relaxed);
                for marque in &PRETS {
                    marque.store(0, Ordering::Relaxed);
                }
                let [a, b] = couples[0].acteurs;
                with(|s| s.ipc.metrics.start(a.0.number, b.0.number));
                crate::sched::profile::ACTIVE.store(true, Ordering::Release);
            }
            start = timer::counter();
            for c in &couples {
                let [a, b] = c.acteurs;
                b.start(u64::MAX - 1, TOURS, 2, 1);
                a.start(u64::MAX - 4, TOURS, 0, 0);
            }
        }
        let mut fins = Vec::new();
        for c in &couples {
            let client = &c.enfants[0];
            loop {
                let _g = Guard::new();
                let (res, fin) = with(|s| {
                    let p = s.processes.get(client.0);
                    (p.outcome, p.exit_counter)
                });
                if let Some(res) = res {
                    assert_eq!(res, Outcome::Exit(0));
                    fins.push(ns(fin - start));
                    break;
                }
                dispatch(Reason::Block(Wait::Process(client.0)));
            }
        }
        let travail = TRAVAIL.load(Ordering::Acquire) - travail_avant;
        let requetes_bloc = crate::hardware::block::kicks() - bloc_avant;
        FIN.store(true, Ordering::Release);
        let duree = fins.iter().max().unwrap();
        hal::log(format_args!(
            "P3 fin serie={serie} ns={duree} activite={travail} requetes_bloc={requetes_bloc}"
        ));
        for (id, fin) in fins.iter().enumerate() {
            hal::log(format_args!("P3 couple id={id} ns={fin}"));
        }
        if sonde {
            crate::sched::profile::report();
            hal::log(format_args!(
                "P3 detail pages_attente={} retraits={} pret_ns={} reprises={}",
                MAPPAGES.load(Ordering::Relaxed),
                RETRAITS.load(Ordering::Relaxed),
                ns(DELAI_PRET.load(Ordering::Relaxed)),
                REPRISES.load(Ordering::Relaxed)
            ));
            with(|s| s.ipc.metrics.report(hz));
        }
        for c in couples {
            let [client, serveur] = c.enfants;
            let b = c.acteurs[1];
            assert_eq!(b.result(), Ok(TOURS));
            b.start(crate::abi::PROCESS_EXIT, 0, 0, 0);
            assert_eq!(client.join(), Outcome::Exit(0));
            assert_eq!(serveur.join(), Outcome::Exit(0));
        }
        if let Some(id) = travailleur {
            while live(id) {
                sleep(1);
            }
        }
        reap();
    }
    hal::log(format_args!("P3 validation OK"));
    hal::cpu::halt()
}
