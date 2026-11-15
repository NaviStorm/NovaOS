//! Sonde de deux familles réellement exécutées, dont une sur le dernier CPU disponible.
use super::*;
use core::sync::atomic::{AtomicUsize, Ordering};

static ACTIVE: AtomicUsize = AtomicUsize::new(0);
static CHECKED: AtomicUsize = AtomicUsize::new(0);
static FINISHED: AtomicUsize = AtomicUsize::new(0);

fn until(mut ready: impl FnMut() -> bool) {
    let deadline = ticks() + 30 * u64::from(HZ);
    while !ready() {
        assert!(ticks() < deadline, "délai de qualification des instances");
        sleep(1);
    }
}

fn sample(mode: u64) -> Child {
    let app = spawn::sample();
    let workers = app.admission(0, false).unwrap();
    let _guard = Guard::new();
    app.principal(&mode.to_le_bytes(), workers).unwrap()
}

fn family(mode: u64) {
    let leader = sample(if mode == 0 || mode == 3 { 11 } else { mode });
    let pid = leader.0;
    let key = with(|s| s.processes.get(pid).instance);
    until(|| {
        with(|s| {
            let p = s.processes.get(pid);
            assert!(p.outcome.is_none(), "principal sorti {:?}", p.outcome);
            p.stop_offered
                && s.processes.slots.iter().flatten().any(|w| {
                    w.instance == key
                        && !w.principal
                        && w.space.as_ref().is_some_and(|space| {
                            heap::with_pages(|m| Ok(space.read_data(m, 4) == key.serial)).unwrap()
                        })
                })
        })
    });
    let worker = with(|s| {
        s.processes
            .slots
            .iter()
            .flatten()
            .find(|p| p.instance == key && !p.principal && p.threads == 2)
            .unwrap()
            .pid
    });
    // Forcer un passage effectif du calculateur du premier au dernier CPU.
    let last = crate::smp::count() - 1;
    for cpu in [0, last] {
        with(|s| {
            s.slots
                .iter_mut()
                .flatten()
                .find(|t| t.owner == Some(worker) && t.user.as_ref().is_some_and(|u| u.secondary))
                .unwrap()
                .cpu_mask = 1 << cpu;
        });
        let progress = || {
            with(|s| {
                heap::with_pages(|m| {
                    Ok(s.processes
                        .get(worker)
                        .space
                        .as_ref()
                        .unwrap()
                        .read_data(m, 5))
                })
                .unwrap()
            })
        };
        until(|| {
            with(|s| {
                s.slots.iter().flatten().any(|t| {
                    t.owner == Some(worker)
                        && t.user.as_ref().is_some_and(|u| u.secondary)
                        && t.last_cpu == cpu
                })
            })
        });
        let at = progress();
        until(|| progress() > at);
    }
    if mode < 2 {
        ACTIVE.fetch_add(1, Ordering::AcqRel);
        until(|| ACTIVE.load(Ordering::Acquire) == 2);
        with(|s| {
            assert!(s.processes.slots.iter().flatten().any(|p| p.principal
                && p.instance != key
                && s.processes.instances.app(p.instance) == Some(iw::SAMPLE)))
        });
        CHECKED.fetch_add(1, Ordering::AcqRel);
        until(|| CHECKED.load(Ordering::Acquire) == 2);
    }
    if mode == 1 {
        let mut config = [0; 16];
        for (dst, n) in config
            .chunks_exact_mut(8)
            .zip([7, WaitRight { target: pid }.key()])
        {
            dst.copy_from_slice(&n.to_le_bytes());
        }
        let ctl = include_bytes!(concat!(env!("OUT_DIR"), "/instance_sample.bin"));
        let monitor = {
            let _gate = Guard::new();
            let monitor = create_image(ctl, &config, None, Some(leader)).unwrap();
            with(|s| {
                let root = heap::with_pages(|m| {
                    Ok(s.processes
                        .get(worker)
                        .space
                        .as_ref()
                        .unwrap()
                        .read_data(m, 6))
                })
                .unwrap();
                let cap = s.ipc.derive(worker, root, monitor.0, SEND.into()).unwrap();
                // Une branche initiale de provenance tierce est fermée par le
                // bénéficiaire après avoir délégué. Sa racine extérieure doit survivre.
                let outer = s.ipc.create(monitor.0).unwrap();
                let grant = s
                    .ipc
                    .derive(monitor.0, outer, worker, (SEND | GRANT).into())
                    .unwrap();
                let sibling = s
                    .ipc
                    .derive(monitor.0, outer, worker, RECEIVE.into())
                    .unwrap();
                bind_grants(s, worker, [grant, sibling]).unwrap();
                let delegated = s.ipc.derive(worker, grant, monitor.0, SEND.into()).unwrap();
                let closed = s.ipc.lookup(worker, grant, 0).unwrap();
                s.ipc.close(closed);
                heap::with_pages(|m| {
                    let space = s.processes.get(monitor.0).space.as_ref().unwrap();
                    space.write_data(m, 3, cap);
                    space.write_data(m, 4, delegated);
                    space.write_data(m, 5, outer);
                    Ok(())
                })
                .unwrap();
                s.slots
                    .iter_mut()
                    .flatten()
                    .find(|t| t.owner == Some(monitor.0))
                    .unwrap()
                    .cpu_mask = 1;
            });
            monitor
        };
        assert_eq!(monitor.join(), Outcome::Exit(7));
    } else if mode == 3 {
        // Injection de perte du gestionnaire côté banc uniquement. La famille
        // est encore active et le travailleur continue de calculer.
        with(|s| {
            let monitor = s
                .processes
                .slots
                .iter()
                .flatten()
                .find(|p| p.watch == supervision::Watch::Bound(key))
                .unwrap()
                .pid;
            stop_process(s, monitor, Outcome::Exit(125), hal::timer::counter());
        });
        assert_eq!(leader.join(), Outcome::Exit(125));
    } else {
        with(|s| {
            heap::with_pages(|m| {
                s.processes
                    .get(pid)
                    .space
                    .as_ref()
                    .unwrap()
                    .write_data(m, 1, 1);
                Ok(())
            })
            .unwrap()
        });
        let outcome = leader.join();
        if mode == 2 {
            assert!(matches!(outcome, Outcome::Fault(_)))
        } else {
            assert_eq!(outcome, Outcome::Exit(0))
        }
    }
    until(|| {
        reap();
        with(|s| s.processes.instances.app(key).is_none())
    });
    hal::log(format_args!(
        "INSTANCE famille mode={mode} id={} pid={} cpu_cible={last} retrait=OK",
        key.serial, pid.number
    ));
}
fn peer() {
    family(1);
    FINISHED.store(1, Ordering::Release);
}

