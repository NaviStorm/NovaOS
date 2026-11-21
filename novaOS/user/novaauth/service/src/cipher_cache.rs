//! Cache de ciphertexts du seul récepteur QEMU. Aucun secret en clair, aucune allocation.
//! Le contexte est l'empreinte du RecoveryContextV2 authentifié complet.
use sha2::{Digest,Sha256};
use zeroize::Zeroize;
pub const BYTES:usize=3820;
pub const FRAGMENTS:usize=8;
#[derive(Debug,PartialEq,Eq)]
pub enum Error { Context,State,Format,Integrity }
pub struct Cache {
    data:[u8;BYTES],
    entries:[[u16;2];FRAGMENTS],
    scope:[u8;32],
    seal:[u8;32],
    used:u16,
    sealed:bool,
    active:bool,
}
const _:()=assert!(core::mem::size_of::<Cache>()<=4096);
impl Cache {
    pub fn new(scope:[u8;32])->Result<Self,Error> {
        if scope==[0;32] {return Err(Error::Context);}
        Ok(Self {data:[0;BYTES],entries:[[0;2];FRAGMENTS],scope,seal:[0;32],used:0,sealed:false,active:true})
    }
    fn context(&self,scope:&[u8;32])->Result<(),Error> {
        if !self.active {return Err(Error::State);}
        if scope!=&self.scope {return Err(Error::Context);}Ok(())
    }
    pub fn insert(&mut self,scope:&[u8;32],index:u32,data:&[u8])->Result<bool,Error> {
        self.context(scope)?;
        if self.sealed {return Err(Error::State);}
        if !(17..=1040).contains(&data.len()) {return Err(Error::Format);}
        let index=index as usize;
        if index>=FRAGMENTS {return Ok(false);}
        if self.entries[index][1]!=0 {return Err(Error::State);}
        let start=self.used as usize;
        if start>BYTES {return Err(Error::Integrity);}
        if data.len()>BYTES-start {return Ok(false);}
        self.data[start..start+data.len()].copy_from_slice(data);
        self.entries[index]=[start as u16,data.len() as u16];
        self.used+=data.len() as u16;Ok(true)
    }
    fn fingerprint(&self)->[u8;32] {
        let mut hash=Sha256::new();hash.update(b"NovaAuth/QEMU/cipher-cache/v1\0");
        hash.update(self.scope);hash.update(self.used.to_le_bytes());
        for entry in self.entries {for word in entry {hash.update(word.to_le_bytes());}}
        hash.update(&self.data);hash.finalize().into()
    }
    pub fn seal(&mut self)->Result<(),Error> {
        if !self.active || self.sealed {return Err(Error::State);}
        self.seal=self.fingerprint();self.sealed=true;Ok(())
    }
    pub fn get(&self,scope:&[u8;32],index:u32)->Result<Option<&[u8]>,Error> {
        self.context(scope)?;
        if !self.sealed {return Err(Error::State);}
        if self.used as usize>BYTES || self.fingerprint()!=self.seal {return Err(Error::Integrity);}
        let Some([offset,len])=self.entries.get(index as usize).copied() else {return Ok(None);};
        if len==0 {return Ok(None);}
        let (offset,len)=(offset as usize,len as usize);
        if !(17..=1040).contains(&len) || offset+len>self.used as usize {return Err(Error::Integrity);}
        Ok(Some(&self.data[offset..offset+len]))
    }
    pub fn used(&self)->usize {self.used as usize}
    pub fn clear(&mut self) {
        self.data.zeroize();self.entries.zeroize();self.scope.zeroize();self.seal.zeroize();
        self.used=0;self.sealed=false;self.active=false;
    }
}
impl Drop for Cache {fn drop(&mut self){self.clear();}}
/// Témoin de composant exécuté dans l'invité, avant le dialogue de récupération.
/// Ces valeurs publiques synthétiques ne sont ni des parts ni des clés.
#[cfg(feature="triplet-guest")]
#[inline(never)]
pub fn qualify()->Result<(),Error> {
    let mut c=Cache::new([1;32])?;
    if c.insert(&[1;32],0,&[9;16])!=Err(Error::Format) ||
       c.insert(&[1;32],0,&[9;1041])!=Err(Error::Format) {return Err(Error::Format);}
    for i in 0..3 {if !c.insert(&[1;32],i,&[7;1040])? {return Err(Error::Format);}}
    if !c.insert(&[1;32],3,&[8;700])? || c.used()!=3820 || c.insert(&[1;32],4,&[9;17])? {return Err(Error::Format);}
    c.seal()?;
    if c.get(&[2;32],0)!=Err(Error::Context) {return Err(Error::Context);}
    c.data[0]^=1;if c.get(&[1;32],0)!=Err(Error::Integrity) {return Err(Error::Integrity);}
    c.data[0]^=1;c.entries[0][0]=1;
    if c.get(&[1;32],0)!=Err(Error::Integrity) {return Err(Error::Integrity);}
    c.clear();c.clear();
    if c.data.iter().any(|v|*v!=0) || c.entries!=[[0;2];8] || c.scope!=[0;32] || c.seal!=[0;32] || c.used()!=0 || c.get(&[1;32],0)!=Err(Error::State) {return Err(Error::State);}
    let mut other=Cache::new([3;32])?;
    for i in 0..8 {if !other.insert(&[3;32],i,&[8;17])? {return Err(Error::Format);}}
    if other.insert(&[3;32],8,&[8;17])? {return Err(Error::Format);}
    other.clear();Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn borne_octets_et_fragments() {
        let mut c=Cache::new([1;32]).unwrap();
        for i in 0..3 {assert_eq!(c.insert(&[1;32],i,&[7;1040]),Ok(true));}
        assert_eq!(c.insert(&[1;32],3,&[8;700]),Ok(true));assert_eq!(c.used(),3820);
        assert_eq!(c.insert(&[1;32],4,&[9;17]),Ok(false));c.seal().unwrap();
        assert_eq!(c.get(&[1;32],4),Ok(None));assert_eq!(c.get(&[1;32],3).unwrap().unwrap().len(),700);
        let mut c=Cache::new([2;32]).unwrap();for i in 0..8 {c.insert(&[2;32],i,&[3;17]).unwrap();}
        assert_eq!(c.insert(&[2;32],8,&[4;17]),Ok(false));
        println!("cache ciphertexts={} fragments={} taille_native={} allocation_heap=0",BYTES,FRAGMENTS,core::mem::size_of::<Cache>());
    }
    #[test] fn contexte_et_integrite() {
        let mut c=Cache::new([1;32]).unwrap();c.insert(&[1;32],0,&[7;100]).unwrap();
        assert_eq!(c.insert(&[2;32],1,&[7;100]),Err(Error::Context));c.seal().unwrap();
        assert_eq!(c.get(&[2;32],0),Err(Error::Context));assert_eq!(c.get(&[1;32],0).unwrap(),Some([7;100].as_slice()));
        c.data[0]^=1;assert_eq!(c.get(&[1;32],0),Err(Error::Integrity));
        c.data[0]^=1;c.entries[0][0]=1;assert_eq!(c.get(&[1;32],0),Err(Error::Integrity));
    }
    #[test] fn format_et_nettoyage() {
        let mut c=Cache::new([1;32]).unwrap();assert_eq!(c.insert(&[1;32],0,&[1;16]),Err(Error::Format));
        assert_eq!(c.insert(&[1;32],0,&[1;1041]),Err(Error::Format));c.insert(&[1;32],0,&[9;64]).unwrap();c.seal().unwrap();
        assert_eq!(c.insert(&[1;32],1,&[1;64]),Err(Error::State));c.clear();c.clear();
        assert!(c.data.iter().all(|v|*v==0));assert_eq!(c.entries,[[0;2];8]);assert_eq!(c.scope,[0;32]);assert_eq!(c.seal,[0;32]);
        assert_eq!(c.get(&[1;32],0),Err(Error::State));
    }
}
