//! Tests de politique sans périphérique. Les essais d'IPC, de VM et de
//! restitution des ressources se trouvent dans le banc invité de phase 3.
#[path = "../user/services/policy.rs"]
mod policy;
use policy::*;

const UNIT: Declaration = Declaration {
    version: VERSION,
    id: 1,
    image: 1,
    args: &[],
    required: &[],
    optional: &[],
    caps: 1,
    auto: false,
    restart: Restart::Never,
    start_ticks: 10,
    stop_ticks: 2,
    heartbeat_ticks: 0,
    retries: 2,
    window: 20,
    backoff: 2,
    log: true,
};

fn launch(p: &mut Policy<'_>, now: u64, expected: u64) -> u64 {
    match p.next_action(now) {
        Some(Action::Launch { id, attempt }) if id == expected => attempt,
        action => panic!("lancement {expected} attendu, obtenu {action:?}"),
    }
}

fn activate(p: &mut Policy<'_>, id: u64, now: u64) -> u64 {
    let n = launch(p, now, id);
    p.ready(id, n, now).unwrap();
    n
}

#[test]
fn admission_et_validation_atomique() {
    for bad in [
        Declaration { version: 2, ..UNIT },
        Declaration { id: 0, ..UNIT },
        Declaration { image: 0, ..UNIT },
        Declaration {
            args: &[1, 2, 3, 4, 5],
            ..UNIT
        },
        Declaration {
            start_ticks: 0,
            ..UNIT
        },
        Declaration {
            stop_ticks: 0,
            ..UNIT
        },
        Declaration { retries: 9, ..UNIT },
        Declaration {
            restart: Restart::Always,
            backoff: 0,
            ..UNIT
        },
        Declaration {
            restart: Restart::Failure,
            window: 0,
            ..UNIT
        },
    ] {
        assert!(matches!(
            Policy::new(&[bad]),
            Err(Error::InvalidDeclaration)
        ));
    }
    assert!(matches!(
        Policy::new(&[UNIT; MAX_SERVICES + 1]),
        Err(Error::InvalidDeclaration)
    ));
    assert!(matches!(Policy::new(&[UNIT, UNIT]), Err(Error::Duplicate)));
    let list = [UNIT];
    let mut p = Policy::new(&list).unwrap();
    assert_eq!(p.start(1), Err(Error::Unregistered));
    assert_eq!(p.register(2), Err(Error::Unknown));
    assert_eq!(p.register(1), Ok(()));
    assert_eq!(p.state(1), Ok(State::Registered));
    assert_eq!(p.register(1), Ok(()));
    assert_eq!(p.next_action(0), None);
}

#[test]
fn cycles_absences_et_doublons() {
    let ab = Declaration {
        required: &[2],
        ..UNIT
    };
    for d in [
        ab,
        Declaration {
            optional: &[2],
            ..UNIT
        },
    ] {
        assert!(matches!(Policy::new(&[d]), Err(Error::MissingDependency)));
    }
    assert!(matches!(
        Policy::new(&[
            ab,
            Declaration {
                id: 2,
                optional: &[1],
                ..UNIT
            }
        ]),
        Err(Error::Cycle)
    ));
    assert!(matches!(
        Policy::new(&[Declaration {
            optional: &[1],
            ..UNIT
        }]),
        Err(Error::Cycle)
    ));
    assert!(matches!(
        Policy::new(&[
            Declaration {
                optional: &[2],
                ..ab
            },
            Declaration { id: 2, ..UNIT }
        ]),
        Err(Error::Duplicate)
    ));
}

#[test]
fn graphe_non_trie_et_lancements_independants() {
    let d = [
        Declaration {
            id: 3,
            required: &[2, 1],
            auto: true,
            ..UNIT
        },
        UNIT,
        Declaration { id: 2, ..UNIT },
    ];
    let mut p = Policy::new(&d).unwrap();
    let a = launch(&mut p, 0, 1);
    let b = launch(&mut p, 0, 2);
    assert_eq!(p.next_action(0), None);
    p.ready(2, b, 1).unwrap();
    assert_eq!(p.next_action(1), None);
    assert_eq!(p.state(3), Ok(State::Waiting));
    p.ready(1, a, 2).unwrap();
    let c = activate(&mut p, 3, 2);
    assert_eq!(p.next_action(2), None);
    p.shutdown();
    assert_eq!(p.next_action(2), Some(Action::Stop { id: 3, attempt: c }));
    assert_eq!(p.next_action(2), None);
    p.finished(3, c, Cause::Exited, 0, 2).unwrap();
    assert_eq!(p.next_action(2), Some(Action::Stop { id: 2, attempt: b }));
    assert_eq!(p.next_action(2), Some(Action::Stop { id: 1, attempt: a }));
    assert_eq!(p.next_action(2), None);
    p.finished(2, b, Cause::Exited, 0, 3).unwrap();
    p.finished(1, a, Cause::Exited, 0, 3).unwrap();
    for id in 1..=3 {
        assert_eq!(p.state(id), Ok(State::Stopped));
    }
}

