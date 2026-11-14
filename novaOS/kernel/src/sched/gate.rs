//! Exclusion des transactions scheduler, processus et IPC. Les appels imbriqués
//! restent sur le même processeur tant que les IRQ sont masquées.
use crate::{hal, smp};
use core::sync::atomic::{AtomicUsize, Ordering};

pub(super) static OWNER: AtomicUsize = AtomicUsize::new(0);
static DEPTH: AtomicUsize = AtomicUsize::new(0);

#[track_caller]
fn acquire(depth: usize) {
    let me = smp::index() + 1;
    if OWNER.load(Ordering::Relaxed) == me {
        // Seul OWNER peut consulter ou modifier DEPTH, IRQ masquées. La
        // barrière de transfert de propriété porte sur OWNER.
        DEPTH.store(DEPTH.load(Ordering::Relaxed) + depth, Ordering::Relaxed);
        return;
    }
    let start = super::profile::stamp();
    let mut conflict = false;
    while OWNER
        .compare_exchange_weak(0, me, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        conflict = true;
        while OWNER.load(Ordering::Relaxed) != 0 {
            crate::smp::check_stop();
            crate::arch::poll_translation();
            core::hint::spin_loop();
        }
    }
    super::profile::acquired(me - 1, start, conflict, core::panic::Location::caller());
    DEPTH.store(depth, Ordering::Relaxed);
}

pub(crate) struct Guard(usize, core::marker::PhantomData<*mut ()>);
impl Guard {
    #[track_caller]
    pub fn new() -> Self {
        let flags = hal::interrupts::save_and_mask();
        acquire(1);
        Self(flags, core::marker::PhantomData)
    }
    pub fn was_enabled(&self) -> bool {
        hal::context::irq_enabled(self.0)
    }
    pub fn unlock_masked(self) {
        core::mem::forget(self);
        unlock();
    }
}
fn unlock() {
    let remaining = DEPTH.load(Ordering::Relaxed) - 1;
    DEPTH.store(remaining, Ordering::Relaxed);
    if remaining == 0 {
        if super::profile::ACTIVE.load(Ordering::Relaxed) {
            super::profile::released(smp::index());
        }
        OWNER.store(0, Ordering::Release);
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        unlock();
        hal::interrupts::restore(self.0);
    }
}

// La pile sortante conserve la profondeur. L'assembleur libère OWNER après
// sauvegarde de SP et installation de la pile cible, avant son premier accès global.
pub(super) fn outgoing() -> usize {
    assert_eq!(OWNER.load(Ordering::Relaxed), smp::index() + 1);
    if super::profile::ACTIVE.load(Ordering::Relaxed) {
        super::profile::released(smp::index());
    }
    let depth = DEPTH.load(Ordering::Relaxed);
    DEPTH.store(0, Ordering::Relaxed);
    depth
}
pub(super) fn resumed(depth: usize) {
    acquire(depth);
}
