//! Registre borné par les slots de processus. Le scheduler sérialise ses mutations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Instance {
    slot: usize,
    pub serial: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Phase {
    Created,
    Initialized,
    Active,
    Stopping,
    Terminated,
    Cleaning,
    Destroyed,
}

#[derive(Clone, Copy)]
struct Entry {
    key: Instance,
    app: u64,
    phase: Phase,
    live: usize,
    resources: usize,
}

pub(super) struct Registry<const N: usize> {
    entries: [Option<Entry>; N],
    // Dernière destruction par slot, sans référence ni droit sur les ressources.
    last_destroyed: [u64; N],
    serial: u64,
}

impl<const N: usize> Registry<N> {
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            last_destroyed: [0; N],
            // Les PID publics sont strictement inférieurs à 2^63.
            serial: 1 << 63,
        }
    }

    fn entry(&mut self, key: Instance) -> Option<&mut Entry> {
        self.entries
            .get_mut(key.slot)?
            .as_mut()
            .filter(|e| e.key == key)
    }

    pub fn available(&self) -> bool {
        self.serial < u64::MAX && self.entries.iter().any(Option::is_none)
    }

    pub fn admit_shell(&mut self, key: Instance) {
        let e = self.entry(key).unwrap();
        assert_eq!(e.app, crate::instance_abi::SYSTEM);
        e.app = crate::instance_abi::SHELL;
    }

    pub fn create(&mut self, app: u64) -> Option<Instance> {
        let slot = self.entries.iter().position(Option::is_none)?;
        let serial = self.serial.checked_add(1)?;
        self.serial = serial;
        let key = Instance { slot, serial };
        self.entries[slot] = Some(Entry {
            key,
            app,
            phase: Phase::Created,
            live: 0,
            resources: 0,
        });
        Some(key)
    }

    fn advance(&mut self, key: Instance, from: Phase, to: Phase) -> bool {
        let Some(entry) = self.entry(key) else {
            return false;
        };
        if entry.phase != from
            || !matches!(
                (from, to),
                (Phase::Created, Phase::Initialized)
                    | (Phase::Initialized, Phase::Active)
                    | (Phase::Active, Phase::Stopping)
                    | (Phase::Stopping, Phase::Terminated)
                    | (Phase::Terminated, Phase::Cleaning)
                    | (Phase::Cleaning, Phase::Destroyed)
            )
        {
            return false;
        }
        entry.phase = to;
        true
    }

    pub fn activate(&mut self, key: Instance) {
        assert!(self.advance(key, Phase::Created, Phase::Initialized));
        assert!(self.advance(key, Phase::Initialized, Phase::Active));
        assert!(self.attach(key));
    }

    pub fn attach(&mut self, key: Instance) -> bool {
        let Some(e) = self.entry(key) else {
            return false;
        };
        if e.phase != Phase::Active || e.resources >= N {
            return false;
        }
        e.live += 1;
        e.resources += 1;
        true
    }

    pub fn app(&self, key: Instance) -> Option<u64> {
        self.entries
            .get(key.slot)?
            .as_ref()
            .filter(|e| e.key == key)
            .map(|e| e.app)
    }

    pub fn stop(&mut self, key: Instance) -> bool {
        self.advance(key, Phase::Active, Phase::Stopping)
    }

    pub fn exited(&mut self, key: Instance) {
        let e = self.entry(key).expect("instance vivante");
        e.live = e.live.checked_sub(1).expect("double terminaison");
        if e.live == 0 {
            assert!(self.advance(key, Phase::Stopping, Phase::Terminated));
            assert!(self.advance(key, Phase::Terminated, Phase::Cleaning));
        }
    }

    pub fn destroyed(&self, key: Instance) -> bool {
        self.last_destroyed.get(key.slot) == Some(&key.serial)
    }

    pub fn released(&mut self, key: Instance) {
        let e = self.entry(key).expect("instance en collecte");
        e.resources = e.resources.checked_sub(1).expect("double restitution");
        if e.resources == 0 {
            assert!(self.advance(key, Phase::Cleaning, Phase::Destroyed));
            self.last_destroyed[key.slot] = key.serial;
            self.entries[key.slot] = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incarnations_and_release() {
        let mut r = Registry::<3>::new();
        let a = r.create(14).unwrap();
        assert!(!r.attach(a));
        assert!(!r.advance(a, Phase::Created, Phase::Destroyed));
        r.activate(a);
        let b = r.create(14).unwrap();
        r.activate(b);
        assert_ne!(a, b);
        assert!(r.attach(a));
        r.exited(a);
        r.released(a);
        assert!(r.attach(a));
        assert!(r.stop(a));
        assert!(!r.stop(a) && !r.attach(a));
        r.exited(a);
        r.exited(a);
        r.released(a);
        r.released(a);
        let c = r.create(14).unwrap();
        assert_eq!(a.slot, c.slot);
        assert!(r.destroyed(a));
        assert!(!r.destroyed(c));
        assert_ne!(a.serial, c.serial);
        assert_eq!(r.app(a), None);
        assert!(!r.attach(a));
        assert_eq!(r.app(b), Some(14));
    }
    #[test]
    fn exhaustion_is_closed() {
        let mut r = Registry::<1>::new();
        let a = r.create(42).unwrap();
        assert!(r.create(42).is_none());
        r.activate(a);
        assert!(!r.attach(a));
        r.stop(a);
        r.exited(a);
        r.released(a);
        r.serial = u64::MAX;
        assert!(r.create(42).is_none());
    }
}
