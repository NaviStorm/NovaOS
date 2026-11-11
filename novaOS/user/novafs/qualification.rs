//! Frontières instrumentées uniquement sur les profils de coupure du lanceur.
use crate::runtime as rt;
use core::cell::Cell;
fn marker(label: &[u8]) {
    rt::call(
        rt::DEBUG_WRITE,
        label.as_ptr() as u64,
        label.len() as u64,
        0,
    );
}
pub(crate) fn stop(label: &[u8]) -> ! {
    marker(label);
    loop {
        rt::call(rt::THREAD_YIELD, 0, 0, 0);
    }
}
#[derive(Default)]
pub(crate) struct FaultProbe {
    mode: Cell<u64>,
    phase: Cell<u8>,
}
impl FaultProbe {
    pub fn arm(&self) {
        let mode = rt::config(1);
        if mode == 25 && self.mode.get() == 0 {
            self.mode.set(mode);
            self.phase.set(1);
        }
        if (14..=21).contains(&mode) {
            self.mode.set(mode);
            self.phase.set(1);
        }
    }
    pub fn sector(&self, lba: u64, write: bool, metadata: bool) {
        if !write || lba < 2 || self.phase.get() != 1 {
            return;
        }
        match (self.mode.get(), metadata) {
            (20, false) => stop(b"COUPURE donnees"),
            (21, true) => stop(b"COUPURE metadonnees"),
            _ => (),
        }
    }
    pub fn before_flush(&self) {
        match (self.mode.get(), self.phase.get()) {
            (25, 1) | (25, 2) => marker(b"bar debut"),
            (14, 1) => stop(b"COUPURE avant F1"),
            (17, 2) => stop(b"COUPURE avant F2"),
            (15, 1) | (18, 2) => {
                marker(b"COUPURE armer blkdebug");
                // Le client attend SYNC sur le canal de retour. Cette attente bornée donne
                // à QMP le temps d'armer le point d'arrêt. Le banc exige ensuite wait_break.
                let r = rt::receive_for(rt::config(4), 250);
                if !matches!(r, Err(rt::TIMED_OUT)) {
                    rt::exit(rt::BAD_ARGUMENT)
                }
                marker(b"COUPURE soumission FLUSH");
            }
            _ => (),
        }
    }
    pub fn after_flush(&self) {
        if self.mode.get() == 25 {
            match self.phase.get() {
                1 => marker(b"bar f1"),
                2 => marker(b"bar f2"),
                _ => (),
            }
        }
        match (self.mode.get(), self.phase.get()) {
            (16, 1) => stop(b"COUPURE apres F1"),
            (19, 2) => stop(b"COUPURE apres F2"),
            (_, 1) => self.phase.set(2),
            (_, 2) => self.phase.set(0),
            _ => (),
        }
    }
}
