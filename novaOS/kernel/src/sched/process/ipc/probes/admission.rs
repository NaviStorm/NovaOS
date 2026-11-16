//! Refus après publication du producteur. Le contrôleur lit la progression
//! du calcul utilisateur et le parent collecte par le véritable PROCESS_WAIT.
use super::super::pipe_admission;
use super::*;
use pipe_admission::ReaderOutcome;

fn compute(cpu: usize) -> Res<Child> {
    let child = create_image(demo::DEMO, &[0; 24], None, None).map_err(launch_error)?;
    with(|s| {
        s.slots
            .iter_mut()
            .flatten()
            .find(|t| t.owner == Some(child.0))
            .unwrap()
            .cpu_mask = 1 << cpu;
    });
    Ok(child)
}

fn progress(pid: Pid) -> u64 {
    Actor(pid).memory(|space, m| space.read_data(m, 3))
}

fn release(pid: Pid) {
    Actor(pid).memory(|space, m| space.write_data(m, 4, 1));
}

fn collect(parent: Actor, key: u64) -> u64 {
    let size = WAIT_SIZE as u64;
    assert_eq!(parent.call(PROCESS_WAIT, key, OUTPUT, size), Ok(size));
    let info = parent.bytes::<WAIT_SIZE>(OUTPUT);
    assert_eq!(u64_at(&info, 8), key & !(1 << 63));
    assert_eq!(u64_at(&info, 16), 0);
    assert_eq!(parent.call(PROCESS_WAIT, key, OUTPUT, size), Err(DENIED));
    u64_at(&info, 24)
}

fn blocked_writer(parent: Actor) {
    let control = with(|s| s.ipc.pipe_open(parent.0, false)).unwrap();
    let (wait, admission) = pipe_admission::start_writer(parent.0, control, || {
        create_image(IMAGE, &[], None, None).map_err(launch_error)
    })
    .unwrap();
    let writer = Actor(with(|s| {
        s.processes.get(parent.0).wait_right.unwrap().target
    }));
    let tx = writer.memory(|space, m| space.read_data(m, 1));
    for _ in 0..8 {
        writer.send(tx, 0, 128, 0);
        assert_eq!(writer.result(), Ok(128));
    }
    writer.send(tx, 0, 128, 15000);
    writer.blocked();
    let start = ticks();
    assert_eq!(
        with(|s| pipe_admission::acknowledge(s, parent.0, control)),
        Err(WOULD_BLOCK)
    );
    let refusal = admission.admit(|| Err(Error::Full)).unwrap_err();
    assert_eq!(
        refusal,
        pipe_admission::ReaderFailure {
            status: LIMIT,
            forced: true
        }
    );
    assert_eq!(
        with(|s| pipe_admission::query(s, parent.0, control)),
        Ok(ReaderOutcome::Rejected(refusal))
    );
    with(|s| pipe_admission::acknowledge(s, parent.0, control)).unwrap();
    assert_eq!(collect(parent, wait), 125);
    assert!(ticks() - start < u64::from(HZ));
    hal::log(format_args!(
        "PIPE admission saturation offres=8 attente=1 collecte=125 OK"
    ));
}

fn natural_125(parent: Actor) {
    let control = with(|s| s.ipc.pipe_open(parent.0, false)).unwrap();
    let (wait, admission) = pipe_admission::start_writer(parent.0, control, || {
        create_image(IMAGE, &[], None, None).map_err(launch_error)
    })
    .unwrap();
    let pid = with(|s| s.processes.get(parent.0).wait_right.unwrap().target);
    Actor(pid).start(PROCESS_EXIT, 125, 0, 0);
    assert_eq!(collect(parent, wait), 125);
    // Le parent a déjà consommé WAIT. Un nouvel acteur peut occuper le même
    // slot sans hériter de l'autorité de retrait attachée à l'ancien PID.
    let (independent, [later]) = group([ALL]);
    assert_eq!(later.0.slot, pid.slot);
    assert_ne!(later.0.number, pid.number);
    let failure = admission.admit(|| Err(Error::Full)).unwrap_err();
    assert_eq!(
        failure,
        pipe_admission::ReaderFailure {
            status: LIMIT,
            forced: false
        }
    );
    assert_eq!(later.call(PROCESS_ID, 0, 0, 0), Ok(later.0.number));
    assert_eq!(
        with(|s| pipe_admission::query(s, later.0, control)),
        Err(DENIED)
    );
    assert_eq!(
        with(|s| pipe_admission::query(s, parent.0, control)),
        Ok(ReaderOutcome::Rejected(failure))
    );
    with(|s| pipe_admission::acknowledge(s, parent.0, control)).unwrap();
    stop(independent, [later]);
    hal::log(format_args!(
        "PIPE admission sortie_normale=125 force=0 slot_recycle=protege OK"
    ));
}

