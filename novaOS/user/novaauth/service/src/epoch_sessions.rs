//! Variante privée QEMU : une époque clôturée ne redevient jamais un bail.
//! La tête et l'époque viennent du registre authentifié, jamais des arguments client.
//! Réutilise le vérificateur de possession et la table historique, sans les modifier.
use super::{sessions::Sessions, *};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EpochBinding {
    pub binding: Binding,
    pub epoch: u64,
}
impl EpochBinding {
    fn bound(self) -> Result<Binding, Error> {
        if self.epoch == 0 {
            return Err(Error::Refused);
        }
        let mut hash = Sha256::new();
        hash.update(b"NovaAuth/QEMU/authentication-epoch/v1\0");
        hash.update(self.binding.head);
        hash.update(self.epoch.to_le_bytes());
        Ok(Binding {
            head: hash.finalize().into(),
            ..self.binding
        })
    }
}
pub struct EpochProof {
    verified: Verified,
    binding: EpochBinding,
}
pub struct EpochVerifier(Verifier);
impl EpochVerifier {
    pub const fn new() -> Self {
        Self(Verifier::new())
    }
    pub fn begin(
        &mut self,
        binding: EpochBinding,
        route: Route,
        request: Id,
        nonce: Digest32,
        now: u64,
        ttl: u32,
    ) -> Result<(u64, Digest32), Error> {
        self.0
            .begin(binding.bound()?, route, request, nonce, now, ttl)
    }
    pub fn finish<F: Factor>(
        &mut self,
        token: u64,
        route: Route,
        now: u64,
        current: Option<EpochBinding>,
        factor: &F,
        response: &F::Response,
    ) -> Result<EpochProof, Error> {
        // Même un refus de liaison consomme le défi historique à usage unique.
        let bound = current.and_then(|v| v.bound().ok());
        let verified = self.0.finish(token, route, now, bound, factor, response)?;
        Ok(EpochProof {
            verified,
            binding: current.ok_or(Error::Refused)?,
        })
    }
    pub fn disconnect(&mut self, route: Route) {
        self.0.disconnect(route);
    }
}

#[derive(Clone, Copy)]
struct EpochLease {
    id: Digest32,
    route: Route,
    domain: Id,
    subject: Id,
    epoch: u64,
    expires: u64,
}
#[derive(Clone, Copy)]
struct ClosedEpoch {
    domain: Id,
    subject: Id,
    through: u64,
}
pub struct EpochSessions {
    sessions: Sessions,
    leases: [Option<EpochLease>; 16],
    last_ms: u64,
    closed: [Option<ClosedEpoch>; 64],
    quarantined: bool,
}
impl EpochSessions {
    pub const fn new() -> Self {
        Self {
            sessions: Sessions::new(),
            leases: [None; 16],
            last_ms: 0,
            closed: [None; 64],
            quarantined: false,
        }
    }
    fn allowed(&self, b: EpochBinding) -> bool {
        !self.quarantined
            && b.epoch != 0
            && !self.closed.iter().flatten().any(|c| {
                c.domain == b.binding.domain
                    && c.subject == b.binding.subject
                    && b.epoch <= c.through
            })
    }
    fn advance(&mut self, now: u64) -> Result<(), Error> {
        let reversed = now < self.last_ms;
        for entry in &mut self.leases {
            if entry.is_some_and(|l| reversed || now >= l.expires) {
                let l = entry.take().unwrap();
                let _ = self.sessions.close(l.id, l.route);
            }
        }
        // Ne jamais abaisser la référence monotone après un recul.
        if reversed {
            return Err(Error::Unavailable);
        }
        self.last_ms = now;
        Ok(())
    }
    pub fn open(
        &mut self,
        proof: EpochProof,
        current: EpochBinding,
        now: u64,
        lifetime: u64,
    ) -> Result<Digest32, Error> {
        self.advance(now)?;
        if proof.binding != current || lifetime == 0 || !self.allowed(current) {
            return Err(Error::Refused);
        }
        let expires = now.checked_add(lifetime).ok_or(Error::Unavailable)?;
        let slot = self
            .leases
            .iter_mut()
            .find(|s| s.is_none())
            .ok_or(Error::Busy)?;
        let route = proof.verified.route;
        let id = self
            .sessions
            .open(proof.verified, current.bound()?, now, lifetime)?;
        *slot = Some(EpochLease {
            id,
            route,
            domain: current.binding.domain,
            subject: current.binding.subject,
            epoch: current.epoch,
            expires,
        });
        Ok(id)
    }
    pub fn inspect(
        &mut self,
        id: Digest32,
        route: Route,
        current: Option<EpochBinding>,
        now: u64,
    ) -> Result<Id, Error> {
        self.advance(now)?;
        let allowed = current.is_some_and(|b| self.allowed(b));
        let slot = self
            .leases
            .iter_mut()
            .find(|l| l.is_some_and(|l| l.id == id && l.route == route))
            .ok_or(Error::Refused)?;
        let lease = slot.unwrap();
        let valid = current.is_some_and(|b| {
            b.epoch != 0
                && b.epoch == lease.epoch
                && b.binding.domain == lease.domain
                && b.binding.subject == lease.subject
        });
        if !valid || !allowed {
            let _ = self.sessions.close(id, route);
            *slot = None;
            return Err(Error::Refused);
        }
        let result = self
            .sessions
            .inspect(id, route, current.and_then(|b| b.bound().ok()), now);
        if result.is_err() {
            *slot = None;
        }
        result
    }
    /// Notification privée d'une clôture. Appel après validation du sujet et de
    /// l'époque par le registre ; ni identifiant client ni heure locale ne font foi.
    pub fn retire(&mut self, domain: Id, subject: Id, epoch: u64) -> Result<usize, Error> {
        if domain == [0; 16] || subject == [0; 16] || epoch == 0 {
            return Err(Error::Refused);
        }
        let slot = self
            .closed
            .iter()
            .position(|c| c.is_some_and(|c| c.domain == domain && c.subject == subject))
            .or_else(|| self.closed.iter().position(Option::is_none));
        if let Some(at) = slot {
            let through = self.closed[at].map_or(epoch, |c| c.through.max(epoch));
            self.closed[at] = Some(ClosedEpoch {
                domain,
                subject,
                through,
            });
        } else {
            // Ne jamais oublier un retrait pour faire de la place. La table est
            // bornée comme le registre (64 sujets) ; saturation = refus sûr.
            self.quarantined = true;
            for entry in &mut self.leases {
                if let Some(l) = entry.take() {
                    let _ = self.sessions.close(l.id, l.route);
                }
            }
            return Err(Error::Busy);
        }
        let mut count = 0;
        for slot in &mut self.leases {
            if slot.is_some_and(|l| l.domain == domain && l.subject == subject && l.epoch <= epoch)
            {
                let l = slot.take().unwrap();
                let _ = self.sessions.close(l.id, l.route);
                count += 1;
            }
        }
        Ok(count)
    }
    pub fn close(&mut self, id: Digest32, route: Route) -> Result<(), Error> {
        self.sessions.close(id, route)?;
        for l in &mut self.leases {
            if l.is_some_and(|l| l.id == id && l.route == route) {
                *l = None;
            }
        }
        Ok(())
    }
    pub fn disconnect(&mut self, route: Route) {
        self.sessions.disconnect(route);
        for l in &mut self.leases {
            if l.is_some_and(|l| l.route == route) {
                *l = None;
            }
        }
    }
}
