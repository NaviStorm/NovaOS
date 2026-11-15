//! Contrôleur du seul ELF admis par le profil thread-probe. L'application
//! effectue elle-même les appels natifs et les vérifications dans CPL3/EL0.
use super::*;

fn launch(mode: u64, argument: u64) -> Child {
    let mut data = [0u8; 16];
    data[..8].copy_from_slice(&mode.to_le_bytes());
    data[8..].copy_from_slice(&argument.to_le_bytes());
    spawn::launch_thread_witness(&data).expect("chargement du témoin")
}

fn word(pid: Pid, index: usize, write: Option<u64>) -> u64 {
    with(|s| {
        let p = s.processes.get(pid);
        assert!(
            p.outcome.is_none(),
            "témoin terminé trop tôt {:?}",
            p.outcome
        );
        let space = p.space.as_ref().unwrap();
        heap::with_pages(|m| {
            if let Some(value) = write {
                space.write_data(m, index, value);
            }
            Ok(space.read_data(m, index))
        })
        .unwrap()
    })
}

fn until(mut pred: impl FnMut() -> bool) {
    let deadline = ticks() + 12 * u64::from(HZ);
    while !pred() {
        assert!(ticks() < deadline, "délai de la sonde des threads");
        sleep(1);
    }
}

fn observe_compute() {
    let child = launch(1, 0);
    let pid = child.0;
    until(|| word(pid, 4, None) > 0 && word(pid, 5, None) > 0);
    let ids = [word(pid, 2, None), word(pid, 3, None)];
    assert_ne!(ids[0], ids[1]);
    let cpus = crate::smp::count();
    let mut switches = [0; 2];
    for target in [0, cpus - 1].into_iter().take(if cpus > 1 { 2 } else { 1 }) {
        // Les deux calculateurs sans yield se disputent le même cœur. En SMP,
        // déplacer ensuite les DEUX threads (mêmes générations et même racine).
        let begin = [word(pid, 4, None), word(pid, 5, None)];
        let initial = with(|s| {
            ids.map(|id| {
                let t = s
                    .slots
                    .iter_mut()
                    .flatten()
                    .find(|t| t.owner == Some(pid) && t.id.generation == id)
                    .unwrap();
                t.cpu_mask = 1 << target;
                t.user.as_ref().unwrap().preemptions
            })
        });
        until(|| {
            let advanced = (0..2).all(|i| word(pid, 4 + i, None) > begin[i]);
            with(|s| {
                (0..2).all(|i| {
                    let t = s
                        .slots
                        .iter()
                        .flatten()
                        .find(|t| t.owner == Some(pid) && t.id.generation == ids[i])
                        .unwrap();
                    let member = t.user.as_ref().unwrap();
                    switches[i] = member.preemptions;
                    advanced
                        && t.last_cpu == target
                        && member.preempted_on & (1 << target) != 0
                        && member.preemptions >= initial[i] + 2
                })
            })
        });
    }
    // Le même ELF voit son propre DATA ; un autre processus reçoit uniquement
    // le numéro de génération et ne peut annuler ni joindre la cible vivante.
    assert_eq!(launch(4, ids[0]).join(), Outcome::Exit(0));
    assert_eq!(word(pid, 14, None), 0);
    word(pid, 1, Some(1));
    assert_eq!(child.join(), Outcome::Exit(0));
    assert_eq!(launch(4, ids[0]).join(), Outcome::Exit(0));
    hal::log(format_args!(
        "THREAD calcul cpu=0->{} preemptions={},{} migrations={} isolation=OK",
        cpus - 1,
        switches[0],
        switches[1],
        if cpus > 1 { 2 } else { 0 }
    ));
}

fn guard_page() {
    let child = launch(2, 0);
    let pid = child.0;
    until(|| word(pid, 2, None) != 0);
    let id = word(pid, 2, None);
    let guard = with(|s| {
        let t = s
            .slots
            .iter()
            .flatten()
            .find(|t| t.owner == Some(pid) && t.id.generation == id)
            .unwrap();
        t.user.as_ref().unwrap().layout.bottom - 1
    });
    word(pid, 1, Some(guard));
    let Outcome::Fault(fault) = child.join() else {
        panic!("page de garde accessible")
    };
    assert_eq!(
        (fault.address, fault.access, fault.cause),
        (guard, "ecriture", "absente")
    );
    hal::log(format_args!(
        "THREAD garde adresse={guard:#x} faute=confirmee"
    ));
}

fn wait_for_two_words(pid: Pid) {
    until(|| {
        with(|s| {
            s.slots
                .iter()
                .flatten()
                .filter(|t| {
                    t.owner == Some(pid) && matches!(t.state, State::Blocked(Wait::UserWord { .. }))
                })
                .count()
                == 2
        })
    });
}

fn parked_join(pid: Pid) -> Option<Id> {
    with(|s| {
        s.slots
            .iter()
            .flatten()
            .find_map(|thread| match thread.state {
                State::Blocked(Wait::Join(id))
                    if thread.owner == Some(pid)
                        && matches!(
                            s.thread(id.slot).state,
                            State::Blocked(Wait::UserWord { .. })
                        ) =>
                {
                    Some(id)
                }
                _ => None,
            })
    })
}

