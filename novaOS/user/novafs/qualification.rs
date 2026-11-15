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
        #[cfg(feature = "nvme-lab")]
        if rt::config(1) == 1001 {
            let cut = rt::config(2);
            if (1..=8).contains(&cut) {
                self.mode.set(100 + cut);
                self.phase.set(1);
            }
            return;
        }
        let mode = match rt::config(1) {
            36 => 25,
            39 => 15,
            40 => 18,
            32 => 14,
            33 => 16,
            34 => 17,
            35 => 19,
            other => other,
        };
        if mode == 25 && self.mode.get() == 0 {
            self.mode.set(mode);
            self.phase.set(1);
        }
        if (14..=21).contains(&mode) {
            self.mode.set(mode);
            self.phase.set(1);
        }
    }
    #[cfg(feature = "nvme-lab")]
    fn arm_backend(&self, label: &[u8]) {
        marker(label);
        // Le client attend SYNC ; aucune requête supplémentaire n'est valide.
        // QMP doit ensuite démontrer que la commande est réellement suspendue.
        if !matches!(rt::receive_for(rt::config(4), 250), Err(rt::TIMED_OUT)) {
            rt::exit(rt::BAD_ARGUMENT);
        }
    }

    #[cfg(feature = "nvme-lab")]
    pub fn before_page_write(&self, block: u64) {
        match (self.mode.get(), self.phase.get(), block < 2) {
            (101, 1, false) => stop(b"NVME RW coupe before-write"),
            (102, 1, false) => self.arm_backend(b"NVME RW coupe during-write"),
            (106, 2, true) => self.arm_backend(b"NVME RW coupe during-publish"),
            _ => (),
        }
    }

    pub fn sector(&self, lba: u64, write: bool, metadata: bool) {
        if !write || lba < 2 || self.phase.get() != 1 {
            return;
        }
        match (self.mode.get(), metadata) {
            #[cfg(feature = "nvme-lab")]
            (103, _) => stop(b"NVME RW coupe after-write"),
            (20, false) => stop(b"COUPURE donnees"),
            (21, true) => stop(b"COUPURE metadonnees"),
            _ => (),
        }
    }
    pub fn before_flush(&self) {
        match (self.mode.get(), self.phase.get()) {
            (25, 1) | (25, 2) => marker(b"bar debut"),
            #[cfg(feature = "nvme-lab")]
            (104, 1) => self.arm_backend(b"NVME RW coupe during-f1"),
            #[cfg(feature = "nvme-lab")]
            (107, 2) => self.arm_backend(b"NVME RW coupe during-f2"),
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
            #[cfg(feature = "nvme-lab")]
            (105, 1) => stop(b"NVME RW coupe after-f1"),
            #[cfg(feature = "nvme-lab")]
            (108, 2) => stop(b"NVME RW coupe after-f2"),
            (16, 1) => stop(b"COUPURE apres F1"),
            (19, 2) => stop(b"COUPURE apres F2"),
            (_, 1) => self.phase.set(2),
            (_, 2) => self.phase.set(0),
            _ => (),
        }
    }
}
