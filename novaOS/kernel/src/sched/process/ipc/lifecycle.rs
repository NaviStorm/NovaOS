//! Qualification mono-CPU des deux ordres de disparition d'une famille.
use super::*;

fn image(action: demo::Action, child: Option<Child>) -> Child {
    let mut data = [0u8; 24];
    data[..8].copy_from_slice(&(action as u64).to_le_bytes());
    data[8..16].copy_from_slice(&27u64.to_le_bytes());
    create_image(demo::DEMO, &data, None, child).unwrap()
}

fn release(pid: Pid) {
    with(|s| {
        let p = s.processes.get(pid);
        assert!(p.outcome.is_none() && p.threads == 1);
        heap::with_pages(|m| {
            p.space.as_ref().unwrap().write_data(m, 4, 1);
            Ok(())
        })
        .unwrap();
    })
}

fn wait_detached(pid: Pid) {
    let deadline = ticks() + u64::from(HZ);
    loop {
        if with(|s| {
            s.processes.slots[pid.slot]
                .as_ref()
                .is_none_or(|p| p.pid != pid || p.space.is_none())
        }) {
            return;
        }
        assert!(ticks() < deadline, "collecte de {pid:?}");
        sleep(1);
    }
}

pub(crate) fn test() {
    let baseline = heap::audit(true);
    let mut previous = None;
    for round in 0..64 {
        let parent_first = round % 2 == 0;
        let fault = round & 2 != 0;
        let first = if fault {
            demo::Action::Privileged
        } else {
            demo::Action::Exit
        };
        let (parent, child, grant) = {
            let _irq = Guard::new();
            let child = image(
                if parent_first {
                    demo::Action::Compute
                } else {
                    first
                },
                None,
            );
            let pid = child.0;
            let parent = image(
                if parent_first {
                    first
                } else {
                    demo::Action::Compute
                },
                Some(child),
            );
            let grant = with(|s| {
                let root = s.ipc.create(parent.0).unwrap();
                let grant = s.ipc.derive(parent.0, root, pid, SEND.into()).unwrap();
                if let Some((old, key)) = previous {
                    assert_eq!(pid.slot, old);
                    assert!(s.ipc.lookup(pid, key, SEND).is_err());
                }
                assert!(s.ipc.lookup(pid, grant, SEND).is_ok());
                if round & 4 != 0 {
                    let node = s.ipc.lookup(parent.0, root, MANAGE).unwrap();
                    assert_eq!(s.ipc.revoke(node), 1);
                    assert!(s.ipc.lookup(pid, grant, SEND).is_err());
                }
                grant
            });
            (parent, pid, grant)
        };
        if parent_first {
            wait_detached(parent.0);
            let outcome = parent.join();
            assert!(if fault {
                matches!(outcome, Outcome::Fault(_))
            } else {
                outcome == Outcome::Exit(27)
            });
            with(|s| {
                assert!(!s.processes.get(child).observed);
                assert!(s.ipc.lookup(child, grant, SEND).is_err());
            });
            release(child);
            wait_detached(child);
        } else {
            wait_detached(child);
            with(|s| {
                let p = s.processes.get(child);
                assert!(p.observed && p.threads == 0);
                assert!(if fault {
                    matches!(p.outcome, Some(Outcome::Fault(_)))
                } else {
                    p.outcome == Some(Outcome::Exit(27))
                });
            });
            release(parent.0);
            assert_eq!(parent.join(), Outcome::Exit(27));
        }
        with(|s| {
            assert_eq!(s.audit(), 1 + crate::smp::count());
            assert!(s.processes.slots.iter().all(Option::is_none));
            assert_eq!(s.ipc.pages(), 0);
        });
        assert_eq!(heap::audit(true), baseline);
        previous = Some((child.slot, grant));
    }
    hal::log(format_args!(
        "PROC familles=64 processus=128 cadres={} restitution OK",
        baseline.free_pages
    ));
}