fn cancelled_join() {
    let child = launch(9, 0);
    let pid = child.0;
    until(|| parked_join(pid).is_some());
    let old = parked_join(pid).unwrap();
    word(pid, 1, Some(1));
    until(|| word(pid, 2, None) != 0);
    let replacement = word(pid, 2, None);
    assert_ne!(old.generation, replacement);
    until(|| {
        with(|s| {
            let t = s.thread(old.slot);
            assert_eq!((t.owner, t.id.generation), (Some(pid), replacement));
            matches!(t.state, State::Blocked(Wait::UserWord { .. }))
        })
    });
    word(pid, 1, Some(2));
    assert_eq!(child.join(), Outcome::Exit(0));
    hal::log(format_args!(
        "THREAD slot={} generations={},{} attente et join annules",
        old.slot, old.generation, replacement
    ));
}

fn destroy_waiters() {
    let child = launch(3, 0);
    let pid = child.0;
    wait_for_two_words(pid);
    until(|| {
        with(|s| {
            s.slots
                .iter()
                .flatten()
                .any(|t| t.owner == Some(pid) && matches!(t.state, State::Blocked(Wait::Ipc(_))))
        })
    });
    until(|| parked_join(pid).is_some());
    until(|| word(pid, 5, None) > 0);
    word(pid, 1, Some(1));
    assert_eq!(child.join(), Outcome::Exit(33));
    assert!(with(|s| s
        .slots
        .iter()
        .flatten()
        .all(|t| t.owner != Some(pid))));
    hal::log(format_args!(
        "THREAD destruction attentes=2 ipc=1 calcul=1 join=1 survivants=0"
    ));
}

fn concurrent_exits() {
    let child = launch(10, 0);
    let pid = child.0;
    until(|| {
        reap();
        word(pid, 2, None) == 2 && with(|s| s.processes.get(pid).threads == 2)
    });
    // Le principal est collecté, les deux derniers membres attendent le même feu vert.
    word(pid, 1, Some(1));
    let Outcome::Exit(result @ (71 | 73)) = child.join() else {
        panic!("résultat du dernier membre");
    };
    assert!(with(|s| s
        .slots
        .iter()
        .flatten()
        .all(|t| t.owner != Some(pid))));
    hal::log(format_args!(
        "THREAD sorties concurrentes=2 dernier={result} survivants=0"
    ));
}

fn withdraw_receive_page() {
    let child = launch(7, 0);
    let pid = child.0;
    until(|| {
        with(|s| {
            s.slots
                .iter()
                .flatten()
                .any(|t| t.owner == Some(pid) && matches!(t.state, State::Blocked(Wait::Ipc(_))))
        })
    });
    // Retrait supervisé dans la racine du témoin mono-thread endormi. La
    // transaction emprunte les mêmes opérations que le retrait normal du tas.
    with(|s| {
        let space = s.processes.get_mut(pid).space.as_mut().unwrap();
        let (base, end) = heap::with_pages(|m| {
            let end = space.resize_heap(m, 0, 0)?;
            let base = end - crate::memory::PAGE;
            assert!(space.atomic_word(m, base).is_some());
            assert_eq!(space.resize_heap(m, end, base)?, base);
            assert!(space.atomic_word(m, base).is_none());
            Ok((base, end))
        })
        .unwrap();
        threads::invalidate_range(s, pid, base, end);
    });
    assert_eq!(child.join(), Outcome::Exit(0));
}

/// Exécute l’ELF public sans protocole de test ni délégation de service.
pub(crate) fn run_example() {
    let initial = heap::audit(true);
    let slots = with(|s| s.audit());
    let app = spawn::launch_thread_witness(&[]).expect("exemple admis au catalogue");
    assert_eq!(app.join(), Outcome::Exit(0));
    assert!(with(
        |s| s.processes.slots.iter().all(Option::is_none) && s.audit() == slots
    ));
    assert_eq!(heap::audit(true), initial);
    hal::log(format_args!("SDK exemple admis sortie=0 restitution=OK"));
}

pub(crate) fn run() {
    let initial = heap::audit(true);
    let slots = with(|s| s.audit());
    let denied = spawn_image(spawn::thread_witness(), &8u64.to_le_bytes(), None, None).unwrap();
    assert_eq!(denied.join(), Outcome::Exit(0));
    assert_eq!(launch(0, 0).join(), Outcome::Exit(0));
    cancelled_join();
    withdraw_receive_page();
    let notifying = launch(5, 0);
    wait_for_two_words(notifying.0);
    word(notifying.0, 1, Some(1));
    assert_eq!(notifying.join(), Outcome::Exit(0));
    observe_compute();
    guard_page();
    destroy_waiters();
    concurrent_exits();
    let last = launch(6, 0);
    until(|| {
        reap();
        with(|s| {
            let p = s.processes.get(last.0);
            p.threads == 1
                && s.slots.iter().flatten().any(|t| {
                    t.owner == Some(last.0)
                        && t.user.as_ref().is_some_and(|member| member.secondary)
                })
        })
    });
    word(last.0, 1, Some(1));
    assert_eq!(last.join(), Outcome::Exit(19));
    hal::log(format_args!(
        "THREAD principal collecte dernier resultat=19"
    ));
    assert!(with(
        |s| s.processes.slots.iter().all(Option::is_none) && s.audit() == slots
    ));
    let current = heap::audit(true);
    assert_eq!(initial, current);
    hal::log(format_args!(
        "THREAD restitution avant={} apres={} blocs={} tcb={} OK",
        initial.free_pages, current.free_pages, current.blocks, slots
    ));
}
