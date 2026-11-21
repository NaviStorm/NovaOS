//! Vérificateur interne. Les liaisons et l'horloge proviennent du courtier de
//! confiance. Aucune de ces structures ne constitue un message IPC d'autorité.
#![no_std]

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

pub mod admission;
pub mod sessions;
#[cfg(feature = "administration-lab")]
pub mod epoch_sessions;

pub type Id = [u8; 16];
pub type Digest32 = [u8; 32];

/// Vue obtenue du registre actif par le service, jamais du client de connexion.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub domain: Id,
    pub subject: Id,
    pub reference: Id,
    pub head: Digest32,
}

/// Identité de l'instance cliente et incarnation du service émetteur.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub client: Digest32,
    pub issuer: Digest32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Refused,
    Busy,
    Unavailable,
}

pub trait Factor {
    type Response;
    /// Référence publique à enrôler dans credentials.db.
    fn reference(&self, domain: Id) -> Id;
    fn verify(&self, challenge: &Digest32, response: &Self::Response) -> bool;
}

pub struct Possession(pub VerifyingKey);

impl Possession {
    pub fn reference_bytes(domain: Id, key: &Digest32) -> Id {
        let mut hash = Sha256::new();
        hash.update(b"NovaAuth/Ed25519/credential/v1");
        hash.update(domain);
        hash.update(key);
        hash.finalize()[..16].try_into().unwrap()
    }
}

impl Factor for Possession {
    type Response = [u8; 64];
    fn reference(&self, domain: Id) -> Id {
        Self::reference_bytes(domain, self.0.as_bytes())
    }
    fn verify(&self, challenge: &Digest32, response: &Self::Response) -> bool {
        self.0
            .verify_strict(challenge, &Signature::from_bytes(response))
            .is_ok()
    }
}

#[derive(Clone, Copy)]
struct Attempt {
    binding: Binding,
    route: Route,
    request: Id,
    token: u64,
    expires: u64,
    challenge: Digest32,
}

/// Preuve consommable émise uniquement par finish après vérification cryptographique.
pub struct Verified {
    binding: Binding,
    request: Id,
    route: Route,
}

impl Verified {
    pub fn subject(&self) -> Id {
        self.binding.subject
    }
}

/// Une seule boucle de réception possède l'objet et sérialise les réponses.
/// Le budget d'admission doit aussi être contrôlé par l'appelant avant begin.
pub struct Verifier {
    seq: u64,
    last_ms: u64,
    pending: [Option<Attempt>; 8],
}

impl Verifier {
    pub const fn new() -> Self {
        Self {
            seq: 0,
            last_ms: 0,
            pending: [None; 8],
        }
    }

    pub fn disconnect(&mut self, route: Route) {
        for slot in &mut self.pending {
            if slot.is_some_and(|a| a.route == route) {
                *slot = None;
            }
        }
    }
    fn advance(&mut self, now_ms: u64) -> Result<(), Error> {
        if now_ms < self.last_ms {
            self.pending.fill(None);
            return Err(Error::Unavailable);
        }
        self.last_ms = now_ms;
        for slot in &mut self.pending {
            if slot.is_some_and(|attempt| now_ms >= attempt.expires) {
                *slot = None;
            }
        }
        Ok(())
    }

    /// nonce est tiré du générateur de confiance à chaque appel. L'identité
    /// complète de l'émetteur doit être neuve à chaque redémarrage du service.
    pub fn begin(
        &mut self,
        binding: Binding,
        route: Route,
        request: Id,
        nonce: Digest32,
        now_ms: u64,
        ttl_ms: u32,
    ) -> Result<(u64, Digest32), Error> {
        self.advance(now_ms)?;
        if ttl_ms == 0 || request == [0; 16] || nonce == [0; 32] {
            return Err(Error::Unavailable);
        }
        if self
            .pending
            .iter()
            .flatten()
            .any(|a| a.route == route && a.request == request)
        {
            return Err(Error::Busy);
        }
        let slot = self
            .pending
            .iter_mut()
            .find(|a| a.is_none())
            .ok_or(Error::Busy)?;
        let token = self.seq.checked_add(1).ok_or(Error::Unavailable)?;
        let expires = now_ms
            .checked_add(u64::from(ttl_ms))
            .ok_or(Error::Unavailable)?;
        let mut hash = Sha256::new();
        hash.update(b"NovaAuth/open-local-session/Ed25519/v1");
        hash.update(binding.domain);
        hash.update(binding.subject);
        hash.update(binding.reference);
        hash.update(binding.head);
        hash.update(route.client);
        hash.update(route.issuer);
        hash.update(request);
        hash.update(token.to_le_bytes());
        hash.update(expires.to_le_bytes());
        hash.update(nonce);
        let challenge = hash.finalize().into();
        self.seq = token;
        *slot = Some(Attempt {
            binding,
            route,
            request,
            token,
            expires,
            challenge,
        });
        Ok((token, challenge))
    }

    /// current exige la relecture du compte actif auprès du registre. L'appelant
    /// contrôle de nouveau la tête à l'octroi effectif sous sa barrière de retrait.
    pub fn finish<F: Factor>(
        &mut self,
        token: u64,
        route: Route,
        now_ms: u64,
        current: Option<Binding>,
        factor: &F,
        response: &F::Response,
    ) -> Result<Verified, Error> {
        self.advance(now_ms)?;
        let attempt = self
            .pending
            .iter_mut()
            .find(|entry| entry.is_some_and(|a| a.route == route && a.token == token))
            .and_then(Option::take)
            .ok_or(Error::Refused)?;
        if current != Some(attempt.binding)
            || factor.reference(attempt.binding.domain) != attempt.binding.reference
            || !factor.verify(&attempt.challenge, response)
        {
            return Err(Error::Refused);
        }
        Ok(Verified {
            binding: attempt.binding,
            request: attempt.request,
            route,
        })
    }
}


#[cfg(all(test,feature="vue-guest"))]
#[path="bin/common/view_heap.rs"]
mod view_heap_test;