fn lost_parent() {
    let (parents, [parent]) = group([ALL]);
    let control = with(|s| s.ipc.pipe_open(parent.0, false)).unwrap();
    let (_, admission) = pipe_admission::start_writer(parent.0, control, || compute(0)).unwrap();
    let pid = with(|s| s.processes.get(parent.0).wait_right.unwrap().target);
    until(|| progress(pid) > 0);
    stop(parents, [parent]);
    until(|| with(|s| s.processes.slots[pid.slot].is_none()));
    // Une réponse retardée ne doit même pas construire un lecteur après
    // la disparition du parent et le recyclage des slots.
    let (later, [independent]) = group([ALL]);
    let failure = admission.admit(|| panic!("admission périmée")).unwrap_err();
    assert_eq!(
        failure,
        pipe_admission::ReaderFailure {
            status: REVOKED,
            forced: false
        }
    );
    assert_eq!(
        independent.call(PROCESS_ID, 0, 0, 0),
        Ok(independent.0.number)
    );
    stop(later, [independent]);
    hal::log(format_args!(
        "PIPE admission parent_disparu orphelins=0 reponse_tardive=refusee OK"
    ));
}

pub(super) fn run() {
    let baseline = heap::audit(false);
    let (parents, [parent]) = group([ALL]);
    let last = crate::smp::count() - 1;
    hal::log(format_args!(
        "PIPE admission vcpu={} cpu_producteur={last}",
        crate::smp::count()
    ));
    let witness = demo::spawn_pinned(last, 41);
    until(|| progress(witness.0) > 0);

    for case in [
        "refus-elf",
        "sortie-normale",
        "course",
        "retrait-canal",
        "admis",
    ] {
        let control = with(|s| s.ipc.pipe_open(parent.0, false)).unwrap();
        let (wait, admission) =
            pipe_admission::start_writer(parent.0, control, || compute(last)).unwrap();
        let pid = with(|s| s.processes.get(parent.0).wait_right.unwrap().target);
        until(|| progress(pid) > 0);
        let ordinary = Actor(pid).memory(|space, m| space.read_data(m, 1));
        if matches!(case, "course" | "sortie-normale") {
            release(pid);
        }
        if case == "sortie-normale" {
            until(|| with(|s| s.processes.get(pid).outcome.is_some()));
        }
        let start = ticks();
        assert_eq!(
            with(|s| pipe_admission::query(s, parent.0, control)),
            Ok(ReaderOutcome::Pending)
        );
        let forced = if case == "admis" {
            let read = admission
                .admit(|| create_image(IMAGE, &[], None, None))
                .unwrap();
            assert!(with(|s| s.processes.get(parent.0).pending_reader.is_none()));
            let reader = with(|s| s.processes.get(parent.0).pipe_reader_wait.unwrap().target);
            // Simuler la perte de réponse en retrouvant WAIT depuis la décision.
            let waited = match with(|s| pipe_admission::query(s, parent.0, control)).unwrap() {
                ReaderOutcome::Published { pid, wait: Some(k) } if pid == reader.number => k,
                other => panic!("issue de publication absente {other:?}"),
            };
            assert_eq!(read, waited);
            Actor(reader).start(PROCESS_EXIT, 0, 0, 0);
            assert_eq!(collect(parent, waited), 0);
            assert_eq!(
                with(|s| pipe_admission::query(s, parent.0, control)),
                Ok(ReaderOutcome::Published {
                    pid: reader.number,
                    wait: None
                })
            );
            parent.call(IPC_CLOSE, control, 0, 0).unwrap();
            release(pid);
            false
        } else {
            if case == "retrait-canal" {
                parent.call(IPC_REVOKE, control, 0, 0).unwrap();
                let before = progress(pid);
                until(|| progress(pid) > before);
            }
            // L’échec est produit dans le chargeur, avant publication du lecteur.
            let failure = admission
                .admit(|| {
                    assert_ne!(case, "retrait-canal");
                    let seal = storage::seal_fixture(b"\x7fELF");
                    let space = heap::with_pages(|m| UserSpace::installed(m, &seal, &[]))
                        .map_err(Error::Memory)?;
                    publish_image(space, None, iw::SAMPLE, None)
                })
                .unwrap_err();
            assert_eq!(
                failure.status,
                if case == "retrait-canal" {
                    REVOKED
                } else {
                    REFUSED
                }
            );
            assert!(with(|s| s
                .processes
                .get(parent.0)
                .pipe_reader_wait
                .is_none()));
            if case != "course" {
                assert_eq!(failure.forced, case != "sortie-normale");
            }
            failure.forced
        };
        assert!(with(|s| s.processes.get(parent.0).reader_receipt.is_some()));
        with(|s| pipe_admission::acknowledge(s, parent.0, control)).unwrap();
        assert_eq!(
            with(|s| pipe_admission::query(s, parent.0, control)),
            Err(DENIED)
        );
        let rc = collect(parent, wait);
        assert_eq!(rc, if forced { 125 } else { ordinary });
        let elapsed = ticks() - start;
        assert!(elapsed < u64::from(HZ), "collecte du producteur hors délai");
        let before = progress(witness.0);
        until(|| progress(witness.0) > before);
        hal::log(format_args!(
            "PIPE admission cas={case} force={} statut={rc} ticks={elapsed} temoin=actif",
            u8::from(forced)
        ));
    }
    hal::log(format_args!(
        "PIPE admission reponse_perdue QUERY meme_wait apres_collecte=aucun acquittement=unique OK"
    ));
    blocked_writer(parent);
    natural_125(parent);
    release(witness.0);
    assert_eq!(witness.join(), Outcome::Exit(41));
    stop(parents, [parent]);
    lost_parent();
    with(|s| {
        assert!(s.processes.slots.iter().all(Option::is_none));
        assert_eq!(s.ipc.pages(), 0);
    });
    assert_eq!(heap::audit(false), baseline);
    hal::log(format_args!("PIPE admission restitution OK"));
}
