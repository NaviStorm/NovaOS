//! Observations de l'invité, par génération du TCB, sous la garde de Core.
//! La fonctionnalité dédiée est absente des noyaux ordinaires et chronométrés.
use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct Fil {
    generation: u64,
    dernier_depart: u64,
    pret: u64,
    bloque: u64,
    pub departs: u64,
    pub reveils: u64,
    pub preemptions: u64,
    pub cpus: u64,
    pub entre_max: u64,
    pub pret_max: u64,
    pub bloque_max: u64,
    pub lents: u64,
}

pub(super) struct Mesures {
    pub active: bool,
    seuil: u64,
    fils: [Fil; MAX],
}

impl Mesures {
    pub fn new() -> Self {
        Self {
            active: false,
            seuil: 0,
            fils: [Fil::default(); MAX],
        }
    }

    pub fn commencer(&mut self, hz: u64) {
        self.seuil = hz; // Alerte diagnostique à une seconde READY sans exécution.
        self.active = true;
    }

    pub fn inscrire(&mut self, id: Id, etat: State) {
        let marque = hal::timer::counter();
        self.fils[id.slot] = Fil {
            generation: id.generation,
            dernier_depart: 0,
            pret: if etat == State::Ready { marque } else { 0 },
            bloque: if matches!(etat, State::Blocked(_)) {
                marque
            } else {
                0
            },
            ..Fil::default()
        };
    }

    fn suivi(&mut self, id: Id) -> Option<&mut Fil> {
        let f = &mut self.fils[id.slot];
        (f.generation == id.generation && self.active).then_some(f)
    }

    pub fn lire(&self, id: Id) -> Fil {
        let f = self.fils[id.slot];
        assert_eq!(f.generation, id.generation);
        f
    }

    pub fn reveil(&mut self, id: Id) {
        if let Some(f) = self.suivi(id) {
            let maintenant = hal::timer::counter();
            if f.bloque != 0 {
                f.bloque_max = f.bloque_max.max(maintenant.saturating_sub(f.bloque));
            }
            f.bloque = 0;
            f.pret = maintenant;
            f.reveils += 1;
        }
    }

    pub fn transition(&mut self, old: Id, next: Id, etat: State, cpu: usize, why: Reason) {
        if old == next {
            return;
        }
        let maintenant = hal::timer::counter();
        if let Some(f) = self.suivi(old) {
            match etat {
                State::Ready => f.pret = maintenant,
                State::Blocked(_) => f.bloque = maintenant,
                _ => (),
            }
            if matches!(why, Reason::Tick(_)) {
                f.preemptions += 1;
            }
        }
        let seuil = self.seuil;
        if let Some(f) = self.suivi(next) {
            f.departs += 1;
            f.cpus |= 1 << cpu;
            if f.dernier_depart != 0 {
                f.entre_max = f.entre_max.max(maintenant.saturating_sub(f.dernier_depart));
            }
            f.dernier_depart = maintenant;
            if f.pret != 0 {
                let attente = maintenant.saturating_sub(f.pret);
                f.pret_max = f.pret_max.max(attente);
                f.lents += u64::from(attente > seuil);
                f.pret = 0;
            }
        }
    }
}
