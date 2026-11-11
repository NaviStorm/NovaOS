//! Sondes de bout en bout. Le contrôleur noyau orchestre des programmes CPL3/EL0.
use super::*;
use core::sync::atomic::{AtomicBool, Ordering};
use demo::DEMO;

static MEASURE_IDLE: AtomicBool = AtomicBool::new(false);
pub(crate) fn idle_sample() {
    if MEASURE_IDLE.load(Ordering::Relaxed) && MEASURE_IDLE.swap(false, Ordering::Relaxed) {
        measure("repos_ui");
    }
}

fn client(mode: u64, target: Pid, cap: Option<Child>, kind: u64, code: u64) -> Child {
    let mut data = [0u8; 88];
    for (i, v) in [
        (0, mode),
        (5, WaitRight { target }.key()),
        (6, target.number),
        (7, kind),
        (10, code),
    ] {
        data[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }
    spawn_image(DEMO, &data, None, cap).unwrap()
}

fn until(mut condition: impl FnMut() -> bool) {
    let deadline = ticks() + u64::from(HZ) * 12;
    while !condition() {
        assert!(ticks() < deadline, "délai de la sonde ABI");
        sleep(1);
    }
}

fn data(pid: Pid, index: usize, value: Option<u64>) -> u64 {
    with(|s| {
        let p = s.processes.get(pid);
        assert!(p.outcome.is_none(), "fin prématurée {:?}", p.outcome);
        let space = p.space.as_ref().unwrap();
        heap::with_pages(|m| {
            if let Some(v) = value {
                space.write_data(m, index, v);
            }
            Ok(space.read_data(m, index))
        })
        .unwrap()
    })
}
fn blocked(waiter: Pid, target: Pid) -> bool {
    with(|s| {
        s.slots
            .iter()
            .flatten()
            .any(|t| t.owner == Some(waiter) && t.state == State::Blocked(Wait::Process(target)))
    })
}

pub(crate) fn test() {
    let baseline = heap::audit(false);
    measure("repos");
    demo::shortage();
    let recovered = demo::spawn_demo(demo::Action::Exit, 31, 0).unwrap();
    assert_eq!(recovered.join(), Outcome::Exit(31));
    measure("reprise");
    let target = demo::spawn_demo(demo::Action::Owner, 73, 0).unwrap();
    let pid = target.0;
    let waiter = client(11, pid, Some(target), 0, 73);
    let thief = client(12, pid, None, 0, 73);
    assert_eq!(thief.join(), Outcome::Exit(12));
    let initial = with(|s| s.processes.syscall_switches);
    until(|| data(waiter.0, 3, None) >= 4096 && with(|s| s.processes.syscall_switches) > initial);
    let loops = data(waiter.0, 3, None);
    assert_eq!(data(waiter.0, 8, None), waiter.0.number);
    let tid = with(|s| {
        s.slots
            .iter()
            .flatten()
            .find(|t| t.owner == Some(waiter.0))
            .unwrap()
            .id
            .generation
    });
    assert_eq!(data(waiter.0, 9, None), tid);
    measure("actifs");
    data(waiter.0, 4, Some(1));
    until(|| blocked(waiter.0, pid));
    data(pid, 4, Some(1));
    assert_eq!(waiter.join(), Outcome::Exit(11));

    // Retirer le mapping de sortie après sa validation et avant le réveil.
    let target = demo::spawn_demo(demo::Action::Compute, 85, 0).unwrap();
    let pid = target.0;
    with(|s| {
        let space = s.processes.get_mut(pid).space.as_mut().unwrap();
        heap::with_pages(|m| {
            space.test_copy(m);
            Ok(())
        })
        .unwrap();
    });
    hal::log(format_args!(
        "ABI copie sur deux pages et retrait atomique OK"
    ));
    let waiter = client(13, pid, Some(target), 0, 85);
    until(|| blocked(waiter.0, pid));
    with(|s| {
        let space = s.processes.get_mut(waiter.0).space.as_mut().unwrap();
        heap::with_pages(|m| {
            space.revoke_demo_data(m);
            Ok(())
        })
        .unwrap();
    });
    data(pid, 4, Some(1));
    assert_eq!(waiter.join(), Outcome::Exit(13));

    // Un droit maintient le statut d’un objet déjà collecté, sans conserver ses pages.
    let target = demo::spawn_demo(demo::Action::Privileged, 0, 0).unwrap();
    until(|| target.try_result().is_some());
    let waiter = client(14, target.0, Some(target), 1, 0);
    assert_eq!(waiter.join(), Outcome::Exit(14));
    // Le receveur meurt sans appeler wait. Son droit est libéré même si la
    // cible est déjà collectée ; aucun enregistrement orphelin ne doit rester.
    let target = demo::spawn_demo(demo::Action::Exit, 27, 0).unwrap();
    until(|| target.try_result().is_some());
    let abandoned = client(4, target.0, Some(target), 0, 27);
    assert!(matches!(abandoned.join(), Outcome::Fault(_)));
    for mode in [15u64, 16] {
        let child = spawn_image(DEMO, &mode.to_le_bytes(), None, None).unwrap();
        let Outcome::Fault(f) = child.join() else {
            panic!("retour invalide accepté")
        };
        assert_eq!((f.trap, f.cause), ("ABI", "contexte"));
    }
    context::report_syscalls();
    assert_eq!(heap::audit(false), baseline);
    assert!(with(|s| s.processes.slots.iter().all(Option::is_none)));
    measure("detruits");
    let (timer, waits) = with(|s| (s.processes.syscall_switches, s.processes.blocked_waits));
    assert!(timer > 0 && waits >= 2);
    hal::log(format_args!(
        "ABI valide series={loops} preemptions={timer} attentes={waits} revocation=OK"
    ));
    MEASURE_IDLE.store(true, Ordering::Relaxed);
}
