//! Table volatile, possédée exclusivement par la boucle d'admission.
use super::*;
#[derive(Clone, Copy)]
struct Lease {
    id: Digest32,
    domain: Id,
    subject: Id,
    reference: Id,
    route: Route,
    expires: u64,
}
pub struct Sessions {
    seq: u64,
    last_ms: u64,
    leases: [Option<Lease>; 16],
}
impl Sessions {
    pub const fn new() -> Self {
        Self {
            seq: 0,
            last_ms: 0,
            leases: [None; 16],
        }
    }
    fn advance(&mut self, now: u64) -> Result<(), Error> {
        if now < self.last_ms {
            self.leases.fill(None);
            return Err(Error::Unavailable);
        }
        self.last_ms = now;
        for item in &mut self.leases {
            if item.is_some_and(|l| now >= l.expires) {
                *item = None;
            }
        }
        Ok(())
    }
    pub fn open(
        &mut self,
        proof: Verified,
        current: Binding,
        now: u64,
        lifetime: u64,
    ) -> Result<Digest32, Error> {
        self.advance(now)?;
        if proof.binding != current || lifetime == 0 {
            return Err(Error::Refused);
        }
        let expires = now.checked_add(lifetime).ok_or(Error::Unavailable)?;
        let slot = self
            .leases
            .iter_mut()
            .find(|x| x.is_none())
            .ok_or(Error::Busy)?;
        let serial = self.seq.checked_add(1).ok_or(Error::Unavailable)?;
        let mut digest = Sha256::new();
        digest.update(b"NovaAuth/session/v1");
        digest.update(proof.route.issuer);
        digest.update(serial.to_le_bytes());
        digest.update(proof.request);
        let id = digest.finalize().into();
        *slot = Some(Lease {
            id,
            domain: current.domain,
            subject: current.subject,
            reference: current.reference,
            route: proof.route,
            expires,
        });
        self.seq = serial;
        Ok(id)
    }
    pub fn inspect(
        &mut self,
        id: Digest32,
        route: Route,
        current: Option<Binding>,
        now: u64,
    ) -> Result<Id, Error> {
        self.advance(now)?;
        let slot = self
            .leases
            .iter_mut()
            .find(|l| l.is_some_and(|l| l.id == id && l.route == route))
            .ok_or(Error::Refused)?;
        let lease = slot.unwrap();
        if !current.is_some_and(|b| {
            b.domain == lease.domain && b.subject == lease.subject && b.reference == lease.reference
        }) {
            *slot = None;
            return Err(Error::Refused);
        }
        Ok(lease.subject)
    }
    pub fn disconnect(&mut self, route: Route) {
        for slot in &mut self.leases {
            if slot.is_some_and(|l| l.route == route) {
                *slot = None;
            }
        }
    }
    pub fn close(&mut self, id: Digest32, route: Route) -> Result<(), Error> {
        let slot = self
            .leases
            .iter_mut()
            .find(|l| l.is_some_and(|l| l.id == id && l.route == route))
            .ok_or(Error::Refused)?;
        *slot = None;
        Ok(())
    }
}
