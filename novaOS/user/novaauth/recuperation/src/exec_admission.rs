//! Contrat privé QEMU : reçus d'admission et issues de lancement.
//! Un reçu d'admission n'est jamais une preuve de drainage ni un droit transférable.
use crate::{Error, Result, hash};
use ed25519_dalek::{Signature, VerifyingKey};
pub const SIZE:usize=256;
pub const ADMIT:&[u8]=b"NovaAuth/QEMU/exec-admission/v1\0admit\0";
pub const FINISH:&[u8]=b"NovaAuth/QEMU/exec-admission/v1\0finish\0";
pub const QUERY:&[u8]=b"NovaAuth/QEMU/exec-admission/v1\0query\0";
pub const RECEIPT:&[u8]=b"NovaAuth/QEMU/exec-admission/v1\0receipt\0";
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct Request([u8;SIZE]);
impl Request {
    pub fn parse(raw:&[u8])->Result<Self> {
        let b:[u8;SIZE]=raw.try_into().map_err(|_|Error::Format)?;
        if &b[..8]!=b"NAEXEC01" || b[72]&128!=0 {return Err(Error::Format);}
        for (start,end) in [(8,24),(24,40),(40,72),(80,112),(112,144),(144,176),
                           (176,192),(192,224),(224,240),(240,256)] {
            if b[start..end].iter().all(|v|*v==0) {return Err(Error::Format);}
        }
        Ok(Self(b))
    }
    pub fn bytes(&self)->&[u8;SIZE] {&self.0}
    pub fn scope(&self,server:&[u8;16],domain:&[u8;16],instance:&[u8;32])->Result<()> {
        if self.0[8..24]!=*server || self.0[24..40]!=*domain || self.0[40..72]!=*instance {return Err(Error::Scope);}
        Ok(())
    }
    pub fn finish(&self,state:u8,child:&[u8;32])->Result<[u8;65]> {
        if !matches!(state,1|2) || (*child==[0;32])!=(state==2) {return Err(Error::Format);}
        let mut out=[0;65];out[..32].copy_from_slice(&hash(&self.0));out[32]=state;
        out[33..].copy_from_slice(child);Ok(out)
    }
    pub fn admission(&self,key:&VerifyingKey,reply:&[u8])->Result<u64> {
        if reply.len()!=8+SIZE+64 || reply[8..8+SIZE]!=self.0 {return Err(Error::Scope);}
        verify(key,RECEIPT,&reply[..8+SIZE],&reply[8+SIZE..])?;
        let position=u64::from_be_bytes(reply[..8].try_into().unwrap());
        if position==0 {return Err(Error::Order);}Ok(position)
    }
    pub fn outcome(&self,key:&VerifyingKey,fresh:&[u8;32],reply:&[u8])->Result<Outcome> {
        const N:usize=SIZE+32+8+1+32+8;
        if *fresh==[0;32] || reply.len()!=N+64 || reply[..SIZE]!=self.0 || reply[SIZE..SIZE+32]!=*fresh {
            return Err(Error::Scope);
        }
        verify(key,QUERY,&reply[..N],&reply[N..])?;
        let p=SIZE+32;
        let admission=u64::from_be_bytes(reply[p..p+8].try_into().unwrap());
        let state=reply[p+8];let child=reply[p+9..p+41].try_into().unwrap();
        let terminal=u64::from_be_bytes(reply[p+41..p+49].try_into().unwrap());
        if admission==0 || state>2 || (state==0 && (child!=[0;32] || terminal!=0)) ||
            (state!=0 && (terminal<=admission || (child==[0;32])!=(state==2))) {return Err(Error::Order);}
        Ok(Outcome {admission,state,child,terminal})
    }
}
#[derive(Debug,Eq,PartialEq)]
pub struct Outcome {pub admission:u64,pub state:u8,pub child:[u8;32],pub terminal:u64}
fn verify(key:&VerifyingKey,prefix:&[u8],data:&[u8],signature:&[u8])->Result<()> {
    let mut packet=[0;512];let n=prefix.len()+data.len();
    if n>packet.len() {return Err(Error::Limit);}
    packet[..prefix.len()].copy_from_slice(prefix);packet[prefix.len()..n].copy_from_slice(data);
    key.verify_strict(&packet[..n],&Signature::from_slice(signature).map_err(|_|Error::Crypto)?)
        .map_err(|_|Error::Crypto)
}
