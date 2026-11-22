//! R2 privé : encodages du registre vers l'autorité, sans secret de facteur.
//! Le signataire ne décide ni les rôles ni les mutations ; il vérifie la portée.
use crate::{Error, Result};
use ed25519_dalek::{Signature, VerifyingKey};
pub const PUBLISH: u16 = 50;
pub const ACKNOWLEDGE: u16 = 51;
pub const OUTCOME: u16 = 52;
pub const HISTORY: u16 = 53;
pub const HISTORY_PREFIX: &[u8] = b"NovaAuth/QEMU/administration-history/v1\0";
pub const CHANGE: &[u8] = b"NovaAuth/QEMU/administration-change/v1\0";
pub const EFFECTS: &[u8] = b"NovaAuth/QEMU/administration-effects/v1\0";
pub const QUERY: &[u8] = b"NovaAuth/QEMU/administration-query/v1\0";
pub const CREATE: u8 = 1;
pub const DISABLE: u8 = 2;
pub const REACTIVATE: u8 = 3;
pub const UPDATE: u8 = 4;
fn word(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(b[at..at + 8].try_into().unwrap())
}
#[derive(Clone, Copy)]
pub struct Message<'a> {
    operation: u16,
    bytes: &'a [u8],
}
impl<'a> Message<'a> {
    pub fn parse(operation: u16, b: &'a [u8]) -> Result<Self> {
        let size = match operation {
            PUBLISH => 209,
            ACKNOWLEDGE => 129,
            OUTCOME | HISTORY => 160,
            _ => return Err(Error::Format),
        };
        if b.len() != size
            || b[..16] == [0; 16]
            || b[16..32] == [0; 16]
            || b[32..64] == [0; 32]
            || word(b, 64) >= 1 << 63
            || word(b, 72) >= 1 << 63
            || b[80..112] == [0; 32]
            || b[112..128] == [0; 16]
        {
            return Err(Error::Format);
        }
        match operation {
            PUBLISH => {
                if word(b, 72) == (1 << 63) - 1
                    || b[128..160] == [0; 32]
                    || b[160..176] == [0; 16]
                    || word(b, 176) == 0
                    || word(b, 176) >= 1 << 63
                    || word(b, 200) >= 1 << 63
                    || !matches!(b[208], CREATE | DISABLE | REACTIVATE | UPDATE)
                    || (b[208] != UPDATE && b[184..200] == [0; 16])
                    || (b[208] == CREATE && word(b, 200) != 0)
                    || (matches!(b[208], DISABLE | REACTIVATE) && word(b, 200) == 0)
                {
                    return Err(Error::Format);
                }
            }
            ACKNOWLEDGE if !(1..=7).contains(&b[128]) => return Err(Error::Format),
            OUTCOME | HISTORY if b[128..] == [0; 32] => return Err(Error::Format),
            _ => (),
        }
        Ok(Self {
            operation,
            bytes: b,
        })
    }
    pub fn scope(
        &self,
        server: &[u8; 16],
        domain: &[u8; 16],
        incarnation: &[u8; 32],
    ) -> Result<()> {
        if self.bytes[..16] != *server
            || self.bytes[16..32] != *domain
            || self.bytes[32..64] != *incarnation
        {
            return Err(Error::Scope);
        }
        Ok(())
    }
    pub fn signed(&self, out: &mut [u8; 384]) -> usize {
        let prefix = match self.operation {
            PUBLISH => CHANGE,
            ACKNOWLEDGE => EFFECTS,
            HISTORY => HISTORY_PREFIX,
            _ => QUERY,
        };
        let n = prefix.len() + self.bytes.len();
        out[..prefix.len()].copy_from_slice(prefix);
        out[prefix.len()..n].copy_from_slice(self.bytes);
        n
    }
    /// Le reçu de publication reste celui de la tête atomique historique.
    pub fn publication(&self, key: &VerifyingKey, signature: &[u8]) -> Result<()> {
        if self.operation != PUBLISH {
            return Err(Error::Order);
        }
        const P: &[u8] =
            b"NovaAuth/QEMU/politique-recu/1\0NovaAuth/QEMU/politique-compare-publie/1\0";
        let mut packet = [0; P.len() + 160];
        packet[..P.len()].copy_from_slice(P);
        packet[P.len()..].copy_from_slice(&self.bytes[..160]);
        key.verify_strict(
            &packet,
            &Signature::from_slice(signature).map_err(|_| Error::Crypto)?,
        )
        .map_err(|_| Error::Crypto)
    }
    /// Acquittement durable d'effecteurs, distinct du reçu terminal RETIREE.
    pub fn acknowledged(&self, key: &VerifyingKey, reply: &[u8]) -> Result<u64> {
        if self.operation != ACKNOWLEDGE || reply.len() != 201 || reply[8..137] != *self.bytes {
            return Err(Error::Scope);
        }
        verify(key, EFFECTS, &reply[..137], &reply[137..])?;
        let position = word(reply, 0);
        if position == 0 {
            return Err(Error::Order);
        }
        Ok(position)
    }
    /// Historique signé avec auteur et époque d'origine. Le principal de la
    /// session courante doit être comparé par le registre avant toute divulgation.
    pub fn history<'b>(
        &self,
        key: &VerifyingKey,
        reply: &'b [u8],
    ) -> Result<(&'b [u8], u8, Option<&'b [u8]>)> {
        if self.operation != HISTORY
            || !matches!(reply.len(), 434 | 626)
            || reply[..160] != *self.bytes
        {
            return Err(Error::Scope);
        }
        let end = reply.len() - 64;
        verify_bound::<640>(key, HISTORY_PREFIX, &reply[..end], &reply[end..])?;
        let original = &reply[160..369];
        Message::parse(PUBLISH, original)?;
        if original[..32] != self.bytes[..32]
            || original[112..128] != self.bytes[112..128]
            || reply[369] > 7
        {
            return Err(Error::Scope);
        }
        let terminal = if end == 562 {
            Some(&reply[370..562])
        } else {
            None
        };
        if terminal.is_some() && (original[208] != DISABLE || reply[369] != 7) {
            return Err(Error::Order);
        }
        Ok((original, reply[369], terminal))
    }

    /// Lecture pure, avec défi frais. Un reçu terminal éventuel doit encore
    /// être vérifié par Retired::verify contre le retrait enregistré du sujet.
    pub fn outcome<'b>(
        &self,
        key: &VerifyingKey,
        reply: &'b [u8],
    ) -> Result<(u8, Option<&'b [u8]>)> {
        if self.operation != OUTCOME
            || !matches!(reply.len(), 225 | 417)
            || reply[..160] != *self.bytes
            || reply[160] > 7
        {
            return Err(Error::Scope);
        }
        let n = reply.len() - 64;
        verify(key, QUERY, &reply[..n], &reply[n..])?;
        let terminal = if n == 353 {
            if reply[160] != 7 {
                return Err(Error::Order);
            }
            Some(&reply[161..353])
        } else {
            None
        };
        Ok((reply[160], terminal))
    }
}
fn verify(key: &VerifyingKey, prefix: &[u8], body: &[u8], signature: &[u8]) -> Result<()> {
    verify_bound::<512>(key, prefix, body, signature)
}
fn verify_bound<const N: usize>(
    key: &VerifyingKey,
    prefix: &[u8],
    body: &[u8],
    signature: &[u8],
) -> Result<()> {
    let mut out = [0; N];
    let n = prefix.len() + body.len();
    if n > out.len() {
        return Err(Error::Limit);
    }
    out[..prefix.len()].copy_from_slice(prefix);
    out[prefix.len()..n].copy_from_slice(body);
    key.verify_strict(
        &out[..n],
        &Signature::from_slice(signature).map_err(|_| Error::Crypto)?,
    )
    .map_err(|_| Error::Crypto)
}