#[test]
fn ancien_ready_pendant_nouvelle_incarnation_refuse() {
    let d = [
        Declaration {
            restart: Restart::Failure,
            ..UNIT
        },
        Declaration {
            id: 2,
            required: &[1],
            auto: true,
            ..UNIT
        },
    ];
    let mut p = Policy::new(&d).unwrap();
    let old = launch(&mut p, 0, 1);
    p.finished(1, old, Cause::Crash, 125, 1).unwrap();
    assert_eq!(p.state(1), Ok(State::Restarting));
    assert_eq!(p.next_action(2), None);
    let new = launch(&mut p, 3, 1);
    assert_ne!(old, new);
    assert_eq!(p.ready(1, old, 4), Err(Error::Stale));
    assert_eq!(p.finished(1, old, Cause::Exited, 0, 4), Err(Error::Stale));
    assert_eq!(p.heartbeat(1, old, 4), Err(Error::Stale));
    assert_eq!(p.fault(1, old, Cause::Communication, 4), Err(Error::Stale));
    assert_eq!(p.next_action(4), None);
    assert_eq!(p.state(2), Ok(State::Waiting));
    p.ready(1, new, 5).unwrap();
    assert_eq!(p.state(1), Ok(State::Ready));
    assert_eq!(p.ready(1, new, 5), Err(Error::InvalidState));
    launch(&mut p, 5, 2);
    assert_eq!(p.attempt(1), Ok(new));
    assert_eq!(p.start(1), Ok(()));
    assert_eq!(p.next_action(5), None);
}

#[test]
fn perte_dependance_arret_avant_redemarrage_et_optionnelle() {
    let d = [
        Declaration {
            auto: true,
            restart: Restart::Always,
            ..UNIT
        },
        Declaration {
            id: 2,
            required: &[1],
            auto: true,
            ..UNIT
        },
        Declaration {
            id: 3,
            optional: &[1],
            auto: true,
            ..UNIT
        },
    ];
    let mut p = Policy::new(&d).unwrap();
    let a = activate(&mut p, 1, 0);
    let b = activate(&mut p, 2, 0);
    activate(&mut p, 3, 0);
    p.next_action(0);
    p.fault(1, a, Cause::Capability, 1).unwrap();
    assert_eq!(p.next_action(1), Some(Action::Stop { id: 2, attempt: b }));
    assert_eq!(p.next_action(1), None);
    assert_eq!(p.state(3), Ok(State::Running));
    p.finished(2, b, Cause::Exited, 0, 2).unwrap();
    assert_eq!(p.next_action(2), Some(Action::Stop { id: 1, attempt: a }));
    assert_eq!(p.next_action(3), None);
    assert_eq!(p.next_action(4), Some(Action::Kill { id: 1, attempt: a }));
    assert_eq!(p.next_action(44), None);
    assert_eq!(p.state(2), Ok(State::Waiting));
    p.finished(1, a, Cause::Killed, 125, 44).unwrap();
    let next = activate(&mut p, 1, 46);
    assert_ne!(a, next);
    assert_ne!(b, launch(&mut p, 46, 2));
}

#[test]
fn limite_glissante_backoff_et_relance_manuelle() {
    let d = [Declaration {
        auto: true,
        restart: Restart::Failure,
        ..UNIT
    }];
    let mut p = Policy::new(&d).unwrap();
    let a = launch(&mut p, 0, 1);
    p.finished(1, a, Cause::LaunchError, 8, 0).unwrap();
    assert_eq!(p.next_action(1), None);
    let b = launch(&mut p, 2, 1);
    p.finished(1, b, Cause::Crash, 1, 3).unwrap();
    assert_eq!(p.next_action(6), None);
    let c = launch(&mut p, 7, 1);
    p.finished(1, c, Cause::Crash, 1, 8).unwrap();
    assert_eq!(p.state(1), Ok(State::Failed));
    assert_eq!(p.next_action(20), None);
    p.restart(1).unwrap();
    let e = launch(&mut p, 20, 1);
    p.finished(1, e, Cause::Crash, 1, 20).unwrap();
    // L'échec à 3 demeure dans la fenêtre, celui de 0 en est sorti.
    assert_eq!(p.next_action(23), None);
    let f = launch(&mut p, 24, 1);
    p.finished(1, f, Cause::Exited, 0, 24).unwrap();
    assert_eq!(p.state(1), Ok(State::Stopped));
    assert_eq!(p.next_action(30), None);
}

