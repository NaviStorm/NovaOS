//! Conversation privée registre/effecteur. Ni une application ni un reçu TLS
//! ne peuvent fournir ces faits. La route authentifie l'instance avant verify.
use crate::{Error, Result, hash};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VfsClosure {
    request: [u8; 128],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VfsFacts {
    removed: u64,
    cancelled: u64,
    retained: u64,
    wake: bool,
}
fn word(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}
impl VfsClosure {
    pub fn new(operation: [u8;16], subject: [u8;16], group: [u8;16], revision: u64,
        position: u64, instance: [u8;32]) -> Result<Self> {
        if operation==[0;16] || subject==[0;16] || group==[0;16] || revision>=1<<63
            || position==0 || position>=1<<63 || instance==[0;32] {return Err(Error::Scope);}
        let mut q=[0;128];q[..8].copy_from_slice(b"NXMGVFS1");
        q[8..24].copy_from_slice(&operation);q[24..40].copy_from_slice(&subject);
        q[40..56].copy_from_slice(&group);q[56..64].copy_from_slice(&revision.to_le_bytes());
        q[64..72].copy_from_slice(&position.to_le_bytes());q[72..104].copy_from_slice(&instance);
        Ok(Self{request:q})
    }
    pub fn bytes(&self)->&[u8;128] {&self.request}
    /// L'appelant vérifie le tag et le pair sur la conversation privée. Les faits
    /// d'annulation doivent encore être rapprochés des admissions individuelles.
    pub fn verify(&self, response:&[u8])->Result<VfsFacts> {
        if response.len()!=128 {return Err(Error::Format);}
        if word(response,0)!=0 {return Err(Error::Missing);}
        if &response[8..16]!=b"NXMGACK1" || response[16..48]!=hash(&self.request)
            || response[48..80]!=self.request[72..104] || word(response,96)!=0
            || word(response,112)>1 || response[120..]!=[0;8] {return Err(Error::Scope);}
        let removed=word(response,80);let cancelled=word(response,88);let retained=word(response,104);
        if removed>12 || retained>12
            || (cancelled!=0 && removed==0) || (word(response,112)!=0 && cancelled==0) {return Err(Error::Format);}
        Ok(VfsFacts{removed,cancelled,retained,wake:word(response,112)==1})
    }
}
impl VfsFacts {
    pub fn removed(&self)->u64 {self.removed}
    pub fn cancelled_prepare(&self)->Option<u64> {(self.cancelled!=0).then_some(self.cancelled)}
    pub fn retained(&self)->u64 {self.retained}
    pub fn notified(&self)->bool {self.wake}
}