pub(crate) fn run() {
    let baseline = heap::audit(true);
    let slots = with(|s| s.audit());
    let launch_client = {
        let _guard = Guard::new();
        let image = include_bytes!(concat!(env!("OUT_DIR"), "/instance_sample.bin"));
        let child = create_image(image, &8u64.to_le_bytes(), None, None).unwrap();
        with(|s| {
            let launcher = s.processes.get_mut(child.0);
            launcher.terminal = crate::terminal::LAUNCH;
            launcher.application_threads = true;
        });
        child
    };
    assert_eq!(launch_client.join(), Outcome::Exit(8));
    hal::log(format_args!(
        "INSTANCE catalogue SPAWN principal et travailleur=OK"
    ));
    assert!(spawn::sample().admission(7, true).is_err());
    spawn_on(peer, 1 << (crate::smp::count() - 1)).unwrap();
    family(0);
    until(|| FINISHED.load(Ordering::Acquire) == 1);
    family(2);
    family(3);
    // Le collecteur réutilise le slot réel de processus. Les deux générations
    // d’instance restent distinctes, même au même indice de table.
    let image = include_bytes!(concat!(env!("OUT_DIR"), "/instance_manager.bin"));
    let once = || {
        let child = spawn_image(image, &[], None, None).unwrap();
        let identity = with(|s| (child.0, s.processes.get(child.0).instance));
        assert_eq!(child.join(), Outcome::Exit(0));
        identity
    };
    let (first, old) = once();
    let (second, fresh) = once();
    assert_eq!(first.slot, second.slot);
    assert_ne!(first.number, second.number);
    assert_ne!(old.serial, fresh.serial);
    assert!(with(|s| s.processes.instances.app(old).is_none()));
    hal::log(format_args!(
        "INSTANCE slot={} anciennes_generations_refusees=OK",
        first.slot
    ));
    until(|| {
        reap();
        with(|s| s.audit() == slots && s.processes.slots.iter().all(Option::is_none))
    });
    assert_eq!(heap::audit(true), baseline);
    hal::log(format_args!(
        "INSTANCE bilan familles=4 cadres={} tcb={} OK",
        baseline.free_pages, slots
    ));
}
