//! Digest natif du triplet, partagé par l'effecteur et le registre isolé.
//! Aucune allocation, aucun accès au volume et aucune autorité d'activation.
use sha2::{Digest,Sha256};
#[derive(Debug)]
pub struct Invalid;
pub struct SnapshotDigest {
    head:[u8;144],
    lengths:[u32;3],
    table:usize,
    offset:u32,
    digest:Sha256,
}
impl SnapshotDigest {
    pub fn new(head:&[u8],lengths:[u32;3])->Result<Self,Invalid> {
        let head:[u8;144]=head.try_into().map_err(|_|Invalid)?;
        let word=|at|u64::from_be_bytes(head[at..at+8].try_into().unwrap());
        let ids=[word(64),word(72),word(80)];
        if word(88)==0 || head[..16]==[0;16] || head[96..112]==[0;16] ||
           ids.contains(&0) || ids[0]==ids[1] || ids[0]==ids[2] || ids[1]==ids[2] ||
           lengths.iter().any(|n|*n==0 || *n>16384-53) {
            return Err(Invalid);
        }
        let mut digest=Sha256::new();
        digest.update(b"NovaAuth/head/1");
        digest.update(&head[..48]);
        digest.update(ids[0].to_le_bytes());
        digest.update(u64::from(lengths[0]).to_le_bytes());
        Ok(Self {head,lengths,table:0,offset:0,digest})
    }
    pub fn push(&mut self,data:&[u8])->Result<(),Invalid> {
        if self.table>=3 || data.is_empty() ||
           data.len()>self.lengths[self.table].saturating_sub(self.offset) as usize {
            return Err(Invalid);
        }
        self.digest.update(data);
        self.offset+=data.len() as u32;
        if self.offset==self.lengths[self.table] {
            self.table+=1;self.offset=0;
            if self.table<3 {
                let at=64+8*self.table;
                let id=u64::from_be_bytes(self.head[at..at+8].try_into().unwrap());
                self.digest.update(id.to_le_bytes());
                self.digest.update(u64::from(self.lengths[self.table]).to_le_bytes());
            }
        }
        Ok(())
    }
    pub fn finish(self)->Result<(),Invalid> {
        if self.table!=3 || self.offset!=0 {return Err(Invalid);}
        let mut result=Sha256::new();
        result.update(b"NVAUTH01");result.update([3]);
        result.update(&self.head[..16]);
        result.update(u64::from_be_bytes(self.head[88..96].try_into().unwrap()).to_le_bytes());
        result.update(&self.head[96..112]);
        result.update(self.digest.finalize());
        let computed:[u8;32]=result.finalize().into();
        if computed!=self.head[112..] {return Err(Invalid);}
        Ok(())
    }
}