#[test]
fn expiration_avant_ready_et_perte_du_canal() {
    let d = [Declaration {
        auto: true,
        heartbeat_ticks: 3,
        ..UNIT
    }];
    let mut p = Policy::new(&d).unwrap();
    let a = launch(&mut p, 0, 1);
    assert_eq!(p.ready(1, a, 10), Err(Error::InvalidState));
    assert_eq!(p.next_action(10), Some(Action::Stop { id: 1, attempt: a }));
    assert_eq!(p.next_action(12), Some(Action::Kill { id: 1, attempt: a }));
    p.finished(1, a, Cause::Killed, 125, 13).unwrap();
    assert_eq!(p.state(1), Ok(State::Failed));
    p.restart(1).unwrap();
    let b = activate(&mut p, 1, 15);
    p.heartbeat(1, b, 17).unwrap();
    assert_eq!(p.next_action(19), None);
    assert_eq!(p.heartbeat(1, b, 20), Err(Error::InvalidState));
    assert_eq!(p.next_action(20), Some(Action::Stop { id: 1, attempt: b }));
}

#[test]
fn arret_volontaire_prime_sur_toujours_et_sur_la_relance() {
    let d = [Declaration {
        auto: true,
        restart: Restart::Always,
        ..UNIT
    }];
    let mut p = Policy::new(&d).unwrap();
    let a = activate(&mut p, 1, 0);
    p.restart(1).unwrap();
    assert_eq!(p.next_action(1), Some(Action::Stop { id: 1, attempt: a }));
    p.stop(1).unwrap();
    p.finished(1, a, Cause::Exited, 0, 2).unwrap();
    assert_eq!(p.next_action(20), None);
    assert_eq!(p.state(1), Ok(State::Stopped));
}

#[test]
fn journal_borne_et_diagnostic_des_transitions_invalides() {
    let d = [Declaration { auto: true, ..UNIT }];
    let mut p = Policy::new(&d).unwrap();
    let a = activate(&mut p, 1, 0);
    for _ in 0..24 {
        assert_eq!(p.ready(1, a, 1), Err(Error::InvalidState));
    }
    assert!(p.dropped_events() > 0);
    let mut n = 0;
    while let Some(event) = p.pop_event() {
        assert_eq!(event.id, 1);
        assert_eq!(event.cause, Cause::Invalid);
        assert_eq!(event.attempt, a);
        assert_eq!((event.from, event.to), (State::Ready, State::Ready));
        n += 1;
    }
    assert_eq!(n, 16);
    println!(
        "Mémoire fixe du moteur de politique pour {MAX_SERVICES} entrées : {} octets",
        core::mem::size_of::<Policy<'_>>()
    );
}

#[test]
fn graphe_entier_jusqua_douze_services() {
    let mut d = [UNIT; MAX_SERVICES];
    for (i, entry) in d.iter_mut().enumerate() {
        entry.id = 1 + i as u64;
    }
    d[0].required = &[12];
    d[0].auto = true;
    let mut p = Policy::new(&d).unwrap();
    let last = launch(&mut p, 0, 12);
    assert_eq!(p.next_action(0), None);
    assert_eq!(p.state(1), Ok(State::Waiting));
    assert_eq!(p.state(11), Err(Error::Unregistered));
    p.ready(12, last, 1).unwrap();
    launch(&mut p, 1, 1);
}

#[test]
fn defaillance_terminale_propagee_a_la_chaine_obligatoire() {
    let d = [
        UNIT,
        Declaration {
            id: 2,
            required: &[1],
            ..UNIT
        },
        Declaration {
            id: 3,
            required: &[2],
            auto: true,
            ..UNIT
        },
    ];
    let mut p = Policy::new(&d).unwrap();
    let one = launch(&mut p, 0, 1);
    p.finished(1, one, Cause::LaunchError, 17, 1).unwrap();
    assert_eq!(p.next_action(1), None);
    for id in 1..=3 {
        assert_eq!(p.state(id), Ok(State::Failed));
    }
    let events: Vec<_> = core::iter::from_fn(|| p.pop_event()).collect();
    assert!(
        events
            .iter()
            .any(|e| (e.id, e.dependency, e.cause) == (2, 1, Cause::Dependency))
    );
    assert!(
        events
            .iter()
            .any(|e| (e.id, e.dependency, e.cause) == (3, 2, Cause::Dependency))
    );
    p.restart(3).unwrap();
    assert_ne!(launch(&mut p, 2, 1), one);
}

#[path = "../user/services/catalog.rs"]
mod catalog;

#[test]
fn profils_embarques_et_plafonds_delegues() {
    let ordinary: Vec<u64> = catalog::INSTALLED.iter().map(|d| d.id).collect();
    let mut installed = Policy::new(catalog::INSTALLED).unwrap();
    let mut qualification = Policy::new(catalog::QUALIFICATION).unwrap();
    for d in catalog::QUALIFICATION {
        assert_eq!(d.caps, catalog::PRIVATE_TMP);
        assert!(matches!(
            d.image,
            catalog::SYSTEM_IMAGE | catalog::STORAGE_IMAGE
        ));
        assert!(d.args.len() <= 1);
        if !ordinary.contains(&d.id) {
            assert_eq!(installed.register(d.id), Err(Error::Unknown));
            assert_eq!(qualification.register(d.id), Ok(()));
        }
    }
    assert!(ordinary.contains(&9));
}
