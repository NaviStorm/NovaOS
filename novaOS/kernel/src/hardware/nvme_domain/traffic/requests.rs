//! Deux emplacements privés, un seul consommateur matériel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key(pub u64, pub u64);

#[derive(Clone, Copy)]
pub(super) enum Owner {
    Disk,
    Nic,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Free,
    Waiting(Key),
    Done(Key),
    Held(Key),
}

pub(super) struct Requests([Slot; 2]);

impl Requests {
    pub(super) const fn new() -> Self {
        Self([Slot::Free; 2])
    }

    pub(super) fn post(&mut self, owner: Owner, key: Key) -> bool {
        let slot = &mut self.0[owner as usize];
        if *slot != Slot::Free || key.0 == 0 || key.1 == 0 {
            return false;
        }
        *slot = Slot::Waiting(key);
        true
    }

    pub(super) fn pair(&self) -> Option<(Key, Key)> {
        match self.0 {
            [Slot::Waiting(d), Slot::Waiting(n)] => Some((d, n)),
            _ => None,
        }
    }

    pub(super) fn hold_pending(&mut self) {
        for slot in &mut self.0 {
            if let Slot::Waiting(key) = *slot {
                *slot = Slot::Held(key);
            }
        }
    }

    pub(super) fn held(&self, owner: Owner, key: Key) -> bool {
        self.0[owner as usize] == Slot::Held(key)
    }

    pub(super) fn complete(&mut self, owner: Owner, key: Key) -> bool {
        let slot = &mut self.0[owner as usize];
        if *slot != Slot::Waiting(key) {
            return false;
        }
        *slot = Slot::Done(key);
        true
    }

    pub(super) fn collect(&mut self, owner: Owner, key: Key) -> bool {
        let slot = &mut self.0[owner as usize];
        if *slot != Slot::Done(key) {
            return false;
        }
        *slot = Slot::Free;
        true
    }
}
