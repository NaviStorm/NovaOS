//! Cycle de supervision côté gestionnaire. Le superviseur privilégié garde les
//! processus, capacités et ressources réelles. Le catalogue livré est vide.
//! Chaque fonction PCI conserve un seul Device pendant la session de gestion.
//! Une relance du gestionnaire devra d'abord retirer les instances de l'ancienne session.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Observed,
    Starting,
    Running,
    Retiring,
    Quarantined,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    Busy,
    Denied,
    Image,
    Stale,
    Exhausted,
}

/// Entrée d'un catalogue de confiance. image identifie la révision exécutable.
/// required identifie le jeu exact de capacités minimales dans le contrat de l'image.
#[derive(Clone, Copy)]
pub struct Driver {
    pub vendor: u16,
    pub product: u16,
    pub image: u32,
    pub required: u32,
}

/// Une identité de tentative, jamais un droit MMIO, IRQ ou DMA.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Instance {
    pub location: u32,
    pub generation: u64,
}

#[derive(Clone, Copy)]
pub struct Authorization {
    pub instance: Instance,
    pub image: u32,
    pub rights: u32,
}

pub struct Device {
    id: Instance,
    vendor: u16,
    product: u16,
    state: State,
    pid: Option<u64>,
}

impl Device {
    pub fn observed(location: u32, vendor: u16, product: u16) -> Self {
        Self {
            id: Instance {
                location,
                generation: 0,
            },
            vendor,
            product,
            state: State::Observed,
            pid: None,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn matching<'a>(&self, catalog: &'a [Driver]) -> Option<&'a Driver> {
        let mut matches = catalog
            .iter()
            .filter(|d| d.vendor == self.vendor && d.product == self.product);
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(first)
        }
    }

    /// Le futur canal privé de supervision doit authentifier l'origine de permit.
    /// L'absence d'offre ou une offre pour une autre incarnation ferme la sélection.
    pub fn start(
        &mut self,
        catalog: &[Driver],
        permit: Option<Authorization>,
    ) -> Result<Instance, Error> {
        if self.state != State::Observed {
            return Err(Error::Busy);
        }
        let driver = self.matching(catalog).ok_or(Error::Image)?;
        let permit = permit.ok_or(Error::Denied)?;
        let next = Instance {
            location: self.id.location,
            generation: self.id.generation.checked_add(1).ok_or(Error::Exhausted)?,
        };
        if permit.instance != next
            || permit.image != driver.image
            || permit.rights != driver.required
        {
            return Err(Error::Denied);
        }
        self.id = next;
        self.state = State::Starting;
        Ok(next)
    }

    /// Événement émis par le lanceur de confiance après publication effective.
    pub fn started(&mut self, instance: Instance, pid: u64) -> Result<(), Error> {
        if instance != self.id || self.state != State::Starting || pid == 0 {
            return Err(Error::Stale);
        }
        self.pid = Some(pid);
        self.state = State::Running;
        Ok(())
    }

    /// Retourne la demande de retrait à remettre au superviseur, également si
    /// la création échoue après une délégation partielle ou si sa réponse est perdue.
    pub fn stop(&mut self, instance: Instance) -> Result<Instance, Error> {
        if instance != self.id || !matches!(self.state, State::Starting | State::Running) {
            return Err(Error::Stale);
        }
        self.state = State::Retiring;
        Ok(self.id)
    }

    /// Le superviseur surveille la sortie du PID publié pendant cette incarnation.
    /// Un échec du lancement avant sa confirmation suit le chemin stop(instance).
    pub fn died(&mut self, instance: Instance, pid: u64) -> Result<Instance, Error> {
        if self.state != State::Running || self.pid != Some(pid) {
            return Err(Error::Stale);
        }
        self.stop(instance)
    }

    /// Seul un message confirmé par le propriétaire privilégié des ressources peut
    /// déclarer l'instance retirée. Le temps écoulé, le PID mort ou l'absence d'IRQ
    /// ne donnent jamais cette confirmation.
    pub fn retired(&mut self, instance: Instance, confirmed: bool) -> Result<(), Error> {
        if instance != self.id || !matches!(self.state, State::Retiring | State::Quarantined) {
            return Err(Error::Stale);
        }
        if confirmed {
            self.pid = None;
            self.state = State::Observed;
        } else {
            self.state = State::Quarantined;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DRIVER: Driver = Driver {
        vendor: 0x1234,
        product: 0x4321,
        image: 9,
        required: 0x03,
    };
    const PCI: u32 = 0x0108;

    fn setup() -> (Device, Authorization) {
        let d = Device::observed(PCI, DRIVER.vendor, DRIVER.product);
        let permit = Authorization {
            instance: Instance {
                location: PCI,
                generation: 1,
            },
            image: DRIVER.image,
            rights: DRIVER.required,
        };
        (d, permit)
    }

    #[test]
    fn catalogue_vide_ambigu_absent_et_decouverte_seule() {
        let (mut d, permit) = setup();
        assert!(d.matching(&[]).is_none());
        for catalog in [
            &[][..],
            &[Driver {
                product: 0,
                ..DRIVER
            }],
            &[DRIVER, DRIVER],
        ] {
            assert_eq!(d.start(catalog, Some(permit)), Err(Error::Image));
        }
        assert_eq!(d.matching(&[DRIVER]).unwrap().image, 9);
        assert_eq!(d.start(&[DRIVER], None), Err(Error::Denied));
        assert_eq!(d.state(), State::Observed);
    }

    #[test]
    fn accord_exact_image_bdf_epoch_et_droits_minimaux() {
        let (mut d, permit) = setup();
        for rejected in [
            Authorization {
                image: 10,
                ..permit
            },
            Authorization {
                rights: 7,
                ..permit
            },
            Authorization {
                rights: 1,
                ..permit
            },
            Authorization {
                instance: Instance {
                    location: PCI + 1,
                    ..permit.instance
                },
                ..permit
            },
            Authorization {
                instance: Instance {
                    generation: 0,
                    ..permit.instance
                },
                ..permit
            },
        ] {
            assert_eq!(d.start(&[DRIVER], Some(rejected)), Err(Error::Denied));
            assert_eq!(d.state(), State::Observed);
        }
        let id = d.start(&[DRIVER], Some(permit)).unwrap();
        assert_eq!(d.start(&[DRIVER], Some(permit)), Err(Error::Busy));
        assert_eq!(d.started(id, 0), Err(Error::Stale));
        d.started(id, 23).unwrap();
        assert_eq!(d.state(), State::Running);
        assert_eq!(d.died(id, 24), Err(Error::Stale));
        assert_eq!(d.started(id, 24), Err(Error::Stale));
    }

    #[test]
    fn mort_quarantaine_et_redemarrage_sur_nouvel_accord() {
        let (mut d, mut permit) = setup();
        let first = d.start(&[DRIVER], Some(permit)).unwrap();
        d.started(first, 90).unwrap();
        let request = d.died(first, 90).unwrap();
        assert_eq!(request, first);
        assert_eq!(d.start(&[DRIVER], Some(permit)), Err(Error::Busy));
        d.retired(request, false).unwrap();
        assert_eq!(d.state(), State::Quarantined);
        let future = Instance {
            generation: 2,
            ..first
        };
        assert_eq!(d.retired(future, true), Err(Error::Stale));
        assert_eq!(d.start(&[DRIVER], Some(permit)), Err(Error::Busy));
        d.retired(request, true).unwrap();
        assert_eq!(d.start(&[DRIVER], Some(permit)), Err(Error::Denied));
        assert_eq!(d.start(&[DRIVER], None), Err(Error::Denied));
        permit.instance = future;
        let second = d.start(&[DRIVER], Some(permit)).unwrap();
        d.started(second, 91).unwrap();
        assert_eq!(d.retired(request, true), Err(Error::Stale));
        assert_eq!(d.died(first, 91), Err(Error::Stale));
        assert_eq!(d.died(second, 90), Err(Error::Stale));
        assert_eq!(d.stop(second), Ok(second));
    }

    #[test]
    fn echec_apres_lancement_partiel_et_exhaustion_generation() {
        let (mut d, permit) = setup();
        let id = d.start(&[DRIVER], Some(permit)).unwrap();
        d.stop(id).unwrap();
        assert_eq!(d.started(id, 21), Err(Error::Stale));
        d.retired(id, true).unwrap();
        d.id.generation = u64::MAX;
        assert_eq!(d.start(&[DRIVER], Some(permit)), Err(Error::Exhausted));
        assert_eq!(d.state(), State::Observed);
    }
}
