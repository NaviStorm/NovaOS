//! Rendez-vous privé pour le gestionnaire isolé. Un PID tiers ne peut ni
//! annoncer la politique ni attendre la fin d’une autre incarnation.
use super::*;
use crate::abi::{BAD_ARGUMENT, DENIED, REFUSED};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(super) enum Watch {
    None,
    Invited,
    Offered,
    Bound(Instance),
    Released,
}

pub(super) struct Admission {
    monitor: Option<Child>,
}

impl Admission {
    pub(super) fn workers(&self) -> bool {
        self.monitor.is_some()
    }

    pub(super) fn bind(mut self, target: Instance) {
        if let Some(child) = self.monitor.take() {
            with(|s| {
                let p = s.processes.get_mut(child.0);
                assert_eq!(p.watch, Watch::Offered);
                p.watch = Watch::Bound(target);
                s.signal(Wait::Policy(child.0));
            });
        }
    }
}

impl Drop for Admission {
    fn drop(&mut self) {
        if let Some(monitor) = &self.monitor {
            with(|s| {
                s.processes.get_mut(monitor.0).watch = Watch::Released;
                s.signal(Wait::Policy(monitor.0));
            });
        }
    }
}

pub(super) fn decide(plan: &[u8]) -> Result<Admission, u64> {
    let image = include_bytes!(concat!(env!("OUT_DIR"), "/instance_manager.bin"));
    let child = {
        let _guard = Guard::new();
        let child = create_image(image, plan, None, None).map_err(ipc::launch_error)?;
        with(|s| s.processes.get_mut(child.0).watch = Watch::Invited);
        child
    };
    loop {
        let guard = Guard::new();
        let (watch, outcome) = with(|s| {
            let p = s.processes.get(child.0);
            (p.watch, p.outcome)
        });
        if outcome.is_none() && watch == Watch::Offered {
            return Ok(Admission {
                monitor: Some(child),
            });
        }
        if outcome.is_some() {
            drop(guard);
            return match child.join() {
                Outcome::Exit(iw::ALLOW) => Ok(Admission { monitor: None }),
                _ => Err(DENIED),
            };
        }
        dispatch(Reason::Block(Wait::Policy(child.0)));
    }
}

pub(super) fn watch(stage: u64, serial: u64, reserved: u64) -> Result<u64, u64> {
    let _guard = Guard::new();
    let pid = with(|s| s.thread(s.current()).owner.unwrap());
    if reserved != 0 {
        return Err(BAD_ARGUMENT);
    }
    match stage {
        0 if serial == 0 => {
            with(|s| {
                let p = s.processes.get_mut(pid);
                if p.watch != Watch::Invited {
                    return Err(DENIED);
                }
                p.watch = Watch::Offered;
                s.signal(Wait::Policy(pid));
                Ok(())
            })?;
            loop {
                match with(|s| s.processes.get(pid).watch) {
                    Watch::Bound(key) => return Ok(key.serial),
                    Watch::Released => return Err(REFUSED),
                    Watch::Offered => dispatch(Reason::Block(Wait::Policy(pid))),
                    _ => return Err(DENIED),
                }
            }
        }
        1 => loop {
            match with(|s| s.processes.get(pid).watch) {
                Watch::Released => return Ok(0),
                Watch::Bound(key) if key.serial == serial => {
                    dispatch(Reason::Block(Wait::Policy(pid)))
                }
                _ => return Err(DENIED),
            }
        },
        _ => Err(BAD_ARGUMENT),
    }
}

pub(super) fn released(s: &mut Core, target: Instance) {
    for slot in 0..MAX {
        if let Some(p) = s.processes.slots[slot].as_mut()
            && p.watch == Watch::Bound(target)
        {
            p.watch = Watch::Released;
            let pid = p.pid;
            s.signal(Wait::Policy(pid));
            break;
        }
    }
}
