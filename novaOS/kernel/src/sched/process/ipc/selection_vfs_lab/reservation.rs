//! Réservation privée de qualification. Les valeurs proviennent des tables
//! du noyau, après résolution des handles locaux et du catalogue immuable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Destination {
    pub(super) shell: [u8; 32],
    pub(super) session: [u8; 32],
    pub(super) pipe: u64,
    pub(super) image: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    AttenteVfs,
    Validee,
    EnAdmission,
}

#[derive(Clone, Copy)]
pub(super) struct Reservation {
    pub(super) cle: u64,
    pub(super) destination: Destination,
    pub(super) limite: u64,
    phase: Phase,
}

#[derive(Default)]
pub(super) struct Reservations(Option<Reservation>);

impl Reservations {
    pub(super) fn preparer(
        &mut self,
        cle: u64,
        destination: Destination,
        maintenant: u64,
        limite: u64,
    ) -> bool {
        self.expirer(maintenant);
        if self.0.is_some() || cle == 0 || maintenant >= limite {
            return false;
        }
        self.0 = Some(Reservation {
            cle,
            destination,
            limite,
            phase: Phase::AttenteVfs,
        });
        true
    }

    pub(super) fn courante(&self, cle: u64, maintenant: u64) -> Option<Reservation> {
        self.0.filter(|r| r.cle == cle && maintenant < r.limite)
    }

    pub(super) fn valider_vfs(&mut self, cle: u64, maintenant: u64) -> bool {
        let Some(r) = self.0.as_mut() else {
            return false;
        };
        if r.cle != cle || maintenant >= r.limite || r.phase != Phase::AttenteVfs {
            return false;
        }
        r.phase = Phase::Validee;
        true
    }

    pub(super) fn prendre(&mut self, cle: u64, destination: Destination, maintenant: u64) -> bool {
        let Some(r) = self.0.as_mut() else {
            return false;
        };
        if r.cle != cle
            || destination != r.destination
            || maintenant >= r.limite
            || r.phase != Phase::Validee
        {
            return false;
        }
        r.phase = Phase::EnAdmission;
        true
    }

    /// Seule la même réservation peut publier après une préparation bloquante.
    pub(super) fn terminer(&mut self, cle: u64, destination: Destination, maintenant: u64) -> bool {
        let valide = self
            .courante(cle, maintenant)
            .is_some_and(|r| r.destination == destination && r.phase == Phase::EnAdmission);
        if valide {
            self.0 = None;
        }
        valide
    }

    pub(super) fn retirer(&mut self, cle: u64) -> bool {
        if self.0.is_some_and(|r| r.cle == cle) {
            self.0 = None;
            return true;
        }
        false
    }

    pub(super) fn expirer(&mut self, maintenant: u64) -> bool {
        if self.0.is_some_and(|r| maintenant >= r.limite) {
            self.0 = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    fn cible() -> Destination {
        Destination {
            shell: [1; 32],
            session: [2; 32],
            pipe: 3,
            image: 4,
        }
    }
    #[test]
    fn vfs_et_destination_exacte() {
        let mut r = Reservations::default();
        let d = cible();
        assert!(r.preparer(17, d, 0, 10));
        assert!(!r.prendre(17, d, 1));
        assert!(!r.valider_vfs(16, 1));
        assert!(r.valider_vfs(17, 1));
        assert!(!r.valider_vfs(17, 1));
        for autre in [
            Destination {
                shell: [9; 32],
                ..d
            },
            Destination {
                session: [9; 32],
                ..d
            },
            Destination { pipe: 9, ..d },
            Destination { image: 9, ..d },
        ] {
            assert!(!r.prendre(17, autre, 2));
        }
        assert!(r.prendre(17, d, 2));
        assert!(!r.prendre(17, d, 3));
        assert!(r.terminer(17, d, 4));
        assert!(!r.terminer(17, d, 4));
    }
    #[test]
    fn echeance_et_retrait_pendant_admission() {
        for retrait in [true, false] {
            let mut r = Reservations::default();
            let d = cible();
            assert!(r.preparer(1, d, 0, 4));
            assert!(r.valider_vfs(1, 1));
            assert!(r.prendre(1, d, 2));
            if retrait {
                assert!(r.retirer(1));
            } else {
                assert!(r.expirer(4));
            }
            assert!(r.preparer(2, d, 5, 10));
            assert!(!r.retirer(1));
            assert!(!r.terminer(1, d, 6));
            assert!(r.valider_vfs(2, 6));
        }
    }
    #[test]
    fn demande_simultanee() {
        let mut r = Reservations::default();
        assert!(r.preparer(1, cible(), 0, 10));
        assert!(r.valider_vfs(1, 1));
        let r = Arc::new(Mutex::new(r));
        let debut = Arc::new(Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let r = Arc::clone(&r);
                let debut = Arc::clone(&debut);
                thread::spawn(move || {
                    debut.wait();
                    r.lock().unwrap().prendre(1, cible(), 2)
                })
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .filter(|ok| *ok)
                .count(),
            1
        );
    }
}
