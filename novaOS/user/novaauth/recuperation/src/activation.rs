//! Un reçu historique ne rend pas une tête courante.
//! Le permis est construit seulement après reçu et observation fraîche signés.
use crate::{Error,Result,RecoveryContextV2};
use ed25519_dalek::{Signature,VerifyingKey};
pub const ACTIVATE:&[u8]=b"NovaAuth/recovery/activate/v1\0";
pub const QUERY:&[u8]=b"NovaAuth/recovery/query/v1\0";
const CURRENT:&[u8]=b"NovaAuth/QEMU/etat-courant/1\0";
fn verify(key:&VerifyingKey,prefix:&[u8],data:&[u8],signature:&[u8])->Result<()> {
    let mut message=[0;512];
    let n=prefix.len().checked_add(data.len()).filter(|&n|n<=message.len()).ok_or(Error::Limit)?;
    message[..prefix.len()].copy_from_slice(prefix);
    message[prefix.len()..n].copy_from_slice(data);
    key.verify_strict(&message[..n],&Signature::from_slice(signature).map_err(|_|Error::Format)?)
        .map_err(|_|Error::Crypto)
}
/// Pas de sérialisation de ce permis : seule l'instance qui a validé les deux
/// signatures peut l'utiliser. Il ne constitue pas à lui seul une capacité VFS.
pub struct Activation { head:[u8;32], incarnation:[u8;32], generation:u64 }
impl Activation {
    pub fn head(&self)->[u8;32] {self.head}
    pub fn incarnation(&self)->[u8;32] {self.incarnation}
    pub fn generation(&self)->u64 {self.generation}
    pub fn verify(key:&VerifyingKey,context:&RecoveryContextV2,receipt:&[u8],
                  nonce:&[u8;32],current:&[u8])->Result<Self> {
        if nonce==&[0;32] || receipt.len()!=128 || current.len()!=176 {return Err(Error::Format);}
        let c=context.bytes();
        if receipt[..32]!=context.digest() || receipt[32..64]!=c[136..168] {return Err(Error::Scope);}
        verify(key,ACTIVATE,&receipt[..64],&receipt[64..])?;
        let mut signed=[0;144];signed[..32].copy_from_slice(nonce);signed[32..].copy_from_slice(&current[..112]);
        verify(key,CURRENT,&signed,&current[112..])?;
        let epoch=u64::from_be_bytes(c[120..128].try_into().unwrap()).checked_add(1).ok_or(Error::Limit)?;
        if current[..32]!=c[24..56] || current[32..64]!=c[88..120] ||
            current[64..72]!=epoch.to_be_bytes() || current[72..112]!=c[128..168] {
            return Err(Error::Scope);
        }
        Ok(Self {head:c[136..168].try_into().unwrap(),incarnation:c[88..120].try_into().unwrap(),
            generation:u64::from_be_bytes(c[128..136].try_into().unwrap())})
    }
}
/// Retrouver le reçu d'une activation d'issue UNCERTAIN. Aucun appel de mutation.
/// L'appelant doit encore obtenir une tête fraîche avant Activation::verify.
pub fn historical_receipt(key:&VerifyingKey,context:&RecoveryContextV2,nonce:&[u8;32],
                          answer:&[u8])->Result<[u8;128]> {
    // H(R), nonce, taille état, "activated", décision(64), reçu(64), signature(64)
    if nonce==&[0;32] || answer.len()!=266 || answer[..32]!=context.digest() ||
        answer[32..64]!=*nonce || answer[64]!=9 || &answer[65..74]!=b"activated" {return Err(Error::Scope);}
    verify(key,QUERY,&answer[..202],&answer[202..])?;
    let receipt:[u8;128]=answer[74..202].try_into().unwrap();
    if receipt[..32]!=context.digest() || receipt[32..64]!=context.bytes()[136..168] {return Err(Error::Scope);}
    verify(key,ACTIVATE,&receipt[..64],&receipt[64..])?;
    Ok(receipt)
}
