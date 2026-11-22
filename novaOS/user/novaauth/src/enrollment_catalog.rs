//! Catalogue public incorporé à l'image attestée du registre QEMU.
//! Il atteste une disponibilité de facteur, jamais une possession de clé.
use crate::{Fault, Hash, Id, Result};
use sha2::{Digest, Sha256};
use ed25519_dalek::VerifyingKey;

pub struct Catalog<'a> { domain: Id, keys: &'a [u8] }
impl<'a> Catalog<'a> {
    pub fn parse(bytes: &'a [u8], expected: Hash, domain: Id) -> Result<Self> {
        if bytes.len()<32 || bytes.len()>544 || &bytes[..8]!=b"NAR2FAC1"
            || bytes[8..24]!=domain || bytes[26..32]!=[0;6]
            || <[u8;32]>::from(Sha256::digest(bytes))!=expected { return Err(Fault::Denied); }
        let count=u16::from_le_bytes(bytes[24..26].try_into().unwrap()) as usize;
        if count>16 || bytes.len()!=32+count*32 { return Err(Fault::BadInput); }
        let keys=&bytes[32..];
        for (i, raw) in keys.chunks_exact(32).enumerate() {
            let key=VerifyingKey::from_bytes(raw.try_into().unwrap()).map_err(|_|Fault::Denied)?;
            if key.is_weak() || keys[..i*32].chunks_exact(32).any(|old| old==raw) { return Err(Fault::Denied); }
        }
        Ok(Self {domain,keys})
    }
    pub fn allows(&self, reference: Id) -> bool {
        reference != [0;16] && self.keys.chunks_exact(32).any(|key| {
            let mut h=Sha256::new();h.update(b"NovaAuth/Ed25519/credential/v1");h.update(self.domain);h.update(key);
            h.finalize()[..16]==reference
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use ed25519_dalek::SigningKey;
    fn bytes(count: u8)->Vec<u8> {
        let mut v=Vec::from(*b"NAR2FAC1");v.extend_from_slice(&[7;16]);v.extend_from_slice(&(count as u16).to_le_bytes());v.extend_from_slice(&[0;6]);
        for i in 0..count {v.extend_from_slice(SigningKey::from_bytes(&[i+1;32]).verifying_key().as_bytes());} v
    }
    fn parse(b:&[u8])->Result<Catalog<'_>> {Catalog::parse(b,Sha256::digest(b).into(),[7;16])}
    #[test]
    fn exact_sizes_and_authenticated_reference() {
        for count in [0,1,16] {let b=bytes(count);assert!(parse(&b).is_ok());}
        let b=bytes(1);let c=parse(&b).unwrap();let mut h=Sha256::new();h.update(b"NovaAuth/Ed25519/credential/v1");h.update([7;16]);h.update(&b[32..]);
        assert!(c.allows(h.finalize()[..16].try_into().unwrap()));assert!(!c.allows([9;16]));
        assert!(Catalog::parse(&b,[0;32],[7;16]).is_err());assert!(Catalog::parse(&b,Sha256::digest(&b).into(),[8;16]).is_err());
    }
    #[test]
    fn malformed_weak_duplicate_and_oversized_catalogues_are_rejected() {
        assert!(parse(&bytes(17)).is_err());let b=bytes(2);
        for n in 0..b.len() {assert!(parse(&b[..n]).is_err());}
        let mut bad=b.clone();bad[26]=1;assert!(parse(&bad).is_err());
        let mut bad=b.clone();bad[24]=3;assert!(parse(&bad).is_err());
        let mut bad=b.clone();bad[64..96].copy_from_slice(&b[32..64]);assert!(parse(&bad).is_err());
        let mut bad=b;bad[32..64].fill(0);assert!(parse(&bad).is_err());
    }
}
