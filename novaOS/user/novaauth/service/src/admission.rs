//! Budgets en mémoire contrôlés avant le décodage de la clé, le registre et Ed25519.
//! Deux origines matérielles (canaux privés) et huit facteurs simultanément pénalisés.
use super::Id;
const MAX_MS: u64 = 640;
#[derive(Clone, Copy, Default)]
struct Delay {
    fails: u32,
    until: u64,
}
impl Delay {
    fn punish(&mut self, now: u64) {
        self.fails = self.fails.saturating_add(1).min(6);
        self.until = now.saturating_add(10 << self.fails);
    }
}
pub struct Admission {
    origins: [Delay; 2],
    factors: [Option<(Id, Delay)>; 8],
    global: u64,
}
impl Admission {
    pub fn new(now: u64) -> Self {
        Self {
            origins: [Delay::default(); 2],
            factors: [None; 8],
            global: now.saturating_add(MAX_MS),
        }
    }
    pub fn allowed(&self, origin: usize, id: Id, now: u64) -> bool {
        now >= self.global
            && self.origins.get(origin).is_some_and(|d| now >= d.until)
            && !self
                .factors
                .iter()
                .flatten()
                .any(|(key, d)| *key == id && now < d.until)
    }
    pub fn fail(&mut self, origin: usize, id: Id, now: u64) {
        let Some(source) = self.origins.get_mut(origin) else {
            return;
        };
        source.punish(now);
        self.global = now.saturating_add(10);
        let index = self
            .factors
            .iter()
            .position(|s| s.is_some_and(|(key, _)| key == id))
            .or_else(|| {
                self.factors
                    .iter()
                    .position(|s| s.is_none_or(|(_, d)| d.until <= now))
            });
        if let Some(index) = index {
            let mut d = self.factors[index]
                .filter(|(key, _)| *key == id)
                .map_or(Delay::default(), |(_, d)| d);
            d.punish(now);
            self.factors[index] = Some((id, d));
        } else {
            self.global = now.saturating_add(MAX_MS);
        }
    }
    pub fn success(&mut self, origin: usize, id: Id) {
        if let Some(s) = self.origins.get_mut(origin) {
            *s = Delay::default();
        }
        for slot in &mut self.factors {
            if slot.is_some_and(|(key, _)| key == id) {
                *slot = None;
            }
        }
    }
}
