//! Contrôleur de la démonstration plate. Les deux processus de calcul ne
//! cèdent jamais la main ; le noyau suit leurs compteurs par le HHDM.
use super::*;
use core::sync::atomic::AtomicBool;

static PREEMPT_REAPER: AtomicBool = AtomicBool::new(false);

// Sonde proc-test uniquement. Attendre un vrai tick dans la fenêtre IRQ ouverte
// entre deux passages d'idle. Le chemin normal conserve son masque habituel.
pub(in crate::sched) fn before_reap_probe() {
    if PREEMPT_REAPER.load(Ordering::Relaxed)
        && with(|s| s.stats.exits > s.stats.reaped)
        && PREEMPT_REAPER.swap(false, Ordering::Relaxed)
    {
        let before = ticks();
        while ticks() == before {
            core::hint::spin_loop();
        }
    }
}

pub(super) static DEMO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/demo.bin"));

// Ordre des opérations dans l’image plate indépendante de chaque cible.
#[derive(Clone, Copy)]
#[repr(u64)]
pub(super) enum Action {
    Compute,
    Read,
    Write,
    Execute,
    Privileged,
    Exit,
    Fpu,
    Owner,
    WriteReadOnly,
    UnknownEntry,
    BadStack,
}
use Action::*;

pub(super) fn spawn_demo(mode: Action, seed: u64, target: u64) -> Result<Child, Error> {
    let mut config = [0u8; 24];
    for (out, value) in config.chunks_exact_mut(8).zip([mode as u64, seed, target]) {
        out.copy_from_slice(&value.to_le_bytes());
    }
    let marker = seed.to_le_bytes();
    spawn_image(
        DEMO,
        &config,
        matches!(mode, Owner | WriteReadOnly).then_some(&marker[..]),
        None,
    )
}

pub(super) fn shortage() {
    let baseline = heap::audit(true);
    let mut held = heap::with_pages(|m| m.hold_for_probe(0)).unwrap();
    // Échecs successifs de racine, tables intermédiaires, pages privées
    // et pile superviseur partiellement construite, via le chemin normal.
    for budget in [0, 2, 5, 9, 20] {
        heap::with_pages(|m| {
            held.restore_to(m, budget);
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            spawn_demo(Exit, 17, 0),
            Err(Error::Memory(crate::memory::Error::Exhausted))
        ));
        assert_eq!(heap::audit(false).free_pages, budget);
        assert_eq!(with(|s| s.audit()), 2);
        assert!(with(|s| s.processes.slots.iter().all(Option::is_none)));
    }
    heap::with_pages(|m| {
        held.restore_to(m, baseline.free_pages);
        Ok(())
    })
    .unwrap();
    assert_eq!(heap::audit(true), baseline);
    hal::log(format_args!("PROC penuries 0,2,5,9,20 cadres, rollback OK"));
}

fn observe(child: &Child, release: bool) -> Option<(u64, u64)> {
    with(|s| {
        let p = s.processes.get(child.0);
        if p.outcome.is_some() {
            return None;
        }
        let space = p.space.as_ref().unwrap();
        let progress = heap::with_pages(|m| {
            if release {
                space.write_data(m, 4, 1);
            }
            Ok(space.read_data(m, 3))
        })
        .unwrap();
        Some((p.preemptions, progress))
    })
}

// Seul le banc de validation interprète un résultat utilisateur inattendu
// comme un échec de test. Le démarrage normal doit toujours rendre le clavier.
pub(crate) fn boot_demo(inject_fault: bool) {
    let a = match spawn_demo(if inject_fault { Privileged } else { Owner }, 41, 0) {
        Ok(child) => child,
        Err(e) => {
            hal::log(format_args!("Démo indisponible : {e:?}"));
            return;
        }
    };
    let b = match spawn_demo(Compute, 82, 0) {
        Ok(child) => child,
        Err(e) => {
            observe(&a, true);
            hal::log(format_args!("Second processus indisponible : {e:?}"));
            return;
        }
    };
    let deadline = ticks() + u64::from(HZ);
    while ticks() < deadline
        && [&a, &b]
            .into_iter()
            .any(|child| observe(child, false).is_some_and(|(n, _)| n < 8))
    {
        sleep(1);
    }
    for child in [&a, &b] {
        observe(child, true);
    }
    let deadline = ticks() + u64::from(HZ);
    while ticks() < deadline && (a.try_result().is_none() || b.try_result().is_none()) {
        sleep(1);
    }
    if a.try_result().is_some() && b.try_result().is_some() {
        hal::log(format_args!("PROC demonstration terminee"));
    } else {
        hal::log(format_args!("PROC delai de demonstration atteint"));
    }
}

