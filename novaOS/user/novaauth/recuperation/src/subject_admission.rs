//! Variante privée R2 : un ticket porte l'époque authentifiée de son titulaire.
//! NAEXEC01 et ses reçus historiques conservent leur encodage et leurs contrôles.
//! Ni le sujet ni l'époque ne proviennent des arguments de la commande.
use crate::{Error, Result, exec_admission as legacy, hash};
use ed25519_dalek::{Signature, VerifyingKey};
pub const SIZE: usize = legacy::SIZE + 24;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request([u8; SIZE]);
impl Request {
    pub fn parse(raw: &[u8]) -> Result<Self> {
        let bytes: [u8; SIZE] = raw.try_into().map_err(|_| Error::Format)?;
        if &bytes[..8] != b"NAEXAD01" || bytes[256..272] == [0; 16] {
            return Err(Error::Format);
        }
        let epoch = u64::from_be_bytes(bytes[272..].try_into().unwrap());
        if epoch == 0 || epoch >= 1 << 63 {
            return Err(Error::Format);
        }
        let mut base = [0; legacy::SIZE];
        base.copy_from_slice(&bytes[..legacy::SIZE]);
        base[..8].copy_from_slice(b"NAEXEC01");
        legacy::Request::parse(&base)?;
        Ok(Self(bytes))
    }
    pub fn bind(base: &legacy::Request, subject: [u8; 16], epoch: u64) -> Result<Self> {
        let mut bytes = [0; SIZE];
        bytes[..legacy::SIZE].copy_from_slice(base.bytes());
        bytes[..8].copy_from_slice(b"NAEXAD01");
        bytes[256..272].copy_from_slice(&subject);
        bytes[272..].copy_from_slice(&epoch.to_be_bytes());
        Self::parse(&bytes)
    }
    pub fn bytes(&self) -> &[u8; SIZE] {
        &self.0
    }
    pub fn subject(&self) -> [u8; 16] {
        self.0[256..272].try_into().unwrap()
    }
    pub fn epoch(&self) -> u64 {
        u64::from_be_bytes(self.0[272..].try_into().unwrap())
    }
    pub fn scope(&self, server: &[u8; 16], domain: &[u8; 16], instance: &[u8; 32]) -> Result<()> {
        if self.0[8..24] != *server || self.0[24..40] != *domain || self.0[40..72] != *instance {
            return Err(Error::Scope);
        }
        Ok(())
    }
    pub fn finish(&self, state: u8, child: &[u8; 32]) -> Result<[u8; 65]> {
        if !matches!(state, 1 | 2) || (*child == [0; 32]) != (state == 2) {
            return Err(Error::Format);
        }
        let mut out = [0; 65];
        // L'empreinte porte aussi sur le sujet et l'époque, jamais sur la base tronquée.
        out[..32].copy_from_slice(&hash(&self.0));
        out[32] = state;
        out[33..].copy_from_slice(child);
        Ok(out)
    }
    pub fn admission(&self, key: &VerifyingKey, reply: &[u8]) -> Result<u64> {
        if reply.len() != 8 + SIZE + 64 || reply[8..8 + SIZE] != self.0 {
            return Err(Error::Scope);
        }
        verify(key, legacy::RECEIPT, &reply[..8 + SIZE], &reply[8 + SIZE..])?;
        let position = u64::from_be_bytes(reply[..8].try_into().unwrap());
        if position == 0 {
            return Err(Error::Order);
        }
        Ok(position)
    }
    pub fn outcome(
        &self,
        key: &VerifyingKey,
        fresh: &[u8; 32],
        reply: &[u8],
    ) -> Result<legacy::Outcome> {
        const N: usize = SIZE + 32 + 8 + 1 + 32 + 8;
        if *fresh == [0; 32]
            || reply.len() != N + 64
            || reply[..SIZE] != self.0
            || reply[SIZE..SIZE + 32] != *fresh
        {
            return Err(Error::Scope);
        }
        verify(key, legacy::QUERY, &reply[..N], &reply[N..])?;
        let p = SIZE + 32;
        let admission = u64::from_be_bytes(reply[p..p + 8].try_into().unwrap());
        let state = reply[p + 8];
        let child = reply[p + 9..p + 41].try_into().unwrap();
        let terminal = u64::from_be_bytes(reply[p + 41..p + 49].try_into().unwrap());
        if admission == 0
            || state > 2
            || (state == 0 && (child != [0; 32] || terminal != 0))
            || (state != 0 && (terminal <= admission || (child == [0; 32]) != (state == 2)))
        {
            return Err(Error::Order);
        }
        Ok(legacy::Outcome {
            admission,
            state,
            child,
            terminal,
        })
    }
}
fn verify(key: &VerifyingKey, prefix: &[u8], data: &[u8], signature: &[u8]) -> Result<()> {
    let mut packet = [0; 512];
    let n = prefix.len() + data.len();
    if n > packet.len() {
        return Err(Error::Limit);
    }
    packet[..prefix.len()].copy_from_slice(prefix);
    packet[prefix.len()..n].copy_from_slice(data);
    key.verify_strict(
        &packet[..n],
        &Signature::from_slice(signature).map_err(|_| Error::Crypto)?,
    )
    .map_err(|_| Error::Crypto)
}
