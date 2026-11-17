//! Encadrement privé de l'effecteur NovaFS. Le canal, jamais ces octets,
//! authentifie le registre. Le port ne possède aucun accès à l'autorité.
use super::*;
use crate::recuperation_store::{Store,Phase,FRAGMENT};
pub struct Port {
    operation:Option<Id>,
    descriptor:[u8;156],
    received:usize,
    store:Option<Store>,
    previous:Option<Head>,
}
impl Default for Port {
    fn default()->Self {Self {operation:None,descriptor:[0;156],received:0,store:None,previous:None}}
}
impl Port {
    pub fn execute<D:Device>(&mut self,fs:&mut Engine<D>,domain:Id,q:&[u8;128])->Result<[u8;128]> {
        let opcode=q[0];let table=usize::from(q[1]);
        let request:Id=q[8..24].try_into().unwrap();
        let offset=u32::from_le_bytes(q[24..28].try_into().unwrap());
        let n=u16::from_le_bytes(q[28..30].try_into().unwrap()) as usize;
        if request==[0;16] || q[2..8]!=[0;6] || q[30..32]!=[0;2] || n>FRAGMENT ||
            !matches!(opcode,1..=6) {return Err(Fault::BadInput);}
        let mut out=[0;128];
        if opcode==6 {
            let old=self.store.as_ref().ok_or(Fault::Denied)?;
            if self.operation!=Some(request) || old.phase()!=Phase::Sealed ||
                table!=0 || offset!=0 || n!=32 || q[32..64]!=old.head().digest || q[64..]!=[0;64] {
                return Err(Fault::Denied);
            }
            self.previous=Some(old.head());self.store=None;self.operation=None;
            self.descriptor.fill(0);self.received=0;
            return Ok(out);
        }
        if opcode==1 {
            if table!=0 || n==0 || offset as usize!=self.received ||
                n>156-self.received || q[32+n..]!=[0;96][..96-n] ||
                self.store.is_some() {return Err(Fault::BadInput);}
            if self.operation.is_some_and(|old|old!=request) {return Err(Fault::Conflict);}
            self.operation=Some(request);
            self.descriptor[self.received..self.received+n].copy_from_slice(&q[32..32+n]);
            self.received+=n;
            if self.received==156 {
                let lengths=core::array::from_fn(|i|u32::from_be_bytes(self.descriptor[144+4*i..148+4*i].try_into().unwrap()));
                let next=crate::recuperation::decode_head(&self.descriptor[..144])?;
                if let Some(old)=self.previous {
                    if next.request!=request || next.request==old.request ||
                        next.generation!=old.generation.checked_add(1).ok_or(Fault::Limit)? ||
                        next.volume!=old.volume || next.domain!=old.domain ||
                        next.objects!=old.objects || next.recovery!=old.recovery {
                        return Err(Fault::Conflict);
                    }
                }
                self.store=Some(Store::begin(fs,&self.descriptor[..144],request,lengths,domain)?);
            }
            out[8..16].copy_from_slice(&(self.received as u64).to_le_bytes());
            return Ok(out);
        }
        if self.operation!=Some(request) {return Err(Fault::Denied);}
        let store=self.store.as_mut().ok_or(Fault::Denied)?;
        match opcode {
            2=>{
                if q[32+n..]!=[0;96][..96-n] {return Err(Fault::BadInput);}
                store.append(fs,request,table,offset,&q[32..32+n])?;
            }
            3|5=>{
                if table!=0 || offset!=0 || n!=0 || q[32..]!=[0;96] {return Err(Fault::BadInput);}
                if opcode==3 {store.seal(fs,request)?;}
                out[8]=match store.phase() {Phase::Preparing=>1,Phase::Uncertain=>2,Phase::Sealed=>3};
                let (t,p)=store.cursor();out[9]=t as u8;out[12..16].copy_from_slice(&p.to_le_bytes());
            }
            4=>{
                if q[32..]!=[0;96] || n==0 {return Err(Fault::BadInput);}
                let size=store.read(fs,request,table,offset,&mut out[32..32+n])?;
                out[8..16].copy_from_slice(&(size as u64).to_le_bytes());
            }
            _=>return Err(Fault::BadInput)
        }
        Ok(out)
    }
}