pub(crate) fn test() {
    let baseline = heap::audit(true);
    shortage();
    let a = spawn_demo(Owner, 41, 0).unwrap();
    let b = spawn_demo(Compute, 82, 0).unwrap();
    let start = ticks();
    while [&a, &b]
        .into_iter()
        .any(|child| observe(child, false).expect("fin prématurée").0 < 8)
    {
        assert!(ticks() - start < u64::from(HZ) * 5);
        sleep(1);
    }
    let before = observe(&a, false).unwrap().1;
    let stack_fault = if cfg!(target_arch = "x86_64") {
        ("retour", "pile")
    } else {
        ("ecriture", "absente")
    };
    for (mode, addr, access, cause) in [
        (Read, user::PRIVATE, "lecture", "absente"),
        (Read, 0xffffffff80000000, "lecture", "protection"),
        (WriteReadOnly, user::PRIVATE, "ecriture", "protection"),
        (Write, user::CODE, "ecriture", "protection"),
        (Execute, user::DATA, "execution", "protection"),
        (Write, user::STACK - 8, "ecriture", "absente"),
        (Write, user::TOP, "ecriture", "absente"),
        (UnknownEntry, 0, "instruction", "invalide"),
        (
            BadStack,
            0x0000_8000_0000_0000,
            stack_fault.0,
            stack_fault.1,
        ),
        (Privileged, 0, "instruction", "privilege"),
        (Fpu, 0, "instruction", "fpu"),
    ] {
        let victim = spawn_demo(mode, 99, addr).unwrap();
        let Outcome::Fault(f) = victim.join() else {
            panic!("violation ignorée");
        };
        assert_eq!((f.access, f.cause), (access, cause));
        if addr != 0 {
            assert_eq!(f.address, addr);
        }
    }
    // Aucun join, reap ou spawn pendant cette attente. Les deux processus
    // de calcul restent toujours prêts et interdiraient une veille naturelle.
    PREEMPT_REAPER.store(true, Ordering::Relaxed);
    let orphan = spawn_demo(Exit, 23, 0).unwrap();
    let pid = orphan.0;
    drop(orphan);
    let deadline = ticks() + u64::from(HZ);
    while with(|s| {
        let Some(p) = &s.processes.slots[pid.slot] else {
            return false;
        };
        assert_eq!(p.pid, pid);
        assert!(
            ticks() < deadline,
            "orphelin non collecté pid={} statut={:?} observe={} fin={} collecte={} preemption_idle={} file={}",
            pid.number,
            p.outcome,
            p.observed,
            s.stats.exits,
            s.stats.reaped,
            s.stats.idle_ticks,
            s.ready.len
        );
        true
    }) {
        sleep(1);
    }
    assert!(!PREEMPT_REAPER.load(Ordering::Relaxed));
    hal::log(format_args!("PROC orphelin collecte sous charge OK"));

    // Laisser s’exécuter les deux survivants après la dernière faute.
    sleep(6);
    let (pa, va) = observe(&a, true).unwrap();
    let (pb, vb) = observe(&b, true).unwrap();
    assert!(va > before && vb > 0);
    let mut last = b.0.number;
    assert_eq!(a.join(), Outcome::Exit(41));
    assert_eq!(b.join(), Outcome::Exit(82));
    hal::log(format_args!(
        "PROC concurrence preemptions={pa},{pb} progressions={va},{vb} registres OK"
    ));
    for _ in 0..64 {
        let pid = spawn_demo(Exit, 17, 0).unwrap();
        assert!(pid.0.number > last);
        last = pid.0.number;
        assert_eq!(pid.join(), Outcome::Exit(17));
    }
    hal::log(format_args!("PROC 64 creations et collectes OK"));
    let end = heap::audit(true);
    assert_eq!(baseline, end);
    assert!(with(|s| s.processes.slots.iter().all(Option::is_none)));
    hal::log(format_args!(
        "PROC ressources avant={} apres={} blocs={} restitution OK",
        baseline.free_pages, end.free_pages, end.blocks
    ));
}
